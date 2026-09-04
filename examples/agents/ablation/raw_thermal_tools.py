#!/usr/bin/env python3
"""Arm B tool for EXP-002: the raw-solver designer.

Runs the same two capability scripts CASE-003 binds
(`examples/capabilities/thermal/thermal_screen.py` and `thermal_fe.py`)
directly by subprocess, with the same interpreters the case package pins
(system python3 for the screen, the thermal virtualenv's python for the
finite-element step). Avila Core is never invoked by this tool.

Every subcommand returns numbers copied verbatim from a script's own JSON
output, or a script's own error text, and nothing else: no verdict, no
PASS/FAIL, no margin, no comparison against a requirement's limit, no
coverage or envelope language, and no refusal wording. The requirement
limits themselves appear in `brief` (the designer needs to know the target,
same as arm A), so the designer must do its own arithmetic against them --
that arithmetic, and Core's absence from it, is exactly what this arm
isolates.

    init      fix the case paths, scripts, interpreters, budgets, prior logs
    brief     print requirements, materials, prior constellation (Core-scored,
              since the prior is a fixed factor shared by every arm), budget,
              rules
    propose   run the screen script on a batch of candidates (JSON list),
              raw numbers or a script error only
    evaluate  run the screen AND finite-element scripts on chosen candidates,
              raw numbers or a script error only
    status    budget used, candidates evaluated so far (raw numbers, no
              pass/fail)
    finish    write summary.md: every candidate this arm evaluated, with its
              raw numbers, and nothing this tool decided about them

Every subprocess call (screen or finite-element) is timed and appended to
`timing.jsonl` in `--out`, so the harness can separate this arm's solver time
from model and orchestration time without instrumenting anything else.
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import common  # noqa: E402
import shield_llm_tools as core_tools  # noqa: E402 (reused only to render the shared, Core-scored prior identically to arm A)


def cmd_init(args):
    out = Path(args.out)
    (out / "candidates").mkdir(parents=True, exist_ok=True)
    manifest = Path(args.case) / "package.json"
    config = {
        "arm": "B-raw-solver",
        "case": args.case,
        "materials_path": args.materials,
        "source_path": args.source,
        "screen_script": args.screen_script,
        "fe_script": args.fe_script,
        "python3": args.python3,
        "thermal_python": args.thermal_python,
        "manifest_sha256": common.sha256_file(manifest),
        "prior_logs": args.prior_log,
        "screen_budget": args.screen_budget,
        "eval_budget": args.eval_budget,
    }
    common.write_json(out / "config.json", config)
    common.save_state(str(out), {"screens": 0, "evals": 0, "next_index": 0, "screened": {}})
    (out / "timing.jsonl").touch()
    print(
        f"initialized {out}: screen budget {args.screen_budget}, eval budget {args.eval_budget}, "
        f"{len(args.prior_log)} prior log(s)"
    )


def _prior_rows(config):
    core_tools.use_config({"thickness_key": common.THICKNESS_KEY})
    rows = []
    for log_path in config["prior_logs"]:
        for row in core_tools.read_log(log_path):
            layers = core_tools.candidate_layers(row, log_path)
            if layers is None or not core_tools.transported(row):
                continue
            rows.append(
                {
                    "origin": "prior (Core-scored)",
                    "layers": layers,
                    "verdicts": core_tools.verdict_map(row),
                    "all_pass": core_tools.all_pass(row),
                }
            )
    return rows


def cmd_brief(args):
    config = common.load_config(args.out)
    requirements = common.load_requirements(Path(config["case"]) / "contract.json")
    materials = common.load_materials(config["materials_path"])
    state = common.load_state(args.out)
    print(common.format_requirements_table(requirements))
    print()
    print(common.format_materials_table(materials))
    print()
    print("# Prior constellation (from the campaign-1 sweep; Core-scored, shared by every arm)")
    core_tools.print_table(_prior_rows(config))
    print()
    print(f"# Budget: {state['evals']}/{config['eval_budget']} evaluations used, {state['screens']}/{config['screen_budget']} screens used")
    print()
    print("# Rules")
    print(
        "- Propose candidates as a JSON list of "
        f'{{"layers": [{{"material": ..., "{common.THICKNESS_KEY}": ...}}, ...], "rationale": ...}}; '
        "thicknesses in whole millimetres, layers listed from the heated face outward."
    )
    print(
        "- `propose` runs the one-dimensional screen script only and reports its raw hotspot "
        "estimate, areal mass, and thickness -- numbers, not a verdict. `evaluate` additionally "
        "runs the two-dimensional finite-element script and reports its raw hotspot bracket. "
        "Neither command compares a number to a limit or tells you whether a design is "
        "acceptable; the limits above are yours to judge against."
    )
    print("- You may not edit any file; you may only propose and choose what to evaluate.")
    print("- Every proposal's rationale is recorded.")


def _run_screen(config, candidate_path):
    output_path = candidate_path.with_suffix(".screen.json")
    command = [
        config["python3"], config["screen_script"],
        "--candidate", str(candidate_path),
        "--materials", config["materials_path"],
        "--source", config["source_path"],
        "--output", str(output_path),
    ]
    with common.Stopwatch() as sw:
        completed = subprocess.run(command, capture_output=True, text=True)
    common.append_jsonl(
        Path(config["_out"]) / "timing.jsonl",
        {"step": "screen", "candidate": candidate_path.stem, "elapsed_s": sw.elapsed_s, "returncode": completed.returncode},
    )
    if completed.returncode != 0:
        return {"error": (completed.stderr or completed.stdout).strip()[-2000:]}
    result = common.read_json(output_path)
    return {
        "hotspot_temperature_screen_K": result["hotspot_temperature"]["value"],
        "areal_mass_kg_m2": result["areal_mass"]["value"],
        "thickness_mm": result["thickness"]["value"],
    }


def _run_fe(config, candidate_path):
    output_path = candidate_path.with_suffix(".fe.json")
    command = [
        config["thermal_python"], config["fe_script"],
        "--candidate", str(candidate_path),
        "--materials", config["materials_path"],
        "--source", config["source_path"],
        "--output", str(output_path),
    ]
    with common.Stopwatch() as sw:
        completed = subprocess.run(command, capture_output=True, text=True)
    common.append_jsonl(
        Path(config["_out"]) / "timing.jsonl",
        {"step": "fe", "candidate": candidate_path.stem, "elapsed_s": sw.elapsed_s, "returncode": completed.returncode},
    )
    if completed.returncode != 0:
        return {"error": (completed.stderr or completed.stdout).strip()[-2000:]}
    result = common.read_json(output_path)
    return {
        "hotspot_temperature_fe_bracket_K": [result["hotspot_temperature"]["lower"], result["hotspot_temperature"]["upper"]],
    }


def cmd_propose(args):
    out = args.out
    config = common.load_config(out)
    config["_out"] = out
    state = common.load_state(out)
    materials = common.load_materials(config["materials_path"])
    proposals = json.loads(Path(args.proposals).read_text())
    notes = Path(out) / "designer-notes.jsonl"
    results = []
    for proposal in proposals:
        if state["screens"] >= config["screen_budget"]:
            print("screen budget exhausted")
            break
        layers = common.canonical_layers(proposal["layers"])
        unknown = [l["material"] for l in layers if l["material"] not in materials]
        if unknown or not layers:
            print(f"rejected proposal (unknown material {unknown} or empty): {proposal}")
            continue
        sig = tuple((l["material"], l[common.THICKNESS_KEY]) for l in layers)
        key = json.dumps(sig)
        if key in state["screened"]:
            print(f"already screened as {state['screened'][key]}: {common.layers_text(layers)}")
            continue
        candidate_id = f"b-{state['next_index']:04d}"
        state["next_index"] += 1
        path = Path(out) / "candidates" / f"{candidate_id}.json"
        common.write_candidate(path, candidate_id, layers)
        raw = _run_screen(config, path)
        state["screens"] += 1
        state["screened"][key] = candidate_id
        common.append_jsonl(
            notes,
            {"candidate_id": candidate_id, "layers": layers, "rationale": proposal.get("rationale", ""), "stage": "screen", "raw": raw},
        )
        results.append((candidate_id, layers, raw))
    common.save_state(out, state)
    for candidate_id, layers, raw in results:
        print(f"{candidate_id} | {common.layers_text(layers)} | {json.dumps(raw)}")
    print(f"screens used {state['screens']}/{config['screen_budget']}")


def cmd_evaluate(args):
    out = args.out
    config = common.load_config(out)
    config["_out"] = out
    state = common.load_state(out)
    notes = Path(out) / "designer-notes.jsonl"
    for candidate_id in args.ids:
        if state["evals"] >= config["eval_budget"]:
            print("evaluation budget exhausted")
            break
        path = Path(out) / "candidates" / f"{candidate_id}.json"
        if not path.is_file():
            print(f"no such candidate: {candidate_id}")
            continue
        candidate = common.read_json(path)
        raw = _run_screen(config, path)
        raw.update(_run_fe(config, path))
        state["evals"] += 1
        common.append_jsonl(
            notes,
            {"candidate_id": candidate_id, "layers": candidate["layers"], "rationale": args.rationale or "", "stage": "evaluate", "raw": raw},
        )
        print(f"{candidate_id} | {common.layers_text(candidate['layers'])} | {json.dumps(raw)}")
    common.save_state(out, state)
    print(f"evaluations used {state['evals']}/{config['eval_budget']}")


def _this_arm_rows(out):
    rows = common.read_jsonl(Path(out) / "designer-notes.jsonl")
    return [r for r in rows if r["stage"] == "evaluate"]


def cmd_status(args):
    config = common.load_config(args.out)
    state = common.load_state(args.out)
    print(f"evaluations used {state['evals']}/{config['eval_budget']}; screens used {state['screens']}/{config['screen_budget']}")
    print("evaluated this arm (raw numbers, no verdict):")
    for row in _this_arm_rows(args.out):
        print(f"  {row['candidate_id']} | {common.layers_text(row['layers'])} | {json.dumps(row['raw'])}")


def cmd_finish(args):
    out = args.out
    config = common.load_config(out)
    state = common.load_state(out)
    rows = _this_arm_rows(out)
    lines = [
        "# Raw-solver designer arm (arm B)",
        "",
        f"{state['screens']} candidates screened; {state['evals']} fully evaluated.",
        "",
        "This tool reports only raw solver numbers. No verdict, margin, or pass/fail",
        "was computed by this tool for any candidate; the designer's own reasoning",
        "against the requirement limits (given in `brief`) is in `designer-notes.jsonl`.",
        "",
        "| candidate | design | screen hotspot K | FE bracket K | mass kg/m2 | thickness mm |",
        "| --- | --- | ---: | --- | ---: | ---: |",
    ]
    for row in rows:
        raw = row["raw"]
        lines.append(
            f"| {row['candidate_id']} | {common.layers_text(row['layers'])} | "
            f"{raw.get('hotspot_temperature_screen_K', raw.get('error', '-'))} | "
            f"{raw.get('hotspot_temperature_fe_bracket_K', '-')} | "
            f"{raw.get('areal_mass_kg_m2', '-')} | {raw.get('thickness_mm', '-')} |"
        )
    (Path(out) / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("init")
    p.add_argument("--out", required=True)
    p.add_argument("--case", required=True)
    p.add_argument("--materials", required=True, help="path to materials.json")
    p.add_argument("--source", required=True, help="path to source.json")
    p.add_argument("--screen-script", required=True)
    p.add_argument("--fe-script", required=True)
    p.add_argument("--python3", default="/usr/bin/python3")
    p.add_argument("--thermal-python", required=True)
    p.add_argument("--prior-log", action="append", default=[])
    p.add_argument("--screen-budget", type=int, required=True)
    p.add_argument("--eval-budget", type=int, required=True)
    p.set_defaults(func=cmd_init)

    for name, func in (("brief", cmd_brief), ("status", cmd_status), ("finish", cmd_finish)):
        p = sub.add_parser(name)
        p.add_argument("--out", required=True)
        p.set_defaults(func=func)

    p = sub.add_parser("propose")
    p.add_argument("--out", required=True)
    p.add_argument("--proposals", required=True)
    p.set_defaults(func=cmd_propose)

    p = sub.add_parser("evaluate")
    p.add_argument("--out", required=True)
    p.add_argument("--rationale", default="")
    p.add_argument("ids", nargs="+")
    p.set_defaults(func=cmd_evaluate)

    args = parser.parse_args()
    # `--source` was validated by init; store it under materials_path's sibling
    # only via config, not recomputed here.
    args.func(args)


if __name__ == "__main__":
    main()
