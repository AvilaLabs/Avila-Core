# Evidence contracts

## Definition

An evidence contract is the versioned agreement that defines what technical
question Core will attempt to resolve and what evidence is admissible for the
answer. It is not a guarantee of `PASS` and not a legal certification by itself.

The contract is the fundamental product and commercial unit because it aligns:

- the requester’s question;
- the professional’s method boundary;
- the provider’s delivery obligation;
- the reviewer’s acceptance criteria; and
- Core’s planning, evidence, and pricing logic.

## Required content

A production contract must identify at least:

1. **Question:** one bounded proposition capable of a defined verdict.
2. **Context of use:** the decision the evidence supports and its consequence.
3. **System boundary:** configurations, populations, geometries, time periods, or
   operating regimes included and excluded.
4. **Requirements:** metrics, comparison semantics, limits, units, rounding, and
   aggregation rules.
5. **Inputs:** roles, immutable identities, allowed transformations, and owners.
6. **Assumptions:** conditions accepted without being established by the campaign.
7. **Uncertainty:** variable domains, dependencies, data uncertainty, numerical
   error, model-form treatment, and coverage policy.
8. **Capability needs:** semantic types and any required implementation or
   qualification constraints.
9. **Evidence policy:** required lineage, validation, reviews, signatures,
   environments, and retention.
10. **Completion:** which verdicts fulfill the delivery obligation and what makes
    a campaign invalid or incomplete.

The current `v0.1` schema represents only a small structural subset.

## Contract lifecycle

```text
draft → in review → approved → instantiated → planned → executed → reviewed
  │          │          │             │           │          │          │
  └ reject ──┴ amend ───┴ retire ─────┴ cancel ───┴ block ───┴ invalidate
```

An amendment creates a new version and preserves the old one. An instantiated
contract points to an immutable approved template version plus case-specific
inputs. Execution never silently changes either.

## Completion versus verdict

These concepts must remain separate:

- **Campaign complete:** every required execution, evidence, and review
  obligation was satisfied.
- **Requirement verdict:** what the admitted evidence establishes about one
  requirement.
- **Commercial completion:** the provider fulfilled the agreed evidence
  contract, which may permit `PASS`, `FAIL`, or `INCONCLUSIVE`.

A process crash is not `INCONCLUSIVE`; it is incomplete. An admitted method whose
sound bound crosses the limit may be `INCONCLUSIVE`. A calculated nominal value
below a limit is not `PASS` when the contract requires a worst-case bound.

## Verdict logic

### PASS

All required evidence exists and is admissible, every applicable review gate is
satisfied, and the encoded requirement follows within the declared boundary.

### FAIL

All evidence required to establish contradiction exists and is admissible, and
the encoded requirement is contradicted within the declared boundary.

### INCONCLUSIVE

The campaign completed according to policy, but the available evidence cannot
establish either pass or fail. Reasons may include overlapping uncertainty
bounds, insufficient method validity, conflicting qualified results, or a
contractually accepted limit of resolution.

### NOT_EVALUATED

No verdict was attempted or the prerequisites for evaluation were not met. This
is the only result in the current scaffold.

## Contract templates as product capital

A useful template captures hard-won professional agreement about scope, input
quality, method selection, evidence, uncertainty, and review. Reuse can reduce
future scoping and review time—but only within the template’s eligibility rules.

Templates require named owners, versioning, validation cases, limitations,
change control, and retirement. Popularity is not qualification.

## Commercial fairness

Before execution, the contract should state:

- provider obligations and excluded work;
- completion and cancellation rules;
- fixed, bounded, or variable cost components;
- treatment of reruns caused by provider error versus changed customer input;
- evidence ownership and retention;
- dispute and independent-review mechanisms; and
- liability and regulatory limitations.

None of those commercial terms exist in the `v0.1` software schema yet.

