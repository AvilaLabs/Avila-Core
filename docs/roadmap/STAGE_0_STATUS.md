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
| Internal composed case | Implemented as an unqualified specimen | CASE-000 revision 2 binds 14 source attestations and 6 output claims; with all three roots supplied the runner re-hashes all 18 artifacts, executes ACTINV 1.0.1 through Aftermatter's frozen R0 builder under a digest-pinned interpreter (`avila-labs.aftermatter/build-r0-case@1`) and then Aftermatter over the fresh inventory (`avila-labs.aftermatter/evaluate@1`), verifies both receipts against the committed ones, extracts all 6 claims from fresh results that reproduce the frozen artifacts byte for byte, and reproduces both technical `PASS / bounded.lt.within` verdicts with no review stage | Capability packages beyond an executable digest, a system interpreter pinned only by digest, categorical route verdicts, qualification evidence, and broader physical applicability |
| Generative-loop case | Implemented as an unqualified specimen | CASE-001 declares a free candidate input, screens it with an unqualified attenuation script, runs OpenMC transport on request under a receipt whose required environment key is identity by name, reports exact margins on all four verdicts, appends a campaign-log line per run, and a scripted designer drives the two-fidelity loop; the reference candidate passes the screen and fails transport; the contract's coverage of the shielding library's seven-entry requirement set is assessed before execution (three covered, three omitted with accepted reasons, one omissible) and an unstated omission stops the run; the transport capability carries a qualification record whose envelope is evaluated over the candidate's facts before the step runs, and a 150 cm candidate's transport verdict is `NOT_EVALUATED` outside the envelope although transport ran; a configured agent presentation gate receives an exact content-identified dossier and instructions, may return the candidate, present it to the user, or abstain, and never gates a technical verdict | Validation evidence behind any envelope, enforcement of qualification for every bounded verdict, physical evidence, and an optimizer or constellation view over the log |
| Evidence records | Partial spike | Minimal record types, SHA-256 helpers, confined-path case manifest with bound capabilities and executions, explicit `not_checked` states, execution receipts verified from bytes, claim/policy binding, content-identified optional presentation requests, a separate unsigned agent routing record, and deterministic replay of claims, receipts, and campaign report | Canonical archive/package identity, writer, full lineage validation, signatures, trust roots, redaction, invalidation, and an independently implemented verifier |
| Planning and selection | Planned | Static type satisfiability only | Bound plans, package discovery, admissibility before ranking, deterministic selection, and estimates |
| Execution | Partial, case-specific | Fresh workspace, staged verified bytes, cleared environment, timeout, declared-output collection, execution receipts, claim extraction, receipt-based reuse with typed SC-12 change classes and `--plan`, and adversarial tests for modified inputs, unchecked bytes, wrong or missing executables, failing runs, drifting outputs, missing or edited receipts, wrong step types, reuse, requirement-only change, selective rerun across the two-step chain, supplied free inputs with receipt-bound outputs, withheld claims for a reached step that did not run, and refusal of an undeclared free input | Sandboxing, resource accounting, artifact store, generic adapter lifecycle, cancellation, recovery, signatures, and change classes a receipt cannot see (policy, qualification, advisory) |
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
| Fixed requirements during search | Partial | Agent cannot silently amend the compiled contract or coverage declaration during an iteration series |
| Known shortcut refusal | Partial | Coverage omissions and out-of-envelope transport are refused; add one benchmark-specific adversarial shortcut |
| Autonomous search outcome | Open | Agent finds an all-gates-passing candidate or records bounded exhaustion on the chosen benchmark |
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

1. Make every bounded verdict require an applicable qualification record, as
   already deferred by ADR-0008.
2. Turn CASE-001's scripted search into a bounded optimizer loop with a declared
   stopping condition and machine-readable constellation summary.
3. Add a candidate or adjusted benchmark for which every technical requirement
   can pass, then prove the optional agent gate can return a practical defect
   and present a clean finalist.
4. Build an independent offline verifier for the package, compiled snapshot,
   claims, and campaign identities.
5. Add physical or reference validation evidence to the first qualification
   envelope and record the exact boundary it supports.

## Update rule

Update this ledger in the same change that materially changes a gate. If fixture
counts change, update the assertions in `avila-core-cli`, run
`avila-core semantic-profile`, and reconcile the README and fixture manifest.
Never replace an evidence link with an unsupported status word.
