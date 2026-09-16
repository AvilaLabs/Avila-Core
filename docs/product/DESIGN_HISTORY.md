# Design history and change review

Recorded: 2026-09-15. Status: product requirements for future increments, not
an implemented feature claim or a replacement for accepted semantic rules.
Schema and semantic changes require an ADR before implementation.

## Purpose

Make it easy for an engineer or agent to answer: what changed in the design,
what evidence supports it, what improved or regressed, and what must be checked
again? Git's revision, branch, and diff model is a useful interaction analogy.
Core's authority remains the evidence contract, admission rules, and explicit
requirement verdicts. A successful execution or an accepted baseline is not a
substitute for those verdicts.

## Existing foundation

Build on [ADR-0014](../adr/0014-identity-bound-attempt-lineage.md), which provides
parent-bound candidate attempts, branching ancestry, derived typed JSON diffs,
and verdict and exact-margin comparisons under a fixed manifest and compiled
snapshot. The candidate is currently one nominated JSON input, limited to
1 MiB; this is not yet general versioning of a complete engineering design.
The shared `core_constellation` query exposes one log's recorded lineage and
results; see [Core tools](CORE_TOOLS.md).

[ADR-0007](../adr/0007-execution-receipts.md) supplies receipt-based execution
reuse. Content hashes identify artifacts but do not themselves provide a
deduplicating artifact store. Broader semantic invalidation is specified in
[ADR-0006 SC-12](../adr/0006-semantic-core.md#sc-12-change-invalidation-and-reuse).
Use the [status ledger](../roadmap/STAGE_0_STATUS.md) for implementation status;
these requirements do not mark any additional gate complete.

## Requirements and acceptance evidence

### DH-01: Distinguish design, execution, assessment, and baseline

A design revision identifies an immutable proposed state and its ancestry. It
can exist before execution and can have multiple executions, including different
seeds, fidelities, and methods. An assessment binds evidence to the exact
contract, semantic profile, and applicable policy and qualification snapshots.
A baseline names an exact revision and assessment with attributable acceptance.
These are conceptual responsibilities, not a prescribed schema or crate split.

Acceptance: one unchanged design acquires additional evidence and a new
assessment without rewriting its earlier records or inventing a geometry
change. An unevaluated proposal never inherits a parent's verdict by display
convention. Accepting a baseline cannot change a technical verdict, and human
acceptance is not required for ordinary iteration or verdict derivation.

### DH-02: Make alternatives and engineering deltas navigable

Provide named alternatives and comparisons between a baseline and a selected
revision, using shared headless operations. Show changed inputs, methods,
datasets, requirements, and assumptions alongside verdict transitions and
compatible quantity and margin deltas. Derived values retain units and evidence
links; incomparable or unavailable values have explicit reasons. Domain-specific
meaning, such as conductor usage, comes from declared data or capabilities,
not a UI inference from arbitrary JSON fields.

Acceptance: a branching history can show an improvement in one requirement and
a regression in another without selecting a winner. Named references resolve
to exact identities; moving a reference preserves its attributable history.
Recorded results are distinguished from current artifact verification.

### DH-03: Preserve fixed questions and make amendments explicit

Keep the existing refusal of manifest or compiled-snapshot changes within an
attempt lineage. A future cross-question history may link a new root through an
explicit amendment recording what changed, who changed it, and the stated
rationale. It must not weaken the existing fixed-question boundary.

Acceptance: a design improvement under an unchanged requirement is visibly
different from an unchanged design passing a relaxed requirement. Comparisons
across amendments disclose the changed boundary and withhold incompatible
margin deltas. Prose rationale never changes evaluation semantics.

### DH-04: Explain change impact before expensive execution

For each affected conclusion, distinguish recomputation, evidence re-admission,
requirement reevaluation, and permitted reuse. Show the dependency path and
identity or governed rule responsible for each decision. Unknown applicability
requires rerunning the affected computation under SC-12; unavailable execution
must not leave unsupported current evidence appearing usable.

Semantic non-dependence must come from explicit input separation or an
applicable governed reuse rule. Core must not assume that a field named
"color" is irrelevant to a solver. Artifact deduplication does not grant
permission to reuse a calculation across cases or assessment boundaries.

Acceptance scenarios include a presentation-only change with established
non-dependence, a material change affecting downstream methods, a requirement
limit change requiring reevaluation without unnecessary solver execution, and
a qualification or defect change affecting evidence usability despite unchanged
artifact bytes. Preserve historical assessments and record subsequent usability
changes separately rather than rewriting history.

### DH-05: Review a proposed change with attributable rationale

A change-review view binds the compared revisions and assessments and presents
the stated goal, actual delta, execution/reuse status, requirement transitions,
remaining uncertainty, and any policy-required acknowledgements. Acceptance
records identify actor, scope, and exact reviewed identities. Changes after
review cannot silently inherit that acceptance.

Record designer or agent hypotheses and selection rationale separately from
Core-derived observations. Parentage establishes ancestry, not why a candidate
was selected or proof that a change caused an observed improvement.

Acceptance: an engineer can review the delta and follow each result to evidence;
missing rationale remains unknown. Review state cannot turn FAIL, INCONCLUSIVE,
or NOT_EVALUATED into PASS or impose universal human signoff.

### DH-06: Keep exploration history cheap and inspectable

Store identical large artifacts once within the supported storage boundary,
referenced by content identity. Define retention and export behavior so retained
assessments either keep their required evidence available or explicitly report
its absence. Maintain portable inspection without an Avila-hosted service.

Campaign summaries may collapse rejected or unevaluated attempts for navigation
but must preserve their records, distinguish execution failures from technical
FAIL, and expose the criteria used to select "meaningful" iterations. Summaries
must never manufacture ancestry or scientific rationale.

Acceptance: measure append, query, comparison, verification, and storage costs
on a declared campaign workload up to 10,000 candidates before claiming support
at that scale. Include repeated large datasets and rejected attempts. The
current per-append history validation is a scaling question to measure, not a
reason to bypass integrity checks or immediately replace storage.

## Delivery order and boundaries

1. Extend the existing recorded-history queries and present design comparisons
   for the selected reference workflow (DH-02 and the observational portion of
   DH-05). Keep source identities and verification limitations visible.
2. Specify revision/assessment separation, named alternatives, baselines, and
   explicit amendments before adding their write operations (DH-01–03, DH-05).
3. Extend change-impact previews and governed invalidation with adversarial
   acceptance cases (DH-04). Use the existing execution reuse as the first slice.
4. Add artifact deduplication, retention, and indexed history as measured
   workloads require them (DH-06).

These increments follow the roadmap's existing evidence gates. They do not
override the measured-workload gates for a transactional store, prepared
session, or scheduler. No automatic engineering merge, distributed Git protocol,
optimizer inside the kernel, or universal CAD semantic diff is required.
Combining alternatives produces a new candidate whose evidence must be assessed;
passing results from its parents do not establish that the combination passes.

## Product language

"Version control for engineering decisions and their evidence" is a useful
explanatory analogy to test with users. A more direct promise is: "Core tracks
how a design evolves, what the evidence supports, and what must be checked again
when something changes." This does not rename the product, replace the evidence
compiler architecture, or claim the full experience already exists.
