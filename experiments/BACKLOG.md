# Experiment backlog

This queue prioritizes tests that reduce uncertainty about Avila Core before
another domain is built.

## P0 — Run next

### EXP-002: Core feedback ablation

Run matched Fable/Sonnet arms with Core feedback, raw solver feedback, and no
iterative feedback. This is the missing control needed to attribute any agent
performance difference to Core.

### EXP-003: Throughput and unattended execution

Reuse CASE-003 and measure the complete path from launch to final report.
Separate setup, agent, Core, solver, retry, and human-intervention time. The
first goal is not a faster solver; it is determining whether a prepared
campaign can run without manual work. If a repeat of an existing case still
takes hours of intervention, treat that as an architectural failure to fix.

### Provenance closure

Bind the Fable orchestrator, Sonnet workers, model versions, prompts, sampling
settings, tool policies, and tool transcripts to the experiment record. Until
this exists, agent attribution and behavioral reproduction remain
operator-reported.

## P1 — Establish reliability and comparative value

### EXP-004: Repeatability

Run fresh trials of a frozen agent protocol. Measure success rate, distribution
of evaluations to first PASS, best objective, failure modes, and run-to-run
variance.

### EXP-005: Refusal during successful search

Include predeclared tempting shortcuts: an out-of-envelope candidate, an
attempted package change, insufficient method fidelity, weakened coverage, and
a wrong capability identity. The experiment should test whether the search can
continue to a valid result after Core refuses the shortcut.

### EXP-006: Conventional optimizer comparison

Give a grid, Bayesian or surrogate optimizer, and the agent arm the same
candidate space, prior data, solver, and full-evaluation budget. This tests
whether agent reasoning adds search value beyond ordinary optimization.

## P2 — Test external validity

### EXP-007: Hard coupled search

Use a frozen problem with multiple interacting variables, competing
requirements, non-monotonic behavior, and plausible local optima. Prefer an
existing qualified capability. Do not build this until EXP-002 and EXP-003
show that experiments are discriminating and inexpensive to run.

### EXP-008: Independent reproduction

Provide the evidence package to a separately implemented verifier that does
not import Core's verdict code. It must reproduce package identity, admission,
and verdicts or name the disagreement.

### Physical validation

For a selected design, compare the computational result with an independent
solver or physical measurement. This tests the scientific model rather than
the Core workflow and must remain a separate claim.

## Ordering rule

Do not add another demonstration merely to increase the number of domains.
Prefer the experiment that isolates the largest current uncertainty:

1. Does Core add value beyond raw tool access?
2. Can a prepared campaign run quickly and unattended?
3. Is the result repeatable?
4. Does the advantage persist against conventional optimization?
5. Does it persist on genuinely difficult and physically validated problems?
