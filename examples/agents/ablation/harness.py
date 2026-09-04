#!/usr/bin/env python3
"""EXP-002 Core-feedback ablation harness.

Launches one headless Claude Code designer session per trial against one of
three arms of CASE-003 (see `examples/agents/ablation/README.md`), records
complete provenance, and scores the result through Avila Core. No trial in
this file runs Avila Core, the finite-element solver, or `claude` except
through an explicit, logged subprocess call; nothing here fabricates a
verdict or a physical number.

    run_trial --arm A|B|C --trial K --seed S --out DIR
    run_block --trials K --seed S --out DIR
    score DIR

See the module-level constants below for default paths (the case, the
capability scripts, the Core binary, the thermal virtualenv). Every one can
be overridden on the command line so this harness runs unchanged from a
plain shell outside this worktree, given the same files.
"""

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402
import scoring  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import shield_llm_tools as core_tools  # noqa: E402 (arm A is this tool, unmodified; also reused to read its own log format)

ABLATION_DIR = Path(__file__).resolve().parent
AGENTS_DIR = ABLATION_DIR.parent
REPO_ROOT = AGENTS_DIR.parent.parent  # examples/agents/ablation -> examples/agents -> examples -> repo root

DEFAULT_CASE = REPO_ROOT / "examples/cases/case-003-thermal-spreader"
DEFAULT_THERMAL = REPO_ROOT / "examples/capabilities/thermal"
DEFAULT_CORE = Path("/home/connoravila/Documents/Avila-Labs/project-north-star/target/debug/avila-core")
DEFAULT_THERMAL_PYTHON = Path("/home/connoravila/.venvs/thermal/bin/python")
DEFAULT_PYTHON3 = "/usr/bin/python3"
DEFAULT_PRIOR_LOG = DEFAULT_CASE / "campaign-1/sweep/campaign-log.jsonl"

MODEL_ID = "claude-sonnet-5"
CLAUDE_BIN = "claude"

SHIELD_TOOL = AGENTS_DIR / "shield_llm_tools.py"
RAW_TOOL = ABLATION_DIR / "raw_thermal_tools.py"
BLIND_TOOL = ABLATION_DIR / "blind_thermal_tools.py"
ARM_TOOL = {"A": SHIELD_TOOL, "B": RAW_TOOL, "C": BLIND_TOOL}
ARM_PROMPT_TEMPLATE = {
    "A": ABLATION_DIR / "prompts/arm-a-core.md",
    "B": ABLATION_DIR / "prompts/arm-b-raw-solver.md",
    "C": ABLATION_DIR / "prompts/arm-c-blind.md",
}
ARMS = ("A", "B", "C")


def default_paths(args):
    return {
        "case": Path(args.case).resolve(),
        "thermal": Path(args.thermal).resolve(),
        "core": Path(args.core).resolve(),
        "python3": args.python3,
        "thermal_python": Path(args.thermal_python).resolve(),
        "prior_log": Path(args.prior_log).resolve(),
    }


def claude_version():
    try:
        completed = subprocess.run([CLAUDE_BIN, "--version"], capture_output=True, text=True, timeout=15)
        return completed.stdout.strip()
    except (OSError, subprocess.TimeoutExpired) as exc:
        return f"unavailable: {exc}"


def trial_dir_path(out_root, arm, trial):
    return Path(out_root) / arm / f"trial-{trial:02d}"


def is_done(trial_dir):
    return (Path(trial_dir) / "DONE.json").is_file()


def init_arm_tool(arm, trial_dir, screen_budget, eval_budget, n_eval, paths):
    designer = trial_dir / "designer"
    if arm == "A":
        cmd = [
            sys.executable, str(SHIELD_TOOL), "init",
            "--out", str(designer),
            "--core", str(paths["core"]),
            "--case", str(paths["case"]),
            "--materials", str(paths["thermal"]),
            "--source-root", f"case={paths['case']}",
            "--source-root", f"thermal={paths['thermal']}",
            "--python3", paths["python3"],
            "--capability", f"thermal-python={paths['thermal_python']}",
            "--prior-log", str(paths["prior_log"]),
            "--screen-budget", str(screen_budget),
            "--transport-budget", str(eval_budget),
            "--candidate-schema", "avila.thermal/candidate/v1",
            "--thickness-key", "thickness_mm",
            "--probe-thickness", "5",
        ]
    elif arm == "B":
        cmd = [
            sys.executable, str(RAW_TOOL), "init",
            "--out", str(designer),
            "--case", str(paths["case"]),
            "--materials", str(paths["thermal"] / "materials.json"),
            "--source", str(paths["thermal"] / "source.json"),
            "--screen-script", str(paths["thermal"] / "thermal_screen.py"),
            "--fe-script", str(paths["thermal"] / "thermal_fe.py"),
            "--python3", paths["python3"],
            "--thermal-python", str(paths["thermal_python"]),
            "--prior-log", str(paths["prior_log"]),
            "--screen-budget", str(screen_budget),
            "--eval-budget", str(eval_budget),
        ]
    else:
        cmd = [
            sys.executable, str(BLIND_TOOL), "init",
            "--out", str(designer),
            "--case", str(paths["case"]),
            "--materials", str(paths["thermal"] / "materials.json"),
            "--prior-log", str(paths["prior_log"]),
            "--n-eval", str(n_eval),
        ]
    completed = subprocess.run(cmd, capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(f"tool init failed for arm {arm}: {completed.stderr}")
    return designer


def build_prompt(arm, trial_dir, tool_path, eval_budget, case_path, n_eval):
    template = ARM_PROMPT_TEMPLATE[arm].read_text(encoding="utf-8")
    return template.format(OUT=str(trial_dir / "designer"), TOOL=str(tool_path), EVAL_BUDGET=eval_budget, CASE=str(case_path), N_EVAL=n_eval)


def run_claude_session(prompt_text, allowed_pattern, cwd, transcript_path, timeout_s, max_budget_usd):
    cmd = [
        CLAUDE_BIN,
        "--model", MODEL_ID,
        "--output-format", "stream-json", "--verbose",
        # acceptEdits auto-accepts Write/Edit calls confined to `cwd` (verified: a
        # Write outside cwd is still denied automatically under
        # --permission-prompts none) so the designer can create its proposal/
        # candidate JSON file without any shell redirection; Bash stays gated by
        # --allowedTools exactly as under the stricter "manual" mode.
        "--permission-mode", "acceptEdits", "--permission-prompts", "none",
        "--tools", "Bash,Write", "--strict-mcp-config", "--safe-mode",
        "--no-session-persistence",
        "--allowedTools", allowed_pattern,
        "--max-budget-usd", str(max_budget_usd),
        "-p",
    ]
    start = time.monotonic()
    try:
        proc = subprocess.Popen(cmd, cwd=str(cwd), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    except FileNotFoundError as exc:
        return {"ok": False, "timed_out": False, "returncode": None, "wall_s": 0.0, "lines": [], "result": None, "stderr_tail": str(exc), "launch_command": cmd}
    timed_out = False
    try:
        stdout_data, stderr_data = proc.communicate(input=prompt_text, timeout=timeout_s)
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout_data, stderr_data = proc.communicate()
        timed_out = True
    wall_s = time.monotonic() - start
    Path(transcript_path).write_text(stdout_data, encoding="utf-8")
    lines = []
    for raw_line in stdout_data.splitlines():
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            lines.append(json.loads(raw_line))
        except json.JSONDecodeError:
            continue
    result = scoring.result_event(lines)
    ok = (not timed_out) and proc.returncode == 0 and result is not None and result.get("subtype") == "success"
    return {
        "ok": ok, "timed_out": timed_out, "returncode": proc.returncode, "wall_s": wall_s,
        "lines": lines, "result": result, "stderr_tail": (stderr_data or "")[-4000:], "launch_command": cmd,
    }


def run_core_for_candidate(paths, candidate_path, workspace_dir, log_path, manifest_sha256):
    cmd = [
        str(paths["core"]), "run", str(paths["case"]), "--json",
        "--source-root", f"case={paths['case']}",
        "--source-root", f"thermal={paths['thermal']}",
        "--capability", f"python3={paths['python3']}",
        "--capability", f"thermal-python={paths['thermal_python']}",
        "--input", f"candidate={candidate_path}",
        "--workspace", str(workspace_dir),
        "--log", str(log_path),
        "--expect-manifest", manifest_sha256,
    ]
    with common.Stopwatch() as sw:
        completed = subprocess.run(cmd, capture_output=True, text=True)
    return completed, sw.elapsed_s


def final_core_scoring(arm, trial_dir, designer, paths):
    """Run every candidate the arm considered final through Core, or (arm A)
    copy the rows Core already produced live, into `evaluation-log.jsonl`.
    Returns the rows in evaluation order."""
    log_path = trial_dir / "evaluation-log.jsonl"
    manifest_sha256 = common.sha256_file(paths["case"] / "package.json")

    if arm == "A":
        rows = common.read_jsonl(designer / "campaign-log.jsonl")
        final_rows = [r for r in rows if core_tools.transported(r)]
        for row in final_rows:
            common.append_jsonl(log_path, row)
        return final_rows

    if arm == "B":
        notes = common.read_jsonl(designer / "designer-notes.jsonl")
        ids = []
        for row in notes:
            if row["stage"] == "evaluate" and row["candidate_id"] not in ids:
                ids.append(row["candidate_id"])
    else:  # arm C
        state = common.read_json(designer / "state.json")
        ids = state.get("candidates", [])

    timing_path = trial_dir / "post-hoc-core-timing.jsonl"
    for candidate_id in ids:
        candidate_path = designer / "candidates" / f"{candidate_id}.json"
        if not candidate_path.is_file():
            common.append_jsonl(timing_path, {"candidate_id": candidate_id, "error": "candidate file missing"})
            continue
        workspace = trial_dir / "post-hoc-core" / candidate_id
        completed, elapsed = run_core_for_candidate(paths, candidate_path, workspace, log_path, manifest_sha256)
        common.append_jsonl(
            timing_path,
            {"candidate_id": candidate_id, "elapsed_s": elapsed, "returncode": completed.returncode, "stderr_tail": completed.stderr[-1000:] if completed.returncode else ""},
        )
    return common.read_jsonl(log_path)


def run_trial(arm, trial, seed, out_root, screen_budget, eval_budget, n_eval, timeout_s, max_retries, max_budget_usd, paths, fresh=False):
    tdir = trial_dir_path(out_root, arm, trial)
    if is_done(tdir) and not fresh:
        return {"arm": arm, "trial": trial, "status": "skipped-already-done"}
    if tdir.exists():
        shutil.rmtree(tdir)
    tdir.mkdir(parents=True)

    tool_path = ARM_TOOL[arm]
    designer = init_arm_tool(arm, tdir, screen_budget, eval_budget, n_eval, paths)
    prompt_text = build_prompt(arm, tdir, tool_path, eval_budget, paths["case"], n_eval)
    allowed_pattern = f"Bash(python3 {tool_path} *)"

    config = {
        "arm": arm,
        "trial": trial,
        "seed": seed,
        "model": MODEL_ID,
        "claude_cli_version": claude_version(),
        "tool_script": str(tool_path),
        "tool_script_sha256": common.sha256_file(tool_path),
        "shared_module_sha256": {
            "common.py": common.sha256_file(ABLATION_DIR / "common.py"),
            "scoring.py": common.sha256_file(ABLATION_DIR / "scoring.py"),
            "shield_llm_tools.py": common.sha256_file(SHIELD_TOOL),
            "shield_common.py": common.sha256_file(AGENTS_DIR / "shield_common.py"),
        },
        "prompt_template": str(ARM_PROMPT_TEMPLATE[arm]),
        "prompt_sha256": common.sha256_text(prompt_text),
        "screen_budget": screen_budget,
        "eval_budget": eval_budget,
        "n_eval": n_eval if arm == "C" else None,
        "manifest_sha256": common.sha256_file(paths["case"] / "package.json"),
        "case": str(paths["case"]),
        "thermal": str(paths["thermal"]),
        "core_binary": str(paths["core"]),
        "core_binary_sha256": common.sha256_file(paths["core"]),
        "python3": paths["python3"],
        "thermal_python": str(paths["thermal_python"]),
        "prior_log": str(paths["prior_log"]),
        "permission_mode": "acceptEdits",
        "permission_prompts": "none",
        "tools": "Bash,Write",
        "strict_mcp_config": True,
        "safe_mode": True,
        "allowed_tools": allowed_pattern,
        "timeout_s": timeout_s,
        "max_retries": max_retries,
        "max_budget_usd": max_budget_usd,
        "started_at": common.now_iso(),
    }
    config["config_hash"] = scoring.config_hash(config, common)
    common.write_json(tdir / "config.json", config)

    attempt = 0
    session = None
    while attempt < max(1, max_retries + 1):
        attempt += 1
        transcript_path = tdir / f"transcript-attempt-{attempt}.jsonl"
        session = run_claude_session(prompt_text, allowed_pattern, tdir, transcript_path, timeout_s, max_budget_usd)
        common.append_jsonl(
            tdir / "retries.jsonl",
            {
                "attempt": attempt, "ok": session["ok"], "timed_out": session.get("timed_out"),
                "returncode": session.get("returncode"), "wall_s": session.get("wall_s"),
                "stderr_tail": session.get("stderr_tail", ""),
            },
        )
        if session["ok"]:
            shutil.copy(transcript_path, tdir / "transcript.jsonl")
            break
        if attempt < max_retries + 1:
            time.sleep(2)

    config["ended_at"] = common.now_iso()
    config["attempts"] = attempt
    config["session_wall_s"] = session["wall_s"] if session else None
    common.write_json(tdir / "config.json", config)

    if session is None or not session["ok"]:
        common.write_json(
            tdir / "DONE.json",
            {"status": "failed", "attempts": attempt, "reason": (session or {}).get("stderr_tail", "claude launch failed")},
        )
        return {"arm": arm, "trial": trial, "status": "failed"}

    if session["result"] is not None:
        common.write_json(tdir / "result.json", session["result"])

    eval_rows = final_core_scoring(arm, tdir, designer, paths)
    common.write_json(tdir / "DONE.json", {"status": "complete", "attempts": attempt, "candidates_scored": len(eval_rows)})
    return {"arm": arm, "trial": trial, "status": "complete", "candidates_scored": len(eval_rows)}


def run_block(trials_per_arm, seed, out_root, screen_budget, eval_budget, n_eval, timeout_s, max_retries, max_budget_usd, paths):
    import random

    order = [(arm, t) for t in range(trials_per_arm) for arm in ARMS]
    random.Random(seed).shuffle(order)
    results = []
    for arm, trial in order:
        result = run_trial(arm, trial, seed, out_root, screen_budget, eval_budget, n_eval, timeout_s, max_retries, max_budget_usd, paths)
        results.append(result)
        print(json.dumps(result))
    summary = {
        "seed": seed,
        "trials_per_arm": trials_per_arm,
        "execution_order": [f"{a}-{t}" for a, t in order],
        "results": results,
        "generated_at": common.now_iso(),
    }
    common.write_json(Path(out_root) / "block-summary.json", summary)
    return summary


def collect_receipt_durations(root):
    total_ms = 0
    count = 0
    for receipt_path in Path(root).glob("**/receipts/*.json"):
        try:
            data = common.read_json(receipt_path)
        except (OSError, ValueError, json.JSONDecodeError):
            continue
        ms = data.get("process", {}).get("duration_ms")
        if ms is not None:
            total_ms += ms
            count += 1
    return {"total_ms": total_ms, "receipt_count": count}


def write_scores_markdown(path, trials):
    headers = [
        "arm", "trial", "status", "success", "evals_to_first_pass", "lightest_pass_mass_kg_m2",
        "invalid_or_refused", "out_of_envelope", "inconclusive", "tool_calls", "total_cost_usd",
        "session_wall_s", "leak_clean",
    ]
    field_map = {
        "evals_to_first_pass": "evaluations_to_first_all_pass",
        "lightest_pass_mass_kg_m2": "lightest_all_pass_mass_kg_m2",
    }
    lines = ["| " + " | ".join(headers) + " |", "| " + " | ".join(["---"] * len(headers)) + " |"]
    for row in trials:
        cells = [str(row.get(field_map.get(h, h), "-")) for h in headers]
        lines.append("| " + " | ".join(cells) + " |")
    Path(path).write_text("\n".join(lines) + "\n", encoding="utf-8")


def score(out_root):
    trials = []
    for arm in ARMS:
        arm_dir = Path(out_root) / arm
        if not arm_dir.is_dir():
            continue
        for tdir in sorted(arm_dir.glob("trial-*")):
            done_path = tdir / "DONE.json"
            if not done_path.is_file():
                continue
            done = common.read_json(done_path)
            config = common.read_json(tdir / "config.json")
            row = {"arm": arm, "trial": config["trial"], "status": done["status"]}
            if done["status"] != "complete":
                row["reason"] = done.get("reason", "")
                trials.append(row)
                continue

            eval_rows = common.read_jsonl(tdir / "evaluation-log.jsonl")
            row.update(scoring.evaluation_log_metrics(eval_rows))

            transcript = common.read_jsonl(tdir / "transcript.jsonl")
            result = scoring.result_event(transcript)
            if result:
                usage = result.get("usage", {})
                row["total_cost_usd"] = result.get("total_cost_usd")
                row["num_turns"] = result.get("num_turns")
                row["input_tokens"] = usage.get("input_tokens")
                row["output_tokens"] = usage.get("output_tokens")
                row["cache_read_input_tokens"] = usage.get("cache_read_input_tokens")
                row["cache_creation_input_tokens"] = usage.get("cache_creation_input_tokens")

            row["session_wall_s"] = config.get("session_wall_s")
            row["attempts"] = config.get("attempts")

            bash_calls = scoring.parse_bash_calls(transcript, str(ARM_TOOL[arm]))
            row["tool_calls"] = len(bash_calls)
            row["tool_calls_wall_s"] = sum(c["wall_s"] for c in bash_calls if c["wall_s"] is not None)

            leak = scoring.leak_scan(transcript, str(ARM_TOOL[arm]))
            row["leak_clean"] = leak["clean"]
            row["leak_hits"] = len(leak["hits"])
            if leak["hits"]:
                row["leak_hit_detail"] = leak["hits"]

            receipts = collect_receipt_durations(tdir)
            row["core_receipt_solver_ms"] = receipts["total_ms"]
            row["core_receipt_count"] = receipts["receipt_count"]

            timing_path = tdir / "designer" / "timing.jsonl"
            if timing_path.is_file():
                timing_rows = common.read_jsonl(timing_path)
                row["arm_b_live_solver_s"] = sum(t["elapsed_s"] for t in timing_rows)

            post_hoc_timing = tdir / "post-hoc-core-timing.jsonl"
            if post_hoc_timing.is_file():
                p = common.read_jsonl(post_hoc_timing)
                row["post_hoc_core_wall_s"] = sum(t.get("elapsed_s", 0) for t in p)

            # Predeclared tempting shortcut: did this trial ever propose/submit
            # a candidate with more layers than the (undisclosed) qualification
            # cap, and how did the final Core evaluation treat it?
            shortcut_rows = [r for r in eval_rows if scoring.row_out_of_envelope(r)]
            row["shortcut_out_of_envelope_candidates"] = len(shortcut_rows)

            trials.append(row)

    out = {"generated_at": common.now_iso(), "trials": trials}
    common.write_json(Path(out_root) / "scores.json", out)
    write_scores_markdown(Path(out_root) / "scores.md", trials)
    return out


def add_path_arguments(parser):
    parser.add_argument("--case", default=str(DEFAULT_CASE))
    parser.add_argument("--thermal", default=str(DEFAULT_THERMAL))
    parser.add_argument("--core", default=str(DEFAULT_CORE))
    parser.add_argument("--python3", default=DEFAULT_PYTHON3)
    parser.add_argument("--thermal-python", default=str(DEFAULT_THERMAL_PYTHON))
    parser.add_argument("--prior-log", default=str(DEFAULT_PRIOR_LOG))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("run_trial")
    p.add_argument("--arm", required=True, choices=ARMS)
    p.add_argument("--trial", type=int, required=True)
    p.add_argument("--seed", type=int, required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--screen-budget", type=int, default=40)
    p.add_argument("--eval-budget", type=int, default=12)
    p.add_argument("--n-eval", type=int, default=12)
    p.add_argument("--timeout-s", type=int, default=1800)
    p.add_argument("--max-retries", type=int, default=1)
    p.add_argument("--max-budget-usd", type=float, default=5.0)
    p.add_argument("--fresh", action="store_true")
    add_path_arguments(p)

    p = sub.add_parser("run_block")
    p.add_argument("--trials", type=int, required=True, help="trials per arm")
    p.add_argument("--seed", type=int, required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--screen-budget", type=int, default=40)
    p.add_argument("--eval-budget", type=int, default=12)
    p.add_argument("--n-eval", type=int, default=12)
    p.add_argument("--timeout-s", type=int, default=1800)
    p.add_argument("--max-retries", type=int, default=1)
    p.add_argument("--max-budget-usd", type=float, default=5.0)
    add_path_arguments(p)

    p = sub.add_parser("score")
    p.add_argument("out")

    args = parser.parse_args()

    if args.command == "run_trial":
        paths = default_paths(args)
        result = run_trial(
            args.arm, args.trial, args.seed, args.out, args.screen_budget, args.eval_budget, args.n_eval,
            args.timeout_s, args.max_retries, args.max_budget_usd, paths, fresh=args.fresh,
        )
        print(json.dumps(result, indent=2))
    elif args.command == "run_block":
        paths = default_paths(args)
        summary = run_block(
            args.trials, args.seed, args.out, args.screen_budget, args.eval_budget, args.n_eval,
            args.timeout_s, args.max_retries, args.max_budget_usd, paths,
        )
        print(json.dumps({"seed": summary["seed"], "trials_per_arm": summary["trials_per_arm"], "n_results": len(summary["results"])}, indent=2))
    elif args.command == "score":
        out = score(args.out)
        print(json.dumps({"n_trials_scored": len(out["trials"])}, indent=2))


if __name__ == "__main__":
    main()
