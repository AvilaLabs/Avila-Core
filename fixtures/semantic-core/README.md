# Semantic-core fixtures: initial corpus and coverage manifest

ADR-0006 remains proposed until every normative rule listed here has the
required fixtures and they pass. This directory currently contains the first
pure vectors and the coverage plan; it is **not yet** a complete executable
specification.

Written semantics and schemas define total behavior. Vectors are conformance
evidence for independent implementations, not a substitute for rules covering
inputs absent from the corpus. A change to a rule updates its vectors and the
semantic profile. A kernel implementation has its own version and may support
multiple historical profiles.

## Current corpus

- `unit-scaling.v1.json`: 10 vectors;
- `scope-predicates.v1.json`: 19 vectors;
- `verdict-calculus.v1.json`: 49 vectors: 41 requirement-evaluation vectors
  plus 8 aggregate-verdict vectors; and
- `canon.v1.json`: 12 initial canonical-value and byte-reader vectors;
- `types/compiler-cases.v1.json`: 22 executable compiler fixtures: 3 compiled
  cases and 19 rejected cases covering the current R1–R6 subset, claim-model
  sufficiency, exact unit lowering, cascade suppression, independent
  findings, and source-layer refusals located by JSON Pointer; and
- `types/compiler-parameter-cases.v1.json`: 9 executable R7 fixtures: 1
  compiled case and 8 rejected cases covering every parameter value family,
  exact canonical lowering, quantity kinds and domains, undeclared values, and
  draft versus approved placeholder behavior;
- `types/compiler-reproducibility-cases.v1.json`: 9 executable type-level R8
  fixtures: 3 compiled cases and 6 rejected cases covering deterministic,
  seeded-stochastic, and nondeterministic types, material execution factors,
  seeds, and explicit role-scoped nondeterminism policy; and
- `types/compiler-review-cases.v1.json`: 9 executable structural R9 fixtures:
  2 compiled cases and 7 rejected cases covering exact review dossiers,
  governance-only dispositions, digest-pinned external eligibility policy,
  explicit independence, and pending review obligations; and
- `types/compiler-purpose-cases.v1.json`: 6 executable R10 fixtures: 3 compiled
  cases and 3 rejected cases covering governed purpose resolution, exact nominal
  exclusions, unrelated and similarly named purposes, and major-version
  mismatch.

All other fixtures below are required before ADR acceptance and are currently
planned unless files exist for them.

The `avila-core-kernel` conformance tests currently execute all 12 vectors in
`canon.v1.json`, all 10 vectors in `unit-scaling.v1.json`, and all 19 vectors in
`scope-predicates.v1.json`, plus the 41 requirement and 8 aggregation vectors in
`verdict-calculus.v1.json`. The compiler harness also executes all 55 cases in
the five compiler manifests and pins each registry digest plus all successful
compiled-snapshot identities. Passing the 90 pure vectors and 55 compiler
fixtures does not accept ADR-0006: package-level rule halves and other vector
families in this coverage plan remain absent, and no result is scientifically
qualified.

The initial `le.bounded.one_sided` specimen retains its stable fixture id, but
its expected rule was corrected to `bounded.le.upper_only` when the harness was
implemented so it matches the SC-10 decision table and the other one-sided rule
identifiers.

## Layout

```
fixtures/semantic-core/
  README.md                 this file
  SEMANTIC_PROFILE          draft profile implemented by these vectors
  vectors/                  pure kernel vectors: canonical JSON in, canonical JSON out
    verdict-calculus.v1.json
    scope-predicates.v1.json
    unit-scaling.v1.json
    canon.v1.json
    (planned) uncertainty-reduction.v1.json,
              admission.v1.json, propagation.v1.json, execution-memo.v1.json
  <area>/<fixture_id>.json   compiler fixtures: a snapshot in, expected findings/records out
  types/compiler-cases.v1.json
                             executable manifest for the current compiler subset
  types/compiler-parameter-cases.v1.json
                             executable manifest for SC-6 R7 parameters
  types/compiler-reproducibility-cases.v1.json
                             executable manifest for type-level SC-6 R8
  types/compiler-review-cases.v1.json
                             executable manifest for structural SC-6 R9
  types/compiler-purpose-cases.v1.json
                             executable manifest for nominal SC-6 R10
  types/*.contract.json     exact contract inputs named by that manifest
  types/compiler.registry.v1.json
                             exact shared registry input pinned by the manifest
  types/compiler.parameters.registry.v1.json
                             exact parameter registry pinned by the R7 manifest
  types/compiler.reproducibility.registry.v1.json
                             exact determinism registry pinned by the R8 manifest
  types/compiler.review.registry.v1.json
                             exact accountable-review registry pinned by the R9 manifest
  types/compiler.purpose.registry.v1.json
                             exact governed-purpose registry pinned by the R10 manifest
  scenarios/                 planned end-to-end campaign fixtures
```

The current compiler corpus is portable as a directory bundle. Its manifest
pins the shared registry's canonical SHA-256 identity, names each exact contract
file, and records the stable finding projection or compiled record projection.
Keeping the inputs separate also exercises the real two-document CLI boundary.
Future standalone snapshots may use the general format below when a case needs
its own registry, policy, approvals, receipts, or artifacts.

### Vector format (`vectors/*.json`)

```json
{
  "vector_set": "verdict-calculus", "version": 1,
  "semantic_profile": "avila.core/semantic/0.2-draft",
  "vectors": [ { "id": "...", "input": { ... }, "expected": { ... } } ],
  "aggregation_vectors": [ { "id": "...", "input": { ... }, "expected": { ... } } ]
}
```

`aggregation_vectors` is present only in sets that need a separately grouped
corpus; a harness treats it as another ordered vector collection.

Vector source files are reviewable, pretty-printed JSON. The harness parses each
`input` and `expected` value, validates it against the named semantic profile,
and canonicalizes it under `avila.core/canon/v1`. Canonical records have sorted
keys, no binary floats, canonical decimal/rational strings, and absent optional
fields rather than `null`. A conforming implementation must reproduce the
canonical `expected` bytes exactly.

### Compiler fixture format (`<area>/<id>.json`)

```json
{
  "fixture_id": "types.R1.ambiguous",
  "clause": "SC-6 R1",
  "snapshot": { "contract": {...}, "registry": {...}, "policy": {...}, "approvals": [], "receipts": [], "artifacts": [] },
  "operation": "compiler/check | plan/preview | admit | compiler/impact | verdict | package/verify",
  "expected": {
    "findings": [ { "code": "CORE-R3102", "class": "invalid", "owner": "contract_author",
                    "primary": { "document": "contract", "pointer": "/workflow/1/inputs/particle_flux" },
                    "repairs": [ { "applicability": "constrained_choice" } ] } ],
    "records": { ... }
  }
}
```

`expected.findings` is matched on code, class, owner, primary location, and
repair applicability — never on message wording. `expected.records` is matched
canonically.

## Naming and coverage rules

- Every rule, table row, admission condition, change class, and state
  transition must eventually have at least `<name>.pass` and `<name>.fail`;
  three-valued evaluations also require `<name>.unknown`.
- Every diagnostic code defined by the semantic profile has at least one fixture
  that emits it (`codes/CORE-XXXXX.json`).
- Fixture ids are stable; renaming one is a documented migration.

## Required fixtures

### kinds/ (SC-1)

| Fixture | Expected |
| --- | --- |
| `kinds.same-dimension-distinct.fail` | Gy metric vs Sv role → `CORE-T2102` |
| `kinds.unit-in-class.pass` | `uSv/h` accepted for `nuclear.dose_equivalent_rate` |
| `kinds.unit-not-in-class.fail` | a known unit belonging to another class → `CORE-T2103`; repair `constrained_choice` = unit class |
| `kinds.unit-not-in-profile.fail` | `rem` under the current restricted profile → `CORE-T2001` |
| `kinds.unit-symbol-unknown.fail` | `Mpa` → `CORE-T2001`; correction requires confirmation unless a governed typo alias makes identity and scale unique |
| `kinds.prefix-case.fail` | `MSv` (mega-sievert) is not `mSv` → `CORE-T2001` |
| `kinds.cross-kind-conversion.fail` | absorbed-dose producer bound to dose-equivalent slot → `CORE-T2101`; repair `method_owner_judgment` naming the conversion capability |
| `kinds.exact-scaling.pass` | vectors `unit-scaling.v1` |
| `kinds.registry-namespace-owner.fail` | kind in `nuclear.*` without owner → `CORE-R3501` |

### numerics/ (SC-2)

| Fixture | Expected |
| --- | --- |
| `numerics.decimal-canonical.pass` | `"9.41"` accepted |
| `numerics.decimal-noncanonical.fail` | `"9.410"`, `"09.4"` → `CORE-S1102` |
| `numerics.json-float.fail` | `25.0` as JSON number in `limit` → `CORE-S1102` |
| `numerics.json-int.pass` | `3` as JSON number for `mesh_refinement_levels` |
| `numerics.nan-inf.fail` | `"NaN"`, `"Infinity"` → `CORE-S1102` |
| `numerics.display-rounding.pass` | presentation changes, canonical value and verdict do not |
| `numerics.decision-rounding-without-capability.fail` | outcome-changing rounding must be a typed, qualified capability |
| `numerics.rational-factor.pass` | `"1/3600000000"` parsed and reduced |
| `numerics.rational-noncanonical.fail` | reducible fraction, negative denominator, leading plus/zeros, zero denominator |
| `numerics.decimal-exponent-normalizes.pass` | authored exponent lowers to one canonical plain decimal |
| `numerics.negative-zero.fail` | `"-0"` is noncanonical; canonical value is `"0"` |

### uncertainty/ (SC-3)

| Fixture | Expected |
| --- | --- |
| `uncertainty.reduce.exact/interval/coverage_interval/worst_case_upper/worst_case_lower/unquantified.pass` | reduced tuples per table |
| `uncertainty.reduce.standard_uncertainty.fail` | `CORE-T2203` irreducible; repair names `core.uncertainty.expand@1` |
| `uncertainty.reduce.samples.fail` | `CORE-T2203` |
| `uncertainty.coverage-out-of-range.fail` | coverage `"1.2"` → `CORE-S1102` |
| `uncertainty.numerical-error-separate.pass` | receipt components recorded, not combined by kernel |

### roles/ (SC-4)

| Fixture | Expected |
| --- | --- |
| `roles.nominal-identity.fail` | same kind, different role id → `CORE-T2101` |
| `roles.major-version.fail` | `role@2` offered to `role@1` slot → `CORE-T2101` |
| `roles.minor-version.pass` | `role@1` with extra optional attribute accepted |
| `roles.validator-required.fail` | role without validator → `CORE-R3501` |
| `roles.attribute-undeclared-in-predicate.fail` | predicate references undeclared attribute → structural finding before evaluation |
| `roles.cardinality-slot-scoped.pass` | two aggregation instances may carry the same role without creating a global duplicate |

### types/ (SC-5, SC-6)

| Fixture | Expected |
| --- | --- |
| `types.R1.resolved.pass` | unique source bound |
| `types.R1.unresolved.fail` | `CORE-R3101` |
| `types.R1.ambiguous.fail` | two producers, no binding → `CORE-R3102`; repair `constrained_choice` |
| `types.R1.explicit-binding.pass` | `bindings` resolves ambiguity |
| `types.R2.role-mismatch.fail` | `CORE-T2101` |
| `types.R3.type-satisfiable.pass` | capability type permits a model capable of satisfying the basis |
| `types.R3.bound-package-coverage-sufficient.pass` | selected package declares 0.95, basis 0.95 |
| `types.R3.coverage-insufficient.fail` | producer 0.90, basis 0.95 → `CORE-T2201` |
| `types.R3.unquantified-governed.fail` | `CORE-T2202` |
| `types.R3.unquantified-nominal.pass` | policy permits nominal basis |
| `types.R3.irreducible.fail` | `CORE-T2203` |
| `types.R3.enclosure-needs-interval.fail` | coverage_interval offered to `enclosure` → `CORE-T2201` |
| `types.R3.worst-case-side-sufficient.pass/fail` | bound side is checked against comparison direction at bind time |
| `types.R4.media.pass/fail` | `CORE-T2301` |
| `types.R5.self-dependency.fail` | `CORE-R3201` |
| `types.R5.cycle.fail` | `CORE-R3202` |
| `types.R5.unknown-step.fail` | `CORE-R3203` |
| `types.R6.unbound-metric.fail` | `CORE-R3301` |
| `types.R6.limit-kind.fail` | `CORE-T2102` |
| `types.R6.limit-unit.fail` | `CORE-T2103` |
| `types.R7.param-kind.fail` | `CORE-T2401` |
| `types.R7.param-domain.fail` | negative cooling time → `CORE-T2402` |
| `types.R7.parameters.pass` | booleans, integers, exact numbers, text, and quantities lower to tagged canonical IR |
| `types.R7.scalar-type.fail` | scalar family mismatch → `CORE-T2401` |
| `types.R7.scalar-domains.fail` | independent integer, text-choice, and exact-number domain findings |
| `types.R7.unknown-param.fail` | undeclared values cannot become implicit defaults → `CORE-S1101` |
| `types.R7.placeholder-draft.pass` | `not_defined` in `draft` → `missing` finding, owner requester |
| `types.R7.placeholder-approved.fail` | `CORE-S1301` |
| `types.R7.required-missing-draft.pass` | absent required value → `missing` finding, owner requester |
| `types.R8.deterministic-bound.pass` | declared material factors are typed and retained in compiled identity |
| `types.R8.missing-factor.fail` | every type-declared material factor must be bound → `CORE-T2501` |
| `types.R8.factor-domain.fail` | factor values use the same typed domains → `CORE-T2402` |
| `types.R8.extraneous-seed.fail` | seeds do not enter deterministic invocation identity → `CORE-T2501` |
| `types.R8.seeded-bound.pass` / `missing-seed.fail` | seeded-stochastic identity requires the seed and material factors |
| `types.R8.nondeterministic-refused.fail` | `CORE-A4301` |
| `types.R8.nondeterministic-permitted.pass` | explicit policy permits every produced role without changing the nondeterministic class |
| `types.R8.permission-scope.fail` | permission for an unrelated role does not enable the step → `CORE-A4301` |
| `types.R9.review-bound.pass` | exact dossier, governance dispositions, external eligibility-policy identity, and independence constraints compile to `pending_external_review` |
| `types.R9.explicit-none.pass` | an explicit lack of separation is preserved as a visible weakening, never inferred as a default |
| `types.R9.missing-review.fail` | a review capability without its contract policy binding → `CORE-R3401` |
| `types.R9.policy-digest.fail` / `policy-revision.fail` | eligibility policy must be pinned by a valid immutable identity → `CORE-R3401` |
| `types.R9.empty-independence.fail` / `duplicate-independence.fail` | constraint mode is nonempty and unambiguous → `CORE-R3401` |
| `types.R9.binding-on-non-review.fail` | a normal capability cannot acquire review semantics from contract syntax → `CORE-R3401` |
| `types.R9.nondeterminism-refused.fail` | review remains nondeterministic and still requires explicit R8 role-scoped permission → `CORE-A4301` |
| `types.R10.allowed.pass` | a resolved purpose not excluded by the producing output is retained in compiled IR |
| `types.R10.excluded.fail` | an exact output-purpose exclusion → `CORE-T2601` |
| `types.R10.unrelated-purpose.pass` | excluding one purpose does not exclude unrelated governed identities |
| `types.R10.nominal-near-name.pass` | `screening@1` exclusion does not match `screening_research@1`; no prefix or prose inference |
| `types.R10.unknown-purpose.fail` / `major-version.fail` | purpose identity must resolve exactly in the pinned registry → `CORE-T2601` |
| `types.satisfiable.pass` | a path of roles reaches the basis |
| `types.satisfiable.fail` | no capability-type path can emit a reducible bounded model → `CORE-T2201` at the requirement; package admissibility is tested separately |
| `types.independent-errors-one-pass.pass` | three unrelated errors reported together |
| `types.source.json-float.fail` | binary float refused at `/workflow/0/parameters/x` → `CORE-S1102` |
| `types.source.unknown-field.fail` | undeclared key refused at its own pointer → `CORE-S1101` |
| `types.source.noncanonical-decimal.fail` | `"100.0"` refused at `/requirements/0/limit/value` → `CORE-S1102`; repair `mechanically_safe` = `"100"` |
| `types.source.duplicate-key.fail` | repeated object key refused at the key → `CORE-S1103` |
| `types.cascade-suppressed.pass` | consequence of a root error summarized under it |
| `types.cascade-preserves-independent.pass` | dependency-blocked step with its own `CORE-P5101` reports both |
| `types.conversion-capability.pass` | Gy→Sv via `core.convert.absorbed_dose_to_dose_equivalent@1` type-checks with weighting role bound |
| `types.human-step.pass` | fulfilled, signed `core.review.decision@1` record is admitted under external organization policy; type-level obligation is now covered by the executable R9 fixtures |
| `types.partial-outputs.pass/fail` | declaration admits only named slots; undeclared partial → `CORE-E7201` |

### applicability/ (SC-7)

| Fixture | Expected |
| --- | --- |
| `scope.pred.all/any/not.pass` | boolean structure |
| `scope.pred.param_in_range.true/false/unknown` | vectors `scope-predicates.v1`; missing endpoints are omitted, never `null` |
| `scope.pred.param_in_range.unit-scaled.true` | min `24 h`, fact `1440 min` |
| `scope.pred.param_in_set.*`, `input_attribute_in.*`, `input_attribute_in_range.*`, `environment_image_in.*`, `platform_in.*`, `fact.*` | true/false/unknown each |
| `scope.exclusions.fail` | exclusion matches → `CORE-A4101` |
| `scope.unknown-never-true.pass` | `unknown` under `not` stays `unknown` |
| `scope.record.active.pass` | admitted |
| `scope.record.superseded.fail` | `CORE-A4101` via change event |
| `scope.record.expired.fail` | `CORE-A4602` |
| `scope.record.revoked.fail` | `CORE-A4603` |
| `scope.record.not-recognized.fail` | `CORE-A4601` under `require_qualification {recognized_by}` |
| `scope.record.digest-bound.fail` | record cites a different package digest → not applicable |
| `scope.maturity-migration.pass` | v0.1 `qualified` → `released`; `allow_unqualified_capabilities` → policy rules |
| `scope.admission-not-in-manifest.fail` | manifest field `admitted` → `CORE-S1101` unknown field |
| `scope.a7-actual-context.fail` | planned context in scope, actual preflight fact out of scope → quarantine `CORE-A4101` |
| `scope.repeated-role-slot-addressing.pass` | predicates distinguish two inputs carrying the same role by slot |
| `scope.fact.provider-asserted-insufficient.fail` | provider assertion cannot satisfy a predicate requiring validated-input or runner-measured authority |
| `scope.fact.source-and-validator.pass` | fact type, source, validator, and receipt match qualification requirements |
| `scope.expiry.explicit-time.pass` | expiry uses a signed evaluation-time record; kernel never reads a clock |
| `scope.resource-limits.fail` | depth/count/work limits fail closed without hang |

### policy/ (SC-8)

| Fixture | Expected |
| --- | --- |
| `policy.lattice.contract-weaker.fail` | `CORE-A4201` |
| `policy.lattice.contract-tighter.pass` | |
| `policy.rule.maturity_floor.pass/fail` | |
| `policy.rule.require_qualification.fail` | `CORE-A4601` |
| `policy.rule.deny_providers.fail`, `allow_capabilities.pass` | |
| `policy.rule.environments.fail` | remote env under local-only policy |
| `policy.rule.independence.fail` | same provider on two named steps |
| `policy.rule.diversity.pass` | two implementations + comparison step |
| `policy.rule.forbid_self_preference.pinned.pass` | Avila package pinned by contract with justification |
| `policy.rule.forbid_self_preference.unpinned.fail` | `CORE-P5501` |
| `policy.rule.permit_nominal_basis.fail` | nominal requirement under governed policy → `CORE-A4201` |
| `policy.rule.required_review_roles.pass` | verdict blocked until review present |
| `policy.rule.separation_of_duties.fail` | approver = requester under `distinct` → `CORE-A4502` |
| `policy.rule.separation_of_duties.waived.pass` | waived; `waived_controls` recorded in package |
| `policy.rule.cost_caps.pass` | `CORE-P5401` → `awaiting_approval` |
| `policy.selection.every-candidate-decided.pass` | selection record lists all candidates with reasons |
| `policy.selection.rank-order.pass` | policy-declared technical/operational criteria; provider maturity is not an implicit quality rank |
| `policy.selection.tie-break.pass` | ascending `capability_id`, descending version |
| `policy.selection.transparent-cost.pass` | requester may optimize cost after admissibility; value and source are recorded |
| `policy.selection.hidden-margin.fail` | provider payment or Avila margin cannot be an undeclared ranking input |
| `policy.selection.no-eligible.fail` | `CORE-P5101` with per-candidate reasons |
| `policy.selection.excluded-by-constraint.pass` | `excluded_by_contract_constraint`, note `CORE-P5201` |
| `policy.explainability.pass` | decision record carries rule ids and facts |

### lifecycle/ (SC-9)

| Fixture | Expected |
| --- | --- |
| `lifecycle.status.draft.permits-check.pass` | |
| `lifecycle.status.draft.refuses-submit.fail` | |
| `lifecycle.status.in_review.refuses-edit.fail` | amend → new draft |
| `lifecycle.status.approved.permits-bind.pass` | contract lifecycle is separate from campaign state |
| `lifecycle.status.retired.read-only.pass` | |
| `lifecycle.instantiation-is-origin.pass` | template instance is immutable origin metadata, not a status |
| `lifecycle.campaign-state-not-contract-state.pass` | planned/executed/reviewed exist only on campaigns |
| `lifecycle.template.instantiate.eligible.pass` | |
| `lifecycle.template.ineligible.fail` | `CORE-A4401` |
| `lifecycle.template.eligibility-unknown.fail` | `CORE-A4405` |
| `lifecycle.template.default-provided_by.pass` | boundary names template digest |
| `lifecycle.template.instance-pins-version.pass` | template amendment does not touch instance; `template_superseded` notice |
| `lifecycle.template.validation-cases-must-compile.fail` | template not approvable |
| `lifecycle.template.policy-only-tightens.fail` | instance loosening → `CORE-A4201` |
| `lifecycle.amend.new-version-supersedes.pass` | `supersedes` edge; classified change event |
| `lifecycle.completion.pass-only.pass` | |
| `lifecycle.completion.permitted-inconclusive.pass` | reason listed → completed |
| `lifecycle.completion.not_evaluated-never.fail` | |

### verdict/ (SC-10) — see `vectors/verdict-calculus.v1.json` for kernel vectors

| Fixture | Expected |
| --- | --- |
| `verdict.le.bounded.within/exceeds/crossing/one_sided` | vectors |
| `verdict.ge.bounded.within/below/crossing/one_sided` | vectors |
| `verdict.lt.bounded.boundary` | hi = L → crossing, not PASS |
| `verdict.gt.bounded.boundary` | lo = L → crossing |
| `verdict.enclosure.*` | as bounded with coverage 1 required |
| `verdict.nominal.within/exceeds` | vectors; `basis: nominal` visible |
| `verdict.equal.within/outside/partial` | vectors |
| `verdict.equal.no-tolerance.fail` | `CORE-T2104` |
| `verdict.display-rounding-does-not-change.pass` | exact canonical values determine the verdict |
| `verdict.decision-rounding-capability.pass` | admitted transformation output is compared exactly and raw input remains linked |
| `verdict.unit-scaling-exact.pass` | 100 uSv/h limit vs Sv/s evidence |
| `verdict.aggregation.all.*`, `any.*` | current precedence examples plus planned exhaustive and property-generated truth tables |
| `verdict.aggregation.max/min.enclosure` | side-aware max/min reduction; a side that cannot be bounded remains absent |
| `verdict.aggregation.coverage-needs-capability.fail` | marginal coverage intervals are not assigned joint coverage by the kernel |
| `verdict.one-sided.lower/upper.*` | both comparison directions and strict boundaries |
| `verdict.equal.nominal/one-sided.*` | nominal tolerance and one-sided contradiction rules |
| `verdict.not_evaluated.missing/quarantined/invalidated/awaiting_review` | reasons and owners listed; no numbers |
| `verdict.not_evaluated.duplicate-claim` | `CORE-E7301` |
| `verdict.pass-requires-reviews.pass` | PASS withheld until reviews present |
| `verdict.fail-with-reviews-outstanding.pass` | FAIL emitted with `reviews_outstanding` |
| `verdict.record-fields.pass` | every field of `avila.core/verdict/v0.2` present |
| `verdict.evaluator-identity.pass` | `kernel:verdict-calculus@1` |
| `verdict.core-requirement-evaluation-step.pass` | specimen step type maps to kernel |
| `verdict.kernel-bug-guard.fail` | undefined `hi` reaching the table → `CORE-V8102` |

### admission/ (SC-11)

| Fixture | Expected |
| --- | --- |
| `admission.A1..A10.pass` | one fixture each |
| `admission.A1.hash-mismatch.fail` | `CORE-E7101` → quarantined |
| `admission.A2.foreign-receipt.fail` | `CORE-E7102` |
| `admission.A2.untrusted-runner-key.fail` | `CORE-E7102` |
| `admission.A3.unadmitted-parent.fail` | `CORE-E7103`; cascade to root |
| `admission.A4.package-mismatch.fail` | `CORE-E7104` |
| `admission.A5.exit-zero-insufficient.fail` | exit 0 with missing declared output → `CORE-X6202`; nothing admitted |
| `admission.A5.timeout/crash/sandbox.fail` | `CORE-X6101/6102/6103` |
| `admission.A6.validator-rejected.fail` | `CORE-E7201` |
| `admission.A6.model-mismatch.fail` | declared coverage_interval, emitted unquantified → `CORE-E7201` |
| `admission.A7.actual-context.fail` | see `scope.a7-actual-context.fail` |
| `admission.A7.vacuous-recorded.pass` | no qualification required → sub-record says so |
| `admission.A8.policy-changed.fail` | → invalidated |
| `admission.A9.awaiting-review.pass` | state, not quarantine |
| `admission.A10.ancestor-invalidated.fail` | |
| `admission.state.quarantine-terminal.pass` | rerun yields new artifact id |
| `admission.review.accept/reject/request_information.pass` | A9 / `review_rejected` / `CORE-E7401` |
| `admission.review.cannot-edit-artifact.fail` | review record with mutated bytes → `CORE-E7101` |
| `admission.non-artifact-records.pass` | snapshot, approvals, selection, preflight, change events are records |
| `admission.sub-record-replayable.pass` | verifier replays A1–A4, A6(kernel), A7, A8, A10 from package alone |
| `admission.undeclared-output-discarded.pass` | `CORE-X6301` note; not evidence |
| `admission.validator-does-not-establish-truth.pass` | obligations report describes the validator's narrow responsibility |
| `admission.as-of-historical/current.pass` | package snapshot and supplied current revocation material produce distinct labeled results |

### change/ (SC-12)

| Fixture | Expected |
| --- | --- |
| `change.class.<each>.pass` | default invalidation set for every change class defined by SC-12 |
| `change.input_metadata.non-dependence.pass` | attribute not consulted → no invalidation |
| `change.propagation.stops-at-unrelated.pass` | upstream and sibling nodes untouched |
| `change.propagation.selected_over-never.pass` | |
| `change.requirement.verdict-only.pass` | evidence untouched |
| `change.reuse-rule.applies.pass` | `reused_under` edge with rule id |
| `change.reuse-rule.condition-unknown.rerun.pass` | |
| `change.reuse-rule.authority-mismatch.fail` | `CORE-E7502` |
| `change.reuse-rule.expired.fail` | |
| `change.reuse-rule.only-narrows.fail` | rule attempting to widen → refused |
| `change.memo.deterministic-hit.pass` | identical invocation digest → `reused` |
| `change.memo.seeded-same-seed.pass` / `different-seed.fail` | |
| `change.memo.nondeterministic-never.fail` | |
| `change.memo.admission-under-new-policy.fail` | old evidence, new policy forbids → rerun |
| `change.memo.validator-version.fail` | validator version changed → rerun |
| `change.impact-report.edge-paths.pass` | each invalidated node names its condemning path |
| `change.engine-vs-language.pass` | cold vs incremental equality is tested elsewhere; this fixture only asserts the module boundary |

### campaign/ (SC-13)

| Fixture | Expected |
| --- | --- |
| `campaign.transitions.<each>.pass` | planned→approved→running→completed; blocked→running; cancelled; superseded; invalidated |
| `campaign.illegal-transition.fail` | e.g. completed→running |
| `campaign.step-states.<each>.pass` | actor allowed to move it |
| `campaign.step.moved-by-wrong-actor.fail` | provider cannot mark `admitted` |
| `campaign.resume.new-run-reuses.pass` | crashed step only |
| `campaign.human.deadline-escalation.pass` | `CORE-X6401`, no decision produced |
| `campaign.amend-in-flight.pass` | superseded; in-flight steps `cancelled` with receipts |

### authority/ (SC-14, SC-15)

| Fixture | Expected |
| --- | --- |
| `authority.approvals-required.fail` | `CORE-A4501` |
| `authority.signature-over-canonical-bytes.pass/fail` | pretty-printed payload signature → `CORE-V8201` |
| `authority.trust-roots-are-verifier-policy.pass` | producer trusts root, verifier does not → `refused` |
| `authority.neutrality-record-fields.pass` | `avila_provided`, `self_preference_check` mandatory |
| `authority.uncredentialed-client-cannot-apply-judgment.fail` | judgment requires an authority record regardless of client type |
| `authority.agent-cannot-submit.fail` | `runner/submit` without authorization record |
| `authority.key-role-not-cognition.pass` | Core enforces key/role authority and does not claim to detect whether a human used assistance |
| `authority.frontend-cannot-construct-verdict.build` | CI grep: no `Verdict {` / `Admission {` outside kernel |
| `ownership.OM-1..OM-7.pass/fail` | one pair per invariant (in-place edit; use after invalidation; unsigned reuse; duplicate claim; hidden nominal basis; snapshot drift) |

### canon/ (SC-2, ADR-0005)

| Fixture | Expected |
| --- | --- |
| `canon.key-order.pass`, `canon.nfc-rejected.fail`, `canon.no-null.fail`, `canon.float-refused.fail`, `canon.digest-algorithm-id.pass` | NFC is required on input; authoritative readers reject rather than silently normalize signed content |

### scenarios/ (planned end-to-end corpus)

| Fixture | Expected |
| --- | --- |
| `scenarios.coverage-crosses-limit` | INCONCLUSIVE `bounded.le.crossing`; next actions with owners |
| `scenarios.qualification-narrowed` | exact invalidated set; transport reused; activation has no admissible candidate |
| `scenarios.pinned-implementation` | Campaign IR has constraint only; Bound Plan records the excluded candidate as `excluded_by_contract_constraint` |
| `scenarios.review-rejects-upstream` | descendants invalidated; nothing reused; owners named |
| `scenarios.verify-without-evaluator` | obligations report uses the four verifier categories in SC-17 |
| `scenarios.bike-hook` | three findings → plan with rejected Elmer → INCONCLUSIVE → geometry change → memo reuse of `fdm_properties` → PASS → obligations report |

## Counting rule

The future `fixtures-check` command must fail if any clause, rule id, admission
condition, change class, state transition, or diagnostic code referenced in
ADR-0006 lacks a fixture, or if a fixture references an undefined code. Until
that command and the required files exist, this README is a coverage manifest
and ADR-0006 stays proposed.
