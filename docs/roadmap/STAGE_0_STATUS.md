# Stage 0 status ledger

**Current gate:** Stage 0 — foundation and discovery  
**Semantic profile:** `avila.core/semantic/0.2-draft`  
**Product claim:** pre-alpha research scaffold; no scientifically qualified
workflow or decision-grade evidence  
**Reconciled:** 2026-09-16 against code at `abd7079` and the executable
reports named below

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
| Canonical semantic kernel | Partial | 122 pure vectors: canonical values, exact unit scaling, three-valued predicates, and four-state verdict calculus including every categorical `equals`/`in_set` edge, the mixed admitted+non-admitted `not_evaluated` ordering, and the enclosure and decisive-side one-sided corners — a second implementation (the Python verifier) agrees with every vector | Total archive/package canonicalization, remaining predicate and aggregation boundaries |
| Static contract compiler | Partial | 70 fixtures across R1–R10, deterministic source diagnostics, exact lowering, and content-identified compiled snapshots | Package binding halves, qualification, organization policy, selection, lifecycle, and accepted profile |
| Diagnostic contract | Implemented for current compiler slice | 34 catalogued finding codes, anchored JSON Pointers, typed RFC 6902 repairs, mutation tests, and a 34-case real-contract defect corpus pinning status, findings, and snapshot identity for seeded defects in the committed CASE-001 pair, and a coverage check that fails when a catalogued code is exercised by no fixture and no named test | Measured false-block/clarification outcomes |
| Campaign evaluation | Partial | 18 fixtures for snapshot binding, type-level admission, quarantine propagation, exact verdicts, qualification boundaries including `require_qualification` enforcement (S-035), and proof that optional presentation state cannot alter PASS or FAIL; CASE-000 adds one ACTINV → Aftermatter composition whose two computational steps are executed; `evaluate --artifact` re-hashes supplied bytes against attested identities and marks every admission `verified` or `not_checked` | Receipts are verified in the case runner, not the evaluator; capability identities beyond an executable digest, signed A2, A7/A8/A10, full qualification and policy evaluation, and invalidation remain open |
| Internal composed case | Implemented as an unqualified specimen | CASE-000 revision 3 binds 14 source attestations and 6 output claims; with all three roots supplied the runner re-hashes all 18 artifacts, executes ACTINV 1.0.1 through Aftermatter's frozen R0 builder under a digest-pinned interpreter (`avila-labs.aftermatter/build-r0-case@1`) and then Aftermatter over the fresh inventory (`avila-labs.aftermatter/evaluate@1`), verifies both receipts against the committed ones, extracts all 6 claims from fresh results that reproduce the frozen artifacts byte for byte, and reproduces two numeric `PASS / bounded.lt.within` verdicts plus the expected Clive route-state `FAIL / categorical.equals.mismatch` with no review stage | Capability packages beyond an executable digest, a system interpreter pinned only by digest, categorical waste-class or multi-route aggregation, qualification evidence, and broader physical applicability |
| Generative-loop case | Implemented as unqualified specimens | CASE-001 declares a free candidate input, screens it with an unqualified attenuation script, runs OpenMC transport on request, reports exact margins, and exercises coverage, qualification, and optional presentation boundaries. CASE-008 adds native identity-bound lineage: an independent circuit-dump root misses two fixed gates and a one-parameter passive mode-selective child passes all ten. CASE-009 carries that mechanism into a dimensional cryoresistive-copper screen: the root amplifies both shape metrics, the stronger child worsens both margins and adds a voltage failure, and Core verifies the exact parent plus unchanged manifest/snapshot while deriving the candidate diff, verdict transition, and 15 exact numeric margin deltas. | Validation evidence behind any envelope, physical evidence, and an optimizer over the log (a read-only per-log constellation landed, S-049); the screen's geometry qualification and `require_qualification` landed (S-039) |
| Coupled case | Implemented as an unqualified specimen, pre-registered experiment | CASE-002 chains screen, coupled neutron-photon OpenMC transport with per-layer FISPACT-709 spectra, and ACTINV activation per layer; six requirements with exact margins; both qualification envelopes evaluated `INSIDE` for the reference, a scoped screen qualification (S-039) covering its mass and thickness slots, and `execution_policy.require_qualification` set; coverage complete with activation and streaming as stated omissions; a learning designer, three practice baselines, and a control sweep exist as scripts; `PROTOCOL.md` pre-declares arms, measurements, and outcomes | The campaign itself (all arms), a bounded activation claim, validation evidence behind either envelope, and transport that is faster than minutes per finalist |
| Thermal case | Implemented as an unqualified specimen | CASE-003: a strip-heated layered plate screened by a one-dimensional resistance estimate and decided by two-dimensional finite-element conduction as an enclosure-basis interval; the finite-element qualification record binds a NAFEMS T4 reproduction as validation evidence; reference blessed and verified with every step reused; a scoped screen qualification (S-039) covers its areal-mass and thickness slots and `execution_policy.require_qualification` is set | A campaign under the manifest pin; interface and transient entries of the thermal library set |
| Evidence records | Partial spike | Minimal record types, SHA-256 helpers, confined-path case manifest with bound capabilities and executions, explicit `not_checked` states, execution receipts verified from bytes, claim/policy binding, content-identified optional presentation requests, deterministic replay, and identity-bound JSON candidate lineages with canonical snapshots, typed diffs, exact parent-record hashes, fixed-manifest/snapshot validation, verdict transitions, and exact margin comparisons; detached Ed25519 signatures over manifests, receipts and log lines verified against a trust root, with `execution_policy.require_signatures` (S-040); an independently implemented, standard-library Python verifier that re-derives package identity, receipt identities, claims binding and every verdict without importing Core (S-041); and ADR-0019 design-history records in the same campaign log — design revisions that exist before any run cites them, assessment rows that bind an exact run line to its revision with verdicts verbatim, per-log named references with supersession history, and contract amendments linking roots across a deliberate question change | Canonical archive-level identity (the signatures are document-level), invalidation, and constellation presentation |
| Planning and selection | Planned | Static type satisfiability plus bound plans — `run --plan` emits a content-identified `bound-plan/v0.1-draft` with a closed per-step decision vocabulary (`reuse_committed`/`execute`/`blocked`/`refused`/`not_executed`) that verifies declared capability bytes and names unmet roots, capabilities, and environment keys — and deterministic capability discovery: `--capability-dir` scans a directory and binds the file whose digest equals the manifest's `executable_sha256` pin, with the resolution source recorded on the bound plan, and recorded-duration estimates — each bound step states its committed receipt's last `duration_ms` as `estimated_duration_ms` | Multi-implementation ranking policy, package discovery across registries, and estimates beyond last-recorded durations |
| Execution | Partial | Fresh workspace, staged verified bytes, cleared environment, timeout, regular-file output collection, execution receipts, claim extraction, receipt-based reuse with typed SC-12 change classes and `--plan`, purpose-built adapters, a hash-bound declarative external-checker adapter with closed exact, interval, numeric unquantified and categorical extraction (ADR-0017), including descriptor-identity invalidation before receipt reuse, and adversarial tests for modified inputs, unchecked bytes, wrong or missing executables, failing runs, drifting outputs, missing or edited receipts, wrong step types, reuse, requirement-only change, selective rerun across the two-step chain, supplied free inputs with receipt-bound outputs, withheld claims for a reached step that did not run, refusal of an undeclared free input, structural validation of every supplied free input against its role's schema before staging (S-036), a step timeout that kills the whole process group (S-037), a run-attempt log append that is one write under an exclusive lock (S-037), an opt-in verified-hash cache for operator artifact roots reported as the distinct `verified_cached` state (S-038), rendering, log append, qualification loading, free-input handling, attempt comparison and the engine core (staging, SC-12 reuse, receipt verification, replay, presentation gates, diagnostic capture) extracted into `case_run` submodules under a golden CLI-output diff (S-048), and adversarial tests pinning that SC-12 reuse never masks a registry, presentation or qualification edit; an SC-12.6 impact report on every bound plan — each invalidated node with its condemning edge path, each reused node under the `deterministic_memo` authority, the minimal rerun subgraph, and a recorded-duration cost estimate — nondeterministic capability types never reusing committed receipts (`ChangeClass::Nondeterministic`); and SC-12.3 reuse rules — signed `reuse-rule/v0.1-draft` non-dependence claims scoped to a `(step, input_slot)` binding edge, verified against a requester key in the supplied trust root, refusing unsigned/expired/edge-widening/unparseable rules with `CORE-X3401` and failing closed with no trust root — an applicable rule exempts its edge's input changes (recorded with `exempted_by`) so the step reuses under `reuse_rule:<id>`; ADR-0015 signature verification against an operator-supplied trust root (S-040): a manifest signature must verify against a listed requester key before compilation and a committed receipt's signature against a listed runner key to be reused, and reuse now also checks a receipt's case_id | Sandboxing, resource accounting, artifact store, generic adapter lifecycle beyond the narrow checker descriptor, recovery, change classes a receipt cannot see (policy, qualification, advisory), and a log lock that is single-host only |
| Application | Partial | Thin egui workbench over the runner and compiler: a local case browser with remembered data/program locations and a per-example needs line (shipped versus operator folders, programs to locate — metadata, not verification), case setup from the package's requested roots and executables with shipped data folders offered on bundled examples and hash-only program probing (typed path, `PATH` search, folder scan) shared with the CLI `capabilities` verb — probing never executes a candidate and the check/run path re-verifies chosen bytes, background Plan/Run with elapsed time, an evidence view with reuse, change classes, receipts, claims, verdicts, boundaries, and optional presentation-gate readiness/instructions, a Tools workspace exposing all fourteen shared recorded-result queries over saved reports, the current workbench run, and one explicit campaign log, a History workspace over `core_constellation`/`core_attempt` showing roots, children, untracked legacy rows, typed candidate changes, recorded verdicts, and the bound-parent comparison with four-state transitions, exact deltas, and stated unavailability, Design attempt fields on machine setup that assemble the runner's `AttemptLineageRequest` (with History prefill of a valid parent), guided help with five spotlight walkthroughs and bundled answers, dark and light themes; a specimen compiler view with JSON-level preflight editing plus a Question workspace of form fields over the question layer (bounded question, policy flags, assumptions, contract inputs with registry-driven role/media-type/claim-model choices, workflow steps with capability-type/input-binding/parameter/seed fields plus typed material-factor and review-binding sections, and numeric and categorical requirements — all writing the same buffer the compiler checks), and a recorded-only campaign supervision roll-up over `core_constellation` | Campaign state beyond one log's recorded lineage (cross-campaign, live in-progress), and autonomous search controls |

## Stage 0 build gates

| Gate | State | Evidence needed to close it |
| --- | --- | --- |
| Authoritative contract and verdict foundation | Partial | Current compiler and campaign slice plus package/bound-plan model driven by the selected pilot |
| Proposed semantic profile and adversarial fixtures | Partial | All ADR-0006 acceptance conditions; draft status remains visible |
| Question-first workbench | Partial | The case workbench renders every stage of a run and the specimen compiler does JSON-level preflight editing plus form-based authoring over the question layer — bounded question, policy, assumptions, inputs, workflow steps (capability type, input bindings, typed parameters, seed, material factors, review binding), and requirements. Testing with pilot participants remains |
| Deterministic planning and blocked states | Implemented for the current slice | `run --plan` emits a content-identified `bound-plan/v0.1-draft` document inside the run report: per step, a closed decision vocabulary — `reuse_committed`, `execute`, `blocked`, `refused`, `not_executed` — where `execute` requires the declared capability's bytes to hash-verify at the supplied path and every required environment key to be valued (a step the report calls `planned` without those is `blocked` with named reasons); a plan-level `ready`/`blocked`/`refused`/`unavailable` status plus an `unresolved` list naming unmet roots and capabilities; state fixtures over synthetic and committed cases pin every decision, including determinism |
| Capability threat model and conformance design | Design documented | `docs/architecture/CAPABILITY_THREAT_MODEL.md` reviews the implemented adapter/execution boundary — each defense named with the adversarial test pinning it — and names the residual risks (no sandbox, check-to-exec TOCTOU, env values outside invocation identity, no resource accounting) plus the conformance-vector definition the Stage-3 suite must satisfy; the suite itself and the capability SDK remain Stage 3 |
| Evidence-package spike | Partial | CASE-000 performs an integrity/compile/execute/generate/bind/evaluate/replay round trip with every declared source byte verified and one execution receipt; package-root semantics are now documented in EVIDENCE_MODEL (confined documents, digest-bound artifacts, content identity under relocation — test-pinned) and the independent Python verifier is exercised with named-outs open; redaction/retention *rules* are a policy decision and are named owner-gated |
| Problem-evidence packet | Ready | The discovery packet is present; interviews may inform product selection but are not an engineering or verdict gate |
| Bounded autonomous benchmark | Open | One fixed requirement set, candidate space, references, known shortcuts, and success/exhaustion condition |

## Stage 0 validation gates

| Gate | Current evidence | Target |
| --- | --- | --- |
| Technical verdict independent of review | Implemented | Kernel, campaign, claims, schemas, CASE-000, and regressions contain no review-gating path |
| Fixed requirements during search | Implemented for native JSON attempt lineages | CASE-008 and CASE-009 each record two attempts under one manifest and compiled snapshot; Core refuses a child if its parent record, manifest, snapshot, candidate state, or derived diff cannot be revalidated. CASE-009 additionally demonstrates exact negative margin comparison and PASS→FAIL transitions. Signed manifests and receipts landed (S-040) and the independent verifier checks them (S-042); `execution_policy.require_signatures` is set on the three loop cases (S-043). |
| Known shortcut refusal | Exercised | An adversarial designer arm (CASE-002 campaign 3, amendment A6) confirmed refusals for out-of-envelope candidates, a wrong interpreter digest, a particle count below its domain, and coverage on a weaker basis, and obtained undeserved verdicts only by rewriting the package; the identity log and the manifest pin (S-030) now make a rewritten package a refusal or a visibly different identity; `execution_policy.require_signatures` is set on CASE-001, CASE-002 and CASE-003, and the campaign adversarial checker verifies each log row's ADR-0015 signature against a trust root (S-043), so a log row edited after Core wrote it classifies `refused` | A predeclared shortcut proposed and refused inside a scored search arm (EXP-005, paused) |
| Autonomous search outcome | Reached twice | CASE-002 campaign 3: a language-model designer found an all-PASS design 277 kg lighter than the sweep's best at its second transport and stopped by judgment; the seeded surrogate found one at its eleventh. CASE-003 campaign 1, under the manifest pin: the same designer found an all-PASS spreader five times lighter than the sweep's bar at its sixth evaluation; see each case's `RESULTS.md` | EXP-002 ran on CASE-003 (2026-09-05, 15 trials, three arms): non-discriminating, every arm reached the optimum in one to three evaluations; Core's contribution must be isolated on a harder case (CASE-002). The predeclared shortcut was never proposed, so refusal during a successful search is still unexercised |
| Optional practicality routing | Implemented for the slice | Exact instructed dossier exercises both `request_changes` and `present_to_user`; omission leaves Core fully usable |
| Independent verification | Implemented for the first profile | `verifier/avila_core_verify.py` (S-041) reproduces package identity, receipt invocation identities, claims binding and every numeric and categorical verdict and margin for six cases with zero mismatches, and agrees with every semantic-core vector — including the registry-invariant checks that reject malformed registries in the defect corpus; ADR-0015 signatures are verified from scratch against a trust root (S-042); every qualified claim's persisted applicability context is re-evaluated against the bound record's scope and bound to the step's receipt (S-046), with CASE-002's pre-ADR-0018 claims named as mismatches until its actinv re-pin; requirement-set coverage is re-derived from the committed declaration (S-047) and `campaign_sha256` recomputed; staged-review records — the presentation gate's committed half — verified (both digest rules, presented-evidence re-binding, readiness, dispositions, eligibility-policy digest), with unresolvable campaign bindings named `not_checked`; the compiled snapshot itself is recomputed by the verifier's independent lowering port (`avila_core_lower.py`), proved against all 70 compiler fixtures' pinned snapshots and every committed case | Reviewer attestation is reported, never authenticated; the port reproduces whether a pair compiles and what it compiles to, not the finding vocabulary a rejected compile would report |
| Performance baseline | Measured once, debug build (2026-09-04) | Verify-only run of CASE-003: 10–40 ms. CASE-002 with every root supplied: 0.31 s per invocation, almost all of it re-hashing ~250 MB of static nuclear and ACTINV artifacts; a fresh screen adds ~30 ms; activation staging copy plus hash 1.1–1.5 s against a 12.4 s step; transport 4–6 min per candidate at 5e5 particles. Attempt lineage: 0.36 ms per log line plus 18 ms. See the [proposal review](reviews/2026-09-04-architecture-proposal-review.md) | A release-build baseline (EXP-003) with a frozen workload and machine, cold and warm separated, machine time separated from model time |
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

For the bounded 2026-09-15 product-workbench handoff, use the
[agent work queue](AGENT_WORK_QUEUE.md). It turns existing lineage and query
mechanisms into a usable history workflow while retaining the experiment pause
and infrastructure gates below. Its first task reconciles stale current-status
descriptions; this link does not mark a gate complete.

Order adopted 2026-09-04 (S-034) after checking the external architecture
proposal against the code; see the
[review](reviews/2026-09-04-architecture-proposal-review.md).

Completed since that ordering — recorded here, not remaining work:

1. EXP-002 ran on CASE-003 and is recorded as non-discriminating: every
   arm, with or without Core, reached the optimum within three evaluations.
   EXP-005 (the same three arms on CASE-002 with the shortcut made
   tempting) is drafted with its harness validated and no scored run;
   it is paused at the owner's request as of 2026-09-05.
2. Strict qualification is landed: mechanism (S-035), candidate validation
   at the design boundary (S-036), per-output-slot scope and the screen
   geometry records with `require_qualification` set on CASE-001, CASE-002
   and CASE-003 (S-039). CASE-003 now carries the same explicit
   outside-envelope pair CASE-001 has — a 63 mm candidate outside the
   screen record's thickness bound (its enclosure requirement still
   evaluates, since the finite-element record bounds no thickness) and a
   four-layer candidate inside the thickness bound that trips the shared
   layer-count term in both records; both run clean and produce the
   intended `not_evaluated.outside_qualification` results.
3. Runner integrity fixes are landed: process-group kill on timeout and one
   locked write per campaign-log line (S-037), the opt-in verified-hash
   cache (S-038), the extraction of rendering, log append, qualification
   loading, free-input handling and attempt comparison out of the case
   runner with a golden diff proving no behaviour change, and adversarial
   tests that SC-12 reuse never masks a registry, presentation or
   qualification edit. The engine core itself is now split as well: `runner`,
   `replay`, `gates`, `stderr` and `tests` submodules under the same golden
   diff (S-048). `run_step` remains a long function inside `runner.rs`;
   further intra-function decomposition is not queued.
4. Signed receipts and manifests (S-040) and the independent verifier,
   now including signature verification (S-041, S-042), are landed, and
   `execution_policy.require_signatures` is now set on CASE-001, CASE-002
   and CASE-003 with the workbench fields, CLI tests, and the campaign
   adversarial checker verifying log-line signatures (S-043). Each step's
   extracted applicability facts are now persisted on the claim's
   qualification assessment so an envelope verdict is re-derived from
   outside (S-046).
5. `core_constellation` (S-049) projects one campaign log in record order —
   lineage edges, candidate states, verdicts, and a derived summary — under
   the same lineage validation as `core_attempt`, so a tampered tail fails
   closed; pre-`schema_version` campaign records (all fifteen example
   campaign logs) read under an explicitly named legacy profile, with
   `[step_id, state]` pairs normalized and no lineage invented (S-050).

Open items, each with its gate:

- The same outside-envelope candidate pair for CASE-002, gated on its
  actinv re-pin; and a second look at the screen envelopes' numeric
  bounds, which are case-author judgment.
- CASE-002's claims await the same actinv re-pin before they can carry
  persisted applicability contexts (S-046).
- A harder coupled search (EXP-007) once an experiment is shown to
  discriminate — EXP-002 did not — and a fresh speed-tier attempt gated
  on agreement with the verified S_N solver (S-031, S-033).
- A release-build performance baseline (EXP-003): the local, non-agent
  portion is prepared and measured once in `experiments/exp-003/` (frozen
  CASE-003 workload, machine/build/executable manifest, verify and reuse
  phases at 22–67 ms wall, fresh execution recorded unavailable on the
  measured machine). The agent and unattended portions remain unmeasured.
- EXP-005 stays paused at the owner's request.

The proposal's prepared session, transactional store, obligation graph and
bounded scheduler are considered only after the scored EXP-002 result and a
release-build baseline show a campaign that needs them.

## Update rule

Update this ledger in the same change that materially changes a gate. If fixture
counts change, update the assertions in `avila-core-cli`, run
`avila-core semantic-profile`, and reconcile the README and fixture manifest.
Never replace an evidence link with an unsupported status word.
