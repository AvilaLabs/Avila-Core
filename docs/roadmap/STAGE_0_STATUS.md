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
  reports the package-integrity, compilation, execution, claim-generation,
  evaluation, and replay workflow; omitted external roots remain visible as
  `not_checked` and an unsupplied executable as `NOT RUN`.
- [ADR-0006](../adr/0006-semantic-core.md) defines the proposed rules and its
  acceptance conditions.
- [Campaign evaluation](../architecture/CAMPAIGN_EVALUATION.md) defines the
  exact boundary of the partial SC-10/SC-11 slice.
- The [roadmap](ROADMAP.md) defines the stage gates.
- The [first-pilot discovery packet](../discovery/FIRST_PILOT_PACKET.md) records
  sanitized validation evidence and the candidate workflow.

When those sources disagree, the executable command is authoritative only for
what is implemented; it cannot satisfy a product, scientific, legal, or
external product gate.

## Executable baseline

| Surface | State | Evidence present | Material boundary still open |
| --- | --- | --- | --- |
| Canonical semantic kernel | Partial | 91 pure vectors: canonical values, exact unit scaling, three-valued predicates, and four-state verdict calculus | Total archive/package canonicalization, remaining predicate and aggregation boundaries, and independent implementation |
| Static contract compiler | Partial | 68 fixtures across R1–R10, deterministic source diagnostics, exact lowering, and content-identified compiled snapshots | Package binding halves, qualification, organization policy, selection, lifecycle, and accepted profile |
| Diagnostic contract | Implemented for current compiler slice | 34 catalogued finding codes, anchored JSON Pointers, typed RFC 6902 repairs, and mutation tests | Real-contract defect corpus, fixture coverage checker, and measured false-block/clarification outcomes |
| Campaign evaluation | Partial | 12 fixtures for snapshot binding, type-level admission, quarantine propagation, exact verdicts, qualification boundaries, and proof that optional presentation state cannot alter PASS or FAIL; CASE-000 adds one ACTINV → Aftermatter composition whose two computational steps are executed | The standalone evaluator still does not read bytes; receipts are verified in the case runner, not the evaluator; capability identities beyond an executable digest, signed A2, A7/A8/A10, full qualification and policy evaluation, and invalidation remain open |
| Internal composed case | Implemented as an unqualified specimen | CASE-000 revision 3 binds 14 source attestations and 6 output claims; with all three roots supplied the runner re-hashes all 18 artifacts, executes ACTINV 1.0.1 through Aftermatter's frozen R0 builder under a digest-pinned interpreter (`avila-labs.aftermatter/build-r0-case@1`) and then Aftermatter over the fresh inventory (`avila-labs.aftermatter/evaluate@1`), verifies both receipts against the committed ones, extracts all 6 claims from fresh results that reproduce the frozen artifacts byte for byte, and reproduces two numeric `PASS / bounded.lt.within` verdicts plus the expected Clive route-state `FAIL / categorical.equals.mismatch` with no review stage | Capability packages beyond an executable digest, a system interpreter pinned only by digest, categorical waste-class or multi-route aggregation, qualification evidence, and broader physical applicability |
| Generative-loop case | Implemented as unqualified specimens | CASE-001 declares a free candidate input, screens it with an unqualified attenuation script, runs OpenMC transport on request, reports exact margins, and exercises coverage, qualification, and optional presentation boundaries. CASE-008 adds native identity-bound lineage: an independent circuit-dump root misses two fixed gates and a one-parameter passive mode-selective child passes all ten. CASE-009 carries that mechanism into a dimensional cryoresistive-copper screen: the root amplifies both shape metrics, the stronger child worsens both margins and adds a voltage failure, and Core verifies the exact parent plus unchanged manifest/snapshot while deriving the candidate diff, verdict transition, and 15 exact numeric margin deltas. | Validation evidence behind any envelope, enforcement of qualification for every bounded verdict, physical evidence, and an optimizer or constellation view over the log |
| Coupled case | Implemented as an unqualified specimen, pre-registered experiment | CASE-002 chains screen, coupled neutron-photon OpenMC transport with per-layer FISPACT-709 spectra, and ACTINV activation per layer; six requirements with exact margins; both qualification envelopes evaluated `INSIDE` for the reference; coverage complete with activation and streaming as stated omissions; a learning designer, three practice baselines, and a control sweep exist as scripts; `PROTOCOL.md` pre-declares arms, measurements, and outcomes | The campaign itself (all arms), a bounded activation claim, validation evidence behind either envelope, and transport that is faster than minutes per finalist |
| Thermal case | Implemented as an unqualified specimen | CASE-003: a strip-heated layered plate screened by a one-dimensional resistance estimate and decided by two-dimensional finite-element conduction as an enclosure-basis interval; the finite-element qualification record binds a NAFEMS T4 reproduction as validation evidence; reference blessed and verified with every step reused | A campaign under the manifest pin; interface and transient entries of the thermal library set |
| Evidence records | Partial spike | Minimal record types, SHA-256 helpers, confined-path case manifest with bound capabilities and executions, explicit `not_checked` states, execution receipts verified from bytes, claim/policy binding, content-identified optional presentation requests, deterministic replay, and identity-bound JSON candidate lineages with canonical snapshots, typed diffs, exact parent-record hashes, fixed-manifest/snapshot validation, verdict transitions, and exact margin comparisons | Canonical archive/package identity, signatures, trust roots, invalidation, constellation presentation, and an independently implemented verifier |
| Planning and selection | Planned | Static type satisfiability only | Bound plans, package discovery, admissibility before ranking, deterministic selection, and estimates |
| Execution | Partial | Fresh workspace, staged verified bytes, cleared environment, timeout, regular-file output collection, execution receipts, claim extraction, receipt-based reuse with typed SC-12 change classes and `--plan`, purpose-built adapters, a hash-bound declarative external-checker adapter with closed exact/categorical extraction, and adversarial tests for modified inputs, unchecked bytes, wrong or missing executables, failing runs, drifting outputs, missing or edited receipts, wrong step types, reuse, requirement-only change, selective rerun across the two-step chain, supplied free inputs with receipt-bound outputs, withheld claims for a reached step that did not run, and refusal of an undeclared free input | Sandboxing, resource accounting, artifact store, generic adapter lifecycle beyond the narrow checker descriptor, cancellation, recovery, signatures, and change classes a receipt cannot see (policy, qualification, advisory) |
| Application | Partial | Thin egui workbench over the runner and compiler: case setup from the package's requested roots and executables, background Plan/Run with elapsed time, an evidence view with reuse, change classes, receipts, claims, verdicts, boundaries, and optional presentation-gate readiness/instructions, guided help with five spotlight walkthroughs and bundled answers, dark and light themes; a specimen compiler view | Question-first editor, contract authoring, campaign state beyond one run, and autonomous search controls |

## Stage 0 build gates

| Gate | State | Evidence needed to close it |
| --- | --- | --- |
| Authoritative contract and verdict foundation | Partial | Current compiler and campaign slice plus package/bound-plan model driven by the selected pilot |
| Proposed semantic profile and adversarial fixtures | Partial | All ADR-0006 acceptance conditions; draft status remains visible |
| Question-first workbench | Partial | The case workbench renders every stage of a run; authoring and preflight flows, and testing with pilot participants, remain |
| Deterministic planning and blocked states | Open | Bound-plan and state fixtures for the selected vertical |
| Capability threat model and conformance design | Partial | Adversarial review tied to the actual adapter and execution boundary |
| Evidence-package spike | Partial | CASE-000 performs an integrity/compile/execute/generate/bind/evaluate/replay round trip with every declared source byte verified and one execution receipt; close only after redaction/retention rules, package-root semantics, and an independently implemented verifier are exercised |
| Problem-evidence packet | Ready | The discovery packet is present; interviews may inform product selection but are not an engineering or verdict gate |
| Bounded autonomous benchmark | Open | One fixed requirement set, candidate space, references, known shortcuts, and success/exhaustion condition |

## Stage 0 validation gates

| Gate | Current evidence | Target |
| --- | --- | --- |
| Technical verdict independent of review | Implemented | Kernel, campaign, claims, schemas, CASE-000, and regressions contain no review-gating path |
| Fixed requirements during search | Implemented for native JSON attempt lineages | CASE-008 and CASE-009 each record two attempts under one manifest and compiled snapshot; Core refuses a child if its parent record, manifest, snapshot, candidate state, or derived diff cannot be revalidated. CASE-009 additionally demonstrates exact negative margin comparison and PASS→FAIL transitions. Signed package roots remain future work. |
| Known shortcut refusal | Exercised | An adversarial designer arm (CASE-002 campaign 3, amendment A6) confirmed refusals for out-of-envelope candidates, a wrong interpreter digest, a particle count below its domain, and coverage on a weaker basis, and obtained undeserved verdicts only by rewriting the package; the identity log and the manifest pin (S-030) now make a rewritten package a refusal or a visibly different identity | Signed receipts and manifests |
| Autonomous search outcome | Reached once | CASE-002 campaign 3: a language-model designer found an all-PASS design 277 kg lighter than the sweep's best at its second transport and stopped by judgment; the seeded surrogate found one at its eleventh; see `RESULTS.md` | Repeat on a second domain (CASE-003) and under the manifest pin |
| Optional practicality routing | Implemented for the slice | Exact instructed dossier exercises both `request_changes` and `present_to_user`; omission leaves Core fully usable |
| Independent verification | Open | A separately implemented verifier reproduces package identity and verdicts |
| Performance baseline | Open | Record candidates explored, wall time, compute, retries, and convergence or exhaustion |
| Tool license and deployment feasibility | Open | Every external tool in the chosen reference chain |

User interviews, partner participation, and external reviews may be useful
product evidence, but none is required to progress the repository or obtain a
Core technical verdict.

## Exit decision

Stage 0 exits when one bounded non-sensitive benchmark has fixed requirements
and known bad cases, and a connected agent uses Core to reject a shortcut and
either finds an all-gates-passing candidate or records bounded exhaustion with
reproducible evidence. A professional reviewer is not an exit dependency.
Before that point:

- expand the generic semantic surface only to fix a demonstrated correctness or
  maintainability problem;
- do not build a horizontal runner, registry, marketplace, or provider-routing
  layer;
- use benchmark evidence to decide the first package, receipt, adapter, and
  invalidation boundaries; and
- keep `0.2` and ADR-0006 in draft status.

## Immediate work queue

1. Sign receipts and manifests so the requester's manifest pin is no longer
   the only anchor against a rewritten package (S-030).
2. Diffusion synthetic acceleration for the slab S_N solver (S-031), so the
   forward screen runs in under a second and the adjoint makes CADIS windows
   worth their cost; then bind the solver as the shielding screen and qualify
   it for the polyethylene families against the Monte Carlo record.
3. Run a CASE-003 campaign under the manifest pin with the same arms as
   CASE-002 campaign 3.
4. Make every bounded verdict require an applicable qualification record, as
   deferred by ADR-0008, and bind a bounded activation claim.
5. Build an independent offline verifier for the package, compiled snapshot,
   claims, and campaign identities.

## Update rule

Update this ledger in the same change that materially changes a gate. If fixture
counts change, update the assertions in `avila-core-cli`, run
`avila-core semantic-profile`, and reconcile the README and fixture manifest.
Never replace an evidence link with an unsupported status word.
