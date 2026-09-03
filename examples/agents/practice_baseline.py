#!/usr/bin/env python3
"""Conventional-practice baseline shields for the shielding configuration search.

These are the comparison arms for judging a learned designer
(`shield_search2.py`, or the random one in `shield_search.py`): three
textbook designs an experienced shielding engineer would sketch by hand in
a few minutes, using nothing but the removal-cross-section method already
implemented in `examples/capabilities/shielding/screen.py`. If a learned
designer cannot beat these, learning bought nothing.

Each candidate is sized by solving the *same* removal-cross-section
attenuation model `screen.py` uses -- dose behind the slab is the
unshielded dose times exp(-sum(removal_cross_section_i * thickness_i)) --
for the thickness that reaches a target dose, and every design here targets
half the stated dose limit rather than the limit itself. A **design factor
of 2** (target = limit / 2) is the conventional first-pass safety margin a
removal-cross-section calculation is given precisely because it has no
buildup factor and no spectrum softening in it: real attenuation falls off
slower than a bare exponential, so a calculation that ignores that is
handed a margin instead. This is exactly the gap CASE-001's own reference
run demonstrates (the screen is optimistic by about a factor of three
against transport), which is the reason a factor of 2 is conventional and
not sufficient by itself -- these baselines are honestly conventional
practice, not a claim that they pass.

The three designs, each documented by the textbook rule it follows:

  (i)   **removal-poly**   Polyethylene alone, thickness solved by the
        removal-cross-section method at a design factor of 2. The plainest
        possible fast-neutron shield: a hydrogenous moderator sized to a
        conventional safety margin, nothing else considered.

  (ii)  **removal-poly-pb** The identical polyethylene thickness as (i),
        with a fixed 5 cm lead layer added after it (on the exit side).
        This is the usual **capture-gamma rule**: a hydrogenous moderator
        that absorbs fast neutrons also produces neutron-capture gammas
        (hydrogen's capture gamma is 2.2 MeV) that a fast-neutron-only
        removal calculation says nothing about, so a fixed thickness of a
        dense photon absorber is placed after the moderator to catch them.
        The lead thickness itself is conventional practice, not solved for
        a photon dose target here, because this baseline has no photon
        transport of its own to size against -- the same limitation the
        unqualified screen has, and the reason the coupled case adds a
        qualified photon-dose requirement at all.

  (iii) **fe-poly-pb**      A classic three-material sandwich: 10 cm of
        iron first, polyethylene next sized by the same removal method and
        design factor for whatever attenuation the iron and lead do not
        already supply, and the same fixed 5 cm lead layer last. Iron
        first is conventional practice because inelastic scattering off
        iron removes high-energy neutrons cheaply per unit cost relative
        to polyethylene, and because a structural first layer is often
        wanted anyway; polyethylene still does the bulk of the moderation;
        lead still catches capture gammas last, for the same reason as (ii).

Every candidate is written in the frozen `avila.shielding/candidate/v1`
schema, byte-for-byte the same shape `shield_search.py` writes. The sizing
arithmetic below only ever proposes a thickness; with `--run`, Core is the
only thing that ever says PASS, FAIL, INCONCLUSIVE, or NOT_EVALUATED about
it. The dose, mass, and thickness limits used to size and check these
candidates are read from a live Core report by unit, not assumed by
requirement id, unless given explicitly with `--dose-limit-uSv-h` etc., so
this script runs unchanged against the coupled case's limits later.
"""

import argparse
import json
import math
from decimal import ROUND_CEILING, Decimal
from pathlib import Path

import shield_common as core

DESIGN_FACTOR_DEFAULT = 2.0

BASELINES_NOTICE = (
    "These are conventional-practice comparison arms, sized by the same "
    "unqualified removal-cross-section method the screen uses, not a "
    "learned or optimized design. --run reports only Core's own verdicts; "
    "nothing here constructs one."
)


def unshielded_dose_rate_uSv_h(source):
    strength = Decimal(source["strength_n_per_s"])
    area = Decimal(source["area_cm2"])
    coefficient = Decimal(source["dose_coefficient_uSv_cm2"])
    return strength / area * coefficient * Decimal(3600)


def round_up_to_grid(thickness, grid_cm):
    grid = Decimal(str(grid_cm))
    if thickness <= 0:
        return Decimal(0)
    steps = (thickness / grid).to_integral_value(rounding=ROUND_CEILING)
    return steps * grid


def size_variable_layer(materials_table, unshielded_uSv_h, target_uSv_h, fixed_layers,
                         variable_material, grid_cm):
    """Solve the removal-cross-section model for the variable layer's
    thickness so the whole stack (fixed layers at their given thickness,
    plus the variable layer) attenuates `unshielded_uSv_h` down to
    `target_uSv_h`, then round up to the grid so the built design is at
    least as thick as the calculation calls for.
    """
    fixed_exponent = sum(
        Decimal(materials_table[material]["removal_cross_section_cm_inv"]) * Decimal(str(thickness))
        for material, thickness in fixed_layers
    )
    if unshielded_uSv_h <= target_uSv_h:
        total_exponent_needed = Decimal(0)
    else:
        total_exponent_needed = Decimal(repr(math.log(float(unshielded_uSv_h) / float(target_uSv_h))))
    remaining_exponent = total_exponent_needed - fixed_exponent
    mu = Decimal(materials_table[variable_material]["removal_cross_section_cm_inv"])
    thickness = (remaining_exponent / mu) if remaining_exponent > 0 else Decimal(0)
    return round_up_to_grid(thickness, grid_cm)


def build_baselines(materials_table, source, dose_limit_uSv_h, design_factor, grid_cm,
                     iron_thickness_cm, lead_thickness_cm):
    unshielded = unshielded_dose_rate_uSv_h(source)
    target = Decimal(str(dose_limit_uSv_h)) / Decimal(str(design_factor))

    poly_alone_cm = size_variable_layer(materials_table, unshielded, target, [], "polyethylene", grid_cm)

    sandwich_fixed = [("iron", iron_thickness_cm), ("lead", lead_thickness_cm)]
    poly_in_sandwich_cm = size_variable_layer(materials_table, unshielded, target, sandwich_fixed, "polyethylene", grid_cm)

    baselines = [
        {
            "candidate_id": "practice-removal-poly",
            "rule": "removal-cross-section method, design factor "
                    f"{design_factor:g}: polyethylene alone sized to attenuate the "
                    f"unshielded dose ({core.canonical_decimal(unshielded)} uSv/h) to "
                    f"limit/{design_factor:g} ({core.canonical_decimal(target)} uSv/h).",
            "layers": [("polyethylene", core.canonical_decimal(poly_alone_cm))],
        },
        {
            "candidate_id": "practice-removal-poly-pb",
            "rule": "removal-cross-section method, design factor "
                    f"{design_factor:g}, identical polyethylene thickness as "
                    "practice-removal-poly, plus the usual capture-gamma rule: a "
                    f"fixed {lead_thickness_cm:g} cm lead layer after the moderator "
                    "to absorb neutron-capture gammas the removal calculation does "
                    "not model.",
            "layers": [
                ("polyethylene", core.canonical_decimal(poly_alone_cm)),
                ("lead", core.canonical_decimal(Decimal(str(lead_thickness_cm)))),
            ],
        },
        {
            "candidate_id": "practice-fe-poly-pb",
            "rule": "classic iron/polyethylene/lead sandwich: a fixed "
                    f"{iron_thickness_cm:g} cm iron first layer (cheap inelastic "
                    "removal of the highest-energy neutrons), polyethylene sized by "
                    "the removal-cross-section method at the same design factor "
                    f"{design_factor:g} for the remaining attenuation the iron and "
                    f"lead do not supply, and the same fixed {lead_thickness_cm:g} cm "
                    "lead capture-gamma layer last.",
            "layers": [
                ("iron", core.canonical_decimal(Decimal(str(iron_thickness_cm)))),
                ("polyethylene", core.canonical_decimal(poly_in_sandwich_cm)),
                ("lead", core.canonical_decimal(Decimal(str(lead_thickness_cm)))),
            ],
        },
    ]
    return baselines, unshielded, target


def discover_dose_limit(core_path, case, source_roots, python3, out, log):
    """Bootstrap the dose limit from a live Core report by unit, exactly as
    shield_search2.py discovers mass/thickness bounds; used only when
    --dose-limit-uSv-h is not given explicitly."""
    materials_table = core.load_materials(Path(source_roots["shielding"]) / "materials.json")
    material_name = sorted(materials_table)[0]
    candidate = core.write_candidate(
        Path(out) / "candidates" / "bootstrap.json", "bootstrap", [(material_name, "5")]
    )
    report = core.run_core(
        core_path, case, Path(out) / "candidates" / "bootstrap.json",
        source_roots=source_roots, capabilities={"python3": python3}, log=log, extra_args=(['--expect-manifest', args.expect_manifest] if getattr(args, 'expect_manifest', None) else None),
    )
    limits = core.discover_limits(report)
    if limits["dose_rate"] is None:
        raise SystemExit("could not discover a dose-rate limit from Core; pass --dose-limit-uSv-h explicitly")
    return limits["dose_rate"]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--shielding", default="examples/capabilities/shielding")
    parser.add_argument("--out", default="workspaces/practice-baseline")
    parser.add_argument("--design-factor", type=float, default=DESIGN_FACTOR_DEFAULT)
    parser.add_argument("--grid-cm", type=float, default=1, help="the sizing calculation rounds thickness up to this grid")
    parser.add_argument("--iron-thickness-cm", type=float, default=10)
    parser.add_argument("--lead-thickness-cm", type=float, default=5)
    parser.add_argument("--dose-limit-uSv-h", type=float, default=None,
                         help="override; otherwise discovered from a live Core report by unit")
    # Only needed to discover the dose limit and/or for --run.
    parser.add_argument("--core", default="target/debug/avila-core")
    parser.add_argument("--case", default="examples/cases/case-001-shield-search")
    parser.add_argument("--agents", default="examples/agents")
    parser.add_argument("--nuclear-data", default=None)
    parser.add_argument("--python3", default="/usr/bin/python3")
    parser.add_argument("--openmc-python", default=None)
    parser.add_argument("--cross-sections", default=None, help="value for OPENMC_CROSS_SECTIONS")
    parser.add_argument("--run", action="store_true", help="run each baseline through Core (screen, and transport if --openmc-python/--cross-sections are given)")
    parser.add_argument("--expect-manifest", default=None, metavar="SHA256",
                         help="refuse any run whose package manifest digest differs from this pinned value")
    parser.add_argument("--source-root", action="append", default=[], metavar="NAME=PATH",
                         help="additional or overriding source root passed to Core (repeatable)")
    args = parser.parse_args()

    out = Path(args.out)
    (out / "candidates").mkdir(parents=True, exist_ok=True)
    log = out / "campaign-log.jsonl"
    source_roots = {"case": args.case, "shielding": args.shielding, "agents": args.agents}
    if args.nuclear_data:
        source_roots["nuclear-data"] = args.nuclear_data
    for spec in args.source_root:
        name, sep, path = spec.partition("=")
        if not sep or not name or not path:
            raise SystemExit(f"--source-root expects NAME=PATH, got {spec!r}")
        source_roots[name] = path

    materials_table = core.load_materials(Path(args.shielding) / "materials.json")
    source = core.load_source(Path(args.shielding) / "source.json")

    dose_limit = args.dose_limit_uSv_h
    if dose_limit is None:
        if not args.nuclear_data:
            raise SystemExit("pass --dose-limit-uSv-h, or --nuclear-data (and the case's other roots) so it can be discovered from Core")
        dose_limit = discover_dose_limit(args.core, args.case, source_roots, args.python3, out, log)
        core.eprint(f"discovered dose limit: {dose_limit} uSv/h")

    baselines, unshielded, target = build_baselines(
        materials_table, source, dose_limit, args.design_factor, args.grid_cm,
        args.iron_thickness_cm, args.lead_thickness_cm,
    )

    written = []
    for baseline in baselines:
        candidate_path = out / "candidates" / f"{baseline['candidate_id']}.json"
        candidate = core.write_candidate(
            candidate_path, baseline["candidate_id"], baseline["layers"], description=baseline["rule"]
        )
        written.append({**baseline, "candidate": candidate, "path": str(candidate_path)})
        core.eprint(f"{baseline['candidate_id']}: " + " + ".join(f"{t} cm {m}" for m, t in baseline["layers"]))

    results = []
    if args.run:
        screen_capabilities = {"python3": args.python3}
        transport_capabilities = dict(screen_capabilities)
        environment = None
        if args.openmc_python and args.cross_sections:
            transport_capabilities["openmc-python"] = args.openmc_python
            environment = {"OPENMC_CROSS_SECTIONS": args.cross_sections}
        for entry in written:
            report = core.run_core(
                args.core, args.case, entry["path"], source_roots=source_roots,
                capabilities=transport_capabilities, environment=environment, log=log, extra_args=(['--expect-manifest', args.expect_manifest] if getattr(args, 'expect_manifest', None) else None),
            )
            results.append({**entry, "margins": core.all_margins(report)})
            core.eprint(f"{entry['candidate_id']}: status={report.get('status')}")
    else:
        results = [{**entry, "margins": None} for entry in written]

    lines = [
        "# Conventional-practice baselines", "",
        f"Unshielded dose rate: {core.canonical_decimal(unshielded)} uSv/h. "
        f"Design target (limit {dose_limit:g} uSv/h / factor {args.design_factor:g}): "
        f"{core.canonical_decimal(target)} uSv/h.",
        "", BASELINES_NOTICE, "",
    ]
    for entry in results:
        layer_text = " + ".join(f"{t} cm {m}" for m, t in entry["layers"])
        lines += [f"## {entry['candidate_id']}", "", entry["rule"], "", f"Layers: {layer_text}", ""]
        if entry["margins"] is not None:
            lines.append("| requirement | status | rule | margin |")
            lines.append("| --- | --- | --- | ---: |")
            for margin in entry["margins"]:
                lines.append(
                    f"| {margin['requirement_id']} | {margin['status']} | {margin.get('rule', '-')} | "
                    f"{core.show(margin['margin']) if margin.get('margin') is not None else '-'} |"
                )
            lines.append("")
    (out / "baseline-summary.md").write_text("\n".join(lines) + "\n")
    print((out / "baseline-summary.md").read_text())

    (out / "baselines.json").write_text(json.dumps([
        {
            "candidate_id": entry["candidate_id"],
            "rule": entry["rule"],
            "layers": entry["layers"],
            "margins": entry["margins"],
        }
        for entry in results
    ], indent=2, default=str) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
