> **Status:** external architecture proposal received 4 September 2026, pinned to commit `3be5902`. Kept as reference architecture. Reviewed in [2026-09-04-architecture-proposal-review.md](../reviews/2026-09-04-architecture-proposal-review.md); its sequencing was not adopted (decision S-034). The text below is verbatim.

# Avila Core: architecture and execution roadmap

**Proposal for Connor Avila / Avila Labs**  
**Date:** 4 September 2026  
**Status:** Proposed design, not an accepted repository decision or an implementation claim  
**Repository baseline:** `AvilaLabs/Avila-Core`, `main`, commit `3be5902374eb985ae5dfde8fa21a535a545a2e84`

## 1. Decision and intended outcome

Evolve the existing Rust implementation into a persistent, designer-agnostic engineering compiler. Its central operation should be: accept a proposed design change, determine what that change affects, check what can be checked immediately, obtain only the additional evidence needed, and return precise findings that support another iteration.

Preserve the existing kernel, compiler diagnostics, execution receipts, adapters, and attempt lineage. Build a typed design boundary, explicit obligation graph, persistent session, transactional evidence history, and bounded execution scheduler around them. This is an architectural evolution, not a rewrite.

The intended advantage is cumulative: less repeated interpretation, less repeated computation, fewer unnoticed errors, and a durable record that makes the next investigation easier. Token savings are one measurable consequence. The product must remain equally usable by a human, an AI agent, or a conventional optimizer.

### The founder's target

A designer receives an ambitious request—such as an Iron Man suit—and proposes a design. Every candidate goes through Core. Core identifies what is incompatible, missing, unsupported, or unsatisfied, and the designer revises. Large numbers of inexpensive proposals can be screened quickly; stronger evidence is obtained for promising candidates. The history remains available across designers and projects.

Core's conclusion is conditional: **this candidate satisfies these requirements under these assumptions, methods, evidence rules, and checked applicability conditions.** It does not establish every unstated requirement or every premise's correspondence to physical reality. A battery-placement hazard can become a machine-checkable obligation when an appropriate domain profile represents it; Core cannot guarantee that it spontaneously notices every missing hazard.

The near-term product should demonstrate this loop on an existing case, then on a second related investigation. The long-term vision does not require Avila to write a solver or procedure for every discipline.

### Recommended decisions

| Decision | Recommendation |
|---|---|
| Authoritative implementation | Continue stable Rust and the existing exact semantic representation. |
| Work unit | Immutable candidate evaluated against a versioned question and evidence profile. |
| Interactive state | Long-lived project session, with an equivalent clean-check path. |
| Core semantic structure | Explicit obligations and their dependency/evidence derivations. |
| Designer | External, interchangeable, and unable to alter campaign authority silently. |
| Execution | Separate effectful scheduler/runner; local first, bounded resources, isolated jobs. |
| Persistence | Immutable content-addressed artifacts plus transactional metadata and event history. |
| Incrementality | Dependency-explicit pure functions first; benchmark a Salsa-backed implementation behind an Avila interface. |
| Search | Optional controller outside the checking authority; batch inexpensive candidates and reserve expensive work. |
| First proof of value | Fast, unattended iteration and correct reuse on CASE-003, followed by a coupled case. |

## 2. Review basis and current implementation

I rechecked repository history, the recursive tree, latest CI, current architecture documents, experimental backlog, and selected source files. The latest commit remained `3be59023`; the corresponding GitHub Actions run reported success. This was a source/design review. I did not build the workspace or rerun the scientific campaigns during this review. No repository files were changed, committed, or pushed.

Sources below are pinned to the reviewed commit. Findings describe that revision, not an assertion about future main.

### What should be retained

| Existing implementation | Architectural value | Next action |
|---|---|---|
| Exact quantities, canonical values, units, numeric and categorical verdicts | Deterministic judgment foundation | Retain; extend through versioned rules and independent vectors. |
| Static contract compilation and typed diagnostics | Already provides a compiler-like authoring loop | Extend to design-local obligations and partial project analysis. |
| Receipt-bound execution and claim extraction | Connects calculations to concrete artifacts | Separate verification from coordination; strengthen execution identity and trust. |
| Selective rerun logic | Initial dependency-aware execution | Generalize into a tested invalidation service. |
| Native candidate lineage and margin comparison | Fixed questions and inspectable iteration | Preserve identities; replace hot-path full-history scans. |
| Package-declared external checker | Domain extension without adding a Rust branch for every checker | Extend evidence models and conformance requirements carefully. |
| CLI and native workbench | Human and programmatic entry points | Have both call the same project session API. |

The six-crate workspace is a useful starting point. The release profile already enables thin LTO, one codegen unit, and overflow checks; dependency optimization is already enabled in the development profile. Do not propose those as missing optimizations. Measure release binaries when evaluating performance. [R1]

### Concrete gaps and performance hypotheses

| Observation at the reviewed revision | Implication | Proposed response |
|---|---|---|
| `execute_case_inner` reads/verifies the package and calls `compile_documents` for a case execution. | Repeated campaigns can redo stable preparation. This is an observed call path, not a measured dominant cost. | Introduce a prepared session with content-identified immutable inputs. [R2] |
| `prepare_attempt`, append revalidation, and parent comparison use history reads; `read_attempts` reads the log and `validate_history` traverses records. | Repeated whole-history work can accumulate superlinearly over a growing campaign. | Validate imported history once, then use transactional indexed parent lookup with immutable record identities. Benchmark scaling. [R3] |
| JSONL append writes the serialized row and newline separately; validation occurs before append. | This is not a concurrent multi-writer transaction or an exactly-once submission protocol. | One transactional writer, unique attempt keys, atomic parent validation and append. Keep JSONL as interchange. [R2] |
| `execute_step` creates a fresh directory, copies and rehashes staged inputs, and starts a process. | Large stable inputs and process startup may dominate cheap checkers. | Immutable mounted artifacts and measured batch/worker interfaces, with equivalent isolation and identities. [R4] |
| `wait_with_timeout` polls with a 20 ms sleep and kills the direct child on timeout. | Short-job completion latency and descendant cleanup need attention. | OS-backed completion notification and process-tree cancellation under a bounded scheduler. [R4] |
| Qualification filtering examines records that are present; a missing qualification is not universally rejected by that filter. | The current prototype boundary is weaker than a universal qualification guarantee. | Introduce an explicit strict profile that requires the necessary qualification record and applicability derivation. Preserve legacy semantics visibly. [R5] |
| Static compilation accepts contract and registry documents; typed candidate geometry/components are not its general first-class source model. | Fine-grained design feedback is limited by opaque inputs. | Add a minimal typed design interface and source mapping. [R6] |
| The runner reports no sandbox, resource accounting, or signatures. | A cleared environment and fresh working directory do not isolate an arbitrary checker. | Enforce the threat model before exposing a hostile designer to trusted campaign state. [R4] |
| Roadmap/strategy documents contain older campaign status alongside newer completed results. | Handoffs can mistake historical statements for current state. | One current gate ledger with evidence links; mark older sections as historical. [R7] |

None of these observations establishes the dominant bottleneck without measurement. The first milestone supplies that measurement.

### Experimental evidence to preserve accurately

CASE-003 demonstrates a working Core-assisted thermal loop, not a causal advantage over the same designer receiving raw solver feedback. EXP-002 remains a draft ablation. CASE-002 records successful and failed search campaigns and adversarial package rewriting; the externally held manifest pin addressed those recorded rewrite attacks. CASE-008/009 demonstrate the importance of changing model fidelity: a promising reduced mechanism did not survive its dimensional follow-up. A passing reduced-model result is not evidence of a fully realizable invention. [R8–R10]

## 3. Product contract and guarantees

### Independent state axes

Preserve the four requirement verdicts: `PASS`, `FAIL`, `INCONCLUSIVE`, and `NOT_EVALUATED`. Add explicit surrounding state rather than redefining these words.

| Axis | Example values | Meaning |
|---|---|---|
| Preparation | incomplete, rejected, ready | Whether a coherent evaluation can be planned. |
| Execution | queued, running, completed, cancelled, errored | What happened operationally. |
| Requirement verdict | existing four-state vocabulary | What admitted evidence establishes. |
| Evidence profile | exploratory, specified bounded profile, named validated workflow | The declared strength and applicability basis. |
| Coverage | required obligations resolved, unresolved, explicit exclusions | Which parts of the selected profile are covered. |

Do not display a global “safe design” badge. A summary may say “all required checks pass under profile X,” with exclusions and assumptions accessible. A process error is not a technical FAIL; an inconclusive result is not proof of infeasibility.

### Guarantees to formalize

Within the declared semantic profile and trusted boundary, accepted conclusions must not contain:

1. Incompatible quantity kinds, units, roles, or coordinate/time conventions.
2. Evidence from a different candidate silently substituted for current evidence.
3. Stale evidence accepted after a relevant change without a justified reuse rule.
4. Screening or nominal evidence silently promoted to stronger assurance.
5. Method use outside the required checked applicability conditions.
6. Dropped assumptions, exclusions, or required obligations during composition.
7. Undeclared goal changes presented as an improvement under the old question.
8. Corrupted or unauthenticated evidence treated as stronger than its actual trust basis.

These are implementation/specification obligations, not guarantees that this review proves. Each needs a normative rule, positive and negative vectors, a derivation format, and a release test. Scientific model adequacy remains separately supported by method validation and applicability records.

### Explicit assumptions instead of a universal unsafe switch

Allow exploratory assumptions, with source, scope, owner, and identity. Propagate them to every dependent conclusion. A profile may permit a nominal or assumed result for a specific use, but a toggle must not convert it into a qualified bounded result. Replacing an assumption with evidence creates a new dependency state and triggers rechecking. Optional agent practicality reviews remain separate from technical verdicts and are never a mandatory professional-signoff gate.

## 4. Target architecture

### Separation of responsibilities

| Component | Owns | Must not own |
|---|---|---|
| Frontends/importers | Human editing, CAD/JSON adapters, source locations, proposed interpretations | Silent acceptance of inferred facts. |
| Project session | Immutable revisions, query orchestration, cancellation, result deltas | Independent scientific verdict logic. |
| Semantic compiler | Types, obligations, dependencies, admissible method plans, explanations | Effectful solver execution or creative design choices. |
| Checking kernel | Small deterministic rule applications for admissibility and verdict derivations | Provider discovery, process management, learned heuristics. |
| Runner/scheduler | Isolated execution, budgets, receipts, output collection | Deciding whether a scientifically weak claim counts as strong evidence. |
| Artifact/history store | Durable bytes, transactional events, indexes and references | Treating text summaries as authoritative evidence. |
| Designer/search controller | Candidate proposals, search strategy, optional ranking | Editing the acceptance boundary without an explicit revision. |
| Offline verifier | Independent checking of exported semantics/evidence within a named supported profile | Claiming independent scientific validation merely from replay. |

Use the same Rust session library from the CLI and egui workbench. Add a local service transport only when needed; do not make a daemon, HTTP stack, or cloud account a prerequisite for human use.

### Proposed data model

All names below are proposed interfaces, not current repository APIs.

| Record | Essential contents |
|---|---|
| `QuestionRevision` | Requirements, evidence profile, operating assumptions, allowed design space, declared exclusions, amendment parent. |
| `CampaignPolicy` | Permitted methods/packages, budgets, stopping policy, identity/trust policy; separate design and operator permissions. |
| `DesignRevision` | Stable entity IDs, typed parameters and connections, artifact references, source map, canonical identity. |
| `MethodSpec` | Input/output meaning, preconditions, applicability, evidence models, material dependencies, numerical/reproducibility policy. |
| `Obligation` | Stable ID, generating rule/profile, affected entities, predicate, required evidence strength, premise IDs. |
| `EvidenceRecord` | Claim, immutable producer/invocation identities, supporting artifacts, uncertainty model, assumptions, applicability derivation. |
| `Derivation` | Rule/version, premises, conclusion, applicability checks and identity bindings sufficient for verification. |
| `Attempt` | Question/candidate identities, designer attribution when available, parent/reference links, results and cost observations. |

Keep two candidate identities when necessary: authored bytes and canonical semantic state. Do not remove metadata from semantic identity until a versioned schema explicitly declares it nonmaterial. Equal-looking dimensions do not establish equivalent physics.

Split question identity from exact package and execution identities. This permits a new numerical method to address the same physical question while still producing visibly different evidence. Existing native lineage's full-manifest equality should remain valid in its legacy profile. In the new profile, changed methods create explicit evaluation revisions; they never pretend to be the same controlled comparison. Cross-revision comparisons report differences and limits.

### Obligation generation and partial analysis

Start with rules over finite typed records. Generate obligations from selected requirements, domain profiles, component interfaces, and method preconditions. Do not introduce arbitrary general-purpose theorem proving as the first implementation.

Example: selecting a conduction method generates obligations for conductivity data, thermal boundary conditions, interface treatment, and applicability over the declared temperature range. Structural/fatigue coverage does not appear automatically unless the selected profile supplies it.

Maintain a partial graph even when a source is incomplete. Nodes with missing premises remain unresolved and carry a source-local finding; independent nodes continue to be checked. No unresolved placeholder enters an executable plan that requires it. Existing strict `compile` behavior can remain; add a partial `analyze` operation rather than silently changing compilation's contract.

Ordinary dataflow is acyclic. Coupled feedback, such as temperature affecting deformation and contact, must be represented as an explicit coupled-method node with convergence and error obligations. Do not conceal a physical fixed point by repeatedly traversing a graph cycle. Where no supported coupled method exists, report the unresolved coupling.

Rule expansion must be bounded: finite domains initially, duplicate obligation elimination, expansion limits, deterministic ordering, and clear diagnostics for recursive/unsupported rules. New expressive features require termination and resource limits.

### Dependencies and lifetimes

Evidence depends on meaning as well as bytes. Track candidate fields/artifacts, methods, datasets, numerical configuration, environment factors, assumptions, applicability/qualification records, and semantic profile. A verdict also depends on requirements and evidence policy.

This yields different invalidation actions:

| Change | Default action |
|---|---|
| Requirement threshold only | Retain applicable numerical evidence; re-evaluate verdict and any threshold-dependent planning rules. |
| Candidate property read by method | Invalidate its result and dependent conclusions. |
| Material dataset or solver implementation | Re-evaluate dependent invocation identity and qualification; normally rerun. |
| Qualification withdrawn or applicability narrowed | Retain historical bytes; withdraw current admissibility and recompute dependent verdicts. |
| Presentation policy | Update presentation only; technical verdict unchanged. |
| Display label | No semantic effect only when schema explicitly classifies it as display-only. |
| Unknown dependency effect | Conservative invalidation with an explanation. |

Do not expose a full opaque artifact to a checker and then cache its result using only a claimed subset of that artifact's fields. Initially hash the whole accessible input. Finer dependencies require a trusted extractor that passes only the declared projection, or a validated method contract with enforceable boundaries.

### Conditional conclusions and checked derivations

Gradually extract admission decisions currently spread between compiler and runner into explicit rules. The runner supplies observations; a deterministic checker validates identity, prerequisites, applicability, and evidence-model sufficiency before a verdict is derived.

Do not move filesystem access into the pure kernel to achieve this. A verified-input handle or equivalent typed record crosses the boundary after byte verification. Its constructor is private to the verification path. Exported derivations include enough evidence for another implementation to reconstruct these checks. Audit the entire trusted computing base; calling one crate “kernel” does not make the rest irrelevant.

## 5. Human and machine interface

### Proposed session operations

```text
open_project(project_root, supported_profile)
analyze(design_revision, question_revision)
propose(parent_revision, typed_patch, expected_revision)
plan(candidate_id, requested_obligations, budget)
evaluate(plan_id, idempotency_key)
submit_batch(candidate_ids, campaign_id, budget)
explain(finding_or_obligation_id)
compare(attempt_a, attempt_b)
history(query, cursor)
export(attempt_id, evidence_profile)
```

These operations return immutable revision IDs. Concurrent edits use optimistic revision checks. A result finishing after the user edits the design attaches to its original candidate, never to the current view by accident. Duplicate submissions with the same idempotency key return the existing attempt. A timed-out client must not create a second expensive job merely by retrying.

A batch has per-candidate outcomes: one malformed design must not erase the valid results of its siblings. Budget reservation and publication are atomic. Events include queued/running/completed/refused/cancelled states and stable causal IDs.

### Diagnostic contract

Extend the existing source locations, codes, owners, and typed patches. Proposed additions: affected entity IDs, obligation IDs, premise/evidence references, blocked descendants, repair category, and result delta cursor.

Keep three authority categories: checked explanation, admissible next action, and heuristic suggestion. For example, an overlapping temperature interval is a checked explanation; obtaining tighter evidence is a possible next action; changing fin geometry is usually a suggestion until evaluated.

An automatic repair may correct a unique canonical representation. It must not silently choose a physical unit, material, tolerance, uncertainty basis, or threshold simply because that edit removes an error. The current diagnostic harness's success in repairing synthetic fixtures should not be interpreted as authorization to apply every listed alternative to real engineering inputs.

Return independent root causes, suppress derivative noise, and let users expand details. The first human view should answer: what changed, what is established, what is unresolved, and what can I do next? Avoid recreating the overloaded dashboard from the first visualization.

### Token-spend strategy

Do not require an LLM call per candidate. Let a designer submit batches, use ordinary numerical optimizers between reasoning calls, and receive compact result deltas. Return only changed verdicts/findings and the requested comparisons by default. Keep complete raw evidence addressable by stable IDs.

A bounded machine response should contain candidate/question IDs, changed findings, exact margins or their requested projection, evidence-profile state, and references for expansion. Deterministic summaries are derived views, not new evidence. Prompt/model/settings provenance is stored when available; recording it does not guarantee identical future model behavior.

Measure actual tokens and model calls for matched tasks. Fewer tokens is not success if it produces more false conclusions, worse designs, or more human intervention.

## 6. Performance architecture

### Measure before selecting optimizations

Instrument monotonic elapsed spans for import, parse/canonicalization, compilation, applicability, planning, queueing, staging, process startup, solver execution, output collection, hashing, admission, verdicts, history commits, and response rendering. Track bytes read/copied/hashed, cache hits and misses by reason, job counts, solver threads, peak memory, and model calls/tokens where applicable.

Report cold startup separately from warm iteration; machine time separately from human/model time; total campaign time separately from sum of concurrent task times. Failed jobs, cache misses, cancellations, and retries remain in the accounting.

### Initial engineering targets—not measured capabilities

Freeze an exact benchmark machine and data manifest in M0. The initial envelope should be a release build on an ordinary 8-core, 16 GB RAM machine with local SSD, no GPU and no network dependency after import. Record the actual hardware rather than substituting a nominal specification.

| Benchmark | Proposed target after M4 | Conditions |
|---|---|---|
| Warm localized analysis | p95 ≤ 50 ms | ≤100 design entities, ≤500 obligations; no solver or large artifact import. |
| Requirement-only verdict update | p95 ≤ 50 ms | Admitted evidence already resident; no new applicability computation requiring execution. |
| Interactive update including durable local commit | p95 ≤ 100 ms | Small candidate and bounded result delta; target includes persistence cost. |
| Small warmed batch | 10,000 candidates within 180 s | Cheap static/analytic work only, ≤10 ms single-core method CPU per candidate, documented parallelism, no hidden solver/model calls. |
| Prepared CASE-003 campaign | Zero manual intervention after launch | Wall-time target set from its measured FE cost; do not invent an absolute physics runtime. |
| Long history | p95 parent lookup/append ≤ 25 ms | 100,000 indexed attempts, small metadata records; blob reads/import audit measured separately. |

These are decision thresholds for experiments, not launch promises. M0 may show that a target or benchmark definition needs revision; record that revision before reporting a scored result. Never redefine a workload after seeing results to claim success.

### Throughput arithmetic

For independent work, a useful optimistic capacity estimate is `N × mean CPU time / effective parallelism`, plus serial work, storage, scheduling, and resource contention. It is not a wall-time guarantee. Ten thousand 10 ms evaluations represent 100 CPU-seconds before overhead; ten thousand 20-minute simulations represent about 139 serial days. Core cannot erase the underlying physics cost.

The practical high-throughput loop spends cheap work on many candidates and detailed work on a much smaller set. Publish separate counts for screened, evaluated, admitted, inconclusive, and passing candidates.

### Optimization sequence

**A. Prepare once.** Parse and validate stable contracts, registries, method descriptors, and profiles at session creation. Intern repeated semantic identifiers and retain immutable compiled graphs. Keep the clean compilation path for equivalence checks. Do not trust mutable external files merely because they were loaded once.

**B. Import immutable bytes once.** Ingest artifacts into a content-addressed store using atomic publication. Hash while importing, then mount verified immutable objects read-only into jobs. A read-only mount must refer to runner-controlled immutable storage, not a host file that the designer can still alter. Reflinks/private copies may reduce staging cost; writable hardlinks to shared artifacts are forbidden. Full rehashing remains available for imported/untrusted storage and offline verification.

**C. Use separate caches.** A compile/query cache tracks semantic inputs; an execution cache tracks exact method invocation; an admission/verdict cache tracks evidence plus qualification/policy/requirements. A changed threshold need not invalidate a solver result. A changed qualification can invalidate admissibility without destroying historical bytes. Cache keys and verification evidence are stored, not only booleans.

**D. Replace history scans.** Use a local transactional database, initially SQLite, for IDs, immutable record digests, parent relationships, revisions, indexes, and scheduling state. Store large objects separately. A single writer performs atomic uniqueness checks and appends; readers use consistent snapshots. Rebuild derived indexes from verified records. Verify imported JSONL once; subsequent appends validate the new record and referenced parents. Do not replace full verification with unaudited mutable cached answers.

**E. Schedule with resource budgets.** Parallelize independent candidates and ready DAG nodes. Reserve CPU, memory, GPU slots if present, and license tokens when needed. A solver using eight threads must not be scheduled as though it uses one core. Expose explicit pool sizes rather than relying on nested defaults. Rayon is a candidate for bounded pure CPU work, not a substitute for supervising external processes. [E3]

**F. Remove process overhead only where measured.** First support a batch checker with one invocation and a per-item result manifest. If startup dominates after batching, prototype persistent workers with request IDs, fresh per-job state, bounded lifetime, explicit reset behavior, and receipts that identify all material resident state. A stateful worker that cannot demonstrate independence is non-cacheable or includes state in its identity. Untrusted extensions remain isolated; they do not become native plugins in the authoritative process for speed.

**G. Replace short-job polling.** Use event-driven process completion and deadline supervision, including child process groups/appropriate platform job containment. Cancellation must collect partial logs, mark the attempt cancelled, release resources, and avoid publishing partial evidence. This matters for both latency and correctness.

**H. Add query incrementality when it earns its cost.** Shape analysis as pure functions over explicit inputs. Prototype Salsa against a simple memoized implementation on localized edits, broad changes, cold startup, and memory use. Salsa tracks dependencies and memoizes query results; it does not decide scientific applicability or provide a durable evidence format. Keep all public IDs and interchange formats owned by Avila. Adopt it only if the benchmark benefit outweighs complexity. [E1]

**I. Optimize numerical work at the method boundary.** Batch/vectorize calculations, reuse fixed geometry or matrix factorizations only under explicit dependency rules, and use supported surrogates for screening. Preserve exact arithmetic for authoritative comparisons and identity semantics. Floating-point solvers remain acceptable when their output/evidence model describes numerical uncertainty correctly; exact comparison of a decimal does not make the physical prediction exact.

**J. Avoid recomputing exports and large reports.** Emit compact events during exploration. Materialize full evidence packages on request or at durable milestones. Refer to shared immutable artifacts instead of copying them into every attempt. Bound memory caches and provide garbage collection rooted in retained questions, attempts, and exports; never evict the only copy of retained evidence.

### Numerical and search-specific protections

Repeated adaptive search can favor candidates whose favorable results are numerical noise or surrogate error. Freeze numerical policies and seeds where meaningful, distinguish replay from independent replication, and reserve a final confirmation procedure that is not silently relaxed by the designer. A stochastic uncertainty interval is not automatically a hard physical enclosure. Combining intervals requires an explicit supported rule, including dependence assumptions where relevant.

Screening predictions can rank candidates without establishing requirements. Pruning as impossible requires a sound bound for the applicable domain. Heuristic pruning is allowed as a search choice, but an exhausted heuristic search cannot certify global infeasibility. Track numerical convergence separately from physical validation. Finite sweep coverage does not by itself prove a continuous worst-case bound.

Bazel's caching documentation is a useful engineering precedent: reusable outputs require controlled, reproducible inputs. Its hermeticity guidance reinforces why unspecified environment dependencies make cache reuse unreliable; Core must additionally track scientific applicability. [E2]

## 7. Safety of the evaluation boundary and persistence

### Practical initial threat model

The designer may be mistaken or actively trying to obtain a favorable verdict. It can edit its candidate workspace and call allowed submission APIs. It cannot write campaign authority, trusted artifacts, evaluator binaries, or signing credentials. The local operator and underlying OS remain trusted in the first deployment; do not claim resistance to a compromised administrator.

Local isolation should enforce read-only method/input mounts, per-job writable output directories, controlled environment, explicit network policy, process-tree limits, and bounded resources. If a platform cannot enforce the selected mode, reject that mode or label a separate development mode honestly. A directory change alone is insufficient.

Signatures are useful for attribution and cross-boundary integrity, but they do not make false receipts true or qualify a method. Keep the requester's pin/trust configuration outside designer control. Add signed checkpoints/exports with a pinned trust root before sharing evidence across trust boundaries; individual signatures may be batched through a specified verified checkpoint structure if measured overhead justifies it. Never give the designer the signing key to solve an integration problem.

### Durable event and artifact protocol

1. Reserve attempt and budget transactionally using the submission key.
2. Execute under the frozen candidate/plan identity.
3. Write outputs to temporary storage and compute identities.
4. Verify completeness and publish immutable blobs atomically.
5. Commit receipt, result, parent link, and terminal attempt state together.
6. Acknowledge completion only after the configured durability boundary.

On restart, recover reserved/running attempts into explicit interrupted or resumable states. Reconcile orphaned blobs conservatively. Never turn a partial result into PASS because the process disappeared. Imports, exports, and index rebuilds must be interruptible and resumable without inventing complete records.

## 8. Engineering constellations and domain extensibility

The constellation is a graph of immutable questions, candidates, attempts, methods, evidence, and explicit relationships. It is not initially a vector database or a conversational memory system.

Begin with indexed queries for equivalent candidates under a declared canonicalization rule, valid evidence reuse, nearest recorded requirement margins within comparable units/profiles, parent-child differences, and unresolved obligations. Learned summaries and similarity search can be optional derived views later. Never promote resemblance or an LLM summary into evidence admission.

A domain package should declare typed entities/interfaces, finite obligation-generating rules, supported methods, evidence models, applicability, explicit exclusions, and conformance cases. Separate method implementation from validation records so that users can distinguish what a package computes from what its evidence supports. Provide reference valid/invalid inputs, boundary cases, environment dependencies, and a migration policy.

Start with a finite rule vocabulary and explicit extension points. New disciplines should usually add packages and adapters; new kernel concepts require a demonstrated semantic gap, a versioned rule, and independent fixtures. “Same units” alone does not make methods substitutable: quantity role, spatial frame, time basis, uncertainty meaning, and applicability must align.

The generality test is a second related case added with package changes while keeping the kernel stable. Requiring a small generic extension is not automatic failure, but repeatedly encoding per-case branches into the kernel signals the abstraction is wrong.

## 9. Roadmap, dependencies, and acceptance gates

Effort ranges below are planning estimates in focused engineering weeks, including implementation and validation, not calendar promises. They assume existing cases and working solver access. Novel solver research, physical validation, and broad CAD integrations are additional. Re-estimate after M0; one founder should expect several months, not an overnight rewrite. The sequential ranges total roughly 15–27 focused weeks before contingency.

| Milestone | Estimate | Dependencies | Exit outcome |
|---|---:|---|---|
| M0 — Baseline and semantic decisions | 1–2 weeks | Current repo | Measured bottlenecks, frozen fixtures, explicit new-profile decisions. |
| M1 — Prepared session and durable attempts | 2–4 weeks | M0 | Repeated prepared execution, atomic history, replay compatibility. |
| M2 — Typed design and obligations | 3–5 weeks | M1 | Partial analysis and precise invalidation for one existing case. |
| M3 — Isolated batch execution | 2–4 weeks | M1; M2 for obligation planning | Bounded unattended batches and hostile-designer tests. |
| M4 — Incremental planning and fast feedback | 2–4 weeks | M2, M3 | Warm latency/batch targets assessed with clean equivalence. |
| M5 — Portable checking and simple workbench | 2–3 weeks | M2–M4 | Independent scoped verification; human and agent use the same loop. |
| M6 — Measured advantage and second-case reuse | 3–5 weeks | M4, M5 | Repeated comparative evidence and a second investigation. |

### M0: Baseline, profiling, and decisions

- Add `experiments/EXP-003-throughput-and-unattended-execution.md` using the existing experiment template; freeze workload, hardware, versions, and logging.
- Baseline CASE-003 and a cheap deterministic synthetic checker. Run release binaries; separate build time from execution.
- Measure history sizes of 100, 1,000, 10,000, and 100,000 attempts without executing physics merely to create benchmark history.
- Record spans and resource metrics listed in section 6. Produce cold/warm and candidate-change/requirement-change measurements.
- Write proposed ADRs for typed design/obligation semantics, session identities, strict evidence profile, transactional history, and execution isolation. Assign numbers from the repository's actual next available ADR IDs when implemented.
- Reconcile the current status ledger and retain historical experimental limitations.

**Gate:** a reproducible benchmark report identifies measured costs; golden current-case results are frozen; each stronger guarantee has a specified rule and negative case. No new domain needed.

### M1: Prepared session, storage, and API foundation

- Extract preparation, execution coordination, verdict projection, and report rendering from `case_run.rs` into cohesive modules without changing legacy verdicts.
- Introduce `PreparedProject`/equivalent and immutable imported inputs; wrap the current one-shot CLI around the same library path.
- Add transactional attempt IDs, parent validation, idempotent submission, compact event records, and indexed history.
- Retain byte-compatible legacy records as immutable imports; export compatibility explicitly rather than rewriting historical hashes.
- Add cancellation/recovery states and ensure every operation names its source revision.

**Gate:** existing conformance fixtures and golden case verdicts remain unchanged under the legacy profile; 10,000 cheap sequential attempts do not require a full-history scan per append; duplicate submissions and crash recovery produce no ambiguous successful attempts.

### M2: Typed design, partial compiler, and qualification closure

- Define the smallest design schema needed for CASE-003: material selection, dimensions, interfaces where represented, operating inputs, and stable IDs linked to artifacts.
- Represent current requirements, coverage checks, and method applicability as explicit obligations without adding unstated physics to the existing case.
- Add partial `analyze`, source-local diagnostics, explicit missing information, and deterministic obligation identities.
- Implement dependency-derived invalidation with conservative whole-artifact defaults.
- Add the strict profile: required qualification presence, checked applicability, explicit assumptions/exclusions, and supported evidence basis. Keep unqualified specimens usable in their named exploratory profile.
- Expose derivations for admission and verdict rules; begin extracting pure checking from coordination code.

**Gate:** missing information blocks only dependent conclusions; out-of-profile evidence is rejected; a missing qualification cannot obtain a strict-profile PASS; an opaque artifact edit cannot evade invalidation through an incomplete dependency declaration.

### M3: Batch scheduler and controlled execution

- Add batch submissions and independent ready-node scheduling with explicit resource reservations.
- Implement isolated inputs/outputs, process-tree timeout/cancellation, budget enforcement, and read-only campaign authority.
- Extend the external checker descriptor with supported interval/bound evidence extraction where needed; preserve exact versus statistical/enclosure distinctions.
- Add immutable artifact reuse and a simple execution cache with complete keys and verified output provenance.
- Prototype batching before persistent workers. Keep the existing process-per-job adapter as a reference path.

**Gate:** a prepared campaign runs without intervention; one malformed candidate does not spoil a batch; CPU/thread oversubscription is controlled; cancellation releases resources; the hostile designer cannot rewrite its question, outputs, or trusted history into accepted success.

### M4: Incremental queries, planning, and feedback

- Add change-driven query caches and a bounded method planner over unresolved obligations.
- Use deterministic admissibility filtering before optional cost/ranking heuristics. Freeze plans by identity before execution.
- Benchmark Salsa versus explicit memoization. Adopt only if justified; retain a no-cache clean execution option.
- Return stable diagnostic deltas, valid next actions, and typed candidate comparisons without reprinting full evidence.
- Add batch deduplication only under declared semantic equivalence; preserve all submitted attempt references and actual budget use.
- Tune staging, process startup, report size, and numerical batching based on the measured critical path.

**Gate:** targets in section 6 are met or revised transparently with evidence; incremental and clean results agree for the tested deterministic mutation corpus; ranker changes do not alter admissibility semantics; cold evidence verification remains available.

### M5: Independent verification and minimal user experience

- Define a minimal portable package for the first supported profile, including all required input/evidence identities and derivation rules.
- Build a small independent verifier, preferably using a separate implementation approach and without importing production verdict/admission code. Recompilation/evidence checking is in scope; rerunning a heavy solver is a separate operation.
- Cover exact quantities, interval comparisons, identity binding, qualification presence/applicability, and the profile's supported obligation rules. Unsupported rules are explicitly refused.
- Add signed export/checkpoint verification with explicit trusted keys where evidence crosses a trust boundary.
- Simplify the workbench to the current design, findings, next action, and expandable history. No mandatory chat panel or agent review.

**Gate:** the verifier agrees on positive/negative vectors and a real case export, detects mutations, and names unsupported rules; a human completes an iteration with no AI integration; an agent uses the same API with no hidden privileged path.

### M6: Establish value and transfer

- Run EXP-002 matched feedback arms, EXP-004 repeatability, EXP-005 refusal during successful search, and EXP-006 optimizer comparison, reusing existing experiment IDs and scope. [R8]
- Add a second related investigation using the same packages, then a coupled case that stresses assumptions and method composition.
- Measure setup labor, iteration latency, total cost, model calls/tokens, valid candidates, defect escapes, and evidence-reconstruction time.
- Separate physical/model validation from workflow performance. Advance a promising research candidate to a higher-fidelity or physical check as a separately recorded scientific gate.

**Gate:** a repeated advantage is supported by matched evidence, or a specific lack of advantage is recorded. A useful provenance benefit counts as that benefit; it must not be relabeled as optimization superiority.

## 10. First implementation backlog

These are proposed reviewable changes; they are not created issues or commits.

| Item | Primary code area | Completion evidence |
|---|---|---|
| P01: release benchmark harness and spans | runner, compiler, experiments | Cold/warm baseline with machine and input identities. |
| P02: separate report rendering from execution | `case_run.rs`, render modules | Golden semantic outputs unchanged. |
| P03: immutable prepared project | compiler/session facade | Repeated case execution skips stable compilation safely. |
| P04: transactional attempts | `attempt.rs`, new store module | Concurrent duplicate/parent/torn-write tests. |
| P05: immutable artifact import | evidence/store | Byte mutation and partial-import rejection. |
| P06: revision-aware API | CLI/app/session | Stale response never replaces current candidate. |
| P07: typed CASE-003 source map | compiler/design | Design-local missing-input findings. |
| P08: obligation IR and partial analysis | compiler/kernel | Independent errors survive incomplete nodes. |
| P09: strict qualification profile | campaign admission/verdicts | Missing/outside/unknown records rejected as specified. |
| P10: isolation and job cancellation | runner/execute | Descendant cleanup and readonly-boundary tests. |
| P11: bounded batch scheduling | runner/scheduler | Per-item receipts, budget reservation, deterministic association. |
| P12: incremental mutation harness | compiler/session/store | Clean/incremental agreement and measured latency. |
| P13: compact diagnostics/history queries | CLI/app | Human and machine consume the same results. |
| P14: portable package and independent checker | evidence/verifier | Mutated export rejection and rule-level agreement. |

Keep new modules inside existing crates initially where dependencies remain clear. Introduce an `avila-core-session` or `avila-core-store` crate only when it enforces a useful dependency boundary. Avoid turning every proposed record into a separate crate.

### First two weeks

Week 1: freeze the baseline, add timing/byte counters, run CASE-003 and synthetic/history benchmarks, document profile/identity decisions, and identify the measured top two costs.

Week 2: extract preparation from execution, add the prepared-session prototype, and implement transactional attempt lookup/append behind the old interface. Repeat the same benchmarks and golden checks. If identity behavior changes, stop and resolve compatibility before layering on concurrency.

The desired first deliverable is a prepared campaign that reruns predictably and a report explaining where the remaining time goes—not another demonstration domain.

## 11. Verification and experiment strategy

### Required correctness gates

- **Normative vectors:** retain and extend exact number, unit, evidence model, categorical, applicability, and verdict tests.
- **Mutation equivalence:** vary candidate fields, requirements, datasets, method versions, policy, qualification, and display-only fields; compare canonical semantic outputs with a clean run.
- **Concurrency:** duplicate attempt IDs, simultaneous siblings, changed parent records, retries after client timeout, stale revision responses, and out-of-order completion.
- **Persistence:** kill the process during each publication phase; restart; verify no acknowledged complete result is missing its retained evidence under the documented durability guarantee.
- **Adversarial execution:** forged receipts, rewritten manifests, wrong executables, escaped output paths, cross-candidate result swaps, writable shared artifacts, and undeclared ambient dependencies.
- **Numerical boundary cases:** just inside/outside limits, overlapping intervals, unsupported aggregations, and qualification transitions.
- **Independent checking:** deliberately corrupt claims, premises, signatures/checkpoints, and derivations; require the independent implementation to reject or explain disagreement.

For deterministic fixtures, compare canonical outputs excluding operational observations such as timestamps and durations. For stochastic calculations, exact artifact replay is one test; independently repeated numerical agreement uses a predeclared method-specific policy. Do not erase numerical differences to force equivalence.

### Comparative evaluation design

Give each arm the same question, design space, methods, priors, machine/resource budget, and stopping policy. Separate Core feedback, raw solver feedback, and no iterative feedback. Compare conventional optimizers separately from the designer-feedback ablation. Randomize run order where feasible, use fresh agent contexts, record failures, and freeze trial counts and analysis before scored execution.

Primary outcomes: valid result quality at fixed cost, evaluations to first qualifying result, human intervention, evidence completeness, and end-to-end time. Secondary outcomes: model tokens, peak memory, cache reuse, and diagnostic repair rounds. Setup labor and second-project reuse deserve explicit measurement because they determine whether formalization pays for itself.

A halted search with no passing candidate is budget exhaustion unless a separate supported argument establishes infeasibility. A passing model candidate is not evidence of novelty, manufacturability, or a granted patent. Core can support the research record behind an invention without claiming to establish those outcomes.

## 12. Migration and release discipline

1. Freeze existing draft-profile behavior and recorded package identities.
2. Add the session facade around existing one-shot behavior.
3. Add new semantic records under a distinct draft profile with explicit feature support.
4. Import old packages as legacy-profile projects; preserve their verdict meaning and limitations.
5. Offer explicit migration reports that identify stronger requirements, missing qualification, new obligations, and changed semantics.
6. Shadow-check migrated cases; investigate every difference before making the new profile default.
7. Introduce stricter guarantees only with vectors, documented trust assumptions, and independent supported-profile checks.

Do not use a software version number to imply scientific maturity. A release can support fast exploratory loops while its example methods remain unqualified. A method's validation status is a separately versioned record.

Keep `.acore` syntax exploratory until semantics and authoring needs stabilize. JSON remains a canonical interchange path; the workbench can make it usable without prematurely designing a large language. Do not add GPU dependencies to the kernel, distributed execution, vector memory, a public marketplace, or mandatory agent orchestration to complete these milestones.

## 13. Risks and decisions that remain open

| Risk | Early indicator | Response |
|---|---|---|
| Formalization costs exceed reuse benefit | Second related project still needs extensive custom integration | Measure setup; simplify package interfaces and narrow the supported contract class. |
| Generality is superficial | Every new case adds kernel branches | Revisit entity/obligation boundaries; require package-level reuse evidence. |
| Compiler overhead is not the main cost | Most wall time lies in physics/model or human setup | Optimize the measured bottleneck; do not expect Salsa to accelerate a solver. |
| Search exploits weak models | Large screening gains vanish on confirmation | Stronger confirmation/applicability rules and explicit screening status. |
| Cache unsoundness | Ambient files, mutable artifacts, hidden worker state | Conservative keys and isolation; disable reuse for unsupported methods. |
| Too much feedback consumes attention/tokens | Designers repeatedly ignore or misunderstand findings | Root-cause grouping, stable deltas, on-demand detail; measure repair effort. |
| Expanding trust surface | Plugins can manufacture admitted records | Narrow checked constructors, explicit derivations, independent verification. |
| Infrastructure absorbs research time | Many months of platform work without reusable iteration | Gate each phase on the existing case and a short measurable outcome. |

Open decisions for implementation: initial supported OS/isolation backend; exact strict-profile qualification semantics; canonical package identity and signature format; SQLite durability configuration and recovery contract; source mapping for the second artifact format; method-level stochastic confirmation policy; and the benchmark-based Salsa adoption decision. Each has a milestone owner/responsibility, but no person is assigned by this proposal.

## 14. What success should look like

At the first useful release, Connor can open a prepared project, submit many candidates, understand specific failures, and refine the design without reconstructing the workflow each time. Core preserves valid evidence, makes changed assumptions visible, and keeps a useful record across sessions and designers.

At the next release, a second investigation reuses a meaningful portion of the first one's methods and evidence structure. A human can work without an AI, an agent can work without privileged authority, and a recipient can independently inspect the supported evidence package.

The decisive success criterion is an observed increase in defensible research throughput at fixed resources. The system earns that claim through reproducible comparison, not the number of agents, schemas, tests, or example domains it contains.

## Sources

Repository references are pinned to the reviewed revision. External references support specific implementation mechanisms, not claims of Core's novelty or performance.

- **R1:** [Workspace and build profiles](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/Cargo.toml); [successful CI run](https://github.com/AvilaLabs/Avila-Core/actions/runs/33909546902).
- **R2:** [Case orchestration, compilation, reporting, and JSONL append](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/crates/avila-core-runner/src/case_run.rs).
- **R3:** [Attempt lineage and history validation](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/crates/avila-core-runner/src/attempt.rs).
- **R4:** [Execution, staging, timeout, and stated isolation limitations](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/crates/avila-core-runner/src/execute/mod.rs); [receipt invocation identities](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/crates/avila-core-evidence/src/receipt.rs).
- **R5:** [Qualification filtering and verdict evaluation](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/crates/avila-core-compiler/src/campaign/verdicts.rs).
- **R6:** [Current semantic compiler](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/docs/architecture/SEMANTIC_COMPILER.md).
- **R7:** [Stage 0 ledger](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/docs/roadmap/STAGE_0_STATUS.md); [generative-loop intent and updates](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/docs/strategy/GENERATIVE_LOOP.md).
- **R8:** [Experiment backlog](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/experiments/BACKLOG.md); [thermal pilot interpretation](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/experiments/EXP-001-core-assisted-thermal-search.md); [feedback ablation](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/experiments/EXP-002-core-feedback-ablation.md).
- **R9:** [Coupled shielding campaigns and adversarial findings](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/examples/cases/case-002-coupled-shield/RESULTS.md).
- **R10:** [Reduced mechanism](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/examples/cases/case-008-mode-selective-quench/README.md); [dimensional follow-up](https://github.com/AvilaLabs/Avila-Core/blob/3be5902374eb985ae5dfde8fa21a535a545a2e84/examples/cases/case-009-ncsx-copper-discharge/README.md).
- **E1:** [Salsa overview](https://salsa-rs.github.io/salsa/overview.html) and [red-green algorithm](https://salsa-rs.github.io/salsa/reference/algorithm.html): dependency tracking and memoized recomputation. Exact API/version selection is deferred to the implementation experiment.
- **E2:** [Bazel remote caching](https://bazel.build/remote/caching) and [hermeticity](https://bazel.build/basics/hermeticity): reusable outputs and controlled execution inputs.
- **E3:** [Rayon ThreadPoolBuilder](https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html): explicit thread-pool sizing; benchmark and pin the chosen version when adopted.
