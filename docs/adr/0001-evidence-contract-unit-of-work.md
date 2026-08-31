# ADR-0001: Evidence contract as unit of work

- Status: accepted
- Date: 2026-08-31

## Context

Solver jobs and workflows describe how computation runs, but they do not fully
state what must be established, which evidence is admissible, who must review it,
or what counts as completion. Starting from tools would make Core easy to compare
with established orchestration products and would misalign pricing with the
customer’s decision.

## Decision

The evidence contract is Core’s primary product, planning, and commercial unit.
Every campaign resolves one immutable contract version. Jobs, methods, reviews,
and evidence are subordinate objects.

## Consequences

- The interface begins with the question and requirement.
- Planning cannot begin without a structurally valid contract.
- `PASS`, `FAIL`, and `INCONCLUSIVE` can all be compliant completions.
- Contracts require lifecycle, ownership, migration, and eligibility rules.
- Generic ad hoc workflows may be less convenient and are not the initial market.

