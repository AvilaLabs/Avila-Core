# ADR-0004: Four-state verdict model

- Status: accepted
- Date: 2026-08-31

## Context

Binary success/failure conflates execution status, evidence sufficiency, and
requirement satisfaction. Safety-relevant work needs to distinguish a contradicted
requirement from a method that cannot decide and from work that never reached
evaluation.

## Decision

Requirement verdicts use four states: `PASS`, `FAIL`, `INCONCLUSIVE`, and
`NOT_EVALUATED`. Execution and campaign completion are separate state machines.

## Consequences

- A crash is incomplete, not inconclusive.
- An admitted bound crossing a limit may be inconclusive.
- No number appears for `NOT_EVALUATED` unless it is clearly non-verdict evidence.
- Pricing cannot depend on returning pass.
- UI, schemas, APIs, tests, and evidence packages must preserve all four states.

