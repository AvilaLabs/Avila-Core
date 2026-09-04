# ADR-0006: Semantic core and evidence-contract language

- Status: proposed; not accepted until the required semantic fixtures and
  independent implementation checks exist
- Date: 2026-08-31
- Revised: 2026-09-01 after internal architecture review; 2026-09-01 to add
  the compile-time tolerance and coverage rules to R6
- Refines: ADR-0001, ADR-0002, ADR-0004, ADR-0005
- Introduces: contract and manifest schema `v0.2`; governed kind, role,
  capability-type, qualification, admission, reuse, and verdict records
- Companion material: `fixtures/semantic-core/README.md` and the exploratory
  `.acore` note in `docs/architecture/ACORE_SYNTAX.md`

## Context

Core is intended to compile evidence campaigns. A compiler needs explicit
semantics, not merely schemas and query names. The `v0.1` scaffold has the shape
of the language but leaves important meaning in prose: qualification scope is
free text, authoritative limits use binary floats, uncertainty does not control
comparison, and maturity, qualification, and organizational admission are
conflated.

Core's durable proposition is narrower than scientific truth and stronger than
workflow logging:

> Given these immutable records, authorities, policies, and semantic rules,
> Core can establish exactly which evidence is admissible and which requirement
> state follows. It cannot establish that an external scientific claim is true
> merely because the records are well formed.

Agents, users, providers, and interfaces may propose work. A separate semantic
authority evaluates what follows from the recorded boundary. An offline verifier
must be able to replay that judgment while clearly separating checks it
performed from scientific work it did not repeat.

This ADR defines the intended language semantics. It does not accept a concrete
runner sandbox, key-management system, textual syntax, or general scientific
qualification framework. Those require separate decisions and evidence.

## Decision

The evidence-contract language is governed by clauses SC-1 through SC-17.
Before this ADR can move to `accepted`, every normative rule and state
transition must have positive, negative, and, where relevant, `unknown`
fixtures. The current fixture archive is an initial conformance corpus and a
coverage plan, not yet a complete executable specification.

Written rules and schemas define behavior for all valid inputs. Vectors test
that behavior; a finite vector set is not, by itself, a complete specification.
Semantic profiles, record schemas, vector sets, package formats, protocols, and
kernel implementations are versioned separately. A kernel may implement more
than one historical semantic profile so archived packages remain verifiable.

### SC-1 Quantity kinds and units

1. Quantities are typed by **kind**, not dimension alone. Equal dimensions do
   not make quantities comparable: absorbed dose is not dose equivalent; energy
   is not torque; frequency is not activity; an absolute temperature is not a
   temperature difference.
2. A kind record declares a namespaced identity and version, dimension
   exponents, canonical unit, unit class, allowed uncertainty models, owner, and
   explicitly distinct kinds. Every unit factor is an exact rational to the
   canonical unit.
3. `core.*` kinds are governed by the Core specification. Domain kinds are
   governed by named owners. Core does not acquire domain authority by storing
   a kind record, and it does not require a professional credential to compile
   one.
4. Unit symbols use a restricted, case-sensitive, UCUM-derived profile. Exact
   scaling within one kind is the only implicit conversion.
5. The kernel normalizes quantities with exact rational arithmetic. A
   non-terminating decimal such as `100 uSv/h = 1/36000000 Sv/s` remains a
   rational for comparison.
6. Conversion between kinds is a capability with typed inputs, scope,
   qualification, receipt, output evidence, and provenance. Core never infers
   one from dimensional equality.
7. Case correction is `mechanically_safe` only for an explicitly governed typo
   alias whose intended unit and scale are unique. A generic case-insensitive
   match is not sufficient because SI symbol case can change magnitude.

Relevant diagnostics: `CORE-T2001`, `CORE-T2101`, `CORE-T2102`, and
`CORE-T2103`.

### SC-2 Numeric and canonical value profile

1. Binary floating point does not enter authoritative kernel arithmetic.
2. An authored decimal may use an exponent. Its canonical form is an exact,
   plain decimal with no exponent, leading plus sign, unnecessary leading zeros,
   trailing fractional zeros, or negative zero. Zero is `"0"`.
3. A canonical rational is reduced to lowest terms with a positive denominator.
   A denominator of one is emitted as an integer string. A zero numerator is
   `"0"`; leading plus signs and unnecessary zeros are forbidden.
4. JSON numbers are permitted only for integers within `±2^53`. Other numeric
   values are decimal or rational strings.
5. Canonical records omit absent optional fields. `null` is not an alias for
   absence unless a future schema explicitly defines a semantically meaningful
   null value.
6. Canonical JSON uses a JCS-derived key order and string escaping profile,
   rejects duplicate object keys, requires valid UTF-8, and requires strings to
   already be NFC. The authoritative reader rejects a non-NFC string rather
   than silently changing signed content.
7. Requirement evaluation compares exact canonical values. Display rounding
   never changes a verdict.
8. If a regulation or method requires rounding as part of a decision, the
   transformation is a separately typed and qualified capability. It declares
   an exact quantum as a `Quantity`, a rounding mode, authority, and raw input
   edge. The final kernel comparison remains exact over that capability's
   admitted output.
9. Adapters convert solver floats to shortest round-trip decimals and disclose
   representation and numerical error. That conversion does not make the
   underlying calculation exact.

The `v0.1` `f64` fields in requirements, bounds, and verdicts are replaced by
canonical decimal/rational values. The `float_roundtrip` feature is transitional
and must leave authoritative paths.

### SC-3 Uncertainty claims and kernel reduction

Every non-exact metric declares an uncertainty claim. The kernel performs only
the reductions below; statistical inference or combination belongs in a
qualified capability.

| Model | Required content | Kernel reduction | Kernel-reducible |
| --- | --- | --- | --- |
| `exact` | value | `lo = hi = nominal = value` | yes; defined constants and exact transformations only |
| `interval` | lower, upper, optional nominal, method claim | declared enclosure | yes |
| `coverage_interval` | lower, upper, nominal, coverage in `(0,1]`, method and coverage interpretation | declared tuple | yes |
| `worst_case` | exactly one of lower or upper, optional nominal, basis | declared side; other side absent | yes |
| `standard_uncertainty` | nominal, `u`, distribution | none | no; expand through a capability |
| `samples` or `distribution` | artifact, seed, family/parameters | none | no; reduce through a capability |
| `unquantified` | nominal | nominal only | yes only for a permitted nominal basis |

For every two-sided model, `lower <= upper`; when nominal exists it lies inside
the declared interval. Values have the role's kind and a unit in its class.

`coverage = 0.95` is an admitted method claim, not a kernel-created probability
that the present true value lies in an interval. `interval` and `worst_case`
mean that an admitted method claims an enclosure or side bound. A verifier can
replay the use of that claim; it cannot establish the claim's scientific
credibility unless it independently performs the relevant method validation.

Numerical error, input uncertainty, and model-form uncertainty remain distinct
in receipts and boundary statements. A capability declares how it transforms
or combines them; Core never silently combines them.

### SC-4 Evidence roles are nominal types

1. A role declares a namespaced identity and version, quantity kind when
   applicable, unit class, permitted claim models, media types, domain,
   validator, typed attributes, non-claims, and owner.
2. Roles are nominal. A slot is satisfied only by the same role identity and a
   compatible major version.
3. Cardinality is scoped to a named slot within a campaign or aggregation
   instance, not globally to every record carrying that role.
4. Kernel validators establish only generic structural properties that can be
   replayed without domain expertise. Geometry, material, and method semantics
   belong in separately qualified validators unless the Core specification has
   explicitly standardized their narrow representation.
5. A capability validator's acceptance is recorded as a method claim and is
   reported as not re-performed by a verifier that does not execute it.
6. Intended-use purposes are governed nominal identities with owners and human
   descriptions. Names, prefixes, or prose never imply compatibility or a
   hierarchy.

### SC-5 Capability types are signatures; workflows are programs

1. A capability type declares accepted slots, produced slots, permitted output
   claim models, typed parameters and domains, determinism class, any
   type-level material execution factors, per-output governed-purpose
   exclusions, partial-output behavior, non-claims, conformance fixtures, and
   owner.
2. A package manifest identifies the exact implementation and declares which
   permitted model it actually emits for each output, its environment and
   hardware requirements, ABI versions, validators, typed preflight facts,
   permissions, maturity, method owner, and signature.
3. Type-level satisfiability and package-level admissibility are separate:
   semantic checking may establish that a capability type can theoretically
   produce a suitable claim; binding must establish that a selected package
   actually does so under current policy.
4. `deterministic` means identical output bytes only for an invocation identity
   that includes every execution factor the package declares material,
   including environment and hardware profile. A package that is stable only
   within numerical tolerances declares numerical reproducibility, not bitwise
   determinism.
5. `seeded_stochastic` includes the seed in invocation identity.
   `nondeterministic` is never execution-memoized and is admissible only under
   explicit policy. Static contract compilation requires every role produced by
   a nondeterministic type to be explicitly permitted; that contract permission
   does not replace organization-level admission policy.
6. Partial results are admitted only for slots the type permits and the receipt
   declares complete.
7. Optional agent practicality checks and cross-kind conversion are capability
   types, not hidden special cases. A compiled presentation gate is routing
   metadata, never an approval or a technical verdict.

### SC-6 Composition rules

| Rule | Statement | Principal diagnostic |
| --- | --- | --- |
| R1 Resolution | Every required slot resolves to exactly one named source; ambiguity requires an explicit binding; an unfed slot names the inputs or capability types in the snapshot that could feed it. | `CORE-R3101`, `CORE-R3102` |
| R2 Nominal identity | Source and destination roles have the same identity and compatible major version. | `CORE-T2101` |
| R3 Claim sufficiency | Type-level permitted models can satisfy the requirement; after binding, the selected package's actual model and bound side can satisfy the comparison basis. | `CORE-T2201`–`CORE-T2203` |
| R4 Media | Produced media is accepted by the destination role. | `CORE-T2301` |
| R5 Graph shape | No self-dependency, cycle, or unknown dependency. | `CORE-R3201`–`CORE-R3203` |
| R6 Requirement binding | A requirement names its metric source or aggregation set; limit kind and unit are compatible; an `equal` comparison carries a nonnegative tolerance of the metric kind and no other comparison carries one; a `coverage` basis is a canonical decimal in `(0, 1]` and appears only on a `bounded` basis. | `CORE-R3301`, `CORE-T2102`, `CORE-T2103`, `CORE-T2104`, `CORE-S1102` |
| R7 Parameters | Values match kind, unit class, and domain; placeholders exist only in drafts. | `CORE-T2401`, `CORE-T2402`, `CORE-S1301` |
| R8 Reproducibility | Seeds and every declared material execution factor are bound; policy governs nondeterminism. | `CORE-T2501`, `CORE-A4301` |
| R9 Optional presentation gate | A configured connected-agent practicality stage binds the complete presented dossier, closed routing dispositions, digest-pinned agent policy, and explicit instructions. It compiles as a `presentation_gate` and never participates in a technical verdict. | `CORE-R3401` |
| R10 Governed purpose exclusions | Every requirement names a resolved nominal purpose; an output cannot serve a purpose it explicitly excludes. Prose is never interpreted, and lack of an exclusion is not positive qualification. | `CORE-T2601` |

The type checker reports independent findings in one pass. It suppresses only a
finding that would not exist if an earlier root cause were repaired.

### SC-7 Applicability predicates and fact provenance

Scope, template eligibility, policy constraints, and reuse conditions share a
closed, non-Turing-complete predicate language:

```text
pred := all(pred...) | any(pred...) | not(pred)
      | param_in_range(param, optional min, optional max, inclusivity)
      | param_in_set(param, values...)
      | input_attribute_in(slot, attribute, values...)
      | input_attribute_in_range(slot, attribute, optional min, optional max)
      | environment_image_in(digests...)
      | platform_in(triples...)
      | fact(name, source_requirement, operation, value)
      | always
```

1. Missing bounds are absent fields, not `null`.
2. Input attributes are addressed by named contract or step slot so repeated
   roles remain unambiguous.
3. Every fact has a governed type and provenance: value, optional quantity kind,
   source class, source identity, validator, and receipt reference.
4. A runner signature proves that a fact was recorded. It does not elevate a
   provider assertion into an independently measured fact. A qualification
   predicate declares acceptable fact source classes and validators.
5. Evaluation uses strong Kleene three-valued logic. `unknown` is never accepted
   as `true`; `not(unknown)` remains `unknown`.
6. Predicate nesting, collection sizes, and evaluation work have explicit
   resource limits.
7. Expiry and time-based policy use an explicit signed evaluation-time record.
   An I/O-free kernel never reads a wall clock.

A qualification record binds an exact package digest to an implemented type,
method version, context of use, predicates, exclusions, uncertainty and
numerical limits, known failure modes, validation evidence, owner, recognition,
lifecycle, and signature. Exact digest binding is deliberately strict; future
reproducible-build or package-family equivalence requires its own separately
validated rule.

Maturity, qualification, and admission remain separate:

- maturity is the provider's implementation-state declaration;
- qualification is a method owner's scoped, evidenced claim; and
- admission is an organization's recorded policy judgment for a contract.

### SC-8 Policy, admissibility, and selection

1. Organization, contract, and selection policy are distinct records.
2. A contract may tighten organization policy only where the schema defines a
   decidable partial order `tightens(contract_rule, organization_rule)`. If no
   order is defined for a rule, policies cannot be merged automatically; an
   exact policy reference or authorized replacement is required.
3. Admissibility and optimization are separate phases. An inadmissible package
   cannot win through price, speed, maturity, or preference.
4. Selection among admitted packages follows an explicit ordered list of
   requester-controlled criteria. Legitimate cost, time, locality, technical,
   diversity, and preference criteria may be used when visible. Provider
   payment or Avila margin may never be a hidden criterion.
5. Provider-declared maturity is a policy fact, not an independent scientific
   quality score. Qualification recency is not a default proxy for quality.
6. Every considered candidate receives a recorded decision. Candidate discovery
   itself is bounded by an identified registry snapshot and query.
7. Every selected step records whether an implementation is Avila-provided and
   the self-preference check that was applied.
8. Separation of duties is evaluated over signed role assertions. Waivers are
   explicit records carried into every affected verdict and package.

### SC-9 Contracts, templates, amendment, and completion

Contract lifecycle and campaign execution are not one state machine.

1. Contract status is `draft | in_review | approved | retired`. A contract
   states its bounded question and any prose assumptions; the compiler carries
   both into the compiled boundary and never interprets them.
2. Template instantiation is immutable origin metadata, not a contract status.
   An instance pins a template digest, parameters, case inputs, and eligibility
   evaluation.
3. Planning, execution, completion, cancellation, and invalidation are
   campaign states under SC-13. Optional presentation routing is a surrounding
   agent state, not a technical campaign gate.
4. Any semantic edit to a non-draft contract produces a new draft version with
   a `supersedes` edge and classified change record. No authoritative contract
   is edited in place.
5. Templates declare typed parameters and domains, workflow, requirements,
   policy floor, eligibility, validation cases, owner, version, and signature.
   An instance may tighten but not loosen the policy floor.
6. A completion block declares which verdict states fulfill delivery and which
   named inconclusive reasons are permitted. `NOT_EVALUATED` never completes a
   substantive contract.

### SC-10 Verdict calculus

A verdict is a deterministic judgment over admitted claims. Its meaning is
always conditional on the boundary recorded in the verdict.

#### Evaluation pipeline

1. Resolve the named metric source or aggregation set.
2. Refuse missing, quarantined, invalidated, or duplicate
   cardinality-one claims with `NOT_EVALUATED` and reasons.
3. Reduce the admitted uncertainty claim under SC-3.
4. Scale evidence and limit exactly to the kind's canonical unit.
5. Compare exact values; display formatting is not part of the decision.
6. Emit the complete boundary statement. Review and presentation state are not
   verdict inputs.

#### `less_than_or_equal`

Let `L` be the limit.

| Available bound and condition | Verdict | Rule |
| --- | --- | --- |
| upper exists and `upper <= L` | `PASS` | `bounded.le.within` |
| lower exists and `lower > L` | `FAIL` | `bounded.le.exceeds` |
| both exist and `lower <= L < upper` | `INCONCLUSIVE` | `bounded.le.crossing` |
| upper only and `upper > L` | `INCONCLUSIVE` | `bounded.le.upper_only` |
| lower only and `lower <= L` | `INCONCLUSIVE` | `bounded.le.lower_only` |
| neither side exists | `NOT_EVALUATED` | `not_evaluated.no_bound` |

`greater_than_or_equal` mirrors the table. Strict comparisons use strict
satisfaction and complementary contradiction: for `<`, `upper == L` is not
PASS and `lower == L` establishes FAIL. `enclosure` uses the same order rules
but requires an admitted enclosure claim.

For a nominal basis, the nominal value is compared exactly and the verdict
visibly states that uncertainty was not used. Nominal basis requires explicit
policy permission; in `v0.2-draft` that permission is the contract execution
policy's `permit_nominal_basis`, and its absence is `CORE-A4201` at compile
time.

For equality, tolerance is a quantity of the metric kind. Under a two-sided
bound, containment in `[L-t, L+t]` is PASS, disjointness is FAIL, and partial
overlap is INCONCLUSIVE. A one-sided bound can establish FAIL only when its
declared side is wholly beyond the tolerance band; it cannot establish PASS.
Under a nominal basis, nominal inside the closed tolerance band is PASS and
outside is FAIL.

#### Aggregation

- `all` and `any` require a non-empty list of already-derived verdicts. For
  `all`, the precedence is `FAIL`, `NOT_EVALUATED`, `INCONCLUSIVE`, `PASS`. For
  `any`, the precedence is `PASS`, `NOT_EVALUATED`, `INCONCLUSIVE`, `FAIL`.
  This means one established contradiction decides `all`, one established
  satisfaction decides `any`, and an unevaluated branch otherwise prevents the
  aggregate from implying that the campaign evaluated every required branch.
- For `max`, the lower side is `max` of every available lower bound when at
  least one exists; an upper side exists only when every member has an upper
  bound, and is `max(upper_i)`.
- For `min`, a lower side exists only when every member has a lower bound, and
  is `min(lower_i)`; the upper side is `min` of every available upper bound
  when at least one exists.
- An empty, incompatible-kind, or duplicate aggregation set is
  `NOT_EVALUATED` with a typed reason.
- Coverage intervals are not combined by `max` or `min` in the kernel because
  marginal coverage does not imply joint coverage. A qualified aggregation
  capability must emit one interval with a declared joint-coverage method.

PASS requires the admitted evidence and qualification state needed to establish
satisfaction. FAIL is emitted from admitted contradicting evidence. Optional
agent routing and external organization processes occur after this derivation
and cannot change it.

Every verdict records the declared and canonical limit, reduced evidence,
semantic profile and rule, configuration, assumptions and their providers,
capabilities and package digests, qualification and scope evaluations, coverage
interpretation, numerical-error treatment, waivers, next actions,
invalidation state, and replayability. Its statement must read as a conditional
derivation, never as unqualified certification or physical truth.

### SC-11 Admission: artifact to evidence

The first executable slice checks the identity, A3, A5, A6 type-level, and
slot-cardinality conditions over an evidence-claims document bound to a
compiled snapshot; see `docs/architecture/CAMPAIGN_EVALUATION.md`. Artifact
bytes, receipts, package identities, signatures, qualification, policy
snapshots, and invalidation remain unimplemented.

An artifact is admitted for role `r` at step `s` only if all conditions hold:

| ID | Condition | Failure state |
| --- | --- | --- |
| A1 | Artifact digest matches the receipt declaration for the slot. | quarantine |
| A2 | Invocation matches the bound plan and the runner signature is trusted by the applicable policy. | quarantine |
| A3 | Every used parent is admitted for its bound slot, or is a verified contract input. | quarantine |
| A4 | Receipt package digest equals the bound package digest. | quarantine |
| A5 | Execution completed; any partial result is explicitly permitted and complete for its named slot. | proposed/failed |
| A6 | The role validator accepted the artifact and its declared claim model matches the package declaration. | quarantine |
| A7 | Qualification scope evaluates true against the actual receipted context, or policy explicitly records qualification as not required. | quarantine |
| A8 | Bind-time admission remains valid for the recorded policy and as-of snapshot. | invalidated |
| A9 | If a presentation-routing record is attached, it binds the exact post-campaign dossier and an allowed disposition. Its absence or disposition never changes artifact admission. | routing record quarantined; artifact unchanged |
| A10 | No current invalidation targets the artifact or an ancestor. | invalidated |

Validation establishes only the validator's declared responsibility. It does not
turn structure, a digest, or a provider assertion into scientific truth.

Evidence states and every transition are explicit. Quarantine is terminal for
that artifact; a rerun creates a new artifact. Policy, presentation,
selection, preflight, conformance, qualification, and change records are
evidence-bearing records with their own identities and authority.

Historical verification is explicitly `as_of` a supplied policy,
qualification, revocation, and time snapshot. A package can prove internal
consistency against its recorded snapshot; it cannot prove present freshness
without current signed material.

### SC-12 Change, invalidation, and reuse

Core distinguishes query memoization, execution memoization, and evidence
identity.

1. Changes are typed: input bytes/metadata, parameters, requirements, quantity
   policy, package or method version, dataset, environment, qualification,
   organization policy, advisory/defect, discovered dependency, and template
   supersession. Agent presentation-policy changes affect routing history, not
   prior technical evidence or verdicts.
2. Default invalidation follows typed dependency edges and is fail closed.
   Informational explanation edges to rejected candidates do not propagate;
   registry-snapshot, policy, selected-binding, and evidence edges do.
3. A reuse rule is an authorized, signed non-dependence claim with scope,
   justification, validation evidence, and expiry. The kernel verifies its
   authority and applicability; it does not independently prove the scientific
   non-dependence asserted by a method owner.
4. `unknown` applicability means rerun.
5. Execution memoization requires the same package, canonical invocation,
   declared material environment/hardware factors, validator versions, and seed
   where relevant. The evidence is re-admitted under the new campaign's policy
   and qualification snapshot.
6. An impact report names every invalidated node, condemning edge path, permitted
   reuse and authority, minimal rerun subgraph, and labeled cost estimate.

### SC-13 Campaign and step state

Campaign states are `planned | running | blocked | completed | cancelled |
superseded | invalidated`. Step states are `pending | reused | staged | running |
collecting | validating | admitted | quarantined | failed | skipped`.

Every transition is an immutable record by an authorized actor. Resumption is a
new run over the same bound plan; admitted current work is reused under SC-12.
An amendment supersedes the campaign and cancels in-flight steps with receipts.
An optional surrounding presentation workflow may wait for an agent or user
acknowledgement, but it does not modify campaign state or manufacture a verdict.

### SC-14 Identity, attestations, and organizational trust

The semantic compiler has no registry of people or institutions it considers
authoritative. It compiles requirements for attributable records:

- contract attestations over an exact contract or plan digest;
- method qualification and reuse records;
- execution receipts and sourced facts; and
- kernel derivations identified by semantic profile and implementation.

Agent presentation policy is an optional, content-identified input outside the
technical campaign. Compilation establishes only that a configured gate has an
exact dossier, closed routing dispositions, and instructions. It does not claim
that the practical checklist is complete or let the routing result alter
admission or a verdict.

Software may verify signatures and key-to-role assertions for technical
records under a named trust policy. Those checks establish attribution and
policy conformance, not scientific truth. Organizations remain responsible for
their trust roots and key custody. A model output, prompt, button click, agent
routing record, or user acknowledgement is not itself technical evidence.

### SC-15 Evidence ownership invariants

1. Authoritative records are immutable and superseded by new versions.
2. Evidence validity depends on the validity of everything it used under the
   named current or historical `as_of` snapshot.
3. Invalidated, quarantined, or superseded records cannot
   satisfy a role.
4. Cross-campaign evidence use requires execution memoization or an authorized
   reuse rule.
5. Cardinality constraints are enforced per named slot or aggregation instance.
6. Every weakening—nominal basis, experimental admission, nondeterminism, or a
   waived control—is explicit in affected verdicts and packages.
7. Compilation uses an immutable registry and policy snapshot.

The Rust ownership analogy is useful design shorthand, not a formal equivalence
or patent claim. The actual authority is this specification and its fixtures.

### SC-16 Front ends and concrete syntax

JSON remains the canonical interchange. Authoring front ends preserve source
locations and lower to the same semantic records. Forms, egui, CLI, YAML, or a
future `.acore` language cannot bypass the kernel.

`.acore` remains exploratory. It is built only after semantic fixtures stabilize
and measured human or tool use shows that JSON/forms materially impede safe
authoring. Any textual syntax must have lossless, tested lowering for its stated
subset; it may not claim byte-identical lowering where paths, digests,
provenance, or defaults are unresolved.

### SC-17 Reproducibility, confidentiality, and verification

Core distinguishes deterministic planning, bitwise execution reproducibility,
numerical reproducibility within tolerance, and scientific credibility in a
context of use.

Canonical packages support encrypted or externally retained artifacts and
signed redaction records. A plaintext digest whose bytes are unavailable is
reported as content unavailable, not checked. Confidential low-entropy values
must not rely on an unsalted public digest as their only confidentiality
control.

The verifier reports at least four categories:

- checked and replayed;
- recorded but not re-performed;
- unavailable; and
- refused because the verifier lacks a required profile, trust root, or record.

The verifier establishes package integrity and conditional derivations. It does
not imply independent scientific recomputation, regulatory acceptance, or
certification.

## Acceptance conditions

This ADR remains proposed until:

1. canonical decimal, rational, Unicode, optional-field, and archive rules are
   total and covered by byte vectors;
2. every decision-table boundary, one-sided bound, equality basis, and
   aggregation rule has vectors;
3. every predicate form has true, false, unknown, resource-limit, source, and
   ambiguity cases;
4. admission A1–A10, state transitions, invalidation, reuse, and historical
   `as_of` verification have fixtures;
5. a narrow end-to-end specimen produces an offline-verifiable package without
   implying scientific qualification;
6. a line-count and dependency budget is adopted for the trusted computing base;
7. a second implementation can reproduce the normative vectors while also
   passing property and adversarial tests; and
8. the supported domain's kinds, purposes, exclusions, uncertainty claims,
   applicability rules, and calculus pass independent reference, adversarial,
   and replay checks before Core makes a supported scientific product claim.

## Consequences

- The semantic language, not orchestration plumbing, becomes Core's durable
  technical center.
- The trusted computing base must be measured and modular; “small” is a target,
  not an unverified claim.
- Exact comparison replaces built-in outcome-changing rounding.
- Contract and campaign lifecycle are separated.
- Fact provenance and source-eligibility requirements become typed inputs to
  applicability.
- Open specification and verifier compatibility are necessary for independent
  trust, but the specification alone is not the commercial moat.
- Domain governance, qualification networks, adapters, templates, accumulated
  evidence history, and enterprise operation are the compounding assets.

## Decision-index entries proposed by this record

| ID | Status | Decision |
| --- | --- | --- |
| S-013 | Proposed here; later accepted by [ADR-0012](0012-agpl-license-and-work-product-boundary.md) | Publish the semantic profiles, vectors, package format, and verifier interface so evidence consumers can independently audit and reimplement the judgment layer; the later licensing decision does not withhold Core mechanisms for patent protection. |
| S-014 | Proposed | Keep `.acore` exploratory until semantic fixtures stabilize and measured authoring evidence justifies a textual language; JSON remains canonical. |
