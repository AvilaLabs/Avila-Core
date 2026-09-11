# ADR-0016: Optional external-checker claims

- Status: proposed
- Date: 2026-09-10
- Refines: ADR-0011 (package-declared checkers and categorical evidence)

## Context

ADR-0011's external checker adapter treats every declared claim as mandatory:
a JSON Pointer that resolves to no value fails extraction atomically, so a
legitimately negative or partial checker result is reported as a process
rejection rather than as evidence. A wrapped search tool that reports
`search_status: FAIL` with no optimum found is a normal scientific outcome —
it simply carries no optimum economics. Before this decision the only way to
express that was to fail the step, which loses the distinction between "the
checker ran correctly and found nothing" and "the checker's output violated
the declared contract".

Core's evaluation layer already handles missing evidence correctly: a
requirement whose metric has no admitted claim evaluates NOT_EVALUATED, and
the admission layer records the absence as a `Missing` record with a finding.
The gap was only at the adapter boundary, where absence could not be
distinguished from malformation.

## Decision

1. **Claims may be declared `optional`.** Both `exact` and `categorical`
   claims in `avila.core/external-checker-adapter/v0.1-draft` accept an
   `optional` boolean (default `false`). The field is part of the
   hash-bound descriptor, so absence semantics are package-committed, not
   decided at run time.

2. **Absence only.** An optional claim is skipped when its pointer resolves
   to no value in an otherwise authoritative output. A value that is present
   but malformed — wrong type, non-canonical number, category outside the
   closed set — still fails extraction. `optional` governs existence, never
   correctness.

3. **The extraction contract relaxes to required-only.** The runner's claim
   coverage check compares extracted slots against the descriptor's
   *required* slots; optional slots may appear or not. An extracted claim
   still must be a declared slot, and required slots must all extract.

4. **Absence is reported, not hidden.** The step's run report records the
   absent optional slot names (`absent_slots`), and the human summary prints
   them. Dependent requirements evaluate NOT_EVALUATED through the existing
   missing-evidence path; nothing converts an absent claim into a pass, a
   fail, or a fabricated value.

## Consequences

- A wrapped checker can now express "ran correctly, produced no result of
  this kind" without the step reporting FAILED: status claims stay required,
  conditional claims (an optimum's economics, a comparison present only on
  success) are optional.
- Replay and reuse are unaffected: extraction over committed-receipt outputs
  uses the same rules, and identical bytes extract identical claim sets.
- A descriptor that marks everything optional can produce a step with zero
  claims; that is a legitimate outcome the requirements then report as
  missing evidence, not a loophole around evaluation.
