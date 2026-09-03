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

- the exact Fable orchestrator and Sonnet worker versions, sampling settings,
  prompts, and tool permissions;
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

- exact model and prompt identities;
- scored trial count;
- fixed screen and full-evaluation budgets for all arms;
- retry, timeout, and failure-handling rules;
- randomization procedure;
- precise wall-time and cost instrumentation;
- immutable harness and case commit;
- package manifest and solver identities;
- analysis script and report format.

No scored run should begin until those items are frozen.
