# ADR-0020: Provider selection records and organization policy rules

- Status: accepted 2026-09-24; implemented for the local case runner
- Implements: ADR-0006 SC-8 (policy, admissibility, and selection)

## Context

SC-8 ratifies that organization policy, contract policy, and the selection
that binds a capability to a step are distinct, auditable records:
admissibility precedes optimization, every considered candidate receives a
recorded decision, selection criteria are explicit and may never hide
provider payment or Avila margin, and self-preference is disclosed.

None of that exists yet. A workflow step names a `capability_type` and the
manifest binds one implementation; whichever candidates were considered and
why they lost is invisible to the package — a selection made in an email
thread and a selection made under a written policy produce identical
evidence. Sixteen `policy.*` fixture rows name the missing machinery.

This ADR proposes the smallest honest slice, on the same posture as
qualification records (ADR-0008): **the engine does not perform selection —
it verifies the recorded selection.** A signed producer document, bound by
digest, checked for internal consistency and against the package it claims
to describe. Candidate discovery is asserted and bounded by the compiled
registry snapshot; the runner checks what the record asserts, never that the
assertion is true.

## Proposed decision

### 1. A `capability_selection` document records the decision

Schema `avila.core/capability-selection/v0.1-draft`, bound under role
`capability_selection`, signed by the requester key like every other
package-asserted record. One document covers every step:

- `registry_snapshot`: `{registry_id, revision, sha256}` — must equal the
  package's bound registry byte-for-byte; discovery is bounded by the exact
  snapshot the contract compiled against (SC-8.6).
- `query`: free text naming how candidates were found — data, never
  interpreted.
- `selections`: per `step_id`:
  - `selected`: `{capability_type: {id, major}, executable_sha256,
    adapter_sha256}` — the triple the manifest must bind.
  - `candidates`: every considered candidate, each carrying
    `decision: selected | excluded | inadmissible` and `reasons:
    [{rule_id, detail}]` — no candidate without a recorded decision
    (SC-8.6).
  - `criteria`: the ordered list of criteria the selection applied, from a
    closed vocabulary `cost | time | locality | technical | diversity |
    preference` — order is requester-controlled and visible (SC-8.4).
  - `avila_provided`: whether the selected implementation is Avila-provided
    (SC-8.7).
  - `self_preference_check`: `{check_id, justification}`, required when
    `avila_provided` and policy forbids unpinned self-preference.
  - `cost_estimate` / `cost_confirmed_by`: the cost datum and the recorded
    human confirmation when a cap is exceeded (SC-8.4).

### 2. `execution_policy` fields declare the org's rules

The contract's `execution_policy` is the org's statement in a single-org
case — the same place `require_qualification` and
`recognized_qualification_owners` already live:

- `deny_providers` / `allow_providers`: capability-type `owner` (the
  registry's provider identity) forbidden / closed-set permitted.
- `require_provider_independence`: no two steps' selected types share an
  owner.
- `require_diverse_implementations`: selected executable digests must
  differ across steps.
- `maturity_floor`: the selected type's registry-declared `maturity` must
  meet the floor — maturity is a provider-declared policy fact (SC-8.5),
  never a scientific quality score and never a verdict input.
- `forbid_self_preference`: an `avila_provided` selection without a
  recorded `self_preference_check` is refused.
- `cost_cap`: `{value, currency}` — an estimate above the cap without a
  `cost_confirmed_by` record is refused before execution (the fixture's
  "external user confirmation" is the recorded field, not an interactive
  prompt).

Registry capability-type entries gain an optional `maturity` declaration
(`prototype | development | qualified | production`) so the floor has a
declared fact to compare; an undeclared maturity fails a declared floor —
fail closed.

### 3. A `CORE-P5xxx` diagnostic family names selection refusals

Selection refusals happen at bind/plan time, before any execution is spent,
and never become verdict states — the same wall ADR-0010 keeps between
policy and the verdict calculus (`presentation-policy.not-a-verdict-input`
already pins the precedent):

- `CORE-P5101` no eligible candidate — every candidate excluded or
  inadmissible; per-candidate reasons are carried on the refusal.
- `CORE-P5102` the bound capability is not the recorded selection — the
  selected triple's digests differ from what the manifest binds.
- `CORE-P5103` a selection record is required but absent — any
  `execution_policy` selection rule is declared and no `capability_selection`
  document is bound, or a step has no selection entry.
- `CORE-P5201` (note) candidate excluded by contract constraint.
- `CORE-P5301` provider denied or not allowed.
- `CORE-P5302` provider independence violated.
- `CORE-P5303` declared maturity below the floor (including undeclared).
- `CORE-P5401` cost estimate exceeds the cap with no recorded confirmation.
- `CORE-P5501` Avila-provided selection without the recorded
  self-preference check.
- `CORE-P5601` a forbidden criterion appears in the criteria list —
  `provider_payment` and `avila_margin` are banned vocabulary (SC-8.4).
- `CORE-P5602` a candidate carries no recorded decision.

### 4. The verifier re-derives the same consistency

From the bound documents alone: selected triple equals the bound
capability digests; every candidate carries a decision and reason;
criteria are in the closed vocabulary and ordered; `avila_provided`
selections carry the check the policy demands; the signature verifies
under the requester trust root when `require_signatures` applies. It does
not judge whether the record is *true* — same boundary as qualification
records.

## Boundary

- **Organization policy as a separate package document** and the `tightens`
  lattice (SC-8.1/8.2, `policy.lattice.*` rows) are deferred: merging an org
  record with a contract record needs a per-rule decidable order that only
  matters once org policy spans contracts. A later refinement adds the
  `organization_policy` document and the order.
- **Signed role-assertion separation of duties** (SC-8.8,
  `policy.rule.separation_of_duties`) needs the authority family's
  credential machinery; v1 expresses separation only as provider-owner
  constraints (`deny_providers`, `require_provider_independence`).
- **`environments` allow-list** (`policy.rule.environments`) needs a
  package-bound environment record; none exists.
- **Automated candidate discovery** — the record asserts the bounded set;
  nothing searches a registry.
- **Cost confirmation is a recorded field**, not a workflow — the
  interactive confirmation flow is a runner-client concern outside this
  record's scope.
