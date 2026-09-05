#!/usr/bin/env python3
"""Arm C tool for EXP-005: the no-iterative-feedback designer, on CASE-002.

The designer reads the brief and the prior once, then submits its entire
final candidate set in a single call. No evaluation of any kind -- not even
a raw number -- is returned for a submitted candidate: `submit` only
confirms how many candidates were recorded. The harness evaluates the
submitted set through Avila Core after this session has already ended, so
nothing this arm's session can read is influenced by that evaluation.

    init      fix the case paths, budgets (N_eval), and prior logs
    brief     print requirements, materials, prior constellation (Core-scored,
              the same fixed prior every arm receives), the shortcut sentence,
              and the one-shot rule
    submit    record up to N_eval final candidates (JSON list); only the
              first call has any effect, exactly as arm C's protocol requires
    status    how many candidates are recorded (not their outcome: there is
              none yet)
    finish    write summary.md: the submitted set and the designer's own
              rationale, nothing evaluated
"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import common  # noqa: E402
import shield_llm_tools as core_tools  # noqa: E402 (reused only to render the shared, Core-scored prior identically to the other arms)

THICKNESS_KEY = "thickness_cm"
CANDIDATE_SCHEMA = "avila.shielding/candidate/v1"


def cmd_init(args):
    out = Path(args.out)
    (out / "candidates").mkdir(parents=True, exist_ok=True)
    manifest = Path(args.case) / "package.json"
    config = {
        "arm": "C-no-iterative-feedback",
        "case": args.case,
        "materials_path": args.materials,
        "manifest_sha256": common.sha256_file(manifest),
        "prior_logs": args.prior_log,
        "n_eval": args.n_eval,
    }
    common.write_json(out / "config.json", config)
    common.save_state(str(out), {"submitted": False, "candidates": []})
    print(f"initialized {out}: N_eval {args.n_eval}, {len(args.prior_log)} prior log(s)")


def _prior_rows(config):
    core_tools.use_config({"thickness_key": THICKNESS_KEY})
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
    print(common.format_requirements_table(requirements))
    print()
    print(common.format_shielding_materials_table(materials))
    print()
    print("# Prior constellation (from the campaign-3 control sweep; Core-scored, shared by every arm)")
    core_tools.print_table(_prior_rows(config))
    print()
    print("# Rules")
    print(
        f"- You get exactly one `submit` call for your final answer: a JSON list of at most "
        f"{config['n_eval']} candidates, each "
        f'{{"layers": [{{"material": ..., "{THICKNESS_KEY}": ...}}, ...], "rationale": ...}}, '
        "thicknesses in whole centimetres, layers listed from the source outward."
    )
    print(
        "- There is no feedback of any kind on your candidates before or during submission: "
        "no screen, no evaluation, no numbers. Decide your entire set from the brief and the "
        "prior alone, then call `submit` once. A second `submit` call has no effect."
    )
    print(f"- {common.SHIELD_LAYER_SHORTCUT_SENTENCE}")
    print("- Every candidate's rationale is recorded.")


def cmd_submit(args):
    out = args.out
    config = common.load_config(out)
    state = common.load_state(out)
    if state["submitted"]:
        print("already submitted; this arm allows exactly one submission. Ignored.")
        return
    materials = common.load_materials(config["materials_path"])
    proposals = json.loads(Path(args.candidates).read_text())
    if len(proposals) > config["n_eval"]:
        print(f"rejected: {len(proposals)} candidates exceeds N_eval {config['n_eval']}. Nothing recorded; resubmit at or under the limit.")
        return
    recorded = []
    for i, proposal in enumerate(proposals):
        layers = common.canonical_layers(proposal["layers"], thickness_key=THICKNESS_KEY)
        unknown = [l["material"] for l in layers if l["material"] not in materials]
        if unknown or not layers:
            print(f"rejected candidate {i} (unknown material {unknown} or empty), whole submission discarded: {proposal}")
            return
        candidate_id = f"c-{i:04d}"
        path = Path(out) / "candidates" / f"{candidate_id}.json"
        common.write_candidate(path, candidate_id, layers, schema=CANDIDATE_SCHEMA, thickness_key=THICKNESS_KEY)
        common.append_jsonl(
            Path(out) / "designer-notes.jsonl",
            {"candidate_id": candidate_id, "layers": layers, "rationale": proposal.get("rationale", "")},
        )
        recorded.append(candidate_id)
    state["submitted"] = True
    state["candidates"] = recorded
    common.save_state(out, state)
    print(f"recorded {len(recorded)} candidate(s): {', '.join(recorded)}. No evaluation is available in this session.")


def cmd_status(args):
    state = common.load_state(args.out)
    print(f"submitted: {state['submitted']}; candidates recorded: {len(state['candidates'])}")


def cmd_finish(args):
    out = args.out
    state = common.load_state(out)
    notes = common.read_jsonl(Path(out) / "designer-notes.jsonl")
    lines = [
        "# No-iterative-feedback designer arm (arm C)",
        "",
        f"{len(state['candidates'])} candidate(s) submitted in one call; none evaluated in this session.",
        "",
        "| candidate | design | rationale |",
        "| --- | --- | --- |",
    ]
    for row in notes:
        lines.append(f"| {row['candidate_id']} | {common.layers_text(row['layers'], thickness_key=THICKNESS_KEY)} | {row['rationale']} |")
    lines += ["", "The harness evaluates this set through Avila Core after this session ends."]
    (Path(out) / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("init")
    p.add_argument("--out", required=True)
    p.add_argument("--case", required=True)
    p.add_argument("--materials", required=True)
    p.add_argument("--prior-log", action="append", default=[])
    p.add_argument("--n-eval", type=int, required=True)
    p.set_defaults(func=cmd_init)

    for name, func in (("brief", cmd_brief), ("status", cmd_status), ("finish", cmd_finish)):
        p = sub.add_parser(name)
        p.add_argument("--out", required=True)
        p.set_defaults(func=func)

    p = sub.add_parser("submit")
    p.add_argument("--out", required=True)
    p.add_argument("--candidates", required=True)
    p.set_defaults(func=cmd_submit)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
