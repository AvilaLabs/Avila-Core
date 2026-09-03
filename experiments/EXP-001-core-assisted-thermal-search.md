# EXP-001 — Core-assisted thermal search

**Status:** Completed pilot  
**Date:** 2026-09-03  
**Executable case:** [CASE-003](../examples/cases/case-003-thermal-spreader/README.md)  
**Protocol:** [CASE-003 protocol](../examples/cases/case-003-thermal-spreader/PROTOCOL.md)  
**Results:** [CASE-003 results](../examples/cases/case-003-thermal-spreader/RESULTS.md)

## Question actually tested

Can a language-model designer, reading Avila Core's verdicts and margins, find
an all-PASS thermal-spreader candidate that is lighter than a declared
twelve-point control sweep, in fewer full evaluations than the sweep's size,
while the case package remains pinned?

This was a test of a Core-assisted workflow against a fixed grid. It was not a
matched test of agents with Core against the same agents without Core.

## Agent configuration and provenance

The operator reports that Fable orchestrated the run and spawned Sonnet agents
to perform work. The committed campaign artifacts identify the arm only as
`llm`; they do not bind the orchestrator or worker model identities, versions,
sampling settings, complete prompts, or tool transcripts. Attribution to
Fable and Sonnet is therefore operator-reported and the agent behavior is not
independently reproducible from the repository.

## Comparison

The control evaluated twelve predeclared designs. Its lightest all-PASS point
was 10 mm graphite at 18 kg/m². The agent arm received the control record as
prior information, could screen at most 60 candidates, and could fully
evaluate at most 20.

There was no matched raw-solver arm, no no-feedback agent arm, and no
conventional optimizer arm.

## Result

The agent arm screened 13 proposed candidates and fully evaluated 8. Five were
all-PASS.

- The first all-PASS candidate appeared at full evaluation 2: 3 mm graphite,
  5.4 kg/m².
- The lightest evaluated all-PASS candidate was 2 mm graphite, 3.6 kg/m², at
  full evaluation 6.
- Its finite-element hotspot interval was approximately 335.31 K against a
  340 K limit.
- One-millimetre graphite failed by about 4.44 K.
- One-millimetre aluminium failed by about 0.193 K, and one-millimetre
  aluminium nitride failed by about 2.53 K.
- The agent stopped with 12 full evaluations unused.

The result satisfies the protocol's declared creation criteria. The refusal
criterion was not exercised.

## Time and throughput

The operator reports approximately 1.5 days of end-to-end work for this test.
Individual finite-element evaluations took seconds, so most elapsed time was
case construction, validation and qualification work, tool generalization,
debugging, orchestration, and documentation rather than numerical execution.

No machine-readable breakdown of setup time, agent time, Core time, solver
time, retries, human interventions, or cost was recorded. The run therefore
does not yet establish campaign throughput.

## What this demonstrates

- A connected agent team can use Core's structured results to propose,
  evaluate, and refine candidates.
- Core, rather than the agents, derives the technical verdicts.
- The workflow transferred from shielding to a thermal finite-element case
  without a domain-specific change to the Core semantic mechanism.
- The recorded rationales show adaptive behavior: the designer inferred that
  thinner graphite was promising, observed a PASS/FAIL boundary, tested the
  missing 2 mm point, and checked lighter alternatives.
- The agent arm beat this particular fixed control grid under the declared
  evaluation-count criterion.

## What this does not demonstrate

- It does not isolate a benefit caused by Core. The same agents were not tested
  with raw solver output or without iterative feedback.
- It does not show superiority to a competent engineer, conventional optimizer,
  or exhaustive search of the full candidate space.
- The apparent fivefold mass improvement is partly a consequence of the
  control grid not testing graphite below 10 mm.
- It does not establish reliable agent performance from one run.
- It does not establish success on a difficult, highly coupled search. This
  case mostly reduces to finding a whole-millimetre thickness boundary.
- It does not establish that 2 mm graphite is a real-world optimum or a
  decision-grade design. The case does not validate material data, perfect
  interfaces, the strip-source geometry, or physical manufacture.
- It does not show that Core refuses an agent shortcut during this campaign.
- Because agent configuration was not bound, it does not independently prove
  which model performed each part of the work.

## Interpretation

Retain EXP-001 as a positive workflow pilot: the end-to-end loop operated and
transferred to a second computational domain. Do not cite it as evidence that
Core itself improves agent design performance or that the agents discovered a
novel engineering solution.

The next causal experiment is [EXP-002](EXP-002-core-feedback-ablation.md).
The next operational requirement is to rerun the existing case through an
automated harness and determine whether a campaign can complete quickly and
without intervention.
