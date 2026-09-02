# Stage 0 status ledger

**Current gate:** Stage 0 — foundation and discovery  
**Semantic profile:** `avila.core/semantic/0.2-draft`  
**Product claim:** pre-alpha research scaffold; no scientifically qualified
workflow or decision-grade evidence

This ledger is the short operational view of the roadmap. It separates an
executable software slice from the evidence required to advance the product.
Passing a fixture proves only the behavior named by that fixture.

## Authoritative status sources

- `cargo run -p avila-core-cli -- semantic-profile` reports executable fixture
  sets, counts, and content identities.
- `cargo run -p avila-core-cli -- run examples/cases/case-000-actinv-aftermatter`
  reports the first package-integrity, compilation, evaluation, and replay
  workflow; omitted external roots remain visible as `not_checked`.
- [ADR-0006](../adr/0006-semantic-core.md) defines the proposed rules and its
  acceptance conditions.
- [Campaign evaluation](../architecture/CAMPAIGN_EVALUATION.md) defines the
  exact boundary of the partial SC-10/SC-11 slice.
- The [roadmap](ROADMAP.md) defines the stage gates.
- The [first-pilot discovery packet](../discovery/FIRST_PILOT_PACKET.md) records
  sanitized validation evidence and the candidate workflow.

When those sources disagree, the executable command is authoritative only for
what is implemented; it cannot satisfy a product, scientific, legal, or
professional gate.

## Executable baseline

| Surface | State | Evidence present | Material boundary still open |
| --- | --- | --- | --- |
| Canonical semantic kernel | Partial | 90 pure vectors: canonical values, exact unit scaling, three-valued predicates, and four-state verdict calculus | Total archive/package canonicalization, remaining predicate and aggregation boundaries, and independent implementation |
| Static contract compiler | Partial | 68 fixtures across R1–R10, deterministic source diagnostics, exact lowering, and content-identified compiled snapshots | Package binding halves, qualification, organization policy, selection, lifecycle, and accepted profile |
| Diagnostic contract | Implemented for current compiler slice | 35 catalogued finding codes, anchored JSON Pointers, typed RFC 6902 repairs, and mutation tests | Real-contract defect corpus, fixture coverage checker, and measured false-block/clarification outcomes |
| Campaign evaluation | Partial | 12 fixtures for snapshot binding, type-level admission, quarantine propagation, exact verdicts, and review asymmetry; CASE-000 adds one pinned ACTINV → Aftermatter composition | The standalone evaluator still does not read bytes; receipts, capability package identities, A2/A4/A7/A8/A10, signed review fulfillment, qualification, policy evaluation, and invalidation remain open |
| Internal composed case | Implemented as an unqualified specimen | CASE-000 pins 12 source attestations and 4 output claims; the runner binds all 16 records, re-hashes 9 of 14 distinct artifacts from the available Aftermatter root, compiles the three-step graph, and reproduces both `NOT_EVALUATED / review_pending` verdicts | Five ACTINV data-release files, tool invocation, receipts, capability packages, categorical route verdicts, qualified review, and any participant-backed applicability decision |
| Evidence records | Partial spike | Minimal record types, SHA-256 helper, confined-path draft case manifest, explicit `not_checked` states, claim/policy binding, and deterministic replay | Canonical archive/package identity, writer, full lineage validation, receipts, signatures, trust roots, redaction, invalidation, and an independently implemented verifier |
| Planning and selection | Planned | Static type satisfiability only | Bound plans, package discovery, admissibility before ranking, deterministic selection, estimates, and approvals |
| Execution | Planned | None | Controlled workspace, adapter lifecycle, artifact store, receipts, isolation, cancellation, and recovery |
| Application | Partial | Thin egui report shell over the authoritative compiler | Question-first editor, campaign state, evidence view, and usability evidence |

## Stage 0 build gates

| Gate | State | Evidence needed to close it |
| --- | --- | --- |
| Authoritative contract and verdict foundation | Partial | Current compiler and campaign slice plus package/bound-plan model driven by the selected pilot |
| Proposed semantic profile and adversarial fixtures | Partial | All ADR-0006 acceptance conditions; draft status remains visible |
| IP and prior-art review before public enabling disclosure | Open | Recorded counsel/strategy decision covering compiler, admission, signed reuse, and selective recomputation |
| Question-first workbench | Partial | Usable authoring and preflight flow tested with pilot participants |
| Deterministic planning and blocked states | Open | Bound-plan and state fixtures for the selected vertical |
| Capability threat model and conformance design | Partial | Adversarial review tied to the actual adapter and execution boundary |
| Evidence-package spike | Partial | CASE-000 performs an offline manifest/integrity/binding/compile/evaluate/replay round trip; close only after every declared source byte or explicit redaction/retention rule, plus receipt and package-root semantics, is exercised |
| Interview materials | Ready | The discovery packet is present; completed interviews are counted separately below |
| Concrete first-contract workflow map | Open | One participant-validated current-state map with authoritative artifacts and owners |

## Stage 0 validation gates

| Gate | Current evidence | Target |
| --- | ---: | ---: |
| Role-separated interviews recorded | 0 | 15–25 |
| Detailed completed decision loops mapped | 0 | At least 3 |
| Quantified cycle-time/labor/compute/review baseline | 0 | At least 1 complete candidate baseline |
| Candidate tool license and deployment feasibility reviews | 0 | Every tool in the proposed pilot chain |
| Named organization committed to a bounded pilot | 0 | 1 |
| Named method owner committed | 0 | At least 1 for every material method |
| Named independent reviewer committed | 0 | At least 1 |
| Representative non-sensitive case committed | 0 | 1 |

Zero means no completion evidence is recorded in this repository. Work may
exist elsewhere, but it does not close a gate until a sanitized source reference
is added to the discovery packet.

## Exit decision

Stage 0 exits only when one organization commits real people, representative
non-sensitive data, a method owner, and a reviewer to a bounded pilot. Before
that point:

- expand the generic semantic surface only to fix a demonstrated correctness or
  maintainability problem;
- do not build a horizontal runner, registry, marketplace, or provider-routing
  layer;
- use real workflow evidence to decide the first package, receipt, adapter, and
  invalidation boundaries; and
- keep `0.2` and ADR-0006 in draft status.

## Immediate work queue

1. Conduct role-separated interviews using the discovery packet and attach only
   sanitized evidence references.
2. Map three recently completed decision loops, including handoffs, rework,
   reviewer questions, elapsed time, labor, compute, and delay cost.
3. Decide whether the shutdown-dose chain survives H1–H3 and the ownership,
   licensing, data-boundary, and qualification gates.
4. If it survives, freeze one non-sensitive reference case and derive the
   smallest vertical backlog: bound plan, required adapters, controlled receipt,
   package identity, offline package, and typed invalidation. Use CASE-000 as a
   pressure test for those boundaries, not as the committed representative
   case.
5. If it fails, record the falsifier and evaluate another recurring technical
   question without preserving the nuclear wedge by inertia.

## Update rule

Update this ledger in the same change that materially changes a gate. If fixture
counts change, update the assertions in `avila-core-cli`, run
`avila-core semantic-profile`, and reconcile the README and fixture manifest.
Never replace an evidence link with an unsupported status word.
