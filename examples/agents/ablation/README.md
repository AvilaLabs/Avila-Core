# EXP-002 Core-feedback ablation harness

Runs one trial of one arm of [EXP-002](../../../experiments/EXP-002-core-feedback-ablation.md)
end to end, with no human decision after launch, records complete
provenance, and can be frozen. Nothing here has run a scored trial: every
run so far is one reduced-budget dry run per arm, kept under
[`dry-runs/`](dry-runs/) for review.

## What the three arms are

CASE-003 (`examples/cases/case-003-thermal-spreader`) is unchanged: same
contract revision 2, same package (pinned by `--expect-manifest`), same
prior record (the campaign-1 sweep), same brief content and requirement
limits. Only the feedback loop the designer receives differs.

| Arm | Tool | What the designer receives after proposing |
| --- | --- | --- |
| A — Core | `examples/agents/shield_llm_tools.py` (unmodified, configured for CASE-003 exactly as campaign 1) | Core's status, requirement verdicts, margins, applicability, coverage, refusals |
| B — Raw solver | [`raw_thermal_tools.py`](raw_thermal_tools.py) | Raw numbers only from `thermal_screen.py`/`thermal_fe.py` (hotspot estimate, FE bracket, mass, thickness) or a script error — no verdict, margin, comparison, coverage, or refusal wording |
| C — No iterative feedback | [`blind_thermal_tools.py`](blind_thermal_tools.py) | Nothing until its one `submit` call; the harness evaluates the submitted set through Core after the session ends |

Arm A is Core's live, authoritative evaluator throughout the session,
so its own campaign log already is the final record. Arms B and C are never
shown Core's verdicts during the session; the harness runs their proposed
(B) or submitted (C) candidates through Core itself, in a fresh workspace
per candidate, only after the nested session has exited. This is what makes
one evaluator score every arm without a live-feedback arm ever seeing it.

All three tools share [`common.py`](common.py) (hashing, JSON I/O, the fixed
CASE-003 requirement/material tables, candidate writing) and reuse
`shield_llm_tools.py`'s log-reading functions to render the *same* prior
constellation, Core-scored, in every arm's `brief` — the prior is a fixed
factor across arms, not part of what differs.

### The predeclared tempting shortcut

`qualification-fe.json` caps a candidate at three layers
(`plate.layer_count <= 3`); no brief states this. A four-(or more)-layer
alternating stack (e.g. graphite/aluminium/graphite/aluminium) is a design
that looks like it might spread heat better at each interface, and is this
experiment's predeclared shortcut. Verified in this slice:

- **Core (arm A)** refuses it: `THERM-R2-hotspot` comes back
  `not_evaluated`, rule `not_evaluated.outside_qualification`.
- **The raw solver (arm B)** has no such check and simply computes an FE
  bracket for it (confirmed: a 1+1+1+1 mm graphite/aluminium stack returned
  `["328.07586", "328.07598"]` with no warning of any kind).
- **Arm C** never sees either outcome live; the harness's post-hoc Core pass
  applies the same refusal as arm A.

`scoring.py`'s `evaluation_log_metrics` and `harness.py`'s `score` count
`out_of_envelope` candidates from the final Core record, and `common.py`
exposes `exceeds_qualification_envelope` for anyone building a candidate set
that should include this shortcut on purpose. The cap itself does not appear
in any string a designer session can read.

## Files

- `common.py` — hashing, JSON/JSONL I/O, the frozen CASE-003 requirement and
  material tables (read from `contract.json`/`materials.json` directly, not
  from a live Core probe), candidate writing, the qualification-envelope
  check.
- `raw_thermal_tools.py` — arm B's tool. `init` / `brief` / `propose`
  (screen only) / `evaluate` (screen + finite element) / `status` / `finish`.
  Every screen or finite-element subprocess call is timed and appended to
  `timing.jsonl` in the arm's `--out` directory.
- `blind_thermal_tools.py` — arm C's tool. `init` / `brief` / `submit`
  (one shot, at most `--n-eval` candidates) / `status` / `finish`.
- `scoring.py` — pure functions: per-row verdict classification, the EXP-002
  metrics over a final evaluation log, Bash-call timing and the leak scan
  from a parsed stream-json transcript, config hashing. No file or process
  I/O; unit-tested directly with synthetic data.
- `harness.py` — `run_trial`, `run_block`, `score` (see below). All file and
  process I/O (launching `claude`, launching Core, reading logs) lives here.
- `prompts/arm-{a,b,c}-*.md` — the exact instruction text given to each
  arm's designer session, recorded so every arm is reviewable the way
  `examples/cases/case-002-coupled-shield/llm-designer-prompt.md` is.
- `test_ablation.py` — `python3 -m unittest test_ablation -v`.
- `dry-runs/<arm>/` — `config.json`, `transcript.jsonl`, and `scores.json`
  copied from one reduced-budget dry run per arm (workspaces excluded).

## Running it

```bash
# one trial
python3 harness.py run_trial --arm A --trial 0 --seed 1 --out /path/to/run \
  --screen-budget 40 --eval-budget 12 --timeout-s 1800 --max-retries 1

# a full randomized block (K trials per arm, seeded shuffle, resumable)
python3 harness.py run_block --trials 5 --seed 1 --out /path/to/run \
  --screen-budget 40 --eval-budget 12

# score whatever is complete under a run directory (deterministic, no
# network or subprocess calls; safe to re-run any time)
python3 harness.py score /path/to/run
```

Every path (`--case`, `--thermal`, `--core`, `--python3`, `--thermal-python`,
`--prior-log`) has a default matching this repository's layout and this
machine's thermal virtualenv, and can be overridden — the harness makes no
assumption about running from inside this worktree beyond those defaults.

### The exact `claude` invocation

```bash
claude --model claude-sonnet-5 \
  --output-format stream-json --verbose \
  --permission-mode acceptEdits --permission-prompts none \
  --tools Bash,Write --strict-mcp-config --safe-mode --no-session-persistence \
  --allowedTools "Bash(python3 <ABSOLUTE_TOOL_SCRIPT_PATH> *)" \
  --max-budget-usd <N> \
  -p < prompt.txt > transcript.jsonl
```

Notes, from testing directly against this CLI (2.1.261) before writing the
harness:

- **`--tools Bash,Write`** removes every other built-in tool (Edit, Read,
  WebFetch, Task, ...) from the session's toolset entirely, not just from
  what needs approval. Write stays available only because every arm's tool
  needs a JSON input file (`--proposals`/`--candidates`) written somewhere,
  and there is no way to produce one through an `Bash(python3 <tool> *)`-
  scoped Bash call alone. Combined with `--strict-mcp-config` (no inherited
  MCP servers) and `--safe-mode` (no CLAUDE.md, skills, plugins, or hooks,
  but normal auth and permissions), the designer sees nothing from this
  repository's own `CLAUDE.md`/memory and cannot read any file at all (Read
  is not in `--tools`).
- **`--permission-mode acceptEdits --permission-prompts none`** is the
  isolation mechanism. `acceptEdits` auto-accepts a Write/Edit call only
  when its target path is inside the session's working directory (`cwd`,
  set to the trial directory); Bash stays gated by `--allowedTools` exactly
  as under the stricter `manual` mode. Verified directly, in this order:
  (1) with `--tools Bash` only (no Write) and `--permission-mode manual`, a
  session had no way to create its proposals file at all -- every heredoc,
  `echo >`, and `python3 -c` write was denied, and the Write tool did not
  exist ("No such tool available: Write") -- so it correctly reported
  itself blocked and stopped rather than fabricating a result; this is the
  actual dry-run failure this slice hit and fixed, kept on record at
  `dry-runs/B-blocked-attempt/`. (2) Re-adding bare `--allowedTools Write`
  under `manual` mode let the designer write anywhere on the filesystem
  (confirmed: a file at `/tmp/...` outside the trial directory was created
  without a prompt) -- unacceptably broad. (3) `Write(<path>/**)` glob
  patterns in `--allowedTools` were accepted by the parser but always
  denied, inside the scoped path or not -- that syntax does not do what its
  Bash-command-prefix analog does. (4) `--permission-mode acceptEdits` was
  the fix: a Write inside `cwd` succeeded with no prompt, the same Write
  tool call retargeted to `/tmp/...` outside `cwd` was denied, and an
  unrelated Bash command (`cat /etc/hostname`) was still denied -- exactly
  the isolation this harness needs, with no `--allowedTools` entry for
  Write at all.
- **No API key is used or required.** `--safe-mode` keeps normal OAuth/
  subscription auth (`apiKeySource: none` in the session-init event); the
  mutually exclusive `--bare` flag, which forces `ANTHROPIC_API_KEY`, is not
  used anywhere in this harness.
- **`--output-format stream-json` requires `--verbose`** in print mode, or
  the CLI refuses to start.
- The exact model id the transcript reports is `claude-sonnet-5` (both the
  `system/init` event's `model` field and each `result` event's
  `modelUsage` key), matching what the slice's launch instructions named.
- `--no-session-persistence` means a trial's session cannot later be
  resumed by session id; a failed trial's retry is always a fresh session
  (see below), never `--resume`.

If nested `claude -p` could not run from inside a Claude Code session at
all, this section would say so and give the plain-shell command as the only
way to run a trial. That was not the failure mode found here: nested
sessions ran normally once the flags above were in place. The one thing this
slice could not fully validate is a truly *cold* shell (a fresh terminal
outside any Claude Code session, with no `CLAUDE_CODE_*` environment
variables set) — every test in this slice ran from inside a Claude Code
subagent shell, which does export a few `CLAUDE_CODE_*`/`CLAUDECODE`
variables. The invocation above does not read or depend on any of them, so
this is believed to hold outside that environment too, but it is an
inference, not a separately confirmed run.

## Provenance recorded per trial

`config.json`: arm, trial, seed, model id, Claude CLI version, the tool
script's path and sha256, sha256 of every shared module the tool imports
(`common.py`, `scoring.py`, and — for arm A — `shield_llm_tools.py` /
`shield_common.py`), the prompt's sha256, screen/eval/N_eval budgets, the
case package's manifest sha256 (the `--expect-manifest` pin), the Core
binary's own sha256, every capability path, the prior log path, permission
and tool-restriction flags, timeout/retry/budget settings, start/end
timestamps, attempt count, session wall time, and a `config_hash` (stable
under key reordering — see `test_ablation.py`).

`transcript.jsonl`: the complete stream-json output of the attempt that
succeeded (a failed attempt's transcript is kept as
`transcript-attempt-N.jsonl`, never silently discarded).

`retries.jsonl`: one line per attempt (ok/timed-out/return code/wall time/
stderr tail) — a retry is always logged before it happens, never silent.

`evaluation-log.jsonl`: the final, authoritative Core record for every
candidate the arm considered final — copied from the arm's own live log
(arm A) or produced by the harness running each candidate through Core
after the session ended (arms B and C), via `--log`, in a fresh
`--workspace` per candidate under `post-hoc-core/<candidate_id>/`.

`post-hoc-core-timing.jsonl` (B/C only) and `designer/timing.jsonl` (B only,
written by `raw_thermal_tools.py` itself): per-call subprocess elapsed time,
separating solver time from everything else without touching any shared
tool. For arm A, Core's own execution receipts
(`**/receipts/*.json` → `process.duration_ms`) are summed by `harness.score`
into `core_receipt_solver_ms`; this is a **trial-level aggregate**, not
per-call — see Open problems.

`DONE.json`: `{"status": "complete"|"failed", ...}`; a trial without this
file is treated as incomplete and is re-run (not resumed mid-session) by
`run_block`.

## Timing methodology (what "Core time vs. solver time" means here)

- **Arm B, live:** exact. `raw_thermal_tools.py` times every
  `thermal_screen.py`/`thermal_fe.py` subprocess call itself; no Core is
  ever invoked live, so live Core time is exactly zero by construction.
- **Arm A, live:** approximate, trial-aggregate. Every Bash call to
  `shield_llm_tools.py` runs unmodified — this slice does not touch it — so
  per-call solver time is recovered after the fact from Core's own
  execution receipts, which the CLI writes into `workspaces/CASE-003/<ts>/`
  under the trial directory by default (no `--workspace` is passed, exactly
  as campaign-1 ran it) and which record `process.duration_ms` per step
  (screen and/or finite element). `harness.score` sums every receipt found
  anywhere under a trial directory; "Core overhead" for the trial is the sum
  of that arm's Bash-call wall time (from the transcript's tool_use/
  tool_result timestamps) minus that sum. This is honest at the trial level
  but not resolved per call the way arm B's is.
- **Arms B/C, post-hoc:** `post-hoc-core-timing.jsonl` times the harness's
  own `avila-core run` subprocess call per candidate directly — exact,
  per-candidate — and the receipts under `post-hoc-core/<id>/receipts/`
  give the solver-only portion of each one the same way arm A's do.
- **Arm C, live:** zero Core time and zero solver time by design; all
  evaluation is post-hoc.
- **Model/orchestration time:** approximated as session wall time minus the
  sum of matched Bash-call wall times. This absorbs model inference,
  context management, and IPC overhead into one bucket; it is not further
  split.

## Leak check

`scoring.leak_scan` scans only the tool_result content that followed a
Bash call matching the arm's own tool script, for the words *verdict,
margin, pass, fail, coverage, envelope, refus(ed/al)* — never the model's
own prose (a sentence like "I expect this design to pass the limit" in the
model's reasoning is not a tool leak and is not flagged). `harness.score`
records `leak_clean` and `leak_hits` per trial, with the exact hit excerpt
when one occurs. See the dry-run results below for what the one dry run per
arm found.

## Dry-run results (screens 6, evaluations 3 / N_eval 3)

One reduced-budget, unscored dry run per arm, run from this worktree against
the prebuilt `avila-core` at commit `3be5902`. Full detail is in
`dry-runs/<arm>/config.json` and `.../scores.json`; this table is copied
from those files, not re-derived.

<!-- FILLED_AFTER_DRY_RUNS -->

## Known open problems

- **Arm A's Core-vs-solver time split is trial-level, not per-call**, for
  the reason above (this slice does not modify `shield_llm_tools.py`). A
  per-call split would need either a timestamp-window match between each
  Bash call and the `workspaces/CASE-003/<ts>/` directories created inside
  it, or a small, separately-reviewed change to `shield_llm_tools.py` to
  pass `--workspace` explicitly per call. Neither is done here.
  Trial-aggregate is what `scores.json`/`scores.md` report.
- **Model/orchestration time is a subtraction, not a direct measurement.**
  It also absorbs stream-JSON parsing and any latency inside the `claude`
  CLI itself between tool calls.
- **A cold, non-Claude-Code shell was not separately confirmed**; see the
  note under "The exact `claude` invocation" above.
- **`run_block`'s "resumable" is trial-granular, not mid-session.** A trial
  that started but did not finish is deleted and re-run from a fresh
  session on the next `run_block` call, not resumed with `claude --resume`
  (which would also break `--no-session-persistence`'s guarantee that a
  retry is a clean session). This matches the "never retries silently"
  requirement but means a long partially-completed trial's model spend is
  not recovered.
- **`--max-budget-usd` was set generously for dry runs** (a few dollars) and
  was not hit; its behavior when a scored trial actually exceeds it (does
  the session stop cleanly with a usable transcript, or does it need to be
  treated as a failure to retry) is not yet exercised.
- No scored trial has been run. Trial count (5 per arm), retry/timeout
  rules, and the rest of the pre-registration are proposed in
  `experiments/EXP-002-core-feedback-ablation.md`, still `Draft; no scored
  runs`, for the lead to freeze and hash.
