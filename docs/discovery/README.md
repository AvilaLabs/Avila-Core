# Discovery records

This directory contains sanitized evidence used to decide whether a candidate
contract should advance into a Core reference vertical. It is not a customer
data room or interview-notes repository.

Use [FIRST_PILOT_PACKET.md](FIRST_PILOT_PACKET.md) as the active Stage 0 record.
If a candidate is rejected, preserve the completed packet under a descriptive
filename, record the falsifier, and start a new active packet rather than
rewriting the history.

## Information boundary

Do not commit customer data, controlled information, credentials, proprietary
solver inputs or outputs, license files, personal contact details, or exported
campaign evidence. Record a coded participant or artifact identifier and a link
or reference to the approved system of record. A digest can preserve identity
but does not make restricted material safe to publish.

Each factual answer should carry:

- a source reference;
- the role that supplied or confirmed it;
- the observation date; and
- whether it is observed, asserted, inferred, or still unknown.

Use stable coded references so evidence can be reconciled without copying it:
`INT-###` for interviews, `CASE-###` for completed decision loops, and
`ART-###` for artifacts in the approved system of record.

`CASE-000` is reserved for the repository's synthetic
[ACTINV → Aftermatter integration specimen](../../examples/cases/case-000-actinv-aftermatter/README.md).
It is not a completed decision loop and never counts toward a discovery target.
Number participant-backed completed loops from `CASE-001`.

Architecture cannot close a discovery field. An implementation may test a
hypothesis, but the requester or buyer owns the decision about what the workflow
requires. Discovery evidence informs product selection; it does not gate Core's
technical verdicts or repository progress.
