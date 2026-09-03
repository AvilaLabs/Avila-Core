# Avila Core experiments

This directory is the index and interpretation layer for experiments about
Avila Core. Executable cases, raw campaign logs, solver outputs, and capability
artifacts remain under `examples/`; records here link to those sources instead
of copying them.

The purpose is to keep three questions separate:

1. Did the software behave as implemented?
2. Did an agent-assisted workflow outperform its declared comparison?
3. Does the evidence isolate a benefit caused by Avila Core?

A successful case does not automatically answer all three.

## Registry

| ID | Status | Priority | Question |
| --- | --- | --- | --- |
| [EXP-001](EXP-001-core-assisted-thermal-search.md) | Completed pilot | — | Can an agent team using Core beat a fixed control grid in a second engineering domain? |
| [EXP-002](EXP-002-core-feedback-ablation.md) | Planned | P0 | Does Core's structured feedback improve an otherwise matched agent search? |
| EXP-003 | Planned | P0 | Can an existing experiment be launched and completed quickly with no intervention after launch? |
| EXP-004 | Planned | P1 | Does the agent result repeat across fresh runs with the same frozen protocol? |
| EXP-005 | Planned | P1 | Does Core refuse known shortcuts during an otherwise successful search? |
| EXP-006 | Planned | P1 | How does the agent search compare with a conventional optimizer under the same evaluation budget? |
| EXP-007 | Planned | P2 | Does the loop remain useful on a harder, coupled problem whose answer is not a monotonic boundary search? |
| EXP-008 | Planned | P2 | Can an independent implementation reproduce package identity and verdicts? |

The detailed queue and ordering rationale are in [BACKLOG.md](BACKLOG.md).
Start a new record from [TEMPLATE.md](TEMPLATE.md).

## Status vocabulary

- **Draft:** The question is being shaped; results must not be collected.
- **Pre-registered:** The protocol, arms, budget, metrics, and interpretation
  rules are frozen before scored runs.
- **Running:** Scored execution has begun. Any change is an amendment.
- **Completed:** Results and limitations are recorded.
- **Invalidated:** A defect prevents the run from answering its stated
  question. Keep the record and explain the defect.

## Rules for credible comparisons

- Name the exact claim each control isolates. A fixed sweep is not a
  no-Core agent control.
- Use the same candidate space, prior information, evaluation budget, and
  success criteria across matched arms.
- Record orchestrator and worker model identities, versions, prompts, tool
  permissions, sampling settings, and full tool transcripts. If they are not
  bound, describe model attribution as operator-reported.
- Record wall time, compute time, model time, retries, human interventions,
  and monetary cost separately.
- Use “exhaustive” only when every candidate in a declared finite space was
  evaluated. Otherwise say “fixed grid” or “control sweep.”
- Preserve failed and amended runs. A post-registration change creates an
  explicit amendment and limits the claim.
- Separate computational PASS from physical validation, certification, or
  real-world fitness.
- For stochastic agent arms, run repeated fresh trials before making a
  reliability claim.
