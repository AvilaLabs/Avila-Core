#!/usr/bin/env python3
"""Derive the CASE-002 campaign tables from Core's campaign logs.

Usage: summarize_campaign.py CAMPAIGN_DIR [--markdown OUT.md]

CAMPAIGN_DIR is the directory `run_campaign.sh` wrote: one sub-directory per
arm (baselines, sweep, recovery, learning, random), each holding the arm's
`campaign-log.jsonl` and `candidates/`. Every number here is read from a log
line Core wrote; this script constructs no verdict. Exact values stay exact
in the JSON summary; the Markdown rounds for people.
"""

import argparse
import hashlib
import json
import sys
from fractions import Fraction
from pathlib import Path

ARMS = ["baselines", "sweep", "recovery", "learning", "random"]


def load_log(path):
    if not path.is_file():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def candidate_of(row, arm_dir):
    for supplied in row.get("supplied_inputs", []):
        if supplied.get("input_id") == "candidate":
            path = supplied.get("path") or supplied.get("workspace_path")
            sha = supplied.get("sha256")
            layers = None
            if path:
                file = Path(path)
                if not file.is_absolute():
                    file = Path.cwd() / file
                if not file.is_file() and arm_dir is not None:
                    file = Path(arm_dir) / "candidates" / file.name
                if file.is_file():
                    try:
                        layers = [(l["material"], l["thickness_cm"]) for l in json.loads(file.read_text())["layers"]]
                    except (KeyError, ValueError):
                        layers = None
            return {"path": path, "sha256": sha, "layers": layers}
    return {"path": None, "sha256": None, "layers": None}


def verdicts_of(row):
    return {v["requirement_id"]: v for v in row.get("verdicts", [])}


def exact(value):
    return Fraction(value) if value is not None else None


def is_all_pass(row):
    verdicts = row.get("verdicts", [])
    return bool(verdicts) and all(v.get("status") == "pass" for v in verdicts)


def transported(row):
    """The log records each step as a `[step_id, state]` pair."""
    for step in row.get("steps", []):
        if isinstance(step, (list, tuple)) and len(step) == 2:
            step_id, state = step
        elif isinstance(step, dict):
            step_id, state = step.get("step_id"), step.get("state")
        else:
            continue
        if step_id == "transport" and state in ("executed", "reused"):
            return True
    return False


def mass_of(row):
    v = verdicts_of(row).get("SHIELD-R4-mass") or {}
    return exact(v.get("nominal")) if v.get("nominal") else None


def layers_text(layers):
    if not layers:
        return "?"
    return " + ".join(f"{t} cm {m}" for m, t in layers)


def summarize_arm(arm, arm_dir):
    rows = load_log(arm_dir / "campaign-log.jsonl")
    rows = [r for r in rows if r.get("status") not in ("rejected",) or True]
    screened = [r for r in rows if not transported(r)]
    full = [r for r in rows if transported(r)]
    histogram = {}
    for r in full:
        for rid, v in verdicts_of(r).items():
            histogram.setdefault(rid, {})
            histogram[rid][v["status"]] = histogram[rid].get(v["status"], 0) + 1
    first_pass = None
    for index, r in enumerate(full, start=1):
        if is_all_pass(r):
            first_pass = index
            break
    passing = [r for r in full if is_all_pass(r)]
    best = min(passing, key=lambda r: mass_of(r) or Fraction(10**9), default=None)
    refusals = [r for r in rows if r.get("status") == "rejected"]
    not_evaluated = []
    for r in full:
        for rid, v in verdicts_of(r).items():
            if v.get("status") == "not_evaluated":
                not_evaluated.append((candidate_of(r, arm_dir)["sha256"], rid, v.get("rule")))
    inconclusive = sum(1 for r in full for v in r.get("verdicts", []) if v.get("status") == "inconclusive")
    return {
        "arm": arm,
        "log_lines": len(rows),
        "screened_only": len(screened),
        "transported": len(full),
        "verdict_histogram_transported": histogram,
        "first_all_pass_transport_index": first_pass,
        "all_pass_count": len(passing),
        "best_all_pass": None if best is None else {
            "candidate": candidate_of(best, arm_dir),
            "mass_kg": str(mass_of(best)),
            "campaign_sha256": best.get("campaign_sha256"),
        },
        "refused_runs": len(refusals),
        "inconclusive_verdicts": inconclusive,
        "not_evaluated": not_evaluated,
        "rows_full": full,
    }


def baseline_table(arm_dir):
    rows = load_log(arm_dir / "campaign-log.jsonl")
    table = []
    for r in rows:
        cand = candidate_of(r, arm_dir)
        if cand["path"] and "bootstrap" in cand["path"]:
            continue
        verdicts = verdicts_of(r)
        table.append({
            "candidate": cand,
            "verdicts": {rid: (v["status"], v.get("margin")) for rid, v in verdicts.items()},
            "all_pass": is_all_pass(r),
            "campaign_sha256": r.get("campaign_sha256"),
        })
    return table


def sweep_optimum(arm_dir):
    rows = [r for r in load_log(arm_dir / "campaign-log.jsonl") if transported(r)]
    passing = [r for r in rows if is_all_pass(r)]
    best = min(passing, key=lambda r: mass_of(r) or Fraction(10**9), default=None)
    return {
        "points": len(rows),
        "all_pass_points": len(passing),
        "optimum": None if best is None else {"candidate": candidate_of(best, arm_dir), "mass_kg": str(mass_of(best))},
        "rows": rows,
    }


def recovery_check(sweep, recovery):
    """Did the confined designer reach the sweep optimum, and after how many transports?"""
    if sweep["optimum"] is None:
        return {"applicable": False, "reason": "the sweep has no all-PASS point"}
    target = sweep["optimum"]["candidate"]["sha256"]
    for index, r in enumerate(recovery["rows_full"], start=1):
        if candidate_of(r, None)["sha256"] == target and is_all_pass(r):
            return {"applicable": True, "reached": True, "transports_to_optimum": index, "sweep_points": sweep["points"]}
    best = recovery["best_all_pass"]
    return {"applicable": True, "reached": False, "sweep_points": sweep["points"],
            "designer_best_mass_kg": None if best is None else best["mass_kg"],
            "sweep_optimum_mass_kg": sweep["optimum"]["mass_kg"]}


def fmt_margin(value):
    if value is None:
        return "-"
    return f"{float(Fraction(value)):.4g}"


def markdown(summary):
    lines = ["# CASE-002 campaign summary", "",
             "Every number below is read from a Core campaign log; the summarizer constructs no verdict.", ""]
    lines += ["## Practice baselines", "", "| candidate | all PASS | verdicts (margin) |", "| --- | --- | --- |"]
    for b in summary["baselines"]:
        vs = ", ".join(f"{rid.split('-')[1]} {st} ({fmt_margin(m)})" for rid, (st, m) in b["verdicts"].items())
        lines.append(f"| {layers_text(b['candidate']['layers'])} | {'yes' if b['all_pass'] else 'no'} | {vs} |")
    lines += ["", "## Search arms", "", "| arm | screened only | transported | all-PASS | first all-PASS at transport # | best all-PASS (mass) | refused runs | INCONCLUSIVE verdicts |", "| --- | ---: | ---: | ---: | ---: | --- | ---: | ---: |"]
    for arm, a in summary["arms"].items():
        best = a["best_all_pass"]
        best_text = "-" if best is None else f"{layers_text(best['candidate']['layers'])} ({float(Fraction(best['mass_kg'])):.4g} kg)"
        lines.append(f"| {arm} | {a['screened_only']} | {a['transported']} | {a['all_pass_count']} | {a['first_all_pass_transport_index'] or '-'} | {best_text} | {a['refused_runs']} | {a['inconclusive_verdicts']} |")
    lines += ["", "## Verdict histograms over transported candidates", ""]
    for arm, a in summary["arms"].items():
        lines.append(f"**{arm}**: " + "; ".join(f"{rid.split('-')[1]}: " + ", ".join(f"{k} {v}" for k, v in sorted(h.items())) for rid, h in sorted(a["verdict_histogram_transported"].items())))
        lines.append("")
    s = summary["sweep"]
    lines += ["## Control sweep", "", f"{s['points']} grid points transported; {s['all_pass_points']} all-PASS."]
    if s["optimum"]:
        lines.append(f"Optimum by mass: {layers_text(s['optimum']['candidate']['layers'])} at {float(Fraction(s['optimum']['mass_kg'])):.4g} kg.")
    lines += ["", "## Recovery check", "", json.dumps(summary["recovery_check"])]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign_dir")
    parser.add_argument("--markdown")
    parser.add_argument("--json")
    args = parser.parse_args()
    root = Path(args.campaign_dir)
    search_arms = sorted(d.name for d in root.iterdir()
                         if d.is_dir() and d.name not in ("baselines", "sweep") and (d / "campaign-log.jsonl").is_file())
    arms = {arm: summarize_arm(arm, root / arm) for arm in search_arms}
    sweep = sweep_optimum(root / "sweep") if (root / "sweep").is_dir() else {"points": 0, "all_pass_points": 0, "optimum": None, "rows": []}
    summary = {
        "baselines": baseline_table(root / "baselines") if (root / "baselines").is_dir() else [],
        "arms": {k: {kk: vv for kk, vv in v.items() if kk != "rows_full"} for k, v in arms.items()},
        "sweep": {k: v for k, v in sweep.items() if k != "rows"},
        "recovery_check": recovery_check(sweep, arms["recovery"]) if "recovery" in arms else {"applicable": False, "reason": "no recovery arm"},
    }
    text = markdown(summary)
    if args.markdown:
        Path(args.markdown).write_text(text, encoding="utf-8")
    if args.json:
        Path(args.json).write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
