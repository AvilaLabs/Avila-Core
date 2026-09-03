#!/usr/bin/env python3
"""Activate each layer of a slab candidate under ACTINV and report I4.

Reads the per-layer neutron spectra a transport run produced (interface I3,
``avila.shielding/layer-spectra/v1``), looks up each layer's material in the
shared shielding material table, and runs ACTINV once per layer over that
layer's own spectrum, converted composition, and computed mass, under one
declared irradiation/cooling schedule. It reports specific activity, decay
heat and, only when ACTINV's own photon-response machinery can produce it, a
contact-gamma dose-rate proxy, at the end of the declared cooling period
(interface I4, ``avila.shielding/activation-result/v1``).

This script interprets nothing scientifically: it maps a layer's already-
computed spectrum and a table entry onto an ``actinv-spec-1`` problem, runs
the exact staged ACTINV executable over exact staged data, and extracts the
fields ACTINV itself computed. All conversions performed here (weight-percent
from atom ratios, mass from density/thickness/area) are arithmetic, not
physics judgements.

Standard library only; runs under the system interpreter (no third-party
imports, no network access).
"""
import argparse
import hashlib
import json
import subprocess
import sys
import time
from decimal import Decimal, getcontext
from pathlib import Path

getcontext().prec = 60

SCHEMA = "avila.shielding/activation-result/v1"
SPECTRA_SCHEMA = "avila.shielding/layer-spectra/v1"
MATERIALS_SCHEMA = "avila.shielding/materials/v1"
SCHEDULE_SCHEMA = "avila.shielding/irradiation-schedule/v1"
ACTINV_SPEC = "actinv-spec-1"

SIGNIFICANT_DIGITS = 8
AREA_CM2 = Decimal("10000")
DOMINANT_NUCLIDE_COUNT = 5

# Standard atomic weights (IUPAC conventional values), g/mol, for the
# elements the shared shielding material table
# (examples/capabilities/shielding/materials.json) uses. Only used to convert
# an atom-fraction ("ao") composition to the weight-percent basis this script
# always hands ACTINV; a "wo" (mass-fraction) composition needs no weight
# table at all. Source: CIAAW conventional atomic weights. Keyed in natural
# title case; looked up case-insensitively, matching ACTINV's own tolerance
# for composition-key case (docs/SPEC.md).
ATOMIC_WEIGHT_G_PER_MOL = {
    "H": Decimal("1.008"),
    "B": Decimal("10.81"),
    "C": Decimal("12.011"),
    "O": Decimal("15.999"),
    "Na": Decimal("22.990"),
    "Al": Decimal("26.982"),
    "Si": Decimal("28.085"),
    "K": Decimal("39.098"),
    "Ca": Decimal("40.078"),
    "Fe": Decimal("55.845"),
    "Pb": Decimal("207.2"),
}


def sha256_of(path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def round_significant(value: Decimal, digits: int = SIGNIFICANT_DIGITS) -> Decimal:
    """Round an exact Decimal to `digits` significant figures.

    Applied to every ACTINV-computed floating value before it becomes a
    claim so that IEEE-754 tail noise never enters an output byte, mirroring
    `transport.py`'s `stable()`. ACTINV's ordinary (non-mesh) solve is
    single-threaded and already bit-reproducible for identical input bytes
    (unlike OpenMC's threaded tally reduction); this rounding is therefore a
    precision-honesty choice, verified separately by a byte-identical
    double-run rather than relied upon to mask nondeterminism.
    """
    if value == 0:
        return Decimal(0)
    quantum = Decimal(1).scaleb(value.adjusted() - digits + 1)
    return value.quantize(quantum)


def stable_from_float(value: float, digits: int = SIGNIFICANT_DIGITS) -> Decimal:
    return round_significant(Decimal(repr(value)), digits)


def canonical(value: Decimal) -> str:
    text = format(value.normalize(), "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    if text in ("", "-0"):
        text = "0"
    return text


def canonical_from_float(value: float, digits: int = SIGNIFICANT_DIGITS) -> str:
    return canonical(stable_from_float(value, digits))


def read_json(path, schema=None):
    with open(path, encoding="utf-8") as handle:
        document = json.load(handle)
    if schema is not None and document.get("schema") != schema:
        raise SystemExit(
            f"{path}: expected schema {schema!r}, got {document.get('schema')!r}"
        )
    return document


def index_path_for(library_path: str) -> str:
    """ACTINV's own adjacency rule: `<stem>.npz` -> `<stem>_index.json`,
    same directory (`actinv_data::builder::index_path`). ACTINV finds the
    index itself from the library path; this script never passes an index
    path to ACTINV. It only uses `--activation-index` to check the staged
    layout matches that rule before running (fail closed on a bad adapter
    staging) and to record the index's own hash in the report.
    """
    if not library_path.endswith(".npz"):
        raise SystemExit(f"activation library path must end in .npz: {library_path}")
    return library_path[: -len(".npz")] + "_index.json"


def wt_percent_composition(material_name: str, spec: dict) -> dict:
    """Convert a shielding-material-table composition to ACTINV's
    `wt_percent` basis (grams per 100 g), the one basis this script always
    uses so every material in the table goes through one code path.

    `"ao"` (atom fraction, arbitrary atom ratios) is converted to mass using
    the embedded standard atomic weights: mass_i = atoms_i * weight_i,
    normalized to 100. `"wo"` (already a mass fraction) only needs
    normalizing to 100; no atomic-weight lookup is needed or performed for
    it, since it is already a weight quantity.
    """
    composition = spec["composition"]
    basis = composition["basis"]
    elements = composition["elements"]
    if not elements:
        raise SystemExit(f"material {material_name!r} has an empty composition")
    if basis == "wo":
        masses = {element: Decimal(value) for element, value in elements.items()}
    elif basis == "ao":
        masses = {}
        for element, value in elements.items():
            weight = ATOMIC_WEIGHT_G_PER_MOL.get(element.title())
            if weight is None:
                raise SystemExit(
                    f"material {material_name!r}: no standard atomic weight embedded "
                    f"for element {element!r}; add it to ATOMIC_WEIGHT_G_PER_MOL"
                )
            masses[element] = Decimal(value) * weight
    else:
        raise SystemExit(
            f"material {material_name!r}: unknown composition basis {basis!r} "
            "(expected 'ao' or 'wo')"
        )
    total = sum(masses.values())
    if total <= 0:
        raise SystemExit(
            f"material {material_name!r}: composition mass totals to zero or less"
        )
    return {element: (mass / total) * Decimal(100) for element, mass in masses.items()}


def build_actinv_spec(
    layer: dict,
    material_name: str,
    material_spec: dict,
    spectra: dict,
    schedule: dict,
    library_path: str,
    library_sha256: str,
    decay_primary_path: str,
    decay_fallback_path: str,
) -> tuple:
    thickness_cm = Decimal(layer["thickness_cm"])
    if thickness_cm <= 0:
        raise SystemExit(f"layer {layer['index']}: thickness_cm must be positive")
    density = Decimal(material_spec["density_g_cm3"])
    mass_g = density * thickness_cm * AREA_CM2

    wt_percent = wt_percent_composition(material_name, material_spec)
    composition = {
        element: float(fraction) for element, fraction in sorted(wt_percent.items())
    }

    flux_per_group = layer["flux_per_group_n_cm2_s"]
    if len(flux_per_group) != 709:
        raise SystemExit(
            f"layer {layer['index']}: expected 709 group fluxes, got {len(flux_per_group)}"
        )
    boundaries_ev = spectra["boundaries_eV"]
    if len(boundaries_ev) != 710:
        raise SystemExit(
            f"spectra file: expected 710 group boundaries, got {len(boundaries_ev)}"
        )

    irradiation_text = schedule["irradiation_s"]
    cooling_text = schedule["cooling_s"]
    flux_scale_text = schedule["flux_scale"]
    irradiation_s = Decimal(irradiation_text)
    cooling_s = Decimal(cooling_text)
    flux_scale = Decimal(flux_scale_text)
    if irradiation_s <= 0:
        raise SystemExit("schedule.irradiation_s must be positive")
    if cooling_s < 0:
        raise SystemExit("schedule.cooling_s must be nonnegative")
    if flux_scale < 0:
        raise SystemExit("schedule.flux_scale must be nonnegative")

    spec = {
        "spec": ACTINV_SPEC,
        "title": (
            f"shield-coupled activation: {spectra.get('candidate_id', '')} "
            f"layer {layer['index']} ({material_name})"
        ),
        "projectile": "neutron",
        "library": {"path": library_path, "sha256": library_sha256},
        "decay": {
            "primary": decay_primary_path,
            "fallback": decay_fallback_path,
        },
        "material": {
            "mass_g": float(mass_g),
            "basis": "wt_percent",
            "composition": composition,
        },
        "spectrum": {
            "structure": "custom",
            "boundaries_eV": [float(value) for value in boundaries_ev],
            "flux_per_group": [float(value) for value in flux_per_group],
            "descending": False,
        },
        "schedule": [
            {"dt": f"{irradiation_text} s", "flux": float(flux_scale)},
            {"dt": f"{cooling_text} s", "flux": 0.0},
        ],
        "options": {
            "mode": "auto",
            "prune": "rate",
            "outputs": ["activity", "heat", "photons", "ledger", "certificate"],
        },
    }
    return spec, mass_g


def run_actinv(actinv_path: str, cache_dir: Path, *args) -> subprocess.CompletedProcess:
    """Run one ACTINV subcommand with a cleared environment: only
    ACTINV_CACHE_DIR (resolved to an absolute path, which ACTINV requires)
    is passed through. No PATH, HOME, or anything else from this script's
    own environment reaches the child process.
    """
    env = {"ACTINV_CACHE_DIR": str(cache_dir)}
    return subprocess.run(
        [actinv_path, *args],
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )


def actinv_version(actinv_path: str, cache_dir: Path) -> str:
    completed = run_actinv(actinv_path, cache_dir, "--version")
    if completed.returncode != 0:
        raise SystemExit(
            f"`actinv --version` failed (exit {completed.returncode}): {completed.stderr}"
        )
    parts = completed.stdout.strip().split()
    if len(parts) != 2:
        raise SystemExit(f"unexpected `actinv --version` output: {completed.stdout!r}")
    return parts[1]


def activate_layer(
    actinv_path: str,
    cache_dir: Path,
    work_dir: Path,
    layer: dict,
    material_name: str,
    material_spec: dict,
    spectra: dict,
    schedule: dict,
    library_path: str,
    library_sha256: str,
    decay_primary_path: str,
    decay_fallback_path: str,
) -> dict:
    index = layer["index"]
    layer_dir = work_dir / f"layer-{index}"
    layer_dir.mkdir(parents=True, exist_ok=True)
    problem_path = layer_dir / "problem.json"
    result_path = layer_dir / "result.json"

    actinv_spec, mass_g = build_actinv_spec(
        layer,
        material_name,
        material_spec,
        spectra,
        schedule,
        library_path,
        library_sha256,
        decay_primary_path,
        decay_fallback_path,
    )
    with open(problem_path, "w", encoding="utf-8") as handle:
        json.dump(actinv_spec, handle, indent=2, sort_keys=True)
        handle.write("\n")

    validated = run_actinv(actinv_path, cache_dir, "validate", str(problem_path))
    if validated.returncode != 0:
        raise SystemExit(
            f"layer {index} ({material_name}): actinv validate failed "
            f"(exit {validated.returncode}): {validated.stderr}"
        )

    started = time.monotonic()
    completed = run_actinv(actinv_path, cache_dir, "run", str(problem_path), str(result_path))
    wall_time_s = time.monotonic() - started
    if completed.returncode != 0:
        raise SystemExit(
            f"layer {index} ({material_name}): actinv run failed "
            f"(exit {completed.returncode}): {completed.stderr}"
        )

    result = read_json(result_path)
    steps = result.get("steps")
    if not steps:
        raise SystemExit(f"layer {index}: actinv result has no steps")
    if len(steps) != 2:
        raise SystemExit(
            f"layer {index}: expected 2 result steps (irradiation, cooling), got {len(steps)}"
        )
    end_of_cooling = steps[-1]

    activity_by_nuclide = end_of_cooling.get("activity_Bq_per_g", {})
    specific_activity_raw = sum(activity_by_nuclide.values())
    specific_activity = stable_from_float(specific_activity_raw)

    heat = end_of_cooling.get("heat_W_per_g", {})
    heat_total_w_per_g = heat.get("total")
    if heat_total_w_per_g is None:
        raise SystemExit(f"layer {index}: actinv result has no heat_W_per_g.total")
    # Round at ACTINV's own reported scale (W/g), then shift the decimal
    # point by an exact power of ten to W/kg; scaling an already-rounded
    # value by 1000 preserves its significant digits exactly.
    decay_heat_w_per_kg = stable_from_float(heat_total_w_per_g) * Decimal(1000)

    photon_source = end_of_cooling.get("photon_source") or {}
    contact_gy_h = photon_source.get("contact_gamma_air_dose_proxy_Gy_h")
    contact_dose_rate_usv_h = None
    if contact_gy_h is not None:
        # Gy (air-kerma proxy) -> Sv is numerically 1:1 for photons (w_R = 1),
        # so uSv/h = Gy/h * 1e6; see the document-level limitations. Round at
        # ACTINV's own reported scale, then shift exactly, as above.
        contact_dose_rate_usv_h = stable_from_float(contact_gy_h) * Decimal(1_000_000)

    dominant = sorted(
        activity_by_nuclide.items(), key=lambda kv: (-kv[1], kv[0])
    )[:DOMINANT_NUCLIDE_COUNT]
    dominant_nuclides = [
        {
            "nuclide": nuclide,
            "activity_Bq_per_g": canonical_from_float(activity),
        }
        for nuclide, activity in dominant
    ]

    omissions = list(layer_omissions(result, photon_source, contact_gy_h))
    gap_products, gap_targets = library_gap_counts(result)

    layer_document = {
        "index": index,
        "material": material_name,
        "mass_kg": canonical(mass_g / Decimal(1000)),
        "specific_activity": {"value": canonical(specific_activity), "unit": "Bq/g"},
        "decay_heat": {"value": canonical(decay_heat_w_per_kg), "unit": "W/kg"},
        "dominant_nuclides": dominant_nuclides,
        "omissions": omissions,
    }
    if contact_dose_rate_usv_h is not None:
        layer_document["contact_dose_rate"] = {
            "value": canonical(contact_dose_rate_usv_h),
            "unit": "uSv/h",
        }

    return {
        "document": layer_document,
        "mass_kg_exact": mass_g / Decimal(1000),
        "specific_activity_exact": specific_activity,
        "decay_heat_w_per_kg_exact": decay_heat_w_per_kg,
        "contact_dose_rate_usv_h_exact": contact_dose_rate_usv_h,
        "wall_time_s": wall_time_s,
        "library_gap_products": gap_products,
        "library_gap_targets": gap_targets,
    }


def layer_omissions(result: dict, photon_source: dict, contact_gy_h) -> list:
    """Genuinely layer-specific gaps only.

    ACTINV's ledger also carries `products_no_evaluated_decay_data` and
    `targets_absent_from_decay_library`: checked directly (see the fixture
    run in the accompanying README), these two lists come back byte-identical
    for polyethylene and iron, and for an unrelated FNS-Fe problem too. In
    `trace` mode (negligible burn-up, the mode both fixture layers select)
    ACTINV explores the full network the activation and decay libraries
    connect down to its numerical floor, so the set of chain gaps it reports
    is a near-fixed property of the bound library pair rather than of any one
    material. Repeating an unfiltered ~150-200-entry list, dominated by heavy
    actinide-region nuclides no light-element activation reaches in reality,
    under each layer would misstate it as specific to that layer's own
    composition. That library-wide count is reported once, at the document
    level, instead (see `limitations`).
    """
    omissions = []
    if contact_gy_h is None:
        omissions.append(
            "contact gamma dose-rate proxy unavailable: no actinv-photon-response-1 "
            "table was staged for this run, so ACTINV computed no dose response"
        )
    ledger = result.get("ledger") or {}
    unknown_elements = ledger.get("composition_elements_unknown") or []
    if unknown_elements:
        omissions.append(
            "composition element(s) unknown to the activation library "
            f"(composition_elements_unknown): {sorted(unknown_elements)}"
        )
    absent_isotopes = ledger.get("composition_isotopes_absent_from_decay_library") or []
    if absent_isotopes:
        omissions.append(
            "composition isotope(s) present in this layer's own material but "
            "absent from the decay library "
            f"(composition_isotopes_absent_from_decay_library): {sorted(absent_isotopes)}"
        )
    if ledger.get("composition_not_summing_to_100"):
        omissions.append(
            "this layer's converted wt_percent composition did not sum to 100 "
            "within ACTINV's own tolerance (composition_not_summing_to_100); "
            "treat the composition conversion for this layer as suspect"
        )
    represented_fraction = photon_source.get("represented_gamma_power_fraction")
    if represented_fraction is not None and represented_fraction < 0.999:
        omissions.append(
            "decay-photon energy closure is incomplete for this layer: only "
            f"{represented_fraction:.6f} of expected electromagnetic power is "
            "represented in the grouped photon source "
            "(photon_source.represented_gamma_power_fraction)"
        )
    return omissions


def library_gap_counts(result: dict) -> tuple:
    """The two library-wide ledger gap lists (see `layer_omissions`), as
    counts, for the document-level limitation note.
    """
    ledger = result.get("ledger") or {}
    products = ledger.get("products_no_evaluated_decay_data") or []
    targets = ledger.get("targets_absent_from_decay_library") or []
    return len(products), len(targets)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--spectra", required=True, help="I3 layer-spectra document")
    parser.add_argument("--materials", required=True, help="shielding material table")
    parser.add_argument("--schedule", required=True, help="irradiation/cooling schedule")
    parser.add_argument("--actinv", required=True, help="staged ACTINV executable")
    parser.add_argument("--activation-library", required=True, dest="activation_library")
    parser.add_argument("--activation-index", required=True, dest="activation_index")
    parser.add_argument("--decay-primary", required=True, dest="decay_primary")
    parser.add_argument("--decay-fallback", required=True, dest="decay_fallback")
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    spectra = read_json(args.spectra, SPECTRA_SCHEMA)
    materials = read_json(args.materials, MATERIALS_SCHEMA)
    schedule = read_json(args.schedule, SCHEDULE_SCHEMA)

    if spectra.get("group_structure") != "fispact-709":
        raise SystemExit(
            f"spectra file: expected group_structure 'fispact-709', got "
            f"{spectra.get('group_structure')!r}"
        )
    if spectra.get("order") != "ascending":
        raise SystemExit(
            f"spectra file: expected order 'ascending', got {spectra.get('order')!r}"
        )
    layers = spectra.get("layers") or []
    if not layers:
        raise SystemExit("spectra file: no layers")

    expected_index = index_path_for(args.activation_library)
    if args.activation_index != expected_index:
        raise SystemExit(
            "activation-index is not staged adjacent to activation-library: "
            f"expected {expected_index!r} (derived from the library's own stem, "
            f"ACTINV's own <stem>_index.json rule), got {args.activation_index!r}"
        )
    if not Path(args.activation_index).is_file():
        raise SystemExit(f"activation-index not found at {args.activation_index}")

    library_sha256 = sha256_of(args.activation_library)
    library_index_sha256 = sha256_of(args.activation_index)
    decay_primary_sha256 = sha256_of(args.decay_primary)
    decay_fallback_sha256 = sha256_of(args.decay_fallback)
    executable_sha256 = sha256_of(args.actinv)

    output_path = Path(args.output)
    work_dir = output_path.parent / "actinv-work"
    work_dir.mkdir(parents=True, exist_ok=True)
    cache_dir = (work_dir / ".cache" / "actinv").resolve()
    cache_dir.mkdir(parents=True, exist_ok=True)

    actinv_path = str(Path(args.actinv))
    version = actinv_version(actinv_path, cache_dir)

    materials_table = materials.get("materials", {})

    layer_results = []
    wall_times = []
    for layer in layers:
        material_name = layer.get("material")
        material_spec = materials_table.get(material_name)
        if material_spec is None:
            raise SystemExit(
                f"layer {layer.get('index')}: material {material_name!r} is not in "
                f"{args.materials}"
            )
        result = activate_layer(
            actinv_path,
            cache_dir,
            work_dir,
            layer,
            material_name,
            material_spec,
            spectra,
            schedule,
            args.activation_library,
            library_sha256,
            args.decay_primary,
            args.decay_fallback,
        )
        layer_results.append(result)
        wall_times.append((layer.get("index"), material_name, result["wall_time_s"]))

    max_specific_activity = max(r["specific_activity_exact"] for r in layer_results)
    total_decay_heat = sum(
        (r["decay_heat_w_per_kg_exact"] * r["mass_kg_exact"] for r in layer_results),
        start=Decimal(0),
    )
    total_decay_heat = round_significant(total_decay_heat)
    contact_candidates = [
        r["contact_dose_rate_usv_h_exact"]
        for r in layer_results
        if r["contact_dose_rate_usv_h_exact"] is not None
    ]
    max_gap_products = max(r["library_gap_products"] for r in layer_results)
    max_gap_targets = max(r["library_gap_targets"] for r in layer_results)

    totals = {
        "max_specific_activity": {
            "value": canonical(max_specific_activity),
            "unit": "Bq/g",
        },
        "total_decay_heat": {"value": canonical(total_decay_heat), "unit": "W"},
    }
    if contact_candidates:
        totals["max_contact_dose_rate"] = {
            "value": canonical(max(contact_candidates)),
            "unit": "uSv/h",
        }

    document = {
        "schema": SCHEMA,
        "candidate_id": spectra.get("candidate_id", ""),
        "schedule": {
            "irradiation_s": schedule["irradiation_s"],
            "cooling_s": schedule["cooling_s"],
            "flux_scale": schedule["flux_scale"],
        },
        "actinv": {
            "executable_sha256": f"sha256:{executable_sha256}",
            "version": version,
            "library_sha256": f"sha256:{library_sha256}",
            "library_index_sha256": f"sha256:{library_index_sha256}",
            "decay": {
                "primary_sha256": f"sha256:{decay_primary_sha256}",
                "fallback_sha256": f"sha256:{decay_fallback_sha256}",
            },
        },
        "layers": [r["document"] for r in layer_results],
        "totals": totals,
        "limitations": [
            "Nominal only: ACTINV reports no per-value uncertainty bound for "
            "specific activity, decay heat, or the contact-dose proxy; these "
            "are unquantified claims under a nominal-basis requirement.",
            "No transported shutdown dose: this is a 0-D activation calculation "
            "over the transport step's cell-averaged flux spectrum for each "
            "layer; it does not transport decay photons through the geometry.",
            "The contact-dose figure, when present, is ACTINV's semi-infinite "
            "homogeneous-slab air-kerma screening proxy at the reported "
            "material's own surface, not a transported or finite-geometry "
            "dose rate; it is reported here in uSv/h by treating photon "
            "air kerma (Gy) as numerically equal to equivalent dose (Sv), "
            "the standard w_R = 1 photon approximation.",
            "The activation data library and decay sublibraries used here "
            "have their own scope, energy range, and validation limits; see "
            "the ACTINV qualification boundary (docs/QUALIFICATION.md) and "
            "this run's `actinv` block for exact identities.",
            "Each layer is activated independently over its own cell-averaged "
            "spectrum; no inter-layer photon or neutron coupling from "
            "activation products is modeled.",
            "Composition is converted to ACTINV's wt_percent basis by this "
            "script using embedded standard atomic weights for atom-fraction "
            "table entries; this is an arithmetic conversion, not an "
            "independent composition measurement.",
            "The bound activation library and decay data (see the `actinv` "
            f"block) record up to {max_gap_products} activation product "
            f"nuclide(s) with no evaluated decay data and up to "
            f"{max_gap_targets} target nuclide(s) absent from the decay "
            "library entirely, per ACTINV's own ledger. These counts are a "
            "property of the bound data release, not of any one layer's "
            "composition: they were observed identical across every layer "
            "of this run and across an unrelated iron reference problem, "
            "because ACTINV's trace-mode solve explores the same broad "
            "chain network regardless of material; they are not filtered "
            "to nuclides actually reachable from a given layer's own "
            "composition, and are dominated by heavy nuclides no light- or "
            "structural-metal activation reaches in practice.",
        ],
    }

    with open(output_path, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2, sort_keys=True)
        handle.write("\n")

    for index, material_name, wall_time_s in wall_times:
        print(
            f"layer {index} ({material_name}): actinv wall time {wall_time_s:.3f} s",
            file=sys.stderr,
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
