# EL-02 revision closure — response to the 2026-09-25 review

Scope: the eleven findings in
[2026-09-25-engineering-language-el02-review.md](2026-09-25-engineering-language-el02-review.md)
and its reproduction harness
(`reproduce.py` / `observations.json`). EL-03 (runner integration) remains
pending; nothing here pre-implements execution or independent replay.

All behavior lives behind the shared `analyze_program` boundary in
`crates/avila-core-compiler/src/language/` — no fix is keyed to a fixture id
or method name.

## Finding-by-finding closure

| # | Finding | Resolution |
|---|---------|------------|
| 1 | Primitive applications manufacture checked postconditions and output types | Primitive bodies are type-checked against the declared output (`type_mismatch` on disagreement), and every `ensures` postcondition is replayed against the computed operand values: equal → `discharged`, unequal → `refuted` + `obligation_refuted`, uncomputable because operands are already unestablished → stays `open` with no redundant finding. External `ensures` all become `runtime` obligations; none are pre-discharged. |
| 2 | Equal semantic identities, different analysis semantics | All declared sets iterate in `canonical_order` (the same order the projection hashes): methods, kind_products, inputs, premises, assumptions, requirements, ensures, requires. Reordering a declared set preserves `semantic_sha256`, computed values, and finding order; duplicate ids refuse identically in both orders. |
| 3 | A payload can claim a stronger type without establishing it | `admit_value` gates every payload: the payload claim must satisfy the declared claim (nominal never satisfies `enclosure`), the value shape must match its kind, and the unit must be the declared kind's canonical unit — at inputs, imports, and requirement limits. |
| 4 | Application context checks accept incompatible relations and domains | `domain`/`within` checks are evaluated tri-state over admitted intervals: wrong-unit or inverted operating domains are `type_mismatch`/`malformed` rather than silently contained; operand relation unions conflict rather than collapse. |
| 5 | Unattributed premises and unreplayed certificates enter as established | `established_by` is required and typed (`assertion-party` or `certificate` + digest + supported check). Premise `at` scopes are validated against proposition `params`. A certificate with a well-formed digest but an unrecognized `check` is `unsupported` and establishes nothing. |
| 6 | Independence witnesses lose their support; provenance stores split | Discharge carries the witness's full support — its own residual assumptions, provenance edges, and a `premise` dependency link — into the derived binding, and is a **fixpoint**: a witness's own assumptions expand through their admissible witnesses, so the folded provenance is the full cone (`discharge_assumptions` → `support_cone`). `combined_rate` in the baseline measurement program now reports `indep-memo-7`, `separate-instrumentation`, and the `p-independence` premise link. |
| 7 | Analysis is not total or bounded over admitted expression strings | The parser/evaluator take a depth + node budget (`eval::ParseFailure`/`FailureKind::{Malformed, Unsupported, Budget}`); over-budget input emits `budget` (`CORE-E8027`) and a refused plan. A 10,000-deep expression returns bounded findings under a 384 MiB/5 s sandbox instead of crashing. |
| 8 | Blocking causes do not propagate to dependent bindings | Blocking rules propagate through the dependency graph: an unmet precondition on `combined_rate` leaves a dependent requirement `not_evaluated.obligation_unmet` (the causal rule, not bare `pending`). Refuted/malformed steps poison the binding; open obligations block dependents only. Unreachable binding-scoped findings stay emitted but non-blocking, so an unused `hole` remains inspectable while the plan is `ready`. |
| 9 | Requirement admission omits constraints and rejects supported forms | `RequirementDecl.scope` is optional and `scenario` is supported; comparison vocabulary, limit shape, and scenario identity are validated at admission (`undeclared_entity` for unknown scenarios). The scope lattice admits `any` over a steady-state subject. |
| 10 | Unknown lifecycle states are treated as usable | Lifecycle keys and states are validated against the closed grammar at the API boundary (`lifecycle[<i>]` findings); `withdrawnn` is `malformed`. Applicable entries retain their actual state and `superseded` notices in `LifecycleReport.entries`/`notices`. |
| 11 | The returned artifact does not yet carry the promised executable plan or source graph | `PlanStepReport` carries `at`, `bind`, `operation`, `target` (method id or primitive rule), resolved `arguments` (slot → binding, positional `_0`/`_1` for infer), generated `obligations` ids, and `unit`. `ObligationReport` carries `id`, `kind`, `at`, `step` (obligation→invocation link), `state`, `check`, the authored `expression`, resolved `operands`, and `detail`. `LanguageFinding` carries `document`, `at`, a resolvable JSON `pointer`, the `binding` it is scoped to, and `blocking`. `BindingReport.dependencies` links operands, discharging premises, applied methods, and imports. |

## Identity and partial admission

Identity reports are now published for every readable document:
`document_sha256` commits to the bytes read (canonical bytes; the raw-byte
hash when the authoritative read itself fails), `admitted` records whether
schema + semantic admission passed, and `semantic_sha256` is emitted only
for admitted documents (`null` otherwise — never the hash of an unchecked
projection).

When one document fails to decode, the other still receives its full
admission pass: the library checks run standalone (`LibraryChecker`), and
program-external lifecycle material is validated. A program cannot be
admitted without its bound library's vocabulary, so its semantic identity
stays unpublished in that path; the library's own findings (e.g.
`undeclared_proposition`) are no longer masked by the program's decode
failure.

## Specification adjudication proposed

Items here are implementation choices the specification does not pin
precisely; flagging for review rather than silently deciding.

1. **Findings carry a JSON `pointer`, not a byte span.** The pointer
   resolves to the declaration position (`/body/2/apply`,
   `/premises/3`); byte-offset spans would need position tracking through
   the authoritative reader. Proposed: pointer satisfies the review's
   "source pointers" requirement for this profile.
2. **An `open` obligation keeps the computed value; `refuted` and
   `malformed` steps poison the binding.** The binding report distinguishes
   `value_state` from the blocking rule the requirement inherits. The
   alternative — poisoning on `open` — would erase the causal rule
   distinction the review asked to keep.
3. **Unreachable blocking findings are demoted, not dropped.** The finding
   remains in `findings` with `blocking: false`; `plan.state` follows
   reachable blocking findings only. This keeps inspection possible without
   refusing usable work.
4. **A repinned program cannot be admitted while the supplied library is
   inadmissible** — `pin_match` is `false` and `library_pin_mismatch` names
   the cause. (The reproduction harness's `null` repin then fails program
   decode, which is load-bearing: a `null` pin is not a claim of identity.)
5. **`budget` is a blocking kind.** Over-budget input means the checked
   representation could not be constructed; refusing is the honest answer.

## Later additions

**Partial-decode coverage is now symmetric.** A second extraction —
`ProgramChecker`, parallel to `LibraryChecker` — runs every
vocabulary-free program check (headers, budgets, duplicate ids, sequential
references, premise attribution, entity sources, requirement shape) even
when the bound library is undecodable, gating only the
vocabulary-dependent checks (quantity kinds, canonical units, proposition
names/params — undecidable without the library's terms, not malformed).
Regression: `partial_decode_reports_each_documents_own_findings` — a
duplicate input id plus undecodable library bytes surfaces the program's
own `malformed` finding at `inputs[reading_a]` with pointer `/inputs/0`,
alongside the library's decode failure and `library_pin_mismatch`.

**Finding `pointer` resolution strengthened.** `at` paths like
`premises[p].established_by` or `body[2].arguments.x` now resolve to the
containing declaration (`/premises/0`, `/body/2`) instead of no pointer —
the position of the defect, not a missing field. Grouped paths
(`premises[a, b]`) still resolve to the first member's position.

**Self-audit additions.** Three tests cover paths the reproduction did
not reach: `partial_decode_reports_each_documents_own_findings`
(above), `scope_check_is_evaluated_through_the_scope_lattice` (the
`scope_check` requires kind — previously unexercised — refutes a
`transient` requirement against a `steady-state` scenario and discharges
on `steady-state`/`any`), and `provenance_disjoint_is_open_when_edges_are_absent`
(missing provenance leaves disjointness `open` with `obligation_unmet`,
not refuted — the opposite of a false refutation).

**Witness cones are transitive, and disjointness sees conditional
provenance.** A self-audit extension of finding 6: single-pass discharge
folded only the direct witness edge, and obligations evaluated during the
body never saw witness provenance at all — a binding resting on an
assumption witnessed under a shared edge could falsely satisfy
`provenance_disjoint`. `discharge_assumptions` is now a fixpoint
(`support_cone` — bounded, declaration-ordered residuals), and
`provenance_disjoint` obligations evaluate `effective_edges`: recorded
edges plus the witness cone of every pending assumption. A
`provenance_disjoint` **premise** is additionally re-verified at use time
— its `over` set may name bindings whose cones did not exist at admission
— with a `verifying` guard bounding the recursion through operand cones,
and a use-time check that refuses to fold a premise whose `over` names a
binding not yet bound (its cone cannot be evaluated yet).
Regressions:
`transitive_witness_provenance_flows_into_disjointness` (a cone-shared
edge two hops deep refutes the obligation and refuses the plan; the same
witness then folds its edge and empties the residual chain) and
`disjointness_premise_is_rechecked_at_use_against_witness_cones` (a
statically-admissible but false premise is flagged `premise_conflict` at
use and never discharges).

**Declared identifiers must be single tokens.** A second audit round found
that `id`/`bind`/entity-key/method-name values are interpolated into
report paths (`premises[a, b]` groups, `arguments.{slot}` segments) and
lifecycle keys (`method:<library>/<id>@<rev>`) with no charset check — a
`,` or `]` in a premise id parses as a member group, `.` splits a false
path segment, and `/` in a library name makes `(lib, method)` pairs
collide in key space. `identifier_charset_ok` now refuses whitespace and
`[ ] , . : /` on program ids, body binds, entity keys, the program `id`,
and on library `name`, method ids, input-slot names, `variables` keys,
`quantity_kinds`, and `propositions` keys. `@` stays legal (declared
entities like `bracket@2` use it; lifecycle keys are compared as
constructed strings, so `@` cannot forge). Regression:
`identifiers_carry_no_path_delimiters` — premise id `p-2, p-3`, bind
`probe.two`, and method `a/b` are each refused as `malformed`.

**Canonical ordering must use the projection's own sort key.** A deeper
pass found two related defects under F2's umbrella. First,
`canonical_order` sorted by the *typed* serialization — annotation fields
(`label`/`note`/`reason`) participated in the sort key even though the
projection strips them, so two libraries differing only in annotations
could order set members differently and shift `requires[{i}]` /
`ensures[{i}]` positions and residual order. `canonical_order` now takes
the element's projection role and sorts by the projected canonical bytes —
the same bytes the identity hash sees (decl serializations strip `null`
placeholders for absent optional fields before the canonical read). Second,
several declared sets still iterated in *source* order: method
`requires`/`ensures`/`assumes`/`projects` in signature checks (with
source positions in `at`, disagreeing with the canonical positions
obligation ids carry), `method.assumes` and `import.assumptions` at
binding time (residual ordering), premise/import assumption lists in
vocabulary checks, premise `over` member loops, `goal_candidates`, and
lifecycle entries. All now iterate `canonical_order`. One fixture
expectation (`cyclic-witnesses` residual order) pinned the old source
order and was updated to the canonical order. Regression:
`annotation_and_order_changes_preserve_the_whole_analysis` — reversing
every declared set *and* changing annotation fields yields identical
`semantic_sha256` values and a byte-identical analysis (findings order,
obligation positions, residual order, plan) modulo `document_sha256`.

A follow-on audit of that fix caught three more order leaks the first pass
missed: `premise_map` built witness lists in source order (flowing into
`dependencies` via `admissible_witnesses`), the cyclic-witness groups
emitted members in source order (drifting the grouped `premises[a, b]`
`at`), and `check_proposition_refs` addressed assumptions/premises by
source index (`premises[{i}]`) where the pointer layer resolves those
paths by *id*. All three now key on canonical iteration or the declared
id. The cyclic-group member list is sorted for the path and detail. The
`canonical_order` fallback (an element that cannot serialize+project)
sorts by raw serialized bytes rather than an empty key, so even a
degenerate element orders by content.

## Validation

- `cargo test --workspace` — all suites pass (compiler 100 lib + 24
  language-fixture tests + diagnostic coverage + all other crates).
- `cargo fmt --check`, `cargo clippy -p avila-core-compiler
  -p avila-core-cli` — clean.
- `python3 docs/roadmap/reviews/2026-09-25-engineering-language-el02-review/reproduce.py
  --repo . --check-depth` — every case now classifies correctly:
  `false-postcondition` refuted, `nominal-payload-enclosure-type` refused,
  `duplicate-input`/`ensures-order` identity-and-value invariant,
  `derived-provenance` discharges with witness support on `combined_rate`,
  `unreplayed-certificate-import` refused as `unsupported`,
  `unknown-lifecycle-state` malformed, `unreachable-hole` non-blocking with
  a ready plan, `undeclared-method-assumption` surfaces
  `undeclared_proposition`, and `bounded-deep-expression-child` exits 1
  with `budget` findings under the bounded sandbox — no crash.
- New API-boundary regressions in
  `crates/avila-core-compiler/tests/language_fixtures.rs`:
  `postconditions_are_replayed_not_discharged`,
  `payloads_must_establish_their_declared_claim`,
  `declared_sets_are_order_invariant`,
  `witnesses_must_carry_attribution_and_support`,
  `context_checks_and_reachability_are_enforced`,
  `requirement_and_lifecycle_admission_are_checked`,
  `identity_is_withheld_and_expressions_are_bounded` — with positive
  counterparts (supported `interval_arithmetic` certificates, attributed
  assertions, compatible `any` scope, scenario identity, disjoint derived
  sources) asserted alongside the refusals.

## Not done (by design)

- Runner-side `evaluate` verdict derivation and independent replay remain
  EL-03/EL-04. Analysis emits `pending` for verdicts awaiting observation
  and `not_evaluated.*` only for statically-decidable refusals.
- `case-010-matmul-rank/` is untouched and still untracked.
- The EL-02 revision remains **uncommitted** for independent review.
