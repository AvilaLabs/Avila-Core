#!/usr/bin/env python3
"""EXP-002/EXP-005 Core-feedback ablation harness.

Launches one headless Claude Code designer session per trial against one of
three arms of a case profile (see `examples/agents/ablation/README.md`),
records complete provenance, and scores the result through Avila Core. No
trial in this file runs Avila Core, a capability script, or `claude` except
through an explicit, logged subprocess call; nothing here fabricates a
verdict or a physical number.

    run_trial --case-id CASE-003|CASE-002 --arm A|B|C --trial K --seed S --out DIR
    run_block --case-id CASE-003|CASE-002 --trials K --seed S --out DIR
    score DIR

`--case-id` selects one of `CASE_PROFILES` below (default `CASE-003`, so
every EXP-002 invocation and unit test that predates this flag is
unaffected). CASE-003 is the original thermal-spreader ablation; CASE-002 is
EXP-005's coupled-shielding repeat, on a harder case with a predeclared
tempting shortcut (see `common.SHIELD_LAYER_SHORTCUT_SENTENCE`). See the
module-level constants below for default paths (the case, the capability
scripts and interpreters, the Core binary, the nuclear-data/ACTINV roots).
Every one can be overridden on the command line so this harness runs
unchanged from a plain shell outside this worktree, given the same files.
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

# Arm A's tool (shield_llm_tools.py) is launched only as a subprocess below,
# exactly like the other arms' tools -- this file does not import it, on
# either case. `final_core_scoring` used to reuse its `transported()` helper
# to filter arm A's own campaign-log.jsonl; that helper is written for the
# pre-v0.3 `steps` shape (a `[name, state]` pair) and always returns False
# against Core's current dict-shaped steps, so this file uses its own
# `scoring.row_transported` instead (see that function's docstring).

ABLATION_DIR = Path(__file__).resolve().parent
AGENTS_DIR = ABLATION_DIR.parent
REPO_ROOT = AGENTS_DIR.parent.parent  # examples/agents/ablation -> examples/agents -> examples -> repo root

# ---------------------------------------------------------------------
# CASE-003 (thermal spreader, EXP-002) defaults -- unchanged from EXP-002.
# ---------------------------------------------------------------------
DEFAULT_CASE = REPO_ROOT / "examples/cases/case-003-thermal-spreader"
DEFAULT_THERMAL = REPO_ROOT / "examples/capabilities/thermal"
DEFAULT_CORE = Path("/home/connoravila/Documents/Avila-Labs/project-north-star/target/debug/avila-core")
DEFAULT_THERMAL_PYTHON = Path("/home/connoravila/.venvs/thermal/bin/python")
DEFAULT_PYTHON3 = "/usr/bin/python3"
DEFAULT_PRIOR_LOG = DEFAULT_CASE / "campaign-1/sweep/campaign-log.jsonl"

# ---------------------------------------------------------------------
# CASE-002 (coupled shielding, EXP-005) defaults, matching this machine's
# layout as recorded in `examples/cases/case-002-coupled-shield/
# run_campaign_rev3.sh` and its README's "Running it" section.
# ---------------------------------------------------------------------
DEFAULT_CASE_002 = REPO_ROOT / "examples/cases/case-002-coupled-shield"
DEFAULT_SHIELD_COUPLED = REPO_ROOT / "examples/capabilities/shield-coupled"
DEFAULT_SHIELDING = REPO_ROOT / "examples/capabilities/shielding"
DEFAULT_NUCLEAR_DATA = Path("/home/connoravila/nuclear-data/endfb-vii.1-hdf5")
DEFAULT_OPENMC_PYTHON = Path("/home/connoravila/.venvs/w003env/bin/python3.12")
DEFAULT_ACTINV_RELEASE = Path("/home/connoravila/Documents/actinv/target/release")
DEFAULT_ACTINV_DATA = Path("/home/connoravila/Documents/Avila-Labs/project-aftermatter/.data/actinv/v1.0.0")
# EXP-005's prior is deliberately narrow: the campaign-3 CONTROL SWEEP log
# only, not the full campaign-1/campaign-2/campaign-3 prior list
# `run_campaign_rev3.sh`'s own LLM arm read. In particular this excludes
# `campaign-rev3/llm/campaign-log.jsonl`, whose own designer found a
# 1277.0 kg design no arm here should start already knowing about. See
# `experiments/EXP-005-refusal-during-search.md` for why 1554.5 kg/m^2 (the
# sweep's lightest all-PASS point, not 1601.5 or the unseen 1277.0) is the
# bar this experiment reports against.
DEFAULT_PRIOR_LOG_002 = DEFAULT_CASE_002 / "campaign-rev3/sweep/campaign-log.jsonl"

# CASE-002's contract pins these transport parameters
# (`contract.json`'s `workflow[].parameters`/`reproducibility.seed` for the
# `transport` step). They are not exposed as CLI flags so a trial cannot
# silently drift from what the contract itself declares; the case's own
# `run_campaign_rev3.sh` does not expose them either.
SHIELD_TRANSPORT_PARTICLES = 500000
SHIELD_TRANSPORT_BATCHES = 10
SHIELD_TRANSPORT_SEED = 1
SHIELD_TRANSPORT_THREADS = 8  # OMP_NUM_THREADS, matching every CASE-002 campaign so far

MODEL_ID = "claude-sonnet-5"
CLAUDE_BIN = "claude"

SHIELD_TOOL = AGENTS_DIR / "shield_llm_tools.py"  # arm A, both cases, unmodified

RAW_TOOL = ABLATION_DIR / "raw_thermal_tools.py"
BLIND_TOOL = ABLATION_DIR / "blind_thermal_tools.py"
RAW_SHIELD_TOOL = ABLATION_DIR / "raw_shield_tools.py"
BLIND_SHIELD_TOOL = ABLATION_DIR / "blind_shield_tools.py"

ARMS = ("A", "B", "C")
CASE_IDS = ("CASE-003", "CASE-002")

# The case-driven profile: everything that differs between CASE-003's
# thermal ablation and CASE-002's coupled-shielding one, apart from the
# operator-supplied filesystem paths in `default_paths` below. Requirement
# ids are informational/provenance only here -- every scoring function in
# `scoring.py` reads a verdict's own unit/rule rather than assuming a
# case's requirement-id set, so nothing branches on this list at runtime.
CASE_PROFILES = {
    "CASE-003": {
        "requirement_ids": ["THERM-R1-screen", "THERM-R2-hotspot", "THERM-R3-mass", "THERM-R4-thickness"],
        "candidate_schema": "avila.thermal/candidate/v1",
        "thickness_key": "thickness_mm",
        "eval_stage_name": "evaluate",  # arm B/C designer-notes.jsonl "stage" for a full evaluation
        "raw_tool": RAW_TOOL,
        "blind_tool": BLIND_TOOL,
        "prompts": {
            "A": ABLATION_DIR / "prompts/arm-a-core.md",
            "B": ABLATION_DIR / "prompts/arm-b-raw-solver.md",
            "C": ABLATION_DIR / "prompts/arm-c-blind.md",
        },
    },
    "CASE-002": {
        "requirement_ids": [
            "SHIELD-R1-screen", "SHIELD-R2-neutron", "SHIELD-R3-photon",
            "SHIELD-R4-mass", "SHIELD-R5-thickness", "SHIELD-R6-activation",
        ],
        "candidate_schema": "avila.shielding/candidate/v1",
        "thickness_key": "thickness_cm",
        "eval_stage_name": "evaluate",
        "raw_tool": RAW_SHIELD_TOOL,
        "blind_tool": BLIND_SHIELD_TOOL,
        "prompts": {
            "A": ABLATION_DIR / "prompts/arm-a-shield.md",
            "B": ABLATION_DIR / "prompts/arm-b-shield-raw.md",
            "C": ABLATION_DIR / "prompts/arm-c-shield-blind.md",
        },
    },
}


def arm_tool_path(case_id, arm):
    if arm == "A":
        return SHIELD_TOOL
    profile = CASE_PROFILES[case_id]
    return profile["raw_tool"] if arm == "B" else profile["blind_tool"]


def default_paths(args):
    """Resolve every filesystem path and environment override this trial
    needs, keyed by `args.case_id`. CASE-003's returned keys and defaults
    are byte-for-byte what EXP-002's harness always returned (plus the new,
    always-present but here-empty `case_id`/`environment` keys), so a
    CASE-003 invocation with no `--case-id` is unaffected by this
    generalization."""
    env_overrides = {}
    for spec in args.env:
        key, _, value = spec.partition("=")
        env_overrides[key] = value

    if args.case_id == "CASE-003":
        return {
            "case_id": "CASE-003",
            "case": Path(args.case).resolve() if args.case else DEFAULT_CASE,
            "thermal": Path(args.thermal).resolve(),
            "core": Path(args.core).resolve(),
            "python3": args.python3,
            "thermal_python": Path(args.thermal_python).resolve(),
            "prior_log": Path(args.prior_log).resolve() if args.prior_log else DEFAULT_PRIOR_LOG,
            "environment": env_overrides,
        }

    nuclear_data = Path(args.nuclear_data).resolve()
    # OPENMC_CROSS_SECTIONS is the one env key CASE-002's live tool and
    # every raw/post-hoc Core call needs (transport.py refuses to run
    # without it); default it from --nuclear-data unless the operator
    # overrode it explicitly via --env, per this slice's "env keys such as
    # OPENMC_CROSS_SECTIONS via the operator's --env" requirement.
    environment = {"OPENMC_CROSS_SECTIONS": env_overrides.pop("OPENMC_CROSS_SECTIONS", str(nuclear_data / "cross_sections.xml"))}
    environment.update(env_overrides)
    return {
        "case_id": "CASE-002",
        "case": Path(args.case).resolve() if args.case else DEFAULT_CASE_002,
        "shield_coupled": Path(args.shield_coupled).resolve(),
        "shielding": Path(args.shielding).resolve(),
        "core": Path(args.core).resolve(),
        "python3": args.python3,
        "openmc_python": Path(args.openmc_python).resolve(),
        "nuclear_data": nuclear_data,
        "actinv_release": Path(args.actinv_release).resolve(),
        "actinv_data": Path(args.actinv_data).resolve(),
        "prior_log": Path(args.prior_log).resolve() if args.prior_log else DEFAULT_PRIOR_LOG_002,
        "environment": environment,
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
    case_id = paths["case_id"]
    profile = CASE_PROFILES[case_id]

    if arm == "A" and case_id == "CASE-003":
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
            "--candidate-schema", profile["candidate_schema"],
            "--thickness-key", profile["thickness_key"],
            "--probe-thickness", "5",
        ]
    elif arm == "A":  # CASE-002, same tool, the coupled shielding roots
        cmd = [
            sys.executable, str(SHIELD_TOOL), "init",
            "--out", str(designer),
            "--core", str(paths["core"]),
            "--case", str(paths["case"]),
            "--materials", str(paths["shield_coupled"]),
            "--source-root", f"case={paths['case']}",
            "--source-root", f"shielding={paths['shielding']}",
            "--source-root", f"coupled={paths['shield_coupled']}",
            "--source-root", f"agents={AGENTS_DIR}",
            "--source-root", f"nuclear-data={paths['nuclear_data']}",
            "--source-root", f"actinv-release={paths['actinv_release']}",
            "--source-root", f"actinv-data={paths['actinv_data']}",
            "--python3", paths["python3"],
            "--openmc-python", str(paths["openmc_python"]),
            "--cross-sections", paths["environment"]["OPENMC_CROSS_SECTIONS"],
            "--prior-log", str(paths["prior_log"]),
            "--screen-budget", str(screen_budget),
            "--transport-budget", str(eval_budget),
            "--candidate-schema", profile["candidate_schema"],
            "--thickness-key", profile["thickness_key"],
            "--probe-thickness", "5",
        ]
        for key, value in paths["environment"].items():
            if key != "OPENMC_CROSS_SECTIONS":
                cmd += ["--env", f"{key}={value}"]
    elif arm == "B" and case_id == "CASE-003":
        cmd = [
            sys.executable, str(profile["raw_tool"]), "init",
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
    elif arm == "B":  # CASE-002
        cmd = [
            sys.executable, str(profile["raw_tool"]), "init",
            "--out", str(designer),
            "--case", str(paths["case"]),
            "--materials", str(paths["shield_coupled"] / "materials.json"),
            "--source", str(paths["shield_coupled"] / "source.json"),
            "--groups", str(paths["shield_coupled"] / "fispact-709-groups.json"),
            "--schedule", str(paths["case"] / "schedule.json"),
            "--screen-script", str(paths["shielding"] / "screen.py"),
            "--transport-script", str(paths["shield_coupled"] / "transport.py"),
            "--activate-script", str(paths["shield_coupled"] / "activate.py"),
            "--python3", paths["python3"],
            "--openmc-python", str(paths["openmc_python"]),
            "--cross-sections-index", str(paths["nuclear_data"] / "cross_sections.xml"),
            "--openmc-cross-sections", paths["environment"]["OPENMC_CROSS_SECTIONS"],
            "--actinv", str(paths["actinv_release"] / "actinv"),
            "--activation-library", str(paths["actinv_data"] / "activation/tendl-2025-neutron-709g.npz"),
            "--activation-index", str(paths["actinv_data"] / "activation/tendl-2025-neutron-709g_index.json"),
            "--decay-primary", str(paths["actinv_data"] / "decay/endf-b-viii-0_decay.dat"),
            "--decay-fallback", str(paths["actinv_data"] / "decay/jeff-3-3_decay.dat"),
            "--particles", str(SHIELD_TRANSPORT_PARTICLES),
            "--batches", str(SHIELD_TRANSPORT_BATCHES),
            "--seed", str(SHIELD_TRANSPORT_SEED),
            "--prior-log", str(paths["prior_log"]),
            "--screen-budget", str(screen_budget),
            "--eval-budget", str(eval_budget),
        ]
    elif case_id == "CASE-003":  # arm C
        cmd = [
            sys.executable, str(profile["blind_tool"]), "init",
            "--out", str(designer),
            "--case", str(paths["case"]),
            "--materials", str(paths["thermal"] / "materials.json"),
            "--prior-log", str(paths["prior_log"]),
            "--n-eval", str(n_eval),
        ]
    else:  # arm C, CASE-002
        cmd = [
            sys.executable, str(profile["blind_tool"]), "init",
            "--out", str(designer),
            "--case", str(paths["case"]),
            "--materials", str(paths["shield_coupled"] / "materials.json"),
            "--prior-log", str(paths["prior_log"]),
            "--n-eval", str(n_eval),
        ]

    completed = subprocess.run(cmd, capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(f"tool init failed for arm {arm}: {completed.stderr}")
    return designer


def build_prompt(case_id, arm, trial_dir, tool_path, eval_budget, case_path, n_eval):
    template_path = CASE_PROFILES[case_id]["prompts"][arm]
    template = template_path.read_text(encoding="utf-8")
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


def core_run_common_args(paths):
    """Source roots, capabilities, and `--env` flags shared by every
    Core invocation for this case's post-hoc pass, so arm B/C's post-hoc
    evaluation binds exactly what the arm's own live tool (arm A) binds.
    CASE-003's list is byte-for-byte what EXP-002's `run_core_for_candidate`
    always built inline."""
    if paths["case_id"] == "CASE-003":
        return [
            "--source-root", f"case={paths['case']}",
            "--source-root", f"thermal={paths['thermal']}",
            "--capability", f"python3={paths['python3']}",
            "--capability", f"thermal-python={paths['thermal_python']}",
        ]
    args = [
        "--source-root", f"case={paths['case']}",
        "--source-root", f"shielding={paths['shielding']}",
        "--source-root", f"coupled={paths['shield_coupled']}",
        "--source-root", f"agents={AGENTS_DIR}",
        "--source-root", f"nuclear-data={paths['nuclear_data']}",
        "--source-root", f"actinv-release={paths['actinv_release']}",
        "--source-root", f"actinv-data={paths['actinv_data']}",
        "--capability", f"python3={paths['python3']}",
        "--capability", f"openmc-python={paths['openmc_python']}",
    ]
    for key, value in paths["environment"].items():
        args += ["--env", f"{key}={value}"]
    return args


def run_core_for_candidate(paths, candidate_path, workspace_dir, log_path, manifest_sha256):
    cmd = [str(paths["core"]), "run", str(paths["case"]), "--json"]
    cmd += core_run_common_args(paths)
    cmd += [
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
    profile = CASE_PROFILES[paths["case_id"]]
    log_path = trial_dir / "evaluation-log.jsonl"
    manifest_sha256 = common.sha256_file(paths["case"] / "package.json")

    if arm == "A":
        rows = common.read_jsonl(designer / "campaign-log.jsonl")
        # scoring.row_transported, not core_tools.transported: see its
        # docstring for the confirmed schema mismatch that makes the shared
        # helper always return False against a freshly-produced log.
        final_rows = [r for r in rows if scoring.row_transported(r)]
        for row in final_rows:
            common.append_jsonl(log_path, row)
        return final_rows

    if arm == "B":
        notes = common.read_jsonl(designer / "designer-notes.jsonl")
        ids = []
        for row in notes:
            if row["stage"] == profile["eval_stage_name"] and row["candidate_id"] not in ids:
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


def build_trial_config(arm, trial, seed, tool_path, prompt_text, screen_budget, eval_budget, n_eval, paths, allowed_pattern, timeout_s, max_retries, max_budget_usd):
    case_id = paths["case_id"]
    config = {
        "case_id": case_id,
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
        "prompt_template": str(CASE_PROFILES[case_id]["prompts"][arm]),
        "prompt_sha256": common.sha256_text(prompt_text),
        "screen_budget": screen_budget,
        "eval_budget": eval_budget,
        "n_eval": n_eval if arm == "C" else None,
        "manifest_sha256": common.sha256_file(paths["case"] / "package.json"),
        "case": str(paths["case"]),
        "core_binary": str(paths["core"]),
        "core_binary_sha256": common.sha256_file(paths["core"]),
        "python3": paths["python3"],
        "prior_log": str(paths["prior_log"]),
        "environment": dict(paths.get("environment") or {}),
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
    if case_id == "CASE-003":
        config["thermal"] = str(paths["thermal"])
        config["thermal_python"] = str(paths["thermal_python"])
    else:
        config["shield_coupled"] = str(paths["shield_coupled"])
        config["shielding"] = str(paths["shielding"])
        config["openmc_python"] = str(paths["openmc_python"])
        config["nuclear_data"] = str(paths["nuclear_data"])
        config["actinv_release"] = str(paths["actinv_release"])
        config["actinv_data"] = str(paths["actinv_data"])
        config["transport_particles"] = SHIELD_TRANSPORT_PARTICLES
        config["transport_batches"] = SHIELD_TRANSPORT_BATCHES
        config["transport_seed"] = SHIELD_TRANSPORT_SEED
    return config


def run_trial(arm, trial, seed, out_root, screen_budget, eval_budget, n_eval, timeout_s, max_retries, max_budget_usd, paths, fresh=False):
    tdir = trial_dir_path(out_root, arm, trial)
    if is_done(tdir) and not fresh:
        return {"arm": arm, "trial": trial, "status": "skipped-already-done"}
    if tdir.exists():
        shutil.rmtree(tdir)
    tdir.mkdir(parents=True)

    case_id = paths["case_id"]
    tool_path = arm_tool_path(case_id, arm)
    designer = init_arm_tool(arm, tdir, screen_budget, eval_budget, n_eval, paths)
    prompt_text = build_prompt(case_id, arm, tdir, tool_path, eval_budget, paths["case"], n_eval)
    allowed_pattern = f"Bash(python3 {tool_path} *)"

    config = build_trial_config(
        arm, trial, seed, tool_path, prompt_text, screen_budget, eval_budget, n_eval, paths,
        allowed_pattern, timeout_s, max_retries, max_budget_usd,
    )
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
        "case_id": paths["case_id"],
        "seed": seed,
        "trials_per_arm": trials_per_arm,
        "execution_order": [f"{a}-{t}" for a, t in order],
        "results": results,
        "generated_at": common.now_iso(),
    }
    common.write_json(Path(out_root) / "block-summary.json", summary)
    return summary


def collect_receipt_durations(root):
    """Sum `process.duration_ms` over every execution receipt under `root`.

    Bug fixed in this slice (recorded as a limitation in EXP-002's results):
    this used to glob `**/receipts/*.json`, a path shape that does not
    exist anywhere a live or post-hoc Core run actually writes a receipt.
    Both arm A's live workspaces (`workspaces/<CASE_ID>/<timestamp>/<step>/
    receipt.json`) and arm B/C's post-hoc pass (`post-hoc-core/<candidate>/
    <step>/receipt.json`) write one `receipt.json` file directly inside
    each step's own directory -- confirmed against the archived
    `campaign-2-ablation/block-1` trial directories, e.g.
    `.../workspaces/CASE-003/<ts>/fe/receipt.json` and
    `.../post-hoc-core/b-0000/fe/receipt.json` -- so every receipt was
    silently skipped and `core_receipt_count`/`core_receipt_solver_ms` were
    zero for every trial in that block, exactly as its limitations section
    says. The corrected pattern below matches both locations for both
    cases, since it depends only on the step-directory layout Core itself
    writes, not on this file's `root` naming."""
    total_ms = 0
    count = 0
    for receipt_path in Path(root).glob("**/receipt.json"):
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

            # The tool path each trial actually ran, straight from its own
            # recorded config -- works for either case without this function
            # needing to know which one it is.
            tool_script = config.get("tool_script", str(arm_tool_path(config.get("case_id", "CASE-003"), arm)))
            bash_calls = scoring.parse_bash_calls(transcript, tool_script)
            row["tool_calls"] = len(bash_calls)
            row["tool_calls_wall_s"] = sum(c["wall_s"] for c in bash_calls if c["wall_s"] is not None)

            leak = scoring.leak_scan(transcript, tool_script)
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

            # EXP-005: whether this arm's own final claimed-best candidate is
            # all-PASS, whether it is itself the out-of-envelope shortcut (the
            # arm "succeeded" with a design Core refuses), and whether the
            # search continued past a refusal to a valid in-envelope design
            # afterward -- the refusal-during-search measure proper for arm A.
            row.update(scoring.refusal_recovery_metrics(eval_rows))

            trials.append(row)

    out = {"generated_at": common.now_iso(), "trials": trials}
    common.write_json(Path(out_root) / "scores.json", out)
    write_scores_markdown(Path(out_root) / "scores.md", trials)
    return out


def add_path_arguments(parser):
    parser.add_argument(
        "--case-id", choices=CASE_IDS, default="CASE-003",
        help="which case profile this trial/block runs: CASE-003 (thermal, EXP-002) or CASE-002 (coupled shielding, EXP-005)",
    )
    parser.add_argument("--case", default=None, help="case package directory; defaults to the --case-id's own case directory")
    parser.add_argument("--core", default=str(DEFAULT_CORE))
    parser.add_argument("--python3", default=DEFAULT_PYTHON3)
    parser.add_argument("--prior-log", default=None, help="prior campaign log; defaults per --case-id")
    parser.add_argument(
        "--env", action="append", default=[], metavar="KEY=VALUE",
        help="environment for the full-evaluation step (e.g. OPENMC_CROSS_SECTIONS=...); repeatable; "
             "CASE-002 fills in OPENMC_CROSS_SECTIONS from --nuclear-data when this omits it",
    )
    # CASE-003 (thermal) paths -- unchanged from EXP-002.
    parser.add_argument("--thermal", default=str(DEFAULT_THERMAL))
    parser.add_argument("--thermal-python", default=str(DEFAULT_THERMAL_PYTHON))
    # CASE-002 (coupled shielding) paths.
    parser.add_argument("--shield-coupled", default=str(DEFAULT_SHIELD_COUPLED))
    parser.add_argument("--shielding", default=str(DEFAULT_SHIELDING))
    parser.add_argument("--openmc-python", default=str(DEFAULT_OPENMC_PYTHON))
    parser.add_argument("--nuclear-data", default=str(DEFAULT_NUCLEAR_DATA))
    parser.add_argument("--actinv-release", default=str(DEFAULT_ACTINV_RELEASE))
    parser.add_argument("--actinv-data", default=str(DEFAULT_ACTINV_DATA))


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
