#!/usr/bin/env python3
"""Monte Carlo transport of a layered slab with OpenMC: coupled neutron and
secondary-photon dose, per-layer neutron flux spectra, and variance
reduction.

Geometry: layers along x starting at x = 0, a vacuum gap in front, reflective
lateral boundaries so the plane source is effectively infinite, and a thin
detector cell of air behind the last layer. The plane source is isotropic and
uniform over the slab face. Photon transport is enabled, so secondary photons
produced in the shield (capture, inelastic scattering, and other reactions)
are tracked to the detector alongside the source neutrons. Two dose tallies
sit in the detector cell, one per particle type, each folded with the
ICRP-116 anterior-posterior dose coefficients shipped with OpenMC and scaled
to the declared source strength over the declared area. A third pair of
tallies, one per material layer, records the neutron flux spectrum in the
FISPACT-709 group structure for a downstream activation step. All reported
intervals are statistical estimates at the declared coverage; none of them
contains nuclear-data, geometry, or model uncertainty.

Variance reduction: survival biasing (implicit capture) and a deterministic,
analytic neutron weight-window mesh along x are applied by default so the
same seed always reproduces the same bytes, with no generator iteration
involved. `--no-survival-biasing` and `--no-weight-windows` exist only to
reproduce an analog or partially biased run for a variance-reduction
comparison; the production adapter never passes them.
"""

import argparse
import hashlib
import json
import math
import os
import sys
from decimal import Decimal, getcontext

getcontext().prec = 40
SIGNIFICANT_DIGITS = 8


def stable(value: float) -> Decimal:
    """Round a threaded tally statistic to SIGNIFICANT_DIGITS significant digits.

    OpenMC reduces per-thread tallies in an order that varies from run to run,
    which perturbs the last binary digits of the mean and standard deviation.
    Those digits carry no information (the statistical uncertainty is orders
    of magnitude larger) but they would change the output bytes, and Avila
    Core binds outputs by their bytes. Rounding removes them so the same seed
    reproduces the same document.
    """
    decimal = Decimal(repr(value))
    if decimal == 0:
        return Decimal(0)
    quantum = Decimal(1).scaleb(decimal.adjusted() - SIGNIFICANT_DIGITS + 1)
    return decimal.quantize(quantum)


def round_decimal(value: Decimal, digits: int = SIGNIFICANT_DIGITS) -> Decimal:
    """Round an already-exact Decimal (not a raw tally float) to `digits`
    significant digits, with the same quantum logic as `stable`. Used for
    quantities derived from a rounded tally statistic by exact Decimal
    arithmetic (scaling, ratios), which need no thread-noise removal but
    should still be reported at a bounded, stable number of digits.
    """
    if value == 0:
        return Decimal(0)
    quantum = Decimal(1).scaleb(value.adjusted() - digits + 1)
    return value.quantize(quantum)


SCHEMA = "avila.shielding/transport-result/v2"
LAYER_SPECTRA_SCHEMA = "avila.shielding/layer-spectra/v1"
GROUP_SCHEMA = "avila.shielding/group-structure/v1"
COVERAGE = "0.95"
K_95 = 1.959964

# Deterministic analytic weight-window construction (see build_weight_windows
# below). Fixed constants, not fitted or searched: the same candidate and
# material table always produce the same mesh and bounds.
BIN_WIDTH_CM = 2.0
MIN_MESH_BINS = 10
MAX_MESH_BINS = 200
UPPER_BOUND_RATIO = 5.0
SURVIVAL_RATIO = 5.0
MAX_SPLIT = 10
MIN_LOWER_BOUND = 1e-30


def sha256_of(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical(value: Decimal) -> str:
    text = format(value.normalize(), "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    if text in ("", "-0"):
        text = "0"
    return text


def build_weight_windows(openmc, layer_records, total_thickness, half):
    """A deterministic, analytic neutron weight-window mesh along x.

    The lower bound at a mesh cell's x midpoint is exp(-optical_depth(x_mid)),
    where optical_depth(x) sums, over every layer between the slab front
    (x = 0) and x, that layer's removal_cross_section_cm_inv (materials.json)
    times the length of overlap with [0, x]. For a single-material slab this
    is exactly exp(-Sigma_r * x); for a layered slab it is the natural
    piecewise extension. The upper bound is the lower bound times a fixed
    ratio. Nothing here is fitted, searched, or iterated: the mesh and every
    bound are a pure function of the candidate and material table, so the
    same inputs always produce the same windows.

    Returns (WeightWindows, record) where record is the JSON-safe description
    written into the output document's run.variance_reduction.weight_windows,
    or (None, record) if there is no material layer to span.
    """
    if not layer_records or total_thickness <= 0:
        return None, {
            "applied": False,
            "reason": "no material layer to span with a mesh",
        }

    n_bins = min(
        MAX_MESH_BINS,
        max(MIN_MESH_BINS, math.ceil(total_thickness / BIN_WIDTH_CM)),
    )
    bin_width = total_thickness / n_bins

    def optical_depth(x: float) -> float:
        total = 0.0
        for record in layer_records:
            overlap = max(0.0, min(x, record["end"]) - record["start"])
            if overlap > 0:
                total += record["removal_xs"] * overlap
        return total

    lower_bounds = []
    for index in range(n_bins):
        x_mid = (index + 0.5) * bin_width
        lower_bounds.append(max(MIN_LOWER_BOUND, math.exp(-optical_depth(x_mid))))

    mesh = openmc.RegularMesh()
    mesh.dimension = (n_bins, 1, 1)
    mesh.lower_left = (0.0, -half, -half)
    mesh.upper_right = (total_thickness, half, half)

    weight_windows = openmc.WeightWindows(
        mesh=mesh,
        lower_ww_bounds=lower_bounds,
        upper_bound_ratio=UPPER_BOUND_RATIO,
        particle_type="neutron",
        survival_ratio=SURVIVAL_RATIO,
        max_split=MAX_SPLIT,
    )

    record = {
        "applied": True,
        "particle_type": "neutron",
        "method": "deterministic-analytic-exponential",
        "construction": (
            "lower_bound(x_mid) = exp(-optical_depth(x_mid)) at each mesh cell's "
            "x midpoint; optical_depth(x) sums removal_cross_section_cm_inv "
            "(materials.json) times overlap length in cm over every layer between "
            "the slab front (x=0) and x; upper_bound = lower_bound * "
            "upper_bound_ratio. No WeightWindowGenerator, no iteration: the mesh "
            "and every bound are computed once from the candidate and material "
            "table, so the same inputs always give the same windows."
        ),
        "mesh": {
            "dimension": [n_bins, 1, 1],
            "lower_left_cm": ["0", canonical(Decimal(repr(-half))), canonical(Decimal(repr(-half)))],
            "upper_right_cm": [
                canonical(Decimal(repr(total_thickness))),
                canonical(Decimal(repr(half))),
                canonical(Decimal(repr(half))),
            ],
            "bin_width_cm": canonical(round_decimal(Decimal(repr(bin_width)))),
        },
        "upper_bound_ratio": canonical(Decimal(repr(UPPER_BOUND_RATIO))),
        "survival_ratio": canonical(Decimal(repr(SURVIVAL_RATIO))),
        "max_split": MAX_SPLIT,
        "lower_bounds": [canonical(round_decimal(Decimal(repr(bound)))) for bound in lower_bounds],
    }
    return weight_windows, record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--materials", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--cross-sections-index", required=True)
    parser.add_argument("--groups", required=True)
    parser.add_argument("--particles", type=int, required=True)
    parser.add_argument("--batches", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--layer-spectra-output", required=True)
    parser.add_argument(
        "--no-survival-biasing",
        action="store_true",
        help="Disable survival biasing. Figure-of-merit comparison only; the production adapter never passes this.",
    )
    parser.add_argument(
        "--no-weight-windows",
        action="store_true",
        help="Disable the deterministic weight windows. Figure-of-merit comparison only; the production adapter never passes this.",
    )
    args = parser.parse_args()

    library = os.environ.get("OPENMC_CROSS_SECTIONS")
    if not library:
        raise SystemExit("OPENMC_CROSS_SECTIONS is not set")
    staged_digest = sha256_of(args.cross_sections_index)
    library_digest = sha256_of(library)
    if staged_digest != library_digest:
        raise SystemExit(
            f"the data library in use ({library_digest}) is not the attested index ({staged_digest})"
        )

    import openmc  # noqa: E402  (imported after the environment checks)
    import openmc.data  # noqa: E402

    with open(args.candidate, encoding="utf-8") as handle:
        candidate = json.load(handle)
    with open(args.materials, encoding="utf-8") as handle:
        table = json.load(handle)["materials"]
    with open(args.source, encoding="utf-8") as handle:
        source = json.load(handle)
    with open(args.groups, encoding="utf-8") as handle:
        groups_doc = json.load(handle)
    if candidate.get("schema") != "avila.shielding/candidate/v1":
        raise SystemExit("candidate schema is not avila.shielding/candidate/v1")
    if groups_doc.get("schema") != GROUP_SCHEMA:
        raise SystemExit("group structure schema is not avila.shielding/group-structure/v1")
    if groups_doc.get("order") != "ascending":
        raise SystemExit("group structure must be ascending order for OpenMC's EnergyFilter")
    boundaries_eV = groups_doc.get("boundaries_eV")
    n_boundaries = groups_doc.get("n_boundaries")
    if not isinstance(boundaries_eV, list) or len(boundaries_eV) != n_boundaries:
        raise SystemExit("group structure boundaries_eV length does not match n_boundaries")
    if any(boundaries_eV[i] >= boundaries_eV[i + 1] for i in range(len(boundaries_eV) - 1)):
        raise SystemExit("group structure boundaries_eV must be strictly ascending")

    lateral = float(source["lateral_cm"])
    half = lateral / 2.0
    detector_thickness = float(source["detector_thickness_cm"])
    energy_eV = float(source["energy_MeV"]) * 1.0e6
    strength = Decimal(source["strength_n_per_s"])
    area = Decimal(source["area_cm2"])
    if Decimal(repr(lateral * lateral)) != area:
        raise SystemExit("source area must equal the square of the lateral extent for this slab model")

    materials = []
    cells = []
    layer_records = []
    x_gap = openmc.XPlane(-1.0, boundary_type="vacuum")
    ymin = openmc.YPlane(-half, boundary_type="reflective")
    ymax = openmc.YPlane(half, boundary_type="reflective")
    zmin = openmc.ZPlane(-half, boundary_type="reflective")
    zmax = openmc.ZPlane(half, boundary_type="reflective")
    lateral_region = +ymin & -ymax & +zmin & -zmax
    previous = openmc.XPlane(0.0)
    cells.append(openmc.Cell(name="gap", region=+x_gap & -previous & lateral_region))
    position = 0.0
    for index, layer in enumerate(candidate["layers"]):
        spec = table[layer["material"]]
        thickness = float(layer["thickness_cm"])
        if thickness <= 0:
            continue
        material = openmc.Material(name=f"layer-{index}-{layer['material']}")
        material.set_density("g/cm3", float(spec["density_g_cm3"]))
        basis = spec["composition"]["basis"]
        for element, fraction in spec["composition"]["elements"].items():
            material.add_element(element, float(fraction), basis)
        materials.append(material)
        start = position
        position += thickness
        boundary = openmc.XPlane(position)
        cell = openmc.Cell(name=material.name, fill=material, region=+previous & -boundary & lateral_region)
        cells.append(cell)
        layer_records.append(
            {
                "index": index,
                "material": layer["material"],
                "thickness_cm": Decimal(layer["thickness_cm"]),
                "volume_cm3": area * Decimal(layer["thickness_cm"]),
                "removal_xs": float(spec["removal_cross_section_cm_inv"]),
                "start": start,
                "end": position,
                "cell": cell,
            }
        )
        previous = boundary
    total_thickness = position
    air = openmc.Material(name="detector-air")
    air.set_density("g/cm3", 0.0012)
    air.add_element("N", 0.79)
    air.add_element("O", 0.21)
    materials.append(air)
    detector_far = openmc.XPlane(position + detector_thickness)
    detector = openmc.Cell(name="detector", fill=air, region=+previous & -detector_far & lateral_region)
    cells.append(detector)
    tail_far = openmc.XPlane(position + detector_thickness + 1.0, boundary_type="vacuum")
    cells.append(openmc.Cell(name="tail", region=+detector_far & -tail_far & lateral_region))

    openmc.Materials(materials).export_to_xml()
    openmc.Geometry(cells).export_to_xml()

    plane = openmc.IndependentSource()
    plane.space = openmc.stats.Box((-0.5, -half, -half), (-0.5, half, half))
    plane.angle = openmc.stats.Isotropic()
    plane.energy = openmc.stats.Discrete([energy_eV], [1.0])
    settings = openmc.Settings()
    settings.run_mode = "fixed source"
    settings.source = plane
    settings.particles = args.particles
    settings.batches = args.batches
    settings.seed = args.seed
    settings.output = {"summary": False, "tallies": False}
    settings.photon_transport = True

    variance_reduction = {"seed": args.seed, "survival_biasing": False}
    if not args.no_survival_biasing:
        settings.survival_biasing = True
        variance_reduction["survival_biasing"] = True

    if args.no_weight_windows:
        variance_reduction["weight_windows"] = {
            "applied": False,
            "reason": "disabled by --no-weight-windows for a variance-reduction comparison run; the production adapter never passes this flag",
        }
    else:
        weight_windows, ww_record = build_weight_windows(openmc, layer_records, total_thickness, half)
        variance_reduction["weight_windows"] = ww_record
        if weight_windows is not None:
            settings.weight_windows = [weight_windows]
            settings.weight_windows_on = True

    settings.export_to_xml()

    n_energies, n_coefficients = openmc.data.dose_coefficients("neutron", geometry="AP")
    p_energies, p_coefficients = openmc.data.dose_coefficients("photon", geometry="AP")

    # Filter objects are constructed once and reused across tallies (rather
    # than re-instantiated per tally) so identical filters share one exported
    # XML definition instead of OpenMC deduplicating equal-content duplicates
    # at export time.
    detector_cell_filter = openmc.CellFilter(detector)
    neutron_particle_filter = openmc.ParticleFilter(["neutron"])

    neutron_dose_tally = openmc.Tally(name="neutron-dose")
    neutron_dose_tally.filters = [
        detector_cell_filter,
        neutron_particle_filter,
        openmc.EnergyFunctionFilter(n_energies, n_coefficients),
    ]
    neutron_dose_tally.scores = ["flux"]

    photon_dose_tally = openmc.Tally(name="photon-dose")
    photon_dose_tally.filters = [
        detector_cell_filter,
        openmc.ParticleFilter(["photon"]),
        openmc.EnergyFunctionFilter(p_energies, p_coefficients),
    ]
    photon_dose_tally.scores = ["flux"]

    tallies = [neutron_dose_tally, photon_dose_tally]
    layer_cells = [record["cell"] for record in layer_records]
    if layer_cells:
        layer_cell_filter = openmc.CellFilter(layer_cells)
        layer_spectrum_tally = openmc.Tally(name="layer-spectrum")
        layer_spectrum_tally.filters = [
            layer_cell_filter,
            neutron_particle_filter,
            openmc.EnergyFilter(boundaries_eV),
        ]
        layer_spectrum_tally.scores = ["flux"]
        layer_total_tally = openmc.Tally(name="layer-total")
        layer_total_tally.filters = [layer_cell_filter, neutron_particle_filter]
        layer_total_tally.scores = ["flux"]
        tallies += [layer_spectrum_tally, layer_total_tally]
    openmc.Tallies(tallies).export_to_xml()

    executable = os.path.join(os.path.dirname(sys.executable), "openmc")
    threads = int(os.environ.get("OMP_NUM_THREADS", "0")) or None
    openmc.run(openmc_exec=executable, output=False, threads=threads)

    statepoint = openmc.StatePoint(f"statepoint.{args.batches}.h5")

    neutron_result = statepoint.get_tally(name="neutron-dose")
    mean_n = stable(float(neutron_result.mean.ravel()[0]))
    std_n = stable(float(neutron_result.std_dev.ravel()[0]))
    photon_result = statepoint.get_tally(name="photon-dose")
    mean_p = stable(float(photon_result.mean.ravel()[0]))
    std_p = stable(float(photon_result.std_dev.ravel()[0]))

    volume = lateral * lateral * detector_thickness
    # flux score is track length per source particle [particle-cm]; divide by
    # the detector volume for fluence per particle [1/cm2], times the dose
    # coefficient [pSv cm2] already folded in, times source strength [1/s],
    # times 3600 s/h, over 1e6 pSv/uSv. Both particle types share this scale:
    # every OpenMC tally is normalized per source (neutron) particle, so a
    # secondary-photon tally scales by the same neutron source rate as the
    # neutron tally it was produced alongside.
    scale = Decimal(3600) * strength / Decimal(repr(volume)) / Decimal(1_000_000)

    def dose_block(mean: Decimal, std: Decimal) -> dict:
        nominal = mean * scale
        sigma = std * scale
        halfwidth = sigma * Decimal(repr(K_95))
        lower = max(Decimal(0), nominal - halfwidth)
        upper = nominal + halfwidth
        return {
            "nominal": {"value": canonical(nominal), "unit": "uSv/h"},
            "lower": {"value": canonical(lower), "unit": "uSv/h"},
            "upper": {"value": canonical(upper), "unit": "uSv/h"},
            "coverage": COVERAGE,
            "interpretation": "normal approximation of the tally statistics at the stated coverage; statistical error only",
        }

    neutron_dose_rate = dose_block(mean_n, std_n)
    photon_dose_rate = dose_block(mean_p, std_p)

    layers_out = []
    if layer_records:
        spectrum_result = statepoint.get_tally(name="layer-spectrum")
        total_result = statepoint.get_tally(name="layer-total")
        for record in layer_records:
            cell_id = record["cell"].id
            volume_dec = record["volume_cm3"]

            energy_slice = spectrum_result.get_slice(filters=[openmc.CellFilter], filter_bins=[(cell_id,)])
            flux_per_group = []
            for raw_mean in energy_slice.mean.ravel():
                rounded = stable(float(raw_mean))
                scaled = round_decimal(rounded / volume_dec * strength)
                flux_per_group.append(float(scaled))

            total_slice = total_result.get_slice(filters=[openmc.CellFilter], filter_bins=[(cell_id,)])
            mean_total = stable(float(total_slice.mean.ravel()[0]))
            std_total = stable(float(total_slice.std_dev.ravel()[0]))
            flux_total = round_decimal(mean_total / volume_dec * strength)
            rel_std_total = round_decimal(std_total / mean_total) if mean_total != 0 else Decimal(0)

            layers_out.append(
                {
                    "index": record["index"],
                    "material": record["material"],
                    "thickness_cm": canonical(record["thickness_cm"]),
                    "volume_cm3": canonical(volume_dec),
                    "flux_per_group_n_cm2_s": flux_per_group,
                    "flux_total_n_cm2_s": float(flux_total),
                    "rel_std_dev_total": float(rel_std_total),
                }
            )

    layer_spectra_document = {
        "schema": LAYER_SPECTRA_SCHEMA,
        "candidate_id": candidate.get("candidate_id"),
        "source_strength_n_per_s": canonical(strength),
        "group_structure": "fispact-709",
        "order": "ascending",
        "boundaries_eV": boundaries_eV,
        "layers": layers_out,
    }

    document = {
        "schema": SCHEMA,
        "candidate_id": candidate.get("candidate_id"),
        "engine": {
            "name": "openmc",
            "version": openmc.__version__,
            "executable_sha256": sha256_of(executable),
            "python": sys.version.split()[0],
        },
        "data_library": {
            "index_sha256": library_digest,
            "note": "identity of the index only; the nuclide files it names are not re-hashed",
        },
        "run": {
            "particles": args.particles,
            "batches": args.batches,
            "seed": args.seed,
            "threads": threads,
            "photon_transport": True,
            "variance_reduction": variance_reduction,
        },
        "tally": {
            "neutron": {"mean_pSv_cm_per_particle": canonical(mean_n), "std_dev": canonical(std_n)},
            "photon": {"mean_pSv_cm_per_particle": canonical(mean_p), "std_dev": canonical(std_p)},
            "significant_digits": SIGNIFICANT_DIGITS,
            "detector_volume_cm3": repr(volume),
        },
        "neutron_dose_rate": neutron_dose_rate,
        "photon_dose_rate": photon_dose_rate,
        "limitations": [
            "Both dose-rate intervals bound Monte Carlo statistical error only; nuclear-data, geometry, and model uncertainty are not evaluated.",
            "The slab is laterally infinite by reflection; a finite shield would leak around its edges.",
            "The photon dose rate counts only photons produced inside the shield by the transported neutrons (capture, inelastic scattering, and other secondary-photon-producing reactions); no separate photon source is modeled and no bremsstrahlung or annihilation transport beyond OpenMC's local photon physics is added.",
            "Tally statistics are rounded to 8 significant digits so that the thread-dependent reduction order of the Monte Carlo sums does not change the output bytes; the discarded digits are far below the statistical uncertainty.",
            "Variance reduction (survival biasing and/or the neutron weight windows) changes which histories are tracked and with what weight; it does not change the expectation being estimated, but the reported statistical uncertainty is a property of the biased estimator, not of an analog calculation.",
            "The deterministic weight-window lower bounds are an approximate analytic importance function (exponential attenuation from the screening removal cross sections); they were not iteratively refined against this model's actual adjoint flux, so they are a heuristic for efficiency, not a certified variance-reduction scheme.",
            "The per-layer flux spectra are track-length estimators in the FISPACT-709 group structure for a downstream activation step; they carry no claim about activation, decay heat, or dose from the activated material itself.",
            "Not qualified for any regulatory, safety, or operational decision.",
        ],
    }
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)
        handle.write("\n")
    with open(args.layer_spectra_output, "w", encoding="utf-8") as handle:
        json.dump(layer_spectra_document, handle, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
