# EXP-002 — Core feedback ablation

**Status:** Draft; no scored runs  
**Priority:** P0  
**Case:** Reuse CASE-003 initially  
**Purpose:** Isolate what Core contributes to an otherwise matched agent search

## Question

Given the same engineering problem, prior information, models, candidate
space, solver, evaluation budget, and stopping rules, does Core's structured
feedback change agent performance relative to raw solver output or no
iterative evaluation feedback?

This experiment must reuse an existing case. It should not require building a
new physics domain.

## Arms

| Arm | Agent receives after proposing a candidate | What it isolates |
| --- | --- | --- |
| A — Core | Core status, requirement verdicts, margins, applicability, coverage, refusals, and prior campaign record | Full Core-assisted workflow |
| B — Raw solver | The same underlying solver outputs, such as hotspot temperature and mass, without Core verdicts, margins, coverage, or refusal semantics | Structured Core feedback versus ordinary numerical feedback |
| C — No iterative feedback | No evaluation result until the arm submits its final set of candidates | Value of iterative evaluation feedback at all |

A conventional optimizer is useful but answers a different question; track it
separately rather than treating it as the no-Core agent control.

## Fixed factors

Before scored execution, freeze:

- **the designer and orchestrator identity, resolved:** in every arm, the
  designer is one flat, headless `claude -p` Sonnet session that both
  reasons about the design and calls the arm's tool script directly —
  there is no separate Fable-orchestrator process — exactly how the
  CASE-003 campaign-1 designer ran (a single Claude Code subagent with
  shell access), not a two-tier orchestrator/worker split. The
  orchestrator across all three arms is the deterministic Python harness
  (`examples/agents/ablation/harness.py`), not a model: it launches each
  session, enforces the fixed budgets and prompt, and does all scoring
  after the session exits. This replaces an earlier draft of this line
  that named "the exact Fable orchestrator and Sonnet worker versions" as
  a single pair to freeze, which did not match what the harness actually
  runs; see "Exact model and prompt identities" below for the full
  identity (CLI version, invocation flags, model id) that is actually
  fixed and recorded per trial;
- the exact Sonnet worker version, sampling settings, prompts, and tool
  permissions;
- fresh context for every trial and no memory transfer between arms;
- the CASE-003 contract revision, candidate space, material table, starting
  campaign record, and whole-millimetre discretization;
- the same full-evaluation budget and candidate-proposal budget;
- the same finite-element implementation and machine environment;
- the success criterion and objective ordering;
- the information exposed before the first proposal.

The arm label and feedback adapter may differ. Nothing else may differ.

## Execution design

1. Build one harness that launches every arm, records all model and tool
   traffic, and requires no human decision after launch.
2. Run one unscored dry run per arm to validate isolation and logging.
3. Freeze the harness and identities.
4. Run repeated scored trials in randomized arm order. Choose the trial count
   before viewing scored results.
5. Evaluate every arm's final candidates through Core so final measurements
   use one authoritative evaluator. Arm B must not see Core's interpretation
   during its search.
6. Preserve failures, retries, timeouts, and human interventions as results.

CASE-003 may produce a ceiling effect because it is easy. If all arms solve it
immediately, record that EXP-002 is non-discriminating and repeat the frozen
comparison on a harder existing case rather than changing CASE-003 after
seeing results.

## Primary measurements

- trial success rate within the fixed budget;
- full evaluations to first all-PASS candidate;
- lightest all-PASS candidate at the fixed budget;
- invalid, refused, inconclusive, and out-of-envelope proposals;
- wall time, Core time, solver time, model time, retries, human interventions,
  token use, and monetary cost;
- whether each arm leaves enough provenance to reproduce the final verdict.

## Interpretation rules

- A beating B would support an incremental performance benefit from Core's
  structured verdict and governance layer.
- B beating C would support the value of iterative numerical feedback, not
  specifically Core.
- A matching B on optimization but producing fewer invalid candidates or a
  stronger reproducible record would support a governance/provenance benefit,
  not an optimization benefit.
- No difference on CASE-003 may mean Core adds no measurable benefit here or
  that the case is too easy to distinguish the arms. Do not choose between
  those explanations without a harder frozen benchmark.
- One successful trial is descriptive only; it is not a reliability claim.

## Pre-registration items still required

Drafted by the harness builder (`examples/agents/ablation/`) from one
reduced-budget dry run per arm; nothing below has been exercised at scored
budgets or trial count. The lead freezes and hashes this section before any
scored run begins.

- **Exact model and prompt identities.** Designer model: `claude-sonnet-5`,
  confirmed as the exact id both the session-init event and every `result`
  event's `modelUsage` key report, via Claude Code CLI 2.1.261, invoked
  headless (`claude -p --output-format stream-json`, no API key). There is
  no separate Fable-orchestrator process in this implementation: one
  headless Sonnet session both reasons about the design and calls the arm's
  tool script directly, matching how the campaign-1 designer ran (a single
  Claude Code subagent with shell access) rather than a two-tier
  orchestrator/worker split — resolved as the fixed factor in "Fixed
  factors" above. Each arm's exact prompt is a frozen file under
  `examples/agents/ablation/prompts/`; its sha256 is recorded per trial in
  `config.json`. Every dry-run transcript's `modelUsage` also carries a
  small `claude-haiku-4-5-20251001` entry (a few thousand direct input
  tokens, under 25 output tokens, no cache activity, ~$0.002-0.003) in
  every arm; every tool_use/text block from the assistant is attributed to
  `claude-sonnet-5`, so this reads as Claude Code's own internal headless-
  session housekeeping (e.g. the transcript's session title) rather than
  designer reasoning, but this slice did not trace its exact purpose
  further. It is small and identical in kind across all three arms, so it
  does not bear on the ablation's comparison, but the lead should know it
  is there before treating `usage.*`/`total_cost_usd` as pure Sonnet spend.
- **Scored trial count.** Proposed: **5 trials per arm (15 total)**, chosen
  before viewing any scored result, per `experiments/README.md`'s rule for
  stochastic agent arms.
- **Fixed screen and full-evaluation budgets for all arms.** Screens: 40.
  Full evaluations: 12. Arm C's one-shot final set: at most `N_eval = 12`
  candidates, submitted in its one `submit` call.
- **Retry, timeout, and failure-handling rules.** Wall-clock timeout: 1800 s
  per designer session (`--timeout-s`). On a non-success session (nonzero
  exit, timeout, or a stream with no well-formed final `success` `result`
  event) the harness retries once (two attempts total) with a **fresh**
  session — never `--resume`, since sessions run with
  `--no-session-persistence` — and logs every attempt to `retries.jsonl`
  before the next one starts, never silently. A trial that fails both
  attempts is recorded `DONE.json: {"status": "failed"}` with every attempt's
  transcript and stderr preserved, is not retried further automatically,
  and is excluded from the success-rate numerator but kept in the record
  with its failure reason (`experiments/README.md`: "preserve failed and
  amended runs").
- **Randomization procedure.** `random.Random(seed).shuffle` over the full
  cross product of arm × local trial index (`run_block`); one seed fixed
  before any scored trial runs and recorded verbatim in
  `block-summary.json`. Execution is strictly sequential, never concurrent.
- **Precise wall-time and cost instrumentation.** Session wall time: the
  designer subprocess's own start-to-exit time (`config.json`
  `session_wall_s`). Tokens and cost: copied verbatim from the stream's
  final `result` event (`total_cost_usd`, `usage.*`) — never recomputed.
  Core time vs. solver time: exact and per-call for arm B (the tool's own
  `timing.jsonl`) and for every arm's post-hoc Core pass
  (`post-hoc-core-timing.jsonl`, plus each call's own execution receipts);
  a **trial-level aggregate only** for arm A's live Core calls, since arm A
  reuses `shield_llm_tools.py` unmodified and per-call attribution would
  need either a timestamp-window match to Core's own
  `workspaces/CASE-003/<ts>/` directories or a reviewed change to that
  shared tool, neither done in this slice. See
  `examples/agents/ablation/README.md`'s "Timing methodology" section for
  the exact method and its stated limits.
- **Immutable harness and case commit.** This worktree's harness
  (`examples/agents/ablation/`) at the commit the lead freezes; CASE-003
  contract revision 2, package manifest
  `sha256:af9667f6dcdcb23e369f04fa06e8a667c02b8703cece4468c45089b2df70bfd0`
  (enforced on every Core invocation, live or post-hoc, via
  `--expect-manifest`); the `avila-core` CLI binary's own sha256 recorded
  per trial in `config.json` (`core_binary_sha256`) — the lead should
  confirm which build is frozen for the scored run rather than assuming the
  commit this dry run used.
  **Correction to an earlier draft of this line:** this is *not* the
  manifest campaign-1 was blessed against, and is not "unchanged from
  campaign-1" as a previous draft of this section claimed. `package.json`'s
  raw bytes legitimately changed after campaign-1 (CASE-003 revision 2,
  then the A1/A2 slices — "Add require_qualification execution policy" and
  "Add registry-declared candidate schemas and free-input validation"), so
  its sha256 changed too; `sha256:9634fcc1a39e9...158216f`, still recorded
  in `campaign-1/sweep/config.json` and `campaign-1/llm/config.json`, is
  campaign-1's own manifest pin from before those revisions, not a stale
  value to carry forward. `harness.py` always recomputes this hash fresh
  from the checked-out `package.json` (`common.sha256_file`) rather than
  reading either recorded value, so no trial in this slice's dry runs was
  affected by the earlier draft's wrong number — this correction only
  matters for anyone reading this document by eye. Confirmed directly in
  this slice: `sha256sum examples/cases/case-003-thermal-spreader/package.json`
  on the commit this branch is at reproduces the value above.
- **Package manifest and solver identities.** Manifest sha256 above; screen
  script (`thermal_screen.py`, `sha256:0c3259481b6e6ccf0ade84705d0def70a34bad4a2de27061713ece99af4834f7`)
  and finite-element script (`thermal_fe.py`,
  `sha256:9a64eb0922b786bd50ece00e8b60206e23194f7566296ccd6ef900a71c491b63`)
  as already bound in `package.json`; the finite-element interpreter is
  identified by the system Python binary's digest, with the actual
  `scikit-fem` package version recorded only in each result document — an
  existing CASE-003 limitation, not introduced here.
- **Analysis script and report format.** `examples/agents/ablation/harness.py
  score <run-dir>` (deterministic, reads only files already on disk) writes
  `scores.json` (one row per trial) and `scores.md` (the same rows as a
  Markdown table). Metric definitions live in
  `examples/agents/ablation/scoring.py` and are unit-tested in
  `test_ablation.py`: `success`, `evaluations_to_first_all_pass`,
  `lightest_all_pass_mass_kg_m2`, `invalid_or_refused`, `out_of_envelope`,
  `inconclusive`, `not_evaluated_other`, plus `leak_clean`/`leak_hits` (the
  isolation check on arms B/C's tool output) and per-trial timing/cost
  fields.
- **The exact command for the scored block.** Once the items above are
  frozen (seed and `--max-budget-usd` chosen before viewing any scored
  result, per this document's own rule):

  ```bash
  python3 examples/agents/ablation/harness.py run_block \
    --trials 5 --seed <FROZEN_SEED> \
    --out <SCORED_RUN_DIR> \
    --screen-budget 40 --eval-budget 12 --n-eval 12 \
    --timeout-s 1800 --max-retries 1 --max-budget-usd <FROZEN_BUDGET_USD> \
    --core /home/connoravila/Documents/Avila-Labs/project-north-star/target/debug/avila-core
  ```

  followed by `python3 examples/agents/ablation/harness.py score
  <SCORED_RUN_DIR>` once every trial's `DONE.json` is present. `--core` is
  spelled out explicitly above (rather than left to its default) because
  the lead should confirm which build is frozen for the scored run, per
  the "Immutable harness and case commit" item; the other path arguments
  (`--case`, `--thermal`, `--python3`, `--thermal-python`, `--prior-log`)
  default to this repository's layout and can be left unset unless the
  lead is running from outside it. `run_block` is resumable at trial
  granularity (a `DONE.json`-complete trial is skipped on a re-invocation
  with the same `--out`), so an interrupted scored block can be restarted
  with the identical command.

No scored run should begin until those items are frozen.
