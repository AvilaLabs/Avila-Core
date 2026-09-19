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
- `verdict-calculus.v1.json`: 79 vectors: 61 requirement-evaluation vectors
  (including the mixed-state `not_evaluated.mixed-*` edges), 10 categorical
  vectors (every `equals`/`in_set` outcome and edge), plus 8
  aggregate-verdict vectors; and
- `canon.v1.json`: 12 initial canonical-value and byte-reader vectors;
- `types/compiler-cases.v1.json`: 37 executable compiler fixtures: 9 compiled
  cases and 28 rejected cases covering the current R1–R6 subset, claim-model
  sufficiency, coverage validity, equality tolerances, exact unit lowering,
  cascade suppression, independent findings, and source-layer refusals located
  by JSON Pointer; and
- `types/compiler-parameter-cases.v1.json`: 9 executable R7 fixtures: 1
  compiled case and 8 rejected cases covering every parameter value family,
  exact canonical lowering, quantity kinds and domains, undeclared values, and
  draft versus approved placeholder behavior;
- `types/compiler-reproducibility-cases.v1.json`: 9 executable type-level R8
  fixtures: 3 compiled cases and 6 rejected cases covering deterministic,
  seeded-stochastic, and nondeterministic types, material execution factors,
  seeds, and explicit role-scoped nondeterminism policy; and
- `types/compiler-review-cases.v1.json`: 9 executable structural R9 fixtures:
  2 compiled cases and 7 rejected cases covering exact optional presentation
  dossiers, agent routing dispositions, digest-pinned policy identity, explicit
  instructions, and `awaiting_agent` state; and
- `types/compiler-purpose-cases.v1.json`: 6 executable R10 fixtures: 3 compiled
  cases and 3 rejected cases covering governed purpose resolution, exact nominal
  exclusions, unrelated and similarly named purposes, and major-version
  mismatch;
- `defects/defects.v1.json`: 35 executable real-contract defect fixtures. The
  base pair is CASE-001's committed contract+registry verbatim; each fixture
  seeds one realistic defect as JSON-pointer mutations (a typo'd field, an
  unresolvable metric step, a mismatched binding role or media type, an
  unadmitted unit, a dropped seed, a rewritten registry unit factor, …) and
  pins the compiler's exact status, findings (code, class, owner, pointer,
  repair applicability), and — for the one defect the compiler legitimately
  absorbs (a dropped redundant explicit binding, resolved by single-candidate
  auto-binding) — the compiled snapshot identity, which differs from the base
  pair's only through the source-document digest. `generate.py` is the
  regeneration aid; the corpus is pinned, not generated at test time;
- `authority/authority-cases.v1.json`: 4 executable authority fixtures over the
  committed CASE-001 manifest signature and examples trust root — a valid
  signature verifies, a tampered signature is refused, a missing trust-root
  entry is refused, and a requester key listed only under the runner role is
  refused (the key travels with its declared role). Both implementations —
  `avila-core-evidence` and the Python verifier — execute the suite; and
- `campaigns/campaign-cases.v1.json`: 18 executable campaign fixtures covering
  the first SC-10/SC-11 claim-admission and verdict slice, including bounded
  outcomes, exact unit scaling, quarantine paths, snapshot mismatch,
  qualification, `require_qualification` enforcement, and proof that
  presentation policy is not a verdict input.

All other fixtures below are required before ADR acceptance and are currently
planned unless files exist for them.

The `avila-core-kernel` conformance tests currently execute all 12 vectors in
`canon.v1.json`, all 10 vectors in `unit-scaling.v1.json`, and all 19 vectors in
`scope-predicates.v1.json`, plus the 60 requirement, 10 categorical, and 8
aggregation vectors in `verdict-calculus.v1.json`. The compiler harness also executes all 70 cases in
the five compiler manifests plus all 34 real-contract defect cases in
`defects/defects.v1.json`, and pins each registry digest plus all successful
compiled-snapshot identities. The evidence harness executes the four authority cases in `authority/authority-cases.v1.json`. The campaign harness executes all 18 cases in its
manifest and pins every successful campaign identity. Passing the 98 pure
vectors, 70 compiler fixtures, and 15 campaign fixtures does not accept
ADR-0006: full package-level admission, package rule halves, and other vector
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
  campaigns/campaign-cases.v1.json
                             executable manifest for campaign evaluation (SC-10, SC-11)
  campaigns/*.claims.json    exact claims documents named by that manifest
  types/compiler.registry.v1.json
                             exact shared registry input pinned by the manifest
  types/compiler.parameters.registry.v1.json
                             exact parameter registry pinned by the R7 manifest
  types/compiler.reproducibility.registry.v1.json
                             exact determinism registry pinned by the R8 manifest
  types/compiler.review.registry.v1.json
                             exact optional agent-presentation registry pinned by the R9 manifest
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
    "findings": [ { "code": "CORE-R3102", "class": "invalid", "owner": "requester",
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
| `kinds.same-dimension-distinct.fail` | Gy metric vs Sv role → `CORE-T2102` — exercised by `types.R6.limit-kind.fail` |
| `kinds.unit-in-class.pass` | `uSv/h` accepted for `nuclear.dose_equivalent_rate` — exercised by `types.R6.equal-tolerance.pass` |
| `kinds.unit-not-in-class.fail` | a known unit belonging to another class → `CORE-T2301`-adjacent `CORE-T2103`; repair `constrained_choice` = unit class — exercised by `types.R6.limit-unit.fail` |
| `kinds.unit-not-in-profile.fail` | `rem` is absent from the profile's unit table → `CORE-T2001`, same check as `types.R6.unknown-unit.fail` |
| `kinds.unit-symbol-unknown.fail` | `Mpa` → `CORE-T2001`; correction requires confirmation unless a governed typo alias makes identity and scale unique — exercised by `types.R6.unknown-unit.fail` |
| `kinds.prefix-case.fail` | `MSv` (mega-sievert) is not `mSv` → `CORE-T2001` — exercised by `defect.contract.prefix-case` |
| `kinds.cross-kind-conversion.fail` | a wrong-role source bound to a slot → `CORE-T2101` — exercised by `defect.contract.role-mismatched-binding`; the conversion-capability repair path itself is unimplemented |
| `kinds.exact-scaling.pass` | vectors `unit-scaling.v1` |
| `kinds.registry-namespace-owner.fail` | a quantity kind without owner → `CORE-S1102` at `/kinds/*/owner` — exercised by `defect.registry.kind-owner-missing` |

### numerics/ (SC-2)

| Fixture | Expected |
| --- | --- |
| `numerics.decimal-canonical.pass` | `"9.41"` accepted — exercised by `canon.v1.json` vector `decimal.canonical` |
| `numerics.decimal-noncanonical.fail` | `"9.410"`, `"09.4"` → `CORE-S1102` — exercised by `decimal.trailing-zero-rejected` |
| `numerics.json-float.fail` | `25.0` as JSON number in `limit` → `CORE-S1102` — exercised by `json.float-number-rejected`, `types.source.json-float.fail`, and `defect.contract.json-float-limit` |
| `numerics.json-int.pass` | `3` as JSON number for `mesh_refinement_levels` — exercised by `json.integer-accepted` |
| `numerics.nan-inf.fail` | `"NaN"`, `"Infinity"` → `CORE-S1102` — exercised by `json.nan-refused` and `json.infinity-refused` |
| `numerics.display-rounding.pass` | presentation changes, canonical value and verdict do not — exercised by `display_rounding.does-not-change-verdict` |
| `numerics.decision-rounding-without-capability.fail` | outcome-changing rounding must be a typed, qualified capability — the requirement vocabulary is closed, so a rounding declaration inside a requirement is rejected before evaluation (`rounding_cannot_be_smuggled_into_a_requirement`); kernel comparisons are exact (`display_rounding.does-not-change-verdict` pins the no-rounding half) |
| `numerics.rational-factor.pass` | `"1/3600000000"` parsed and reduced — exercised by `rational.canonical` |
| `numerics.rational-noncanonical.fail` | reducible fraction, negative denominator, leading plus/zeros, zero denominator — exercised by `rational.reducible-rejected` and `rational.negative-denominator-rejected` |
| `numerics.decimal-exponent-normalizes.pass` | authored exponent lowers to one canonical plain decimal — exercised by `decimal.exponent-authored-lowering` |
| `numerics.negative-zero.fail` | `"-0"` is noncanonical; canonical value is `"0"` — exercised by `decimal.negative-zero-authored-lowering` and `defect.contract.negative-zero-limit` |

### uncertainty/ (SC-3)

| Fixture | Expected |
| --- | --- |
| `uncertainty.reduce.exact/interval/coverage_interval/worst_case_upper/worst_case_lower/unquantified.pass` | reduced tuples per table — `le.nominal.within` exercises the unquantified side |
| `uncertainty.reduce.standard_uncertainty.fail` | `CORE-T2203` irreducible; repair names `core.uncertainty.expand@1` — exercised by `types.R3.irreducible.fail` |
| `uncertainty.reduce.samples.fail` | `CORE-T2203` — same check as `types.R3.irreducible.fail` |
| `uncertainty.coverage-out-of-range.fail` | coverage `"1.2"` → `CORE-S1102` — exercised by `types.R3.coverage-out-of-range.fail` |
| `uncertainty.numerical-error-separate.pass` | `representation_error` and `numerical_error` are separate optional receipt-output components, shape-checked at verification and never carried onto a claim — exercised by `disclosed_error_components_are_recorded_and_shape_checked` and `error_disclosures_do_not_exist_on_a_claim` |

### roles/ (SC-4)

| Fixture | Expected |
| --- | --- |
| `roles.nominal-identity.fail` | same kind, different role id → `CORE-T2101` — exercised by `types.R2.role-mismatch.fail` |
| `roles.major-version.fail` | `role@2` offered to `role@1` slot → `CORE-T2101` — the resolution half is exercised by `defect.contract.role-major-version` (input references `role@2` absent from the registry → `CORE-R3101`); the binding-level T2101 variant needs a registry carrying both majors |
| `roles.minor-version.pass` | `role@1` with extra optional attribute accepted — needs the ADR-0025 minor-version compatibility model (proposed) |
| `roles.validator-required.fail` | role without validator → `CORE-R3501` — exercised by `defect.registry.missing-validator` |
| `roles.attribute-undeclared-in-predicate.fail` | predicate references undeclared attribute → `CORE-T2701` under the ADR-0025 role-attribute vocabulary (proposed) |
| `roles.cardinality-slot-scoped.pass` | two inputs carrying the same role (`actinv-decay-primary`/`actinv-decay-fallback` on `actinv.decay-data`) are distinct admitted records, never a global duplicate — `case_000_is_reproducible_and_technically_evaluated`; predicate addressing stays slot-scoped — `inputs_carrying_the_same_role_are_addressed_by_slot` |

### types/ (SC-5, SC-6)

| Fixture | Expected |
| --- | --- |
| `types.R1.resolved.pass` | unique source bound |
| `types.R1.unresolved.fail` | `CORE-R3101`; repair `constrained_choice` lists `declare_input:<role>` and every `add_step:<type>/<output>` in the snapshot that could feed the slot |
| `types.R1.ambiguous.fail` | two producers, no binding → `CORE-R3102`; repair `constrained_choice` |
| `types.R1.explicit-binding.pass` | `bindings` resolves ambiguity; the input left unbound is reported as notice `CORE-R3601` |
| `types.R2.role-mismatch.fail` | `CORE-T2101` |
| `types.R3.type-satisfiable.pass` | capability type permits a model capable of satisfying the basis — exercised by every compiled fixture, e.g. `types.R1.resolved.pass` |
| `types.R3.bound-package-coverage-sufficient.pass` | declared coverage meets the basis exactly — exercised by `le.bounded.coverage-meets-basis` |
| `types.R3.coverage-insufficient.fail` | declared coverage below the basis is a verdict-level refusal `CORE-S1102` — `coverage_below_the_requirement_basis_refuses_evaluation` |
| `types.R3.unquantified-governed.fail` | an unquantified claim cannot satisfy a quantified basis → `CORE-T2201` (the profile has no distinct T2202) — executable fixture |
| `types.R3.unquantified-nominal.pass` | policy permits nominal basis — executable fixture |
| `types.R3.irreducible.fail` | `CORE-T2203` |
| `types.R3.enclosure-needs-interval.fail` | coverage_interval offered to `enclosure` → `CORE-T2201` |
| `types.R3.coverage-basis.pass` | `bounded` basis with coverage `"0.95"` compiles and retains the coverage |
| `types.R3.coverage-out-of-range.fail` | coverage `"1.2"` → `CORE-S1102` at `/requirements/0/basis/coverage`; a non-canonical coverage such as `"0.950"` carries a `mechanically_safe` repair |
| `types.R3.coverage-without-bounded-basis.fail` | coverage on an `enclosure` or `nominal` basis → `CORE-S1102` |
| `types.R3.worst-case-side-sufficient.pass/fail` | bound side is checked against comparison direction at bind time — the `.fail` half is `types.R3.worst-case-side-insufficient.fail` |
| `types.R4.media.pass/fail` | `CORE-T2301`; the `.pass` half is every compiled fixture's media-matched binding, e.g. `types.R1.resolved.pass` |
| `types.R5.self-dependency.fail` | `CORE-R3201` |
| `types.R5.cycle.fail` | `CORE-R3202` |
| `types.R5.unknown-step.fail` | `CORE-R3203` |
| `types.R5.unknown-output-slot.fail` | a known step with no such output slot → `CORE-R3203` naming the declared outputs |
| `types.R6.unbound-metric.fail` | `CORE-R3301` |
| `types.R6.limit-kind.fail` | `CORE-T2102` |
| `types.R6.limit-unit.fail` | `CORE-T2103` |
| `types.R6.equal-tolerance.pass` | `equal` with a nonnegative tolerance of the metric kind lowers the tolerance to the canonical unit |
| `types.R6.equal-no-tolerance.fail` | `equal` without a tolerance → `CORE-T2104` `missing`; the kernel could never evaluate it |
| `types.R6.tolerance-without-equality.fail` | a tolerance on an inequality → `CORE-T2104` `invalid`, with a mechanically safe removal |
| `types.R6.nominal-basis-unpermitted.fail` | a `nominal` basis without `permit_nominal_basis` → `CORE-A4201`; the edit that permits it is offered |
| `types.R6.nominal-basis-permitted.pass` | the same requirement compiles once the execution policy permits the weakening |
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
| `types.R9.review-bound.pass` | exact dossier, agent routing dispositions, policy identity, and instructions compile to `presentation_gate.state = awaiting_agent` |
| `types.R9.explicit-none.pass` | an explicit lack of separation metadata is preserved; it never affects a technical verdict |
| `types.R9.missing-review.fail` | a configured presentation capability without its contract policy binding → `CORE-R3401` |
| `types.R9.policy-digest.fail` / `policy-revision.fail` | agent policy must be pinned by a valid immutable identity → `CORE-R3401` |
| `types.R9.empty-independence.fail` / `duplicate-independence.fail` | constraint mode is nonempty and unambiguous → `CORE-R3401` |
| `types.R9.binding-on-non-review.fail` | a normal capability cannot acquire review semantics from contract syntax → `CORE-R3401` |
| `types.R9.nondeterminism-refused.fail` | a nondeterministic routing output still requires explicit R8 role-scoped permission → `CORE-A4301` |
| `types.R10.allowed.pass` | a resolved purpose not excluded by the producing output is retained in compiled IR |
| `types.R10.excluded.fail` | an exact output-purpose exclusion → `CORE-T2601` |
| `types.R10.unrelated-purpose.pass` | excluding one purpose does not exclude unrelated governed identities |
| `types.R10.nominal-near-name.pass` | `screening@1` exclusion does not match `screening_research@1`; no prefix or prose inference |
| `types.R10.unknown-purpose.fail` / `major-version.fail` | purpose identity must resolve exactly in the pinned registry → `CORE-T2601` |
| `types.satisfiable.pass` | a path of roles reaches the basis — exercised by every compiled fixture, e.g. `types.R1.resolved.pass` |
| `types.satisfiable.fail` | no capability-type path can emit a reducible bounded model → `CORE-T2201` at the requirement; package admissibility is tested separately — exercised by `types.R3.unquantified-governed.fail` |
| `types.independent-errors-one-pass.pass` | three unrelated errors reported together |
| `types.notice.unused-input.pass` | an input bound to no step compiles with notice `CORE-R3601` at the input |
| `types.notice.unconsumed-step.pass` | a non-review step whose outputs feed nothing compiles with notice `CORE-R3602` at the step |
| `types.source.json-float.fail` | binary float refused at `/workflow/0/parameters/x` → `CORE-S1102` |
| `types.source.unknown-field.fail` | undeclared key refused at its own pointer → `CORE-S1101` |
| `types.source.noncanonical-decimal.fail` | `"100.0"` refused at `/requirements/0/limit/value` → `CORE-S1102`; repair `mechanically_safe` = `"100"` |
| `types.source.duplicate-key.fail` | repeated object key refused at the key → `CORE-S1103` |
| `types.source.independent-refusals.pass` | a float, a `null`, an unknown field, a non-canonical number, a wrong variant, and a missing required property are all reported in one pass, each at its own pointer |
| `types.cascade-suppressed.pass` | consequence of a root error summarized under it — exercised by `types.cascade.unknown-type-suppressed.pass` |
| `types.cascade.unknown-type-suppressed.pass` | a step of unknown capability type is reported once at its `capability_type`; bindings and metrics naming its outputs are suppressed, not reported as nonexistent |
| `types.cascade-preserves-independent.pass` | a dependency-blocked step's own `CORE-P5101` reports beside the upstream refusal, not under it — exercised by `a_dependency_blocked_step_still_reports_its_own_selection_refusal` |
| `types.conversion-capability.pass` | Gy→Sv via `core.convert.absorbed_dose_to_dose_equivalent@1` type-checks with the weighting role bound — exercised by `a_cross_kind_conversion_is_an_explicit_capability_type` |
| `types.presentation-agent.pass` | optional agent presentation request binds an exact dossier and stays outside campaign admission — exercised by `types.R9.review-bound.pass` and `campaign.practical-review.pass` |
| `types.partial-outputs.pass` | `permits_partial` on the output slot admits a claim whose receipt declared `partial` — exercised by `campaign.partial.permitted.pass` |
| `types.partial-outputs.fail` | a `partial` claim on a slot without `permits_partial` quarantines → `CORE-E7201` — exercised by `campaign.partial-undeclared.fail` |

### applicability/ (SC-7)

| Fixture | Expected |
| --- | --- |
| `scope.pred.all/any/not.pass` | boolean structure — exercised by `all.false-dominates-unknown`, `all.true-and-unknown`, `any.true-dominates-unknown`, `not.unknown-stays-unknown` |
| `scope.pred.param_in_range.true/false/unknown` | vectors `scope-predicates.v1`; missing endpoints are omitted, never `null` |
| `scope.pred.param_in_range.unit-scaled.true` | min `24 h`, fact `1440 min` |
| `scope.pred.param_in_set.*`, `input_attribute_in.*`, `input_attribute_in_range.*`, `environment_image_in.*`, `platform_in.*`, `fact.*` | true/false/unknown each |
| `scope.exclusions.fail` | exclusion matches → `CORE-A4101` — exercised by `exclusion.matches.false` |
| `scope.unknown-never-true.pass` | `unknown` under `not` stays `unknown` — exercised by `not.unknown-stays-unknown` |
| `scope.record.active.pass` | admitted — exercised by `a_run_inside_the_envelope_carries_the_qualification_on_its_claims` |
| `scope.record.superseded.fail` | `CORE-A4101` — a bound newer record names the dead record's digest in `supersedes`; the dead record's assessment attaches marked `superseded_by` — exercised by `campaign.qualification-superseded.not_evaluated`, `a_superseded_record_attaches_marked_and_the_campaign_refuses_it`, and `a_live_record_serves_the_step_its_dead_peer_also_matches` |
| `scope.record.expired.fail` | `CORE-A4602` — exercised by `campaign.qualification-expired.not_evaluated`, `a_lapsed_record_is_expired_whatever_its_terms_say`, and `a_lapsed_qualification_is_expired_and_cannot_establish_a_bounded_requirement`; a claim regenerated from a committed receipt keeps the state of its producing instant (`a_reused_claim_keeps_the_state_of_its_producing_instant`) |
| `scope.record.revoked.fail` | `CORE-A4603` — a bound `qualification_revocation` names the record's digest; under issuer recognition it must verify under the declared key (`CORE-X3405` ignores an unauthenticated withdrawal) — exercised by `campaign.qualification-revoked.not_evaluated`, `a_package_asserted_revocation_marks_the_record_and_the_campaign_refuses_it`, `an_unsigned_revocation_of_a_recognized_record_is_ignored`, and `a_signed_revocation_withdraws_a_recognized_record` |
| `scope.record.not-recognized.fail` | `CORE-A4601` under `execution_policy.recognized_qualification_owners` — the record's `owner` is not a listed issuer — exercised by `campaign.qualification-not-recognized.not_evaluated` and `an_unlisted_owners_assessment_attaches_but_the_campaign_refuses_it`; a listed owner must also sign the record under the declared key or the runner refuses it (`CORE-X3404`) — `a_recognized_owners_signed_record_is_applied_and_names_its_issuer`, `a_listed_owners_unsigned_record_is_refused`, `a_listed_owners_record_signed_by_another_key_is_refused` |
| `scope.record.digest-bound.fail` | record binds the capability's exact `executable_sha256`; a digest the step does not run is refused at binding — exercised by `a_qualification_for_a_different_executable_is_refused` |
| `scope.maturity-migration.pass` | not applicable — no v0.1 contract format ever shipped in this repository (`schemas/` carries only `v0.2-draft` for contract, registry, and claims), so there is no historical `qualified` → `released` or `allow_unqualified_capabilities` vocabulary to migrate from; deferred unless a real v0.1 corpus is supplied |
| `scope.admission-not-in-manifest.fail` | manifest field `admitted` → unknown-field refusal at package parse — exercised by `admission_is_not_a_manifest_field` |
| `scope.a7-actual-context.fail` | the runner evaluates the envelope over the actual extracted facts before the step runs; an out-of-envelope actual context quarantines → `CORE-A4401` — exercised by `a_run_outside_the_envelope_cannot_establish_a_bounded_requirement` |
| `scope.repeated-role-slot-addressing.pass` | `context.inputs` is keyed by slot and predicates address `inputs.<slot>` — two inputs carrying the same role stay distinct — exercised by `inputs_carrying_the_same_role_are_addressed_by_slot` |
| `scope.fact.provider-asserted-insufficient.fail` | provider assertion cannot satisfy a predicate requiring validated-input or runner-measured authority — exercised by `inside_outside_and_unknown_are_reported_per_term` (`claimed` → `unknown`) |
| `scope.fact.source-and-validator.pass` | fact type, source, validator, and receipt match qualification requirements — exercised by `a_run_inside_the_envelope_carries_the_qualification_on_its_claims` (`runner_measured` + adapter validator accepted) and the verifier's `_check_context_receipt_binding` |
| `scope.expiry.explicit-time.pass` | expiry compares `not_after` against the producing receipt's `started_at` — the signed evaluation-time record; the kernel never reads a clock and claims carry no run-varying stamp — exercised by `a_lapsed_record_is_expired_whatever_its_terms_say`, `a_lapsed_qualification_is_expired_and_cannot_establish_a_bounded_requirement`, and `a_reused_claim_keeps_the_state_of_its_producing_instant` |
| `scope.resource-limits.fail` | the predicate evaluator's depth and node bounds (`max_depth`/`max_nodes`) error → `unknown` → quarantined, never an affirmative verdict or a hang — exercised by `deeply_nested_predicate_fails_closed`, `a_wide_predicate_fails_closed_on_the_node_limit`, and `a_scope_term_over_the_resource_limit_is_unknown_not_affirmative` |

### policy/ (SC-8)

| Fixture | Expected |
| --- | --- |
| `policy.lattice.contract-weaker.fail` | `CORE-A4201` — exercised by `types.R6.nominal-basis-unpermitted.fail` |
| `policy.lattice.contract-tighter.pass` | the `organization_policy` document and `tightens` merge order are proposed in ADR-0023 (the ADR-0020 deferral) |
| `policy.rule.maturity_floor.pass/fail` | `CORE-P5303` — `a_undeclared_maturity_fails_a_declared_floor`, `a_declared_maturity_meeting_the_floor_runs`; verifier `test_undeclared_maturity_fails_the_floor`, `test_declared_maturity_meeting_the_floor_verifies` |
| `policy.rule.require_qualification.fail` | `CORE-A4601` — exercised by `campaign.require-qualification.unqualified.not_evaluated` |
| `policy.rule.deny_providers.fail`, `allow_capabilities.pass` | `CORE-P5301` — `a_denied_provider_is_refused_before_any_execution`, `a_selection_outside_the_allow_list_is_refused`, `a_recorded_selection_matching_the_bound_capability_runs` |
| `policy.rule.environments.fail` | remote env under local-only policy — needs a package-bound environment record type (ADR-0020 deferred; not yet designed) |
| `policy.rule.independence.fail` | `CORE-P5302` — `two_steps_sharing_a_provider_violate_independence`; verifier `test_shared_provider_violates_independence` |
| `policy.rule.diversity.pass` | `CORE-P5304` — `two_steps_sharing_an_executable_violate_diversity`; verifier `test_shared_executable_violates_diversity` |
| `policy.rule.forbid_self_preference.pinned.pass` | `an_avila_provided_selection_with_the_check_runs` — the recorded `self_preference_check` carries the justification |
| `policy.rule.forbid_self_preference.unpinned.fail` | `CORE-P5501` — `an_avila_provided_selection_without_the_check_is_refused`; verifier `test_avila_provided_without_the_check_is_a_mismatch` |
| `policy.rule.permit_nominal_basis.fail` | nominal requirement under governed policy → `CORE-A4201` — exercised by `types.R6.nominal-basis-unpermitted.fail` |
| `policy.rule.presentation-cannot-gate-verdict.pass` | optional routing policy never enters verdict calculus — exercised by `presentation-policy.not-a-verdict-input` |
| `policy.rule.separation_of_duties.fail` | two technical roles violate an explicitly declared organization policy → `CORE-A4502` — needs the ADR-0021 attestation record's signed role assertions (proposed) |
| `policy.rule.separation_of_duties.waived.pass` | waived; `waived_controls` recorded in package — needs the ADR-0021 attestation record carrying the waiver (proposed) |
| `policy.rule.cost_caps.pass` | `CORE-P5401` — `a_cost_over_the_cap_without_confirmation_is_refused`; a confirmed over-cap selection runs — `a_cost_over_the_cap_with_confirmation_runs`; verifier `test_cost_over_the_cap_*` |
| `policy.selection.every-candidate-decided.pass` | `CORE-P5602` — `a_candidate_without_a_recorded_decision_is_refused`; verifier `test_undecided_candidate_is_a_mismatch` |
| `policy.selection.rank-order.pass` | the declared criteria vocabulary is enforced — `a_banned_criterion_is_refused`; the ordering itself is producer-asserted data — the engine verifies the recorded selection, it does not rank |
| `policy.selection.tie-break.pass` | not applicable by design — tie-break ordering is producer-side data; the engine verifies a recorded selection (ADR-0020) |
| `policy.selection.transparent-cost.pass` | `cost_estimate`/`cost_confirmed_by` recorded on the selection — `a_cost_over_the_cap_with_confirmation_runs`, `a_cost_over_the_cap_without_confirmation_is_refused` |
| `policy.selection.hidden-margin.fail` | `CORE-P5601` — `a_banned_criterion_is_refused`; verifier `test_banned_criterion_is_a_mismatch` |
| `policy.selection.no-eligible.fail` | `CORE-P5101` — `a_selection_with_no_eligible_candidate_is_refused`; verifier `test_no_winner_is_a_mismatch` |
| `policy.selection.excluded-by-constraint.pass` | `CORE-P5201` notice — `a_candidate_excluded_by_a_contract_constraint_carries_a_notice_and_runs` |
| `policy.explainability.pass` | decision reasons carry `rule_id` + fact — enforced structurally via `CORE-P5602`, `a_candidate_without_a_recorded_decision_is_refused` |
| `policy.selection.record-consistency.fail` | `CORE-P5102`/`CORE-P5103` — `a_selection_naming_a_different_capability_is_refused`, `a_selection_describing_a_different_registry_snapshot_is_refused`, `an_active_policy_without_a_selection_record_is_refused` |

### lifecycle/ (SC-9)

| Fixture | Expected |
| --- | --- |
| `lifecycle.status.draft.permits-check.pass` | exercised by every compiled fixture — e.g. `types.R1.resolved.pass` |
| `lifecycle.status.draft.refuses-submit.fail` | the ADR-0021 transition record exists — `draft→in_review` is the only forward move from `draft`, so a direct `draft→approved` "submit" fails legality (`contract_vocabulary_and_legality_table`); contract status is a document-owner label, not an execution gate |
| `lifecycle.status.in_review.refuses-edit.fail` | amend → new draft — the ADR-0019 amendment record (`an_amendment_admits_a_new_root_across_changed_fixed_identities`) and ADR-0021 legality table (`contract_vocabulary_and_legality_table`: `in_review→draft|approved` only) make an in-place semantic move unrecordable |
| `lifecycle.status.approved.permits-bind.pass` | status is carried in the compiled boundary; `approved` permits binding because contract status never gates execution — the ADR-0021 transition records that set it exist (`contract_vocabulary_and_legality_table`) |
| `lifecycle.status.retired.read-only.pass` | `approved→retired` is the last legal contract move and `retired` is terminal — the legality table refuses every outgoing edge (`contract_vocabulary_and_legality_table`) |
| `lifecycle.instantiation-is-origin.pass` | needs the ADR-0022 `contract_template` document type (proposed) |
| `lifecycle.campaign-state-not-contract-state.pass` | pinned by `campaign_states_do_not_exist_on_a_contract` — the contract vocabulary is closed, so a campaign state cannot be named |
| `lifecycle.template.instantiate.eligible.pass` | needs the `contract_template` document type (not yet designed) |
| `lifecycle.template.ineligible.fail` | needs the ADR-0022 `contract_template` document type (proposed; fresh code allocation) |
| `lifecycle.template.eligibility-unknown.fail` | needs the ADR-0022 `contract_template` document type (proposed; fresh code allocation) |
| `lifecycle.template.default-provided_by.pass` | boundary names template digest — needs the ADR-0022 `contract_template` document type (proposed) |
| `lifecycle.template.instance-pins-version.pass` | template amendment does not touch instance; `template_superseded` notice — needs the ADR-0022 `contract_template` document type (proposed) |
| `lifecycle.template.validation-cases-must-compile.fail` | template not approvable — needs the ADR-0022 `contract_template` document type (proposed) |
| `lifecycle.template.policy-only-tightens.fail` | instance loosening → `CORE-A4803` under the ADR-0022 proposal |
| `lifecycle.amend.new-version-supersedes.pass` | the ADR-0019 amendment record links roots across changed fixed identities (`an_amendment_admits_a_new_root_across_changed_fixed_identities`); the ADR-0021 `superseded` campaign transition exists — `amend` does not yet emit it automatically |
| `lifecycle.completion.pass-only.pass` | `a_declared_pass_verdict_completes` — the contract `completion` block declares fulfilling verdicts; the campaign assesses delivery |
| `lifecycle.completion.permitted-inconclusive.pass` | `an_inconclusive_verdict_completes_only_under_a_permitted_reason` — a listed reason completes; an unlisted one does not |
| `lifecycle.completion.not_evaluated-never.fail` | `not_evaluated_never_completes` at assessment, `a_completion_block_cannot_declare_not_evaluated_fulfilling` (`CORE-A4701`) at declaration |

### verdict/ (SC-10) — see `vectors/verdict-calculus.v1.json` for kernel vectors

| Fixture | Expected |
| --- | --- |
| `verdict.le.bounded.within/exceeds/crossing/one_sided` | vectors |
| `verdict.ge.bounded.within/below/crossing/one_sided` | vectors `ge.bounded.within`, `ge.bounded.below`, `ge.bounded.crossing`, `ge.bounded.upper_only.below`, `ge.bounded.upper_only.inconclusive` |
| `verdict.lt.bounded.boundary` | hi = L → crossing, not PASS |
| `verdict.gt.bounded.boundary` | lo = L → crossing — exercised by `gt.bounded.boundary`; the `gt` family is `gt.bounded.within/below/crossing/boundary` |
| `verdict.enclosure.*` | as bounded with coverage 1 required — exercised by `le.enclosure.within`, `le.enclosure.crossing`, `lt.enclosure.within`, `gt.enclosure.below`, `ge.enclosure.crossing`, `equal.enclosure.outside`, `aggregation.max.enclosure`, `aggregation.min.enclosure` |
| `verdict.nominal.within/exceeds` | vectors `le.nominal.within`, `le.nominal.exceeds`, `equal.nominal.within` |
| `verdict.equal.within/outside/partial` | vectors |
| `verdict.equal.no-tolerance.fail` | `CORE-T2104`; the compile-time half is covered by `types.R6.equal-no-tolerance.fail` |
| `verdict.display-rounding-does-not-change.pass` | exact canonical values determine the verdict — exercised by `display_rounding.does-not-change-verdict` |
| `verdict.decision-rounding-capability.pass` | admitted transformation output is compared exactly and raw input remains linked — exercised by `verdict.decision-rounding-capability.pass` (raw `95.34` rounds to `95.3` under the declared `half_up`/`0.1` quantum; the verdict compares the rounded output, which would fail on the raw) and `a_decision_rounding_capability_declares_its_transformation` |
| `verdict.unit-scaling-exact.pass` | 100 uSv/h limit vs Sv/s evidence — exercised by `le.bounded.unit-mixed` and `campaign.unit-scaled.pass` |
| `verdict.aggregation.all.*`, `any.*` | current precedence examples plus planned exhaustive and property-generated truth tables |
| `verdict.aggregation.max/min.enclosure` | side-aware max/min reduction; a side that cannot be bounded remains absent |
| `verdict.aggregation.coverage-needs-capability.fail` | marginal coverage intervals are not assigned joint coverage by the kernel — exercised by `aggregation.coverage.requires-capability` |
| `verdict.one-sided.lower/upper.*` | both comparison directions and strict boundaries — exercised by `le.bounded.one_sided`, `le.bounded.lower_only.*`, `ge.bounded.upper_only.*`, `lt.bounded.lower_only.*`, `lt.bounded.upper_only.within`, `gt.bounded.lower_only.within`, `gt.bounded.upper_only.*` |
| `verdict.equal.nominal/one-sided.*` | nominal tolerance and one-sided contradiction rules — exercised by `equal.within`, `equal.outside`, `equal.partial`, `equal.nominal.within`, `equal.nominal.outside`, `equal.nominal.boundary`, `equal.one_sided.outside`, `equal.one_sided.partial` |
| `verdict.not_evaluated.missing/quarantined/invalidated` | reasons and owners listed; no numbers — exercised by `not_evaluated.missing`, `not_evaluated.quarantined`, `not_evaluated.invalidated`, `not_evaluated.mixed-admitted-quarantined`, `not_evaluated.mixed-admitted-invalidated` |
| `verdict.not_evaluated.duplicate-claim` | `CORE-E7301` — exercised by `not_evaluated.duplicate-claim` and `campaign.duplicate-claim.quarantine` |
| `verdict.presentation-policy.not-an-input.pass` | the same admitted claims produce the same verdict with or without optional presentation routing — exercised by `presentation-policy.not-a-verdict-input` and `presentation-policy.cannot-mask-fail` |
| `verdict.record-fields.pass` | every field of `avila.core/verdict/v0.2` present — the vector harness compares each `verdict-calculus` vector's full serialized record |
| `verdict.evaluator-identity.pass` | `kernel:verdict-calculus@1` — every `verdict-calculus` vector's record is produced by that evaluator |
| `verdict.core-requirement-evaluation-step.pass` | specimen step type maps to kernel — exercised by every compiled fixture carrying a `fixture.requirement_evaluation` requirement, e.g. `types.R1.resolved.pass` |
| `verdict.kernel-bug-guard.fail` | the bounds tables are total over their inputs — no undefined `hi` path exists to guard — `bounded_inequality_tables_cover_every_small_interval` and `equality_table_covers_every_small_interval` |

### admission/ (SC-11)

| Fixture | Expected |
| --- | --- |
| `admission.A1..A8.pass` / `admission.A10.pass` | each pass condition runs inside the admitted-claims fixtures — `campaign.le.within.pass` exercises A1–A6, A7-vacuous, A8, and A10; `campaign.require-qualification.qualified-inside.pass` exercises A7-as-required — dedicated per-condition pinpoint fixtures remain |
| `admission.A1.hash-mismatch.fail` | `CORE-E7101` → quarantined — exercised by `campaign.snapshot-mismatch.rejected` |
| `admission.A2.foreign-receipt.fail` | a receipt produced for another case is never reused — `ChangeClass::DifferentCase` — exercised by `a_receipt_copied_from_a_donor_package_is_refused` |
| `admission.A2.untrusted-runner-key.fail` | a receipt signature that does not verify under a listed runner key is never reused — `ChangeClass::ReceiptSignatureInvalid` — exercised by `a_receipt_signed_by_an_unlisted_runner_key_is_not_reused` and `a_hand_forged_receipt_signature_does_not_verify` |
| `admission.A3.unadmitted-parent.fail` | `CORE-E7103`; cascade to root — exercised by `campaign.parent-missing.not_evaluated` |
| `admission.A4.package-mismatch.fail` | `CORE-E7001` (the profile has no distinct E7104) — exercised by `campaign.snapshot-mismatch.rejected` |
| `admission.A5.exit-zero-insufficient.fail` | exit 0 with a declared output missing → `CORE-X2501`; nothing admitted — `a_clean_exit_without_the_declared_output_is_not_evidence` |
| `admission.A5.timeout/crash/sandbox.fail` | timeout and crash land as `CORE-X2501` — `timeout_kills_the_whole_process_group` and `a_failing_execution_produces_a_failed_receipt_and_no_verdict`; a sandbox boundary is not implemented |
| `admission.A6.validator-rejected.fail` | `CORE-E7201` — exercised by `campaign.model-not-permitted.quarantine` |
| `admission.A6.model-mismatch.fail` | declared interval-only slot, emitted unquantified → `CORE-E7201` — exercised by `campaign.model-mismatch.quarantine` |
| `admission.A7.actual-context.fail` | the envelope evaluates over the actual extracted facts; an out-of-envelope actual context quarantines → `CORE-A4401` — exercised by `a_run_outside_the_envelope_cannot_establish_a_bounded_requirement` (same mechanism as `scope.a7-actual-context.fail`) |
| `admission.A7.vacuous-recorded.pass` | no qualification required → sub-record says so — every admitted-claims fixture without `require_qualification`, e.g. `campaign.le.within.pass` |
| `admission.A8.policy-changed.fail` | claims bound to a different compiled snapshot cannot be attributed — `CORE-E7001` — exercised by `campaign.snapshot-mismatch.rejected`; SC-12 deliberately does not invalidate receipts for a requirement/policy edit — verdicts re-derive over the same evidence (`a_requirement_change_reuses_evidence_and_recomputes_verdicts`) |
| `admission.A9.pass` | the ADR-0024 machinery exists: `staged_review_record` (the gate's `decision_role` is literally `core.presentation.routing-record`) binds `request_sha256` + `record_sha256` digests, the allowed disposition, and the pinned eligibility policy — `a_routing_record_verifies_and_each_broken_binding_quarantines` exercises every binding |
| `admission.A10.ancestor-invalidated.fail` | `CORE-E7103` cascade — `campaign.parent-missing.not_evaluated` leaves every descendant `not_evaluated` |
| `admission.state.quarantine-terminal.pass` | a quarantined record never contributes to a verdict — `campaign.model-not-permitted.quarantine`; re-evaluation rederives the quarantine deterministically |
| `admission.presentation.present/return/abstain.pass` | closed routing dispositions; technical verdict unchanged — exercised by `campaign.practical-review.pass` and `campaign.practical-review.fail` |
| `admission.presentation.cannot-edit-artifact.fail` | a record bound to a rewritten dossier quarantines (`CORE-X6501`) — `a_routing_record_verifies_and_each_broken_binding_quarantines` mutates `campaign_sha256` inside `review_request`; the record carries no artifact-mutation machinery by construction, so the finding marks the record and the artifact/verdict stay byte-identical |
| `admission.non-artifact-records.pass` | snapshot, approvals, selection, preflight, change events are records — receipts, qualification records, revocations, reuse rules, signatures, staged-review records, and provider-selection records exist; approvals fold into the ADR-0021 attestation record (proposed); the preflight record type is still undesigned |
| `admission.sub-record-replayable.pass` | the independent verifier replays admission and verdicts from the committed documents alone — `test_every_campaign_fixture` |
| `admission.undeclared-output-discarded.pass` | `CORE-X6301` note; not evidence — the claims-level half is `campaign.undeclared-slot.rejected` (`CORE-E7002` when a claim names a slot the step does not declare); the artifact-file discard half remains runner-level |
| `admission.validator-does-not-establish-truth.pass` | obligations report describes the validator's narrow responsibility — no obligations report exists yet |
| `admission.as-of-historical.pass` | the same material verified as_of its recorded timestamp returns the recorded states — `avila_core_verify.py verify-case --as-of <instant>`; `test_as_of_lines_emit_alongside_recorded_checks` reproduces all 9 recorded states on CASE-002 |
| `admission.current.pass` | the same material verified under a supplied snapshot may return a distinct labeled result — `--as-of-material <package dir>` supplies the ADR-0006 policy/qualification/revocation snapshot; `test_supplied_material_snapshot_labels_the_revoked_record` labels transport claims `revoked` while siblings stay `inside` |

### change/ (SC-12)

| Fixture | Expected |
| --- | --- |
| `change.class.<each>.pass` | default invalidation set for every change class defined by SC-12 — classes emitted by `changes_since`; several pinned across the adversarial suite |
| `change.input_metadata.non-dependence.pass` | attribute not consulted → no invalidation — needs the ADR-0025 `input_metadata` field (proposed); the `(step, input_slot)` edge scope reuse rules exempt is implemented |
| `change.propagation.stops-at-unrelated.pass` | upstream and sibling nodes untouched — `a_two_step_chain_reruns_only_what_a_change_reaches` and `a_plan_reports_the_impact_of_every_change_origin` |
| `change.propagation.selected_over-never.pass` | |
| `change.requirement.verdict-only.pass` | evidence untouched — `a_requirement_change_reuses_evidence_and_recomputes_verdicts` |
| `change.reuse-rule.applies.pass` | `reused_under` names rule id — `a_signed_reuse_rule_permits_reuse_across_its_scoped_edge` |
| `change.reuse-rule.condition-unknown.rerun.pass` | unknown applicability → rerun — `a_reuse_rule_without_a_trust_root_fails_closed` |
| `change.reuse-rule.authority-mismatch.fail` | `CORE-X3401` — `a_reuse_rule_signed_by_a_runner_key_is_refused` |
| `change.reuse-rule.expired.fail` | `an_expired_reuse_rule_is_refused_and_reruns` |
| `change.reuse-rule.only-narrows.fail` | rule attempting to widen → refused — `a_reuse_rule_scoped_beyond_the_binding_edges_is_refused` |
| `change.memo.deterministic-hit.pass` | identical invocation digest → `reused` — `a_plan_reports_the_impact_of_every_change_origin` names the `deterministic_memo` reuse authority |
| `change.memo.seeded-same-seed.pass` / `different-seed.fail` | seed enters argv → invocation identity — `transport_arguments_carry_seed_and_parameters` |
| `change.memo.nondeterministic-never.fail` | `a_nondeterministic_step_never_reuses_its_committed_receipt` — `ChangeClass::Nondeterministic` defeats reuse |
| `change.memo.admission-under-new-policy.fail` | old evidence, new policy forbids → rerun; the re-evaluation half is `a_requirement_change_reuses_evidence_and_recomputes_verdicts` |
| `change.memo.validator-version.fail` | a descriptor change — a validator version bump — is invocation identity → `ChangeClass::Invocation` — `an_adapter_descriptor_edit_invalidates_the_committed_receipt` |
| `change.impact-report.edge-paths.pass` | each invalidated node names its condemning path — `impact.invalidated[].condemned_by` on the bound plan, pinned by `a_plan_reports_the_impact_of_every_change_origin` |
| `change.engine-vs-language.pass` | cold vs incremental equality is tested elsewhere; this fixture only asserts the module boundary — `authority_boundaries` |

### campaign/ (SC-13)

| Fixture | Expected |
| --- | --- |
| `campaign.transitions.<each>.pass` | the closed legality table is pinned — `campaign_vocabulary_and_legality_table` (every legal edge accepted, forward-skip/backward/self/terminal-exit refused); derived state folds in append order (`an_attestation_and_transition_derive_the_campaign_state`, `a_terminal_subject_rejects_every_move`); the committed-fixture form does not exist |
| `campaign.illegal-transition.fail` | `CORE-X6402` — `an_illegal_transition_edge_is_refused` (`planned→completed`, self-transition), `a_hand_written_line_claiming_the_wrong_from_state_fails_closed` (a forged `from_state` fails the fold) |
| `campaign.step-states.<each>.pass` | step states are derived from receipts and run rows, never recorded — the only recorded move is cancellation, legal from any non-cancelled position and runner-keyed (`step_vocabulary_and_legality`, `step_states_use_their_own_vocabulary`) |
| `campaign.step.moved-by-wrong-actor.fail` | `CORE-X6403` — a transition requires the role its class names (`a_transition_under_the_wrong_role_is_refused`); `admitted` is not a recordable step state at all — evidence derives it — so no attestation can write it |
| `campaign.resume.new-run-reuses.pass` | crashed step only — `a_resumed_run_reuses_the_completed_steps_and_executes_only_the_crashed_one` (activation's committed receipt reuses; the receipt-less crashed step executes); the campaign-state record exists (ADR-0021 `state_transition`) |
| `campaign.human.deadline-escalation.pass` | `CORE-X6401` — a prior gate row's `respond_by` earlier than `now` emits a notice once per `request_sha256` and no decision (`a_lapsed_presentation_gate_deadline_is_found_once_per_request`); the routing record that would resolve the gate is ADR-0024 (proposed) |
| `campaign.amend-in-flight.pass` | the record exists — `running→superseded` under a requester attestation with `step_effects` naming cancelled steps (`campaign_vocabulary_and_legality_table`, `step_states_use_their_own_vocabulary`); `amend` does not yet emit the supersession transition automatically |

### authority/ (SC-14, SC-15)

| Fixture | Expected |
| --- | --- |
| `authority.approvals-required.fail` | `CORE-A4501` — the ADR-0021 `attestation` record and role-checked authorization exist (`a_transition_under_the_wrong_role_is_refused`); a requirement *requiring* an approval at admission is a separate gate that does not exist |
| `authority.signature-over-canonical-bytes.pass/fail` | a signature binds the target's re-hashed canonical digest, so a signature over any other byte representation fails consistency — `a_rewritten_manifest_with_the_old_signature_is_refused`, `test_flipped_signature_byte_is_named_by_signature_verification`; the dedicated `CORE-V8201` code does not exist |
| `authority.trust-roots-are-verifier-policy.pass` | producer trusts root, verifier does not → `not_checked`, never `verified` — `test_without_a_trust_root_no_signature_reports_verified`, `without_a_trust_root_signatures_are_reported_but_never_verified` |
| `authority.neutrality-record-fields.pass` | `avila_provided`, `self_preference_check` mandatory — `an_avila_provided_selection_without_the_check_is_refused`, `an_avila_provided_selection_with_the_check_runs` |
| `authority.uncredentialed-client-cannot-apply-judgment.fail` | the ADR-0021 `attestation` record exists; "applying judgment" is the ADR-0024 routing record's act (proposed) |
| `authority.agent-cannot-submit.fail` | a transition whose actor kind is `agent` can still carry a valid role signature — the role check governs, not the actor kind (`a_transition_under_the_wrong_role_is_refused`); a `runner/submit` operation does not exist |
| `authority.key-role-not-cognition.pass` | Core enforces key/role authority and does not claim to detect whether a human used assistance — `a_reuse_rule_signed_by_a_runner_key_is_refused`, `a_signature_made_with_an_unlisted_key_is_refused`, `require_signatures_refuses_a_run_without_a_trust_root` |
| `authority.frontend-cannot-construct-verdict.build` | no `VerdictOutput {` / `AdmissionRecord {` construction outside the kernel/compiler boundary — pinned by `authority_boundaries.rs` |
| `ownership.OM-1..OM-7.pass/fail` | the seven SC-15 invariants are each enforced and pinned: immutability — `a_rewritten_manifest_with_the_old_signature_is_refused`, `an_edited_receipt_cannot_be_reused_and_the_rerun_drifts_from_it`; `as_of` dependence — `test_as_of_lines_emit_alongside_recorded_checks`; invalidated/quarantined never satisfies — `campaign.model-not-permitted.quarantine`, `campaign.parent-missing.not_evaluated`; cross-campaign use needs memo/reuse rule — `a_signed_reuse_rule_permits_reuse_across_its_scoped_edge`, `a_reuse_rule_without_a_trust_root_fails_closed`; cardinality — admission's per-slot claim binding (`campaign.parent-missing` cascade); weakening explicit — `permit_nominal_basis` is a declared policy flag and nominal basis cannot enter bounded evaluation (`verdict.rs`); immutable snapshots — `campaign.snapshot-mismatch.rejected`, `a_selection_describing_a_different_registry_snapshot_is_refused`. A committed one-pair-per-invariant fixture corpus does not exist |

### canon/ (SC-2, ADR-0005)

| Fixture | Expected |
| --- | --- |
| `canon.key-order.pass`, `canon.nfc-rejected.fail`, `canon.no-null.fail`, `canon.float-refused.fail`, `canon.digest-algorithm-id.pass` | NFC is required on input; authoritative readers reject rather than silently normalize signed content — exercised by `json.key-order`, `json.non-nfc-string-rejected`, `json.null-as-absence-rejected`, `json.float-number-rejected` |

### scenarios/ (planned end-to-end corpus)

| Fixture | Expected |
| --- | --- |
| `scenarios.coverage-crosses-limit` | INCONCLUSIVE `bounded.le.crossing`; next actions with owners — the verdict half is exercised by `le.bounded.crossing` (and `campaign.le.crossing.inconclusive` end-to-end); the owner-named next-actions half remains |
| `scenarios.qualification-narrowed` | exact invalidated set; transport reused; activation has no admissible candidate — the invalidated-set and reuse halves are exercised (`a_changed_input_reruns_the_step_and_names_the_change`, `a_signed_reuse_rule_permits_reuse_across_its_scoped_edge`, `a_selection_with_no_eligible_candidate_is_refused`); the committed end-to-end scenario package remains |
| `scenarios.pinned-implementation` | Campaign IR has constraint only; the excluded candidate is recorded as a `CORE-P5201` notice — `a_candidate_excluded_by_a_contract_constraint_carries_a_notice_and_runs`; the persisted Bound Plan record remains |
| `scenarios.review-rejects-upstream` | descendants invalidated; nothing reused; owners named — the cascade and non-reuse halves are exercised (`a_two_step_chain_reruns_only_what_a_change_reaches`, `campaign.parent-missing.not_evaluated`); the owner-attribution half needs ADR-0021 records |
| `scenarios.verify-without-evaluator` | the independent Python verifier replays every committed case and signature without avila-core — `test_every_campaign_fixture`, `test_every_committed_receipt_invocation_identity_reproduces`, `test_every_committed_signature_document_verifies`; the obligations report's four-category rendering remains |
| `scenarios.bike-hook` | three findings → plan with rejected Elmer → INCONCLUSIVE → geometry change → memo reuse of `fdm_properties` → PASS → obligations report — the verdict half is exercised by `le.bounded.bike-hook.first-run` and `le.bounded.bike-hook.second-run`; the memo and obligations halves remain |

## Campaign corpus

`campaigns/campaign-cases.v1.json` pins fifteen campaign-evaluation cases over
the type fixtures' contracts and registries: a claims document in, and the
expected status, findings, admission states, verdicts, and campaign identity
out. `crates/avila-core-compiler/tests/campaign_fixtures.rs` executes them.
They cover the three bounded outcomes, exact unit scaling, a missing parent,
a model the output does not permit, inverted bounds, a duplicate claim, a
snapshot mismatch, the three review states, and three `require_qualification`
cases: enforced refusal of unqualified evidence, the same evidence permitted
with an informational reason under the default policy, and a qualified
`inside` claim unaffected by the policy; see
`docs/architecture/CAMPAIGN_EVALUATION.md`.

## Mutation harness

`crates/avila-core-compiler/tests/diagnostic_contract.rs` reuses every compiled
fixture here as a base, applies a fixed set of authoring mistakes to each, and
enforces the diagnostic contract on every report: determinism, anchored
pointers, effective repairs, and bounded rounds to green for mechanically
repairable mistakes. Adding a compiled fixture therefore widens that corpus
automatically.

## Counting rule

`avila-core fixtures-check` executes the counting rule over the committed
corpus: it parses the required-fixture tables above, indexes every fixture and
vector identity, and reports each required name as `covered` (its exact
identity exists), `named` (the plan row references an existing vector set,
vector, fixture, or test file that exercises the rule under a different
identity — e.g. `authority_boundaries.rs` for the construction-boundary
row),
`unbounded` (a pattern such as `*`, `<each>`, or a non-numeric `..` that needs
an enumerated domain), or `absent`. A fixture that references a diagnostic
code neither catalog defines fails the check outright; `--strict` also fails
on any `absent` name, and `--json` emits the report. Until every required name
is covered and none is absent, ADR-0006 stays proposed.
