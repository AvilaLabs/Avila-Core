# ADR-0023: Organization policy documents and the `tightens` merge order

- Status: proposed 2026-09-19

## Context

SC-8.1 and SC-8.2 ratify:

- Organization, contract, and selection policy are **distinct records**.
- A contract may tighten organization policy only where the schema
  defines a decidable partial order `tightens(contract_rule,
  organization_rule)`. Where no order is defined, policies cannot be
  merged automatically; an exact policy reference or authorized
  replacement is required.

ADR-0020 implemented the contract side: `execution_policy` fields on the
contract declare the rules the runner enforces, and a signed
`capability_selection` record proves the selection honored them. The
organization side was explicitly deferred: "merging an org record with a
contract record needs a per-rule decidable order that only matters once
org policy spans contracts."

Two `policy.lattice.*` fixture rows name the gap. Today an organization
cannot state its floor anywhere — a contract that declares no
`deny_providers` and a contract that bans the same provider produce
identical evidence about what the organization requires.

Same posture: **the engine does not merge policies by judgment — it
checks the recorded merge against a mechanical order.** The
`organization_policy` document declares the floor; the contract's
`execution_policy` either tightens it field-by-field, names an exact
policy reference, or carries an authorized replacement attestation.

## Proposed decision

### 1. An `organization_policy` document declares the floor

`avila.core/organization-policy/v0.1-draft`, carried as a package
document. Fields:

- `policy_id`, `policy_revision`, `schema_version`,
  `semantic_profile`, `owner`.
- `rules`: the same field vocabulary `execution_policy` already
  defines — `deny_providers`, `allow_providers`,
  `permitted_nondeterministic_roles`, `require_provider_independence`,
  `require_diverse_implementations`, `maturity_floor`,
  `forbid_self_preference`, `cost_cap`, `require_signatures`,
  `permit_nominal_basis`, `require_qualification`,
  `recognized_qualification_owners` — no second rule language. The org
  document declares the *floor*: the least strict rule the organization
  accepts. The merge-naming fields (`organization_policy`, `relation`,
  `replacement_attestation`) are meaningless inside `rules` and are
  rejected there.
- `signature`: the policy owner's signature document, verified under
  the supplied trust root — an org floor signed by a requester key is
  as meaningless as a requester-signed runner receipt.

The contract names the floor it derives from:

```json
"execution_policy": {
  "organization_policy": { "policy_id": "…", "policy_revision": 2, "sha256": "sha256:…" },
  "relation": "tightens",
  "deny_providers": ["provider.b", "provider.c"],
  ...
}
```

`relation` is a closed vocabulary:

- `tightens` — the contract's fields tighten the org floor per the
  order below.
- `exact` — the contract adopts the org floor verbatim (its other
  rule fields must then be absent — there is nothing to merge).
- `replaces` — the contract supersedes the org floor; requires a
  `replacement_attestation` pinning the canonical digest of a bound
  ADR-0021 `attestation` package document in which a `policy_owner`
  key authorized this exact contract identity to replace this exact
  policy identity. The attestation names the policy by `subject`
  (new kind `organization_policy`: identity and digest) and the
  contract by `target` (new optional field: `id@revision` — named,
  not digest-pinned, because the contract pins the attestation; a
  digest in both directions is a cycle). The compiler checks the
  binding fields; the runner verifies the signature against the
  supplied trust root. Without it, `replaces` is refused.
- `none` — the contract names no org policy (the ADR-0020 status quo).

### 2. The `tightens` order, per field, total and mechanical

For each field `f`, `tightens(contract.f, org.f)` holds iff:

| field | `contract.f` tightens `org.f` iff |
| --- | --- |
| `deny_providers` | `contract ⊇ org` |
| `allow_providers` | `contract ⊆ org` (a smaller allow-list tightens) |
| `require_provider_independence` | `contract ≥ org` |
| `require_diverse_implementations` | `contract ≥ org` |
| `maturity_floor` | `contract` names a maturity `≥` org's on the `prototype < development < qualified < production` order |
| `forbid_self_preference` | `org off ⇒ contract anything`; `org on ⇒ contract on` |
| `cost_cap` | `contract ≤ org` (a lower cap tightens) |
| `require_signatures` | `org off ⇒ contract anything`; `org on ⇒ contract on` |
| `permit_nominal_basis` | `org on ⇒ contract on`; `org off ⇒ contract anything` — *permitting* a weakening is itself the strictness; forbidding it tightens |
| `require_qualification` | `org off ⇒ contract anything`; `org on ⇒ contract on` |
| `permitted_nondeterministic_roles` | `contract ⊆ org` (a smaller grant tightens) |
| `recognized_qualification_owners` | `contract ⊆ org` (recognizing fewer issuers tightens) |

A field the org leaves undeclared is its least-strict value; a field
the contract leaves undeclared inherits the floor (it equals it — the
merge reads the org value). The merge result is deterministic: every
field resolves to `contract.f` where present else `org.f`, after
proving `tightens` field-by-field.

Where a future rule has no defined order, `tightens` cannot be
checked — the contract must name `exact` or carry a `replaces`
attestation. That is SC-8.2's escape hatch, not a judgment call.

### 3. `CORE-A49xx` names the merge failures

- `CORE-A4901` — `relation: tightens` but some field loosens the floor
  (`contract.deny_providers ⊉ org`, a lower `maturity_floor`, a higher
  `cost_cap`, an off switch the org set on).
- `CORE-A4902` — `relation: exact` but the contract declares other
  rule fields, or the pinned policy identity/digest is wrong.
- `CORE-A4903` — `relation: replaces` with no attestation, or the
  attestation names a different policy, contract, or role.
- `CORE-A4904` — the `organization_policy` document is malformed,
  carries merge-naming fields inside `rules`, is unsigned, or is
  signed by a key the trust root does not list as `policy_owner`.
- `CORE-A4905` — notice only: the package binds a newer revision of
  the pinned policy; drift information, never invalidation.

The merged `execution_policy` is what the runner enforces — ADR-0020's
selection checks read the merged result unchanged, so an org floor
strengthens every contract that names it without new runner code.

### 4. Verifier parity

`verify_org_policies` re-derives the merge: resolves the pinned
`organization_policy` document among bound package documents, checks
the pinned identity, replays `tightens` field-by-field, checks the
`exact`/`replaces`/`none` contract shapes, and reports
`organization_policy.*` verified or mismatch per check. The floor's
signature and a `replaces` relation's attestation (binding fields and
signature) are re-checked under the supplied trust root; without one
they report `not_checked`, matching the verifier's "not forged, not
checked" posture.

## Boundary

- **The floor is per-package, not organizational state.** The document
  travels with the package; Core does not resolve a global org policy
  or notice the org changed its mind — a `policy_revision` pin and a
  `policy_superseded` notice (when a newer revision is named) report
  drift without gating.
- **`replaces` is deliberately heavyweight.** It exists because SC-8.2
  requires an escape where no order is defined; making it require a
  `policy_owner` attestation keeps the escape auditable rather than a
  quiet loosening channel.
- **No cross-contract aggregation.** Each contract's merge is
  independent; an org cannot declare "at most N contracts may use
  provider X" — that is organizational workflow, not mergeable data.
- **`environments` and signed separation-of-duties stay deferred** —
  they are rule *kinds* that need their own record types (environment
  records; ADR-0021 role assertions), orthogonal to the merge order.

## Implements

- ADR-0006 SC-8.1/8.2; completes the ADR-0020 deferral.
- Fixture rows: `policy.lattice.contract-weaker` (the merged-policy
  refusal half), `policy.lattice.contract-tighter`.
