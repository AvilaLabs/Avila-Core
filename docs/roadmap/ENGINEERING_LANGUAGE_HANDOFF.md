# SWE-2 handoff: build the engineering language experiment

Prepared 25 September 2026. Planning baseline: `b1587ad`. Status: proposed next
work order; no EL milestone is claimed implemented by this document.

Read the [language charter](proposals/2026-09-25-engineering-language-charter.md)
first. The owner has agreed to the direction: Core's type system, method
interfaces, execution, revision handling, and authoring tools should grow from
one engineering language. This handoff defines a finite experiment that tests
that architectural commitment.

The [RA-01–RA-06 handoff](RUST_ARCHITECTURE_HANDOFF.md) remains the record of
earlier boundary hardening and its review fixes. Finishing those fixes is a
baseline obligation, not evidence that this language already exists.

## Deliverable

Deliver a small executable engineering language, using the charter's synthetic
thermal-expansion/clearance example, in which applying a declared method
generates related-type checks, propagated assumptions, and explicit obligations.
Its actual execution must yield a conclusion whose premises an independent
checker can reconstruct. A second synthetic library must reuse the same rules.

Do not reduce the assignment to visibility changes, more report metadata,
adapter-specific conditionals, or a new syntax over unchanged semantics.
Every new semantic field must participate in an inference, refusal, explicit
assumption, or unresolved obligation demonstrated by a fixture.

## Scope and working rules

- Follow [AGENTS.md](../../AGENTS.md) and
  [CONTRIBUTING.md](../../CONTRIBUTING.md). Keep authority in the existing
  kernel/compiler/evidence/runner boundaries. Frontends consume shared results.
- Use current source, not historical status prose, to establish the baseline.
  `b1587ad` contains the implementation author's review follow-up; its existence
  is not an independent sign-off. Preserve unrelated user work, including the
  untracked `examples/cases/case-010-matmul-rank/` directory if still present.
- The owner's new language direction makes a finite semantic-obligation
  experiment part of the next work, despite its earlier deferral in S-034.
  Record this scoped sequencing change in the new ADR. Do not mark S-034's
  performance gates satisfied or use it to justify a persistent session system,
  database, scheduler, or general reasoning platform. Do not resume paused
  engineering experiments or change roadmap stage status.
- Preserve historical profiles and canonical identities. Introduce a separately
  identified experimental profile for new semantics; choose and document its
  actual name in EL-01. Existing records cannot acquire stronger guarantees by
  migration or deserialization alone.
- Do the specification work before choosing a solver library or reorganizing
  crates. Begin with finite terms, nominal identity equality, rational interval
  operations, bounded conjunction/domain checks, and an acyclic method graph.
  Unsupported predicates and budget exhaustion remain explicit.
- Routine implementation choices do not require a new permission exchange.
  Resolve them from the charter and document them. A missing scientific premise
  remains attributed or unresolved; never manufacture it to complete a demo.

## Read before implementation

- [ADR-0006](../adr/0006-semantic-core.md), particularly quantity kinds,
  uncertainty, composition, admission, and reuse.
- [ADR-0026](../adr/0026-context-bound-verdict-derivations.md) and its current
  compiler, runner, and Python verifier implementations.
- [Semantic compiler](../architecture/SEMANTIC_COMPILER.md),
  [campaign evaluation](../architecture/CAMPAIGN_EVALUATION.md),
  [capability threat model](../architecture/CAPABILITY_THREAT_MODEL.md), and
  [Core tools](../product/CORE_TOOLS.md).
- Existing numeric/predicate/verdict code in `crates/avila-core-kernel/src/`;
  compiler `compile/`, `qualification.rs`, and `campaign/`; evidence receipts;
  runner `case_run/plan.rs`, `runner.rs`, and reuse rules; `verifier/`.

When using engineering case records, discover tools with `avila-core tools
list`. Use `inspect`, `history`, and `attempt` with source identities for saved
facts; use `run --plan` for current reuse checks. Normal `run` checks reuse;
use `--no-reuse` only when deliberately testing fresh execution.

## EL-00 — Close the existing review and establish compatibility

Review the follow-up to the five findings from the ADR-0026 review. Confirm at
the public API and independent-verifier boundaries that foreign claims,
forged context metadata, contradictory report/derivation pairs, and overlapping
qualification refusals are handled correctly. Check the actual runner chain:
receipt byte identity alone must not be described as verified receipt semantics,
successful execution, signature verification, or satisfaction of a runtime goal.

Record checked revision, commands, and remaining limits in a concise review
note. Capture existing representative report identities and behavior through
the supported operations. Keep unrelated draft-case failures separate. EL-01
can proceed while this baseline review is underway; experimental execution
integration must not rest on an unresolved authority bypass.

Done when the baseline and its trust boundaries are explicit, with no known
unresolved bypass used by the proposed experiment. An implementation author's
completion statement is not sufficient evidence.

**Status (post-implementation):** all five review findings re-reproduced as
closed at `b1587ad` — wrong-snapshot claims refused at `bind`, forged context
and evaluator rejected under recomputed digests, contradictory campaign
reports rejected on content, mixed qualification lifecycle reasons preserved,
and the runner's observed chain binds checked artifact and receipt bytes.
Baseline identities and commands are recorded in
[2026-09-25-adr-0026-followup-verification](reviews/2026-09-25-adr-0026-followup-verification.md).
One honest boundary: the observed derivation persists only when a workspace
exists, and receipt premises are byte identity, not receipt semantics.

## EL-01 — Specify the language and write its example programs

Produce an ADR using the next unused number and a small normative specification
linked from it. The charter is the target, not that specification. Write the
positive and negative authored programs before implementing their checker.

Specify:

1. Terms for input binding, scoped values, method application, explicit
   assumptions, checked primitive inference, goal holes, and requirement use.
2. Types parameterized by the relationships needed in the example: geometry,
   scenario, material/applicability identity, quantity kind, and claim model.
   Specify which parameters require equality and which support restriction.
3. Separate representations for propositions, established evidence, assumptions,
   unresolved obligations, uncertainty dependencies, and runtime effects.
4. Inference rules for substitution/unification, application, assumption
   propagation, domain restriction, exact/interval arithmetic, and evidence use.
   State the premises and source attribution of every rule.
5. Static checking versus runtime discharge. Define readable incomplete analysis,
   executable-plan readiness, and evaluated outcomes without adding a fifth
   requirement verdict. Contradiction, missing data, ambiguity, unsupported
   constructs, and exhausted work budgets must not become PASS.
6. The trust model for imported assertions and certificates. Explain which
   premises the kernel establishes, which the runner observes, and which a
   named party asserts. Specify how obligations can be discharged without
   erasing residual trust dependencies.
7. Canonical identity bodies, profile negotiation, source maps, context binding,
   change dependencies, and the semantics preserved by each lowering stage.
8. Finite limits on term size, proof depth, goals, numeric work, and candidate
   search. Apply limits before unbounded expansion or expensive inference.

Write a preservation argument by cases over the primitive rules: if their
premises hold, their conclusions hold under the same remaining assumptions.
For numerical intervals, explain containment; for domain specialization,
explain the direction of implication. Name unproved implementation obligations.
A formal proof assistant is optional for this experiment; a tested interpreter
is not itself a proof of soundness.

Done when the specification determines the expected result of every initial
program without case-specific implementation knowledge. Every mechanism in the
charter's first experiment must map to a rule and an observable example.

**Status (post-implementation):**
[ADR-0027](../adr/0027-engineering-language-fragment.md) records the scoped
sequencing change and names the experimental profile
`avila.core/language/0.1-draft`. The normative specification is
[docs/architecture/ENGINEERING_LANGUAGE.md](../architecture/ENGINEERING_LANGUAGE.md);
twenty-four authored programs and two method libraries live in
`examples/language/` with `expectations.json` pinning the specified result
of each. No checker exists; the examples' expected outcomes are derived by
hand from the specification. Specification **r2** incorporated the
[EL-01 specification review](reviews/2026-09-25-engineering-language-spec-review.md)
— kind-directed multiplication via declared `kind_products`, scenario
identity separated from scope acceptance, witness-based assumption
discharge with cyclic refusal, `independent` vs `provenance_disjoint`,
relation-map semantics, semantic vs document identity, and the lifecycle
context. Specification **r3** incorporates the
[r2 follow-up review](reviews/2026-09-25-engineering-language-r2-review.md)
— schema-directed identity projection (positional operands are
identity-bearing, map keys are never stripped as annotations, admission
precedes hashing), `clearance-difference`/`clearance-heuristic` carry
`material` through their outputs, shared provenance refutes only
`provenance_disjoint` (never `independent`), nominal operands make
arithmetic rules inapplicable rather than propagating, and lifecycle
material combines all applicable entries with all refusal reasons
preserved. The [r3 closure review](reviews/2026-09-25-engineering-language-r3-closure.md)
closes the six remaining specification findings. EL-01 is ready for EL-02's
finite shared analysis and obligation generation implementation.

## EL-02 — Implement shared analysis and obligation generation

Implement the finite language in the existing compiler/kernel boundaries with
an explicitly selected experimental entry path. Expose one analysis operation
returning typed relationships, residual assumptions, supported method candidates,
open goals, blocking findings, and source-linked dependency explanations.

Method application must substitute signature variables and generate obligations
generically. A method identifier selects library data, not a branch containing
the meaning of that particular example. Library compositions expand to supported
primitives; they cannot introduce proof authority through arbitrary callbacks.

Allow inspection of incomplete programs. Separate static inconsistency from
work awaiting a runtime observation. Preserve all required checks when lowering
an executable plan. Reuse existing exact-number limits and canonical handling
where applicable, and catalog any new diagnostic codes with emitting fixtures.

Done when valid, incompatible, and incomplete examples follow the same rules,
and an attempted nominal-to-enclosure promotion fails at the actual consumer.
CLI/agent callers must see the same diagnostics as library callers. A new GUI
or LSP server is unnecessary; their future analysis boundary must be real.

Implemented in `avila_core_compiler::language` (spec §10 admission and
schema-directed projection, §7 rule evaluation, generic method application
with `projects` relation accounting, generated obligations, premise
admission/discharge, §10.2 lifecycle, requirement reports). The shared
operation is `language::analyze_program`; `avila-core language analyze` is
a client of it. Language diagnostics are catalogued as `CORE-E8001`–
`CORE-E8027` (DIAGNOSTICS.md); all 24 fixture programs plus the reviewed
counterexamples are regression-tested at the shared API boundary.

Revised against the
[2026-09-25 EL-02 review](reviews/2026-09-25-engineering-language-el02-review.md):
postconditions are replayed rather than asserted, typed payloads are admitted
against their declared claim and unit, declared sets are order-invariant in
identity and derivation, witnesses carry their support to the conclusion,
certificates require supported replayable checks, expression evaluation is
bounded (`CORE-E8027`), and the returned artifact carries a structured plan
(step arguments, obligation links, typed rule expressions) with scoped
findings. The finding-by-finding closure and adjudication items are in
[the revision response](reviews/2026-09-25-engineering-language-el02-closure.md).
Returned for independent review before EL-03.

An [implementation self-review](reviews/2026-09-26-engineering-language-el02-implementation-review.md)
later found and fixed a unit gap the corpus never exercised (kinds without
`canonical_unit` admitted mixed-unit arithmetic; now refused at admission
and at the additive rules, in both implementations). Independent review
remains open.

## EL-03 — Execute the synthetic program through the shared runner

After EL-00 and EL-02, integrate a narrow experimental plan with the existing
runner. Exercise a synthetic external executable or the existing synthetic
execution harness; do not replace actual observations with fixture status flags.

The example composes expansion and clearance as described in the charter. Its
external input bounds are attributed premises. Its arithmetic transformations
have independently checkable exact/interval justification. Its geometry and
scenario relationships, linear/uniform model assumptions, and applicability
conditions must survive into the resulting claim.

Bind execution inputs, implementation, outputs, environment/effect declarations,
and receipts to the method application and its required observations. Identify
which effects are enforced and which remain trusted declarations. A process
exit, a file hash, and a mathematical certificate cannot substitute for one
another. Runtime checks discharge only the propositions they actually establish.

Produce a derivation of the experimental conclusion through a shared operation,
with a minimal headless interface and discoverable supported operations. Keep
historical run/report behavior under its existing profile. Propagate artifact
write failures that would leave a claimed completed result unreplayable.

Done when a real run emits the full attributable argument, failed or incomplete
execution cannot establish its promised claim, and policy-required missing
premises remain blocking. No live scientific solver is required for this test.

### EL-03 status — implemented, in review

The analyze → plan → execute → evaluate chain is live end-to-end:

- `language::execution_plan` emits `avila.core/execution-plan/v0.1-draft` —
  per-invocation staged inputs (canonical `ValueDecl` bytes + digests), the
  declared `produces`, the obligation ids the invocation can discharge, and a
  plan digest binding the analysis identity. Unstageable operands are
  declared `unstaged`, not hidden.
- `runner::language::execute_plan` stages `inputs/{slot}.json`, resolves
  `synthetic/…` executables only through a caller-supplied map, spawns under
  a cleared environment + minimal PATH with the shared `wait_with_timeout`,
  writes `logs/`, collects the declared output as a canonical `ValueDecl`,
  and emits `avila.core/language-observations/v0.1-draft` — each record
  carrying a receipt (`avila.core/language-receipt/v0.1-draft`) with
  invocation identity, input digests, output digest, and process status.
- `language::evaluate_program` replays the analyzer against the supplied
  observations: every record is re-hashed and bound to its application site
  (receipt digest, plan digest, site, executable, input digest set, output
  digest — §7 O1); an admitted output is re-admitted at the declared type and
  runtime postconditions replay against it (§7 O2 — discharged/refuted/open).
  Requirements then derive `pass | fail | inconclusive | not_evaluated`; a
  rejected or absent observation leaves the obligation open and the
  requirement `not_evaluated` with `CORE-E8028 observation_foreign`.
- `avila-core language plan|execute|evaluate` expose all three stages.

Synthetic executables live in `examples/language/executables/` (exact-rational
interval arithmetic over the staged `ValueDecl`s). Coverage: compiler tests
`language_execution.rs` (digest-binding matrix — tampered input/output/
receipt/site, foreign plan, absent record, transplanted receipts, verdict
boundaries) plus runner tests `language_execute.rs` (real processes, timeout,
missing output). All pinned `expected_on_execution` verdicts in
`examples/language/expectations.json` reproduce, including the conditional-on
residual grammar. EL-04's independent replay remains a separate verifier —
this chain is the reference implementation, not the check of it.

## EL-04 — Replay the language and explain changes

### EL-04 status — implemented, in review

`verifier/language_verify.py` is a stdlib-only port of the evaluate chain:
schema-directed projection for the program/library semantic digests, the
full §7 [O1] binding (receipt re-hash, plan/site/executable triple, staged
input digest-set, invocation identity, completed status, output digest,
typed re-admission), [O2] postcondition replay under exact-rational
interval arithmetic, the static `requires` obligations
(`domain_containment`, `scope_check`, `provenance_disjoint`,
`independence` — including premise-witness support joining the residual
cone), premise admissibility (attribution, denial, cyclic-witness groups,
provenance-disjoint at use), and the `bounded.ge`/`bounded.le` verdict
comparator with residual `conditional on` detail. `explain` diffs two
evaluations of one program at requirement, obligation, and observation
granularity.

The committed corpus under `verifier/fixtures/language/` was generated
once from the reference implementation and held constant, keeping the
replay independent of the Rust toolchain. `test_language_verify.py`
covers all seven corpus programs plus the adversarial matrix — tampered
output, forged executable digest, foreign plan, transplanted receipts,
duplicate site claims, absent and malformed observation documents, and a
forged evaluation record that recomputes its own digests honestly (still
flagged `mismatch`). The CI `verifier` job runs it.

Admission is re-derived, not trusted: the context's `""`-vs-digest
program/library identities must agree with the full admission set —
document schema/profile, §11 budgets, identifier charset, sequential
single-assignment references, requirement and premise shapes, provenance
objects, proposition vocabulary and scope params, entity intervals, plus
the admission-kind findings the analyze run itself publishes — and the
library gate is the whole `LibraryChecker` (method signatures,
`requires`/`ensures` declaration shape, implementation kinds,
`kind_products`). `fixtures/language/inadmissible-{program,library}/`
pin honest `""` records; forged identities on inadmissible documents
mismatch. Lifecycle material (`--lifecycle key=state`), the
`expired`/`withdrawn` refusal, assumption `contradiction` blocking, and
the `nominal`-claim gates are replayed rather than read from the record.

Remaining known-divergent surface (documented, fails closed): the record's
`findings` list is explanatory — the verifier does not compare it
entry-for-entry; admission kinds already decide the published identity,
so a record omitting findings still fails closed through the `""` gate.
`analysis_sha256` is recomputed only when `--analysis` supplies the
document (`not_checked` otherwise).

### EL-04 brief

Extend the independent verifier from the written rules. Reconstruct supported
type relationships, assumptions, interval transformations, runtime bindings,
and requirement conclusions from supplied material. Check the relation between
the report and derivation, not only their separate hashes. Unavailable or
unsupported required premises remain visibly unchecked.

Explain changes through semantic dependency edges, connecting an altered source
to the affected obligation, method application, or requirement. An explanation
that says only "context hash changed" is insufficient. Integrate with existing
plan/reuse decisions; never replace their authorization rules with the fact
that an output happens to have the same number.

Compare clean recomputation with the result of reconsidering existing uses
under changed premises. The prototype can recompute everything: correctness of
invalidation does not require a persistent incremental engine. Keep historical
derivations intact and distinguish a historical context from current supplied
qualification/revocation material.

Done when both implementations agree on the adversarial matrix below, honest
derivations replay, and rehashing a false semantic claim never makes it verified.

## EL-05 — Demonstrate transfer and assess the experiment

### EL-05 status — implemented, in review

The second library is `examples/language/libraries/measurement-scaling.v1.json`
(written at EL-01, exercised at EL-02, replayed at EL-04). `scaled-sum`
declares `provenance_disjoint` plus an `independence` obligation that only
an attested premise closes; `invalid-shared-source-independence` and
`positive-measurement-pass` pin both directions. The compiler contains no
branch keyed to a library, method, or domain name — checked, not claimed.

The assessment is
[2026-09-26-engineering-language-el05-assessment.md](reviews/2026-09-26-engineering-language-el05-assessment.md):
what the common rules established, the trusted-assertion remainder, the
burdensome declarations, what failed to generalize, the migrate-vs-shared
profile surface, and the measured costs (analyze ~8 ms, plan ~9 ms,
execute ~48 ms subprocess-bound, evaluate ~11 ms, Python replay ~138 ms;
i3-N305, rustc 1.95.0, Python 3.14.4).

A third library — `matmul-rank` — further supports the transfer claim on
non-toy material. Derived from `examples/cases/case-010-matmul-rank`
(Brent parity-equation verification of tensor decompositions over GF(2)),
its `brent-verify` method composes `domain_containment` (field
characteristic within verifier applicability), `provenance_disjoint`, and
`independence` over the same obligations vocabulary with no compiler
branches — replayed clean by the Python verifier. Its seeded-error
battery refuses each realistic error at the right boundary: wrong tensor
format → `coverage` at the requirement; GF(3) run on a GF(2) verifier →
`precondition_refuted` before execution; the search verifying itself →
`obligation_refuted` (an attested independence premise does not launder
recorded provenance); rank-vs-count conflation on the same unit →
`type_mismatch` at the requirement; superseded verifier →
`lifecycle_refused` under supplied lifecycle material. One friction data
point: `independence` discharge requires the premise's `at` to equal the
full union scope of the operands' relations — the scope-equality burden
the assessment already names.

### EL-05 brief

Add a second synthetic library exercising different composition over the same
primitives, for example scoped measurement transformation with shared source
dependencies. It must require no compiler branches keyed to its domain or
method name. Include a method requiring an explicit independence premise;
two source identifiers alone must leave that goal unresolved. Do not implement
a statistical combination formula merely to close that example.

Write a short architecture assessment: what the common rules established,
what still requires trusted assertions, which declarations were burdensome,
what failed to generalize, and which code would migrate versus remain shared.
Measure the finite example's analysis and execution costs, labeling the workload
and build. These measurements are not a claim that broad infrastructure gates
have passed.

Done when the full language architecture is demonstrated across both libraries
and the next semantic/migration decision is supported by evidence. Keep wider
domain coverage, regional certificates, syntax, LSP, and infrastructure as
explicit later work; do not mark the long-term charter complete.

## Required adversarial matrix

Use positive counterparts as well as these negative/changed cases. Derive
expectations from EL-01, not by blessing whatever the implementation emits.

| Change or attempted use | Required behavior |
| --- | --- |
| Same-shaped input from another geometry revision | Related-type refusal at the consuming method, naming both identities. |
| Steady-state result used for a startup/transient goal | Scope mismatch or explicit unresolved coverage obligation. |
| Required model assumption omitted from a dependent output | Propagation restores it, or imported derivation is rejected; it cannot vanish. |
| Conflicting supported assumptions | Explicit contradiction; never vacuous success. |
| Nominal output relabeled as an enclosure | Refusal unless a supported rule establishes the stronger claim. |
| Shared calibration sources presented as independent | Independence remains a named unmet premise unless supported evidence establishes it. |
| Missing precondition or postcondition | Visible authoring hole, blocked launch, or unusable result at its specified stage. |
| Method applicability narrowed, expired, or withdrawn | Current use reconsidered under supplied context; original history retained. |
| Requirement limit changed in a separate authored program | Re-derive the verdict; preserve every method dependency on that limit. |
| Input, geometry, method, or material effect changed | Identify affected method uses and run the existing permitted reuse checks. |
| Only presentation changed | Preserve semantic conclusions; do not invent changed scientific premises. |
| Process fails, output is absent, or receipt is transplanted | No runtime obligation is discharged by the failed or foreign observation. |
| Numerical certificate or report conclusion forged and all hashes recomputed | Independent semantic replay rejects it. |
| Arithmetic or goal budget exhausted | Bounded explicit refusal/unsupported result; never successful partial checking. |
| Second library installed | Same checking rules; no case-name or method-name switch in the compiler. |

## Validation and completion report

Run focused rule, API-boundary, runner, and independent-replay tests as changes
land. Run doctests for newly claimed construction restrictions. Before the
handoff is marked complete, run CONTRIBUTING's formatting, Clippy, and workspace
test commands, plus the independent Python suite. Record failures attributable
to unrelated user work separately; do not edit that work to make a check green.

Return links to the specification/ADR, example programs, shared entry points,
and reproducible commands; explain what changed and its remaining boundary.
Keep operational inventories and results in machine-readable records, with
source identities, rather than duplicating them into status narratives.

## Copyable worker prompt

> Read `AGENTS.md`, `CONTRIBUTING.md`, the engineering-language charter linked
> from `docs/roadmap/ENGINEERING_LANGUAGE_HANDOFF.md`, and that handoff. Carry
> out EL-00 through EL-05 in dependency order. Write the finite language rules
> and example programs before implementing their checker. Deliver one coherent
> path from related engineering types and propagated assumptions, through
> generated obligations and actual execution observations, to independently
> replayable conclusions. Use the synthetic expansion/clearance model and a
> second library to test that the rules generalize. Preserve old profiles and
> unrelated user work. Do not substitute metadata, private-field refactoring,
> case-specific branches, or a new syntax for the language semantics. Resolve
> routine implementation choices autonomously; keep genuine missing premises
> explicit. Finish with validation evidence and a bounded migration assessment.
