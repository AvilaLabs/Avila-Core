#!/usr/bin/env python3
"""An explicit grid sweep over the candidate space, run through Core with
transport on every point, as ground truth for judging a designer.

Where `shield_search.py` and `shield_search2.py` propose candidates,
`control_sweep.py` proposes none: it enumerates a small, fully explicit
grid (by default one or two layers, two materials, 10 cm steps, bounded by
the mass and thickness limits a live Core report exposes) and runs *every*
point through Core with both capabilities, so R2 (or whatever the coupled
case's bounded dose requirements are) gets a real transport verdict for
every grid point, not just the finalists a designer chose to spend budget
on. The result (`sweep.jsonl` / `sweep.md`) is the closest thing this
case has to a exhaustive answer within the swept region: every value and
verdict in it is Core's own, never estimated.

`evaluations_to_reach_optimum` then answers a different question: given a
designer's `--log` campaign log, how many of its evaluations did it need
before it evaluated the same design as the grid's best feasible point? That
is the number that judges whether learning from the campaign log actually
saved evaluations, once both the sweep and a real designer run exist.

Because transport is slow (about a minute per point at CASE-001's frozen
1e6 particles today, and the whole point of a "grid" is many points), this
script does not run its full default grid by itself: pass `--limit N` to
cap how many grid points actually execute, and see the module docstring in
`shield_search2.py`'s sibling scripts for the shared machine constraints
(one OpenMC process at a time, bounded threads and particles) that make a
full sweep something to run deliberately, later, once faster transport
lands, not incidentally from this file's default arguments.
"""

import argparse
import json
import math
from decimal import Decimal
from fractions import Fraction
from pathlib import Path

import shield_common as core

GRID_NOTICE = (
    "Every value and verdict in this table is copied from Avila Core's own "
    "--json report for that exact candidate. This script proposes nothing; "
    "it only enumerates and asks Core."
)


# ----------------------------------------------------------------------
# Grid enumeration
# ----------------------------------------------------------------------


def enumerate_one_layer(materials, grid_cm, max_total_cm):
    steps = int(Decimal(str(max_total_cm)) // Decimal(str(grid_cm)))
    for material in materials:
        for step in range(1, steps + 1):
            thickness = Decimal(str(grid_cm)) * step
            yield [(material, core.canonical_decimal(thickness))]


def enumerate_two_layer(materials, grid_cm, max_total_cm):
    steps = int(Decimal(str(max_total_cm)) // Decimal(str(grid_cm)))
    grid = Decimal(str(grid_cm))
    ordered_pairs = [(a, b) for a in materials for b in materials if a != b]
    for material_a, material_b in ordered_pairs:
        for step_a in range(1, steps + 1):
            for step_b in range(1, steps + 1 - step_a):
                yield [
                    (material_a, core.canonical_decimal(grid * step_a)),
                    (material_b, core.canonical_decimal(grid * step_b)),
                ]


def enumerate_grid(materials, grid_cm, max_total_cm, max_layers):
    if max_layers >= 1:
        yield from enumerate_one_layer(materials, grid_cm, max_total_cm)
    if max_layers >= 2:
        yield from enumerate_two_layer(materials, grid_cm, max_total_cm)
    if max_layers > 2:
        raise NotImplementedError(
            "control_sweep enumerates at most 2 layers by design (an explicit "
            "small grid); use shield_search2.py or practice_baseline.py for "
            "more layers"
        )


def filter_grid_points(points, min_total_cm=None, first_material=None):
    """Grid points (each a list of `(material, thickness_cm)` pairs) whose
    total thickness is at least `min_total_cm` (if given) and whose first
    layer's material is `first_material` (if given). Pure: no Core calls,
    no mass/limit lookups -- `run_sweep` applies the mass bound separately.
    """
    kept = []
    for layers in points:
        if min_total_cm is not None:
            total_cm = sum(Decimal(thickness_cm) for _material, thickness_cm in layers)
            if total_cm < Decimal(str(min_total_cm)):
                continue
        if first_material is not None and layers[0][0] != first_material:
            continue
        kept.append(layers)
    return kept


def estimated_mass_kg(layers, materials_table, area_cm2):
    total_g = Decimal(0)
    for material, thickness_cm in layers:
        density = Decimal(materials_table[material]["density_g_cm3"])
        total_g += density * Decimal(thickness_cm) * Decimal(str(area_cm2))
    return total_g / Decimal(1000)


def grid_candidate_id(layers):
    parts = "-".join(f"{material}{thickness}" for material, thickness in layers)
    return f"grid-{len(layers)}L-{parts}"


# ----------------------------------------------------------------------
# Running the grid through Core
# ----------------------------------------------------------------------


def discover_limits_via_bootstrap(core_path, case, source_roots, python3, out, log, first_material):
    candidate = core.write_candidate(Path(out) / "candidates" / "bootstrap.json", "bootstrap", [(first_material, "5")])
    report = core.run_core(
        core_path, case, Path(out) / "candidates" / "bootstrap.json",
        source_roots=source_roots, capabilities={"python3": python3}, log=log, extra_args=(['--expect-manifest', args.expect_manifest] if getattr(args, 'expect_manifest', None) else None),
    )
    return core.discover_limits(report)


def run_sweep(args):
    out = Path(args.out)
    (out / "candidates").mkdir(parents=True, exist_ok=True)
    log = out / "campaign-log.jsonl"
    source_roots = {"case": args.case, "shielding": args.shielding, "agents": args.agents,
                     "nuclear-data": args.nuclear_data}
    for spec in args.source_root:
        name, sep, path = spec.partition("=")
        if not sep or not name or not path:
            raise SystemExit(f"--source-root expects NAME=PATH, got {spec!r}")
        source_roots[name] = path
    materials_table = core.load_materials(Path(args.shielding) / "materials.json")
    source = core.load_source(Path(args.shielding) / "source.json")
    area_cm2 = float(source["area_cm2"]) if "area_cm2" in source else None
    materials = args.materials or sorted(materials_table)[:2]
    for material in materials:
        if material not in materials_table:
            raise SystemExit(f"unknown material {material!r}; choose from {sorted(materials_table)}")
    if args.first_material is not None and args.first_material not in materials:
        raise SystemExit(f"--first-material {args.first_material!r} is not among the swept materials {materials}")

    limits = discover_limits_via_bootstrap(args.core, args.case, source_roots, args.python3, out, log, materials[0])
    thickness_limit = args.max_total_cm if args.max_total_cm is not None else limits["length"]
    if thickness_limit is None:
        raise SystemExit("no length-unit limit discovered; pass --max-total-cm explicitly")
    mass_limit = args.mass_limit_kg if args.mass_limit_kg is not None else limits["mass"]
    core.eprint(f"grid bounds: materials={materials}, grid={args.grid_cm} cm, "
                f"max_total={thickness_limit} cm, mass_limit={mass_limit}")

    points = []
    for layers in filter_grid_points(
        enumerate_grid(materials, args.grid_cm, thickness_limit, args.max_layers),
        min_total_cm=args.min_total_cm, first_material=args.first_material,
    ):
        if mass_limit is not None and area_cm2 is not None:
            if estimated_mass_kg(layers, materials_table, area_cm2) > Decimal(str(mass_limit)):
                continue
        points.append(layers)
    core.eprint(
        f"grid has {len(points)} point(s) within bounds"
        + (f", min_total_cm={args.min_total_cm}" if args.min_total_cm is not None else "")
        + (f", first_material={args.first_material}" if args.first_material is not None else "")
    )
    if args.limit is not None:
        points = points[: args.limit]
        core.eprint(f"running only the first {len(points)} point(s) (--limit {args.limit})")

    capabilities = {"python3": args.python3, "openmc-python": args.openmc_python}
    environment = {"OPENMC_CROSS_SECTIONS": args.cross_sections}

    rows = []
    for layers in points:
        candidate_id = grid_candidate_id(layers)
        candidate_path = out / "candidates" / f"{candidate_id}.json"
        candidate = core.write_candidate(candidate_path, candidate_id, layers)
        report = core.run_core(
            args.core, args.case, candidate_path, source_roots=source_roots,
            capabilities=capabilities, environment=environment, log=log, extra_args=(['--expect-manifest', args.expect_manifest] if getattr(args, 'expect_manifest', None) else None),
        )
        margins = core.all_margins(report)
        row = {
            "candidate_id": candidate_id,
            "candidate_sha256": core.sha256_file(candidate_path),
            "layers": [{"material": m, "thickness_cm": t} for m, t in layers],
            "status": report.get("status"),
            "verdicts": margins,
        }
        rows.append(row)
        core.eprint(f"{candidate_id}: status={report.get('status')} "
                    f"margins={[(m['requirement_id'], m['status']) for m in margins]}")

    write_outputs(out, rows)
    return rows


def write_outputs(out, rows):
    with (out / "sweep.jsonl").open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, separators=(",", ":")) + "\n")

    roles = core.classify_requirements(rows)
    requirement_ids = sorted(roles)
    lines = ["# Control sweep: ground truth", "", GRID_NOTICE, "",
             f"{len(rows)} grid point(s) run through Core with transport.", "",
             "| candidate | layers | " + " | ".join(requirement_ids) + " |",
             "| --- | --- | " + " | ".join(["---"] * len(requirement_ids)) + " |"]
    for row in rows:
        layer_text = " + ".join(f"{l['thickness_cm']} cm {l['material']}" for l in row["layers"])
        by_id = {v["requirement_id"]: v for v in row["verdicts"]}
        cells = []
        for requirement_id in requirement_ids:
            entry = by_id.get(requirement_id)
            if not entry:
                cells.append("-")
            elif entry["status"] == "not_evaluated":
                cells.append("not_evaluated")
            else:
                cells.append(f"{entry['status']} ({core.show(entry.get('nominal'))})")
        lines.append(f"| {row['candidate_id']} | {layer_text} | " + " | ".join(cells) + " |")
    (out / "sweep.md").write_text("\n".join(lines) + "\n")


# ----------------------------------------------------------------------
# "How many evaluations did the designer need to reach the grid optimum?"
# ----------------------------------------------------------------------


def grid_optimum(sweep_rows, roles=None):
    """The feasible (every requirement `pass`) sweep row minimizing the
    worst bounded dose requirement, ties broken by mass. `roles` lets a
    caller reuse a `classify_requirements` call across sweep and log rows
    that share requirement ids; computed from `sweep_rows` alone if omitted.
    Returns `(row, primary_dose, mass)` or `None` if no grid point is feasible.
    """
    roles = roles or core.classify_requirements(sweep_rows)
    dose_ids = [rid for rid, info in roles.items() if info["dose_rate_bounded"]]
    mass_ids = [rid for rid, info in roles.items() if info["mass"]]
    best = None
    for row in sweep_rows:
        entries = {v["requirement_id"]: v for v in row["verdicts"]}
        if not entries or any(entry["status"] != "pass" for entry in entries.values()):
            continue
        if not dose_ids or any(rid not in entries or entries[rid].get("nominal") is None for rid in dose_ids):
            continue
        dose_value = max(float(Fraction(entries[rid]["nominal"])) for rid in dose_ids)
        mass_value = None
        for rid in mass_ids:
            if rid in entries and entries[rid].get("nominal") is not None:
                mass_value = float(Fraction(entries[rid]["nominal"]))
                break
        key = (dose_value, mass_value if mass_value is not None else math.inf)
        if best is None or key < best[0]:
            best = (key, row, dose_value, mass_value)
    if best is None:
        return None
    _key, row, dose_value, mass_value = best
    return row, dose_value, mass_value


def evaluations_to_reach_optimum(sweep_rows, campaign_log_rows, signature_index):
    """How many candidate-bearing rows of a designer's campaign log it took
    before it evaluated a design matching the grid's best feasible point.
    `signature_index`: `sha256 -> {"signature": ...}`, built by
    `shield_common.build_signature_index` over the sweep's own and the
    designer's candidate directories, so matching does not depend on a
    logged path still existing or on byte-identical JSON formatting.
    """
    optimum = grid_optimum(sweep_rows)
    if optimum is None:
        return {"grid_has_feasible_point": False, "evaluations_needed": None}
    row, dose_value, mass_value = optimum
    optimum_signature = core.layer_signature(row["layers"])
    seen = 0
    for log_row in campaign_log_rows:
        supplied = [s for s in log_row.get("supplied_inputs", []) if s.get("input_id") == "candidate"]
        if not supplied:
            continue
        seen += 1
        entry = signature_index.get(supplied[0].get("sha256"))
        if entry is not None and entry["signature"] == optimum_signature:
            return {
                "grid_has_feasible_point": True,
                "optimum_candidate_id": row["candidate_id"],
                "optimum_primary_dose": dose_value,
                "optimum_mass": mass_value,
                "evaluations_needed": seen,
            }
    return {
        "grid_has_feasible_point": True,
        "optimum_candidate_id": row["candidate_id"],
        "optimum_primary_dose": dose_value,
        "optimum_mass": mass_value,
        "evaluations_needed": None,
        "note": f"the grid optimum's design was not among the {seen} candidate-bearing "
                "row(s) with a recognized signature in this log",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    sweep_parser = sub.add_parser("sweep", help="enumerate and run the grid through Core")
    sweep_parser.add_argument("--core", default="target/debug/avila-core")
    sweep_parser.add_argument("--case", default="examples/cases/case-001-shield-search")
    sweep_parser.add_argument("--shielding", default="examples/capabilities/shielding")
    sweep_parser.add_argument("--agents", default="examples/agents")
    sweep_parser.add_argument("--nuclear-data", required=True)
    sweep_parser.add_argument("--cross-sections", required=True)
    sweep_parser.add_argument("--python3", default="/usr/bin/python3")
    sweep_parser.add_argument("--openmc-python", required=True)
    sweep_parser.add_argument("--out", default="workspaces/control-sweep")
    sweep_parser.add_argument("--materials", nargs="*", default=None,
                               help="default: the first two materials in the table, alphabetically")
    sweep_parser.add_argument("--grid-cm", type=float, default=10)
    sweep_parser.add_argument("--max-layers", type=int, default=2)
    sweep_parser.add_argument("--max-total-cm", type=float, default=None, help="override; else discovered from Core")
    sweep_parser.add_argument("--mass-limit-kg", type=float, default=None, help="override; else discovered from Core")
    sweep_parser.add_argument("--min-total-cm", type=float, default=None,
                               help="skip grid points whose total thickness across all layers is less than this")
    sweep_parser.add_argument("--first-material", default=None,
                               help="keep only grid points whose first layer is this material")
    sweep_parser.add_argument("--expect-manifest", default=None, metavar="SHA256",
                              help="refuse any run whose package manifest digest differs from this pinned value")
    sweep_parser.add_argument("--source-root", action="append", default=[], metavar="NAME=PATH",
                              help="additional or overriding source root passed to Core (repeatable)")
    sweep_parser.add_argument("--limit", type=int, default=None,
                               help="run only the first LIMIT grid points (the full default grid is not meant to "
                                    "run until transport is fast; see the module docstring)")

    report_parser = sub.add_parser("evaluations-to-optimum",
                                    help="how many designer evaluations it took to reach the grid optimum")
    report_parser.add_argument("--sweep", required=True, help="sweep.jsonl from a prior `sweep` run")
    report_parser.add_argument("--designer-log", required=True, help="a designer's --log campaign-log.jsonl")
    report_parser.add_argument("--candidates-dir", action="append", default=[],
                                help="directory of candidate JSON files to hash for signature matching; repeatable "
                                     "(pass both the sweep's and the designer's candidates/ directories)")

    args = parser.parse_args()

    if args.command == "sweep":
        run_sweep(args)
        return 0

    sweep_rows = core.read_jsonl(args.sweep)
    log_rows = core.read_jsonl(args.designer_log)
    signature_index = core.build_signature_index(args.candidates_dir)
    report = evaluations_to_reach_optimum(sweep_rows, log_rows, signature_index)
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
