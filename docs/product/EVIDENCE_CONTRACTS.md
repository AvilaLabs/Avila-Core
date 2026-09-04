# Evidence contracts

## Definition

An evidence contract is the versioned agreement that defines what technical
question Core will attempt to resolve and what evidence is admissible for the
answer. It is not a guarantee of `PASS` and not a legal certification by itself.

The contract is the fundamental product and commercial unit because it aligns:

- the requester’s question;
- the method owner’s explicit boundary and validation evidence;
- the provider’s delivery obligation;
- the connected agent’s fixed search target and any optional presentation
  instructions;
- the independent verifier’s inputs; and
- Core’s planning, evidence, and pricing logic.

## Required content

A production contract must identify at least:

1. **Question:** one bounded proposition capable of a defined verdict.
2. **Context of use:** the decision the evidence supports and its consequence,
   represented by governed nominal purpose identities where compiler rules use
   it.
3. **System boundary:** configurations, populations, geometries, time periods, or
   operating regimes included and excluded.
4. **Requirements:** quantitative metrics, comparisons, limits, nominal
   quantity kinds and exact unit scaling; or non-quantity metrics with
   role-owned closed vocabularies and categorical predicates. Display rounding
   is never requirement logic.
5. **Inputs:** roles, immutable identities, allowed transformations, and owners.
6. **Assumptions and facts:** conditions accepted without being established by
   the campaign, typed facts used by applicability predicates, their providers,
   and required provenance.
7. **Uncertainty:** variable domains, dependencies, data uncertainty, numerical
   error, model-form treatment, and coverage policy.
8. **Capability needs:** semantic types and any required implementation or
   qualification constraints.
9. **Evidence policy:** required lineage, validation, qualification,
   signatures, environments, and retention; plus any optional agent
   presentation policy kept outside the verdict.
10. **Completion:** which verdicts fulfill the delivery obligation and what makes
    a campaign invalid or incomplete.

The current `v0.2-draft` schema represents the question, prose assumptions,
inputs, workflow, requirements, execution policy, and optional presentation-gate bindings. Facts,
system boundary, capability constraints, evidence policy, and completion rules
remain unrepresented.

## Contract and campaign lifecycles

```text
contract:  draft → in_review → approved → retired
              │         │          │
              └ reject ─┴ amend ───┘

campaign: planned → running → complete
             │         │         │
             └ cancel ─┴ block ──┴ invalidate
```

Contract status describes the governed document only. Template instantiation is
immutable campaign origin metadata, not an additional contract or campaign
status. An amendment to a non-draft contract creates a new draft version and
preserves the old one. An instantiated campaign points to an immutable accepted
contract or template version plus case-specific inputs. Here `in_review` and
`approved` are document-owner lifecycle labels, not professional-review gates.
Execution never silently changes either.

## Completion versus verdict

These concepts must remain separate:

- **Campaign complete:** every required execution and evidence condition was
  satisfied and every requirement received a technical state.
- **Requirement verdict:** what the admitted evidence establishes about one
  requirement.
- **Commercial completion:** the provider fulfilled the agreed evidence
  contract, which may permit `PASS`, `FAIL`, or `INCONCLUSIVE`.

A process crash is not `INCONCLUSIVE`; it is incomplete. An admitted method whose
sound bound crosses the limit may be `INCONCLUSIVE`. A calculated nominal value
below a limit is not `PASS` when the contract requires a worst-case bound.

## Verdict logic

### PASS

All evidence required to establish satisfaction exists and is admissible, and
the encoded requirement follows within the declared boundary. Review absence
cannot suppress this state.

### FAIL

All evidence required to establish contradiction exists and is admissible, and
the encoded requirement is contradicted within the declared boundary.

### INCONCLUSIVE

The campaign completed according to policy, but the available evidence cannot
establish either pass or fail. Reasons may include overlapping uncertainty
bounds, insufficient method validity, conflicting qualified results, or a
contractually accepted limit of resolution.

### NOT_EVALUATED

No verdict was attempted or the prerequisites for evaluation were not met. The
kernel derives all four states over admitted claims. Missing professional review
is not a reason for `NOT_EVALUATED`.

Every verdict names the semantic profile, requirement purpose, policy, admitted
evidence, attestations, assumptions, limitations, and exact comparison rule
that produced it. It states a conditional result inside that boundary; it is
not a standalone claim of scientific truth or certification.

## Contract templates as product capital

A useful template captures hard-won knowledge about scope, input quality,
method selection, evidence, uncertainty, and practical concerns. Reuse can
reduce future scoping and iteration time—but only within the template’s
eligibility rules.

Templates require named owners, versioning, validation cases, limitations,
change control, and retirement. Popularity is not qualification.

## Commercial fairness

Before execution, the contract should state:

- provider obligations and excluded work;
- completion and cancellation rules;
- fixed, bounded, or variable cost components;
- treatment of reruns caused by provider error versus changed customer input;
- evidence ownership and retention;
- dispute and independent-resolution mechanisms; and
- liability and regulatory limitations.

None of those commercial terms exist in the `v0.2-draft` software schema yet.
