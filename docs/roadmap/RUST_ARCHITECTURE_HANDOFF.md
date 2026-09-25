# SWE-2 handoff: enforce the Rust-informed semantic architecture

Prepared 24 September 2026 at repository baseline
`0ee55d08be7000b93dbf8da56d6e7f5b3830f22f`.
Status: RA-01 through RA-06 implemented; per-item status and evidence below.
Recheck source and worktree state before implementation.

Read the [research and architecture proposal](proposals/2026-09-24-rust-informed-semantic-architecture.md)
for the rationale and Rust sources. This handoff defines the first complete
implementation increment and the conditions for expanding it. Completion of
this increment establishes only its stated software properties; it does not
complete the entire proposed architecture or qualify a scientific method.

**Implement one complete chain of enforced guarantees.** A bounded requirement
must reach its verdict through checked semantic values, context-bound
observations and admission, and a replayable derivation. A changed input,
qualification, policy, or requirement must expose the affected uses. A frontend
must be unable to manufacture a checked admission or transfer it to another
context. Private fields alone do not satisfy this task.

Keep the implementation in the existing compiler/kernel/evidence/runner
boundaries, with clients consuming shared operations. Start from existing
rules and conformance fixtures. General rule solving, persistent sessions,
databases, schedulers, a new syntax, and stronger process isolation are follow-on
increments below. The first increment makes the current rule applications
explicit; it does not introduce a general obligation engine.

The owner's request authorizes preparing and implementing this bounded
direction. Continue through the ready dependencies without routine approval
requests. Write any ADR required by [CONTRIBUTING.md](../../CONTRIBUTING.md)
as part of the implementation. Do not reinterpret an ADR requirement as an
automatic permission stop. Preserve the existing semantic rules unless a
change is explicitly specified and versioned. An unresolved scientific choice
remains the designer's responsibility.

**Read these sources before editing.**

- [AGENTS.md](../../AGENTS.md) and [CONTRIBUTING.md](../../CONTRIBUTING.md).
- The proposal linked above, [ADR-0006](../adr/0006-semantic-core.md), and the
  relevant receipt, qualification, and context ADRs:
  [0007](../adr/0007-execution-receipts.md),
  [0008](../adr/0008-qualification-envelopes.md), and
  [0018](../adr/0018-persisted-applicability-contexts.md).
- [Compiler boundary](../architecture/SEMANTIC_COMPILER.md),
  [campaign evaluation](../architecture/CAMPAIGN_EVALUATION.md),
  [execution threat model](../architecture/CAPABILITY_THREAT_MODEL.md), and
  [verifier usage](../../verifier/README.md).
- [S-034](../../DECISIONS.md) and the earlier
  [architecture review](reviews/2026-09-04-architecture-proposal-review.md)
  for the gates on larger platform infrastructure. Do not mark those gates
  satisfied based on this handoff or resume a paused experiment.

Use current source to resolve stale implementation descriptions. When working
with engineering cases, discover tools with `avila-core tools list`; use
`inspect`, `history`, and `attempt` for recorded facts and `run --plan` for
current reuse checks. Use normal `run` for execution; choose `--no-reuse` only
when deliberately testing fresh execution. Record source identities rather
than copying case inventories or result tables into this document.

**Preserve these distinctions throughout the work.**

1. Authored records, reports, and imported derivations are untrusted data until
   the checks for the particular use succeed. A serializable report may have
   public fields if no authoritative API accepts it as a checked witness.
2. A compiled draft may legitimately have unresolved parameters. Compilation
   success does not by itself mean execution readiness.
3. Content hashing, cached hash observations, receipt checks, qualification,
   policy, and verdict derivation establish different properties. A wrapper
   called `Verified` must not hide an unchecked or weaker observation.
4. `PASS`, `FAIL`, `INCONCLUSIVE`, and `NOT_EVALUATED`, exact comparison,
   requirement identities, and the existing quarantine rules remain distinct.
   Do not weaken a requirement or expand qualification to make a fixture pass.
5. Claims-only evaluation retains its existing explicitly limited meaning.
   It cannot mint a witness asserting that bytes, receipts, or signatures were
   checked. Pure mathematical evaluation under supplied premises remains
   distinct from an authoritative campaign admission.
6. Scientific assumptions and provider assertions keep their attribution.
   Optional presentation review remains outside technical admission/verdicts.
7. Historical conclusions retain their original context. Current admissibility
   requires the supplied current/as-of material; no hidden wall-clock or
   ambient registry reads belong in a pure checker.

**RA-01 — Specify the first slice and capture the baseline.**

Dependencies: none. Deliver an ADR using the next unused number and a concise
implementation note linked from this handoff. Document the existing rules used
by the slice, which crate establishes each premise, context identity, allowed
transitions, and what each checked value proves. State where the compiler,
runner, and independent verifier must agree.

Select a synthetic conformance workflow with a parent/child dependency, one
nominal quantity role, an exact or interval claim, and a bounded requirement.
Exercise qualification, a receipt, and the existing policy requiring the
relevant evidence. Choose fixture values with known expected comparisons;
do not introduce scientific qualification claims or modify a real case's
acceptance boundary. Reuse existing synthetic fixtures wherever possible.

Capture the existing compilation identity, canonical report identities,
verdict/rule/boundary, diagnostic codes and locations, and relevant reuse
decisions through the current operations. Preserve representative successful,
rejected, incomplete-draft, and missing-evidence behavior as regression checks.
Use temporary directories for generated reports.

The ADR must specify a small versioned derivation record sufficient for RA-04
and RA-05. Prefer a separate optional artifact so existing canonical reports
and signed packages do not acquire new bytes implicitly. Define its own
identity, references, unsupported-version behavior, and acquisition/replay
interface. If an existing wire format must change, explicitly version that
change and preserve historical decoding; never silently re-bless fixtures.

Done when the rule/authority/identity contract and regression baseline are
reviewable. This is the design step of the implementation, not its endpoint.

*Status — implemented.* The design is
[ADR-0026](../adr/0026-context-bound-verdict-derivations.md), including the
versioned `avila.core/verdict-derivation/v0.1-draft` record, its
self-excluding identity, and its replay interface. The slice is the
existing `fixtures/semantic-core/` campaign corpus plus committed
derivation fixtures under `fixtures/semantic-core/derivations/`; the ADR
captures baseline identities for the compiled snapshot, contract, registry,
claims, and both digest-only and observed campaign reports, all computed
through the current operations.

**RA-02 — Make checked semantic values preserve their invariants.**

Dependencies: RA-01. Start with
[compiled IR](../../crates/avila-core-compiler/src/compile/ir.rs),
[compilation](../../crates/avila-core-compiler/src/compile/mod.rs),
[exact values](../../crates/avila-core-kernel/src/number.rs), and
[package verification](../../crates/avila-core-evidence/src/package.rs).

- Separate report/document shapes from checked internal values where necessary.
  Restrict construction and mutation of the latter. Read-only accessors must
  not expose mutable collections, setters, or dereferencing routes that defeat
  the invariant. Audit `Clone`, `Default`, conversion, and deserialization paths.
- Represent the authoritative compile outcome as a closed outcome carrying
  its appropriate data; a successful checked outcome must carry its compiled
  value. Preserve the existing report wire representation where possible.
- Retain `ExactNumber` and resolved nominal kind/unit information in lowered
  authoritative quantities. Convert to the existing canonical string form at
  serialization boundaries. Do not parse strings repeatedly to recover facts
  compilation already established. Preserve accepted canonical identities.
- Protect the association between a package's manifest, checked document bytes,
  and integrity observations. The current `verify_case_package` can return
  `VerifiedCasePackage` with partial or failed integrity: model those outcomes
  explicitly rather than treating its type name as proof of completeness.
- Migrate actual compiler, runner, CLI, and app consumers to the protected
  path. A parallel checked API unused by execution does not complete the task.

Done when downstream safe Rust code cannot construct or mutate the protected
states through public APIs, and existing canonical outputs remain equivalent
for unchanged semantics. A complete draft must not be confused with an
executable plan during this migration.

*Status — implemented.* `CompiledContract`'s fields are crate-private with
shared-reference accessors; `Compilation` is the closed compile outcome and
`CheckedCompilation::contract()` is the only route to a checked contract.
`CanonicalTypedQuantity` stores `ExactNumber`; `CompiledParameterValue`
variants retain `ExactNumber` — serialization still produces the canonical
string, so every pinned snapshot hash is preserved (the Python lowerer
recomputes all committed snapshot identities unchanged). The evidence crate
returns the closed `PackageVerification` outcome (`Verified` /
`PartiallyVerified` / `Refused`); `VerifiedCasePackage` fields are private
and `supply_free_input` canonicalizes and hashes supplied bytes itself.
`compile_fail` doctests pin each sealed boundary; every consumer in
compiler, runner, CLI, and app migrated to the protected path (`cargo
clippy --workspace --all-targets -D warnings` clean).

**RA-03 — Bind observations and admission inputs to an explicit context.**

Dependencies: RA-02. Inspect
[campaign entry points](../../crates/avila-core-compiler/src/campaign/mod.rs),
[receipts](../../crates/avila-core-evidence/src/receipt.rs),
[runner orchestration](../../crates/avila-core-runner/src/case_run.rs), and
[step execution](../../crates/avila-core-runner/src/case_run/runner.rs).

Create an immutable evaluation context binding the relevant compiled snapshot,
claims, registry/profile, policies, qualification/revocation material, supplied
time, and verification scope. Document which identities are covered by other
bound digests. Distinguish evidence identity from permission to use it in this
context. Do not derive context equality from a Rust lifetime parameter.

Give observation types narrowly defined constructors. In particular, inspect
`evaluate_campaign_with_artifacts`: its current public `BTreeSet<String>` is a
caller-supplied assertion that hashing occurred. A set of supplied digest
strings must not serve as an unforgeable byte-check witness. Use an API that
checks supplied bytes, or an opaque observation from the actual verification
boundary. A bare digest remains an assertion. Maintain I/O separation and an
acyclic crate dependency graph when choosing the representation.

Differentiate fresh byte verification, an accepted hash-cache observation,
unavailable bytes, and failed verification according to current semantics.
Do not claim a hashed path stays unchanged indefinitely. A historical receipt
check also does not prove a fresh process ran. Bind each observation to the
artifact, invocation, and scope it actually concerns.

Require matching logical contexts at authoritative use sites, even when both
handles are simultaneously live in Rust. Imported serialized witnesses require
rechecking. Existing public compatibility APIs may remain only if they preserve
their limited assertion boundary and cannot bypass the protected path.

Done when fabricated digest sets and transplanted/mutated records cannot mint
stronger checked observations, and mismatched contexts produce bounded
diagnostics or explicit refusal. Existing missing-data outcomes remain honest.

*Status — implemented.* `EvaluationContext::bind` moves the checked contract
in, refuses a registry whose bytes hash differently than the contract's
recorded `registry_sha256` (`ContextError::RegistryMismatch`), refuses claims
naming a different `compiled_snapshot_sha256` (`ContextError::SnapshotMismatch`
— the claims↔compilation binding lives at the bind boundary, so no public
path can mint verdicts from a mixed context; the shared wrapper surfaces it
as `CORE-E7001`), hashes the claims bytes, and records every qualification
identity riding the claims — no ambient clock; the only time facts are the
instants already on the qualification records. `ArtifactObservations` records
a digest only through `check_bytes`/`check_file` (artifacts) and
`check_receipt`/`check_receipt_file` (receipts) — each hashes supplied bytes
itself; an empty set is the digest-only evaluation and emits no `artifact`
field, preserving committed report bytes. `derive_verdicts` refuses
admissions minted under a different `context_sha256` (`CORE-E7401`). The
runner feeds it what the run actually checked — fresh workspace outputs and
receipts, attested package artifacts under the supplied roots, and every
committed receipt document — via `collect_run_observations`, and writes the
resulting `derivation.json` + `campaign-report-observed.json` into the
workspace; the digest-only `campaign-report.json` stays the replay-compared
artifact. The digest-set parameter of `evaluate_campaign_with_artifacts` is
gone; `evaluate_campaign` delegates to `evaluate_campaign_in_context` with
`ArtifactObservations::none()`.

**RA-04 — Carry explicit rule applications through admission and verdict.**

Dependencies: RA-03. Work from
[admission](../../crates/avila-core-compiler/src/campaign/admission.rs),
[verdict construction](../../crates/avila-core-compiler/src/campaign/verdicts.rs),
[qualification](../../crates/avila-core-compiler/src/qualification.rs), and
[the kernel calculus](../../crates/avila-core-kernel/src/verdict.rs).

Represent the slice's existing prerequisites and rule applications explicitly.
Each established premise needs its rule/version, source identity, context,
dependent premises, and narrow conclusion. Record missing or contradicted
premises as such. These checking outcomes do not add new requirement verdicts.
Bound record size, recursive checking, and numeric work; unsupported rules or
exhausted budgets must be explicit refusals.

Only the admission checker may construct the checked admission used for the
slice. Only the authoritative evaluation path may construct the bound verdict
derivation. Callers may request evaluation; they may not submit a status field
as a substitute for its premises. Keep the pure kernel's mathematical boundary
separate from claims about how its inputs were obtained.

Make pre-execution requirements and post-execution checks explicit in the
existing runner path. A failed process, missing output, or absent required
premise cannot be treated as an admitted result. Preserve the exact current
rules for permitted partial output and qualification exemptions. Extend the
existing path to categorical or other models as required for compatibility;
do not silently reroute unaffected requirements through weaker checks.

Provide the versioned derivation through one shared operation and a minimal
headless interface. Extend the existing CLI/query catalog as appropriate;
frontends only render the shared result. Keep ordinary historical reports
decodable and their recorded-only meaning intact. The new artifact must contain
or reference enough material to reconstruct its premises without trusting a
serialized `admitted` or `PASS` field.

Done when a real run through the slice emits an attributable chain from input
observations to the unchanged bounded verdict, and a missing premise identifies
the blocked rule/use. Do not stop after changing public field visibility.

*Status — implemented.* `context.bind`, `admission.input`, `admission.claim`,
`qualification.envelope`, `qualification.required`, and one application per
requirement verdict are emitted in evaluation order; each carries its typed
premises (state `declared`/`admitted`/`match`/`not_checked`/`contradicted`/…,
never free text) and its conclusion. `CheckedAdmissions` and `DerivedVerdicts`
are minted only inside `EvaluationContext`; `MAX_RULE_APPLICATIONS` makes an
exhausted bound a `CORE-E7501` rejected report, never a partial record. The
runner writes `derivation.json` beside `campaign-report.json` on every run;
`avila-core evaluate --derivation` and `derivation diff` are the headless
interfaces. Committed campaign fixtures — including the digest-only case-000
report bytes — are unchanged.

**RA-05 — Verify derivations independently and explain changed uses.**

Dependencies: RA-04. Extend the existing
[independent verifier](../../verifier/avila_core_verify.py) from the written ADR
and fixtures, preserving its independent implementation and standard-library
boundary. It must reconstruct each supported inference, compare identities,
and distinguish checked premises, attributed assertions, and unavailable data.
Unsupported required material cannot be reported as successful verification.
Preserve existing verifier exit/status semantics; test the actual per-check
outcome rather than treating exit code zero alone as proof that all checks ran.

Use the existing [plan/impact representation](../../crates/avila-core-runner/src/case_run/plan.rs)
and [reuse rules](../../crates/avila-core-runner/src/case_run/reuse_rules.rs)
to expose the dependency path affected by a change in the slice. Implement
explicit recomputation over current rule applications; a persistent general
query engine is unnecessary for this increment. Distinguish:

- a requirement change that needs a new verdict and any threshold-dependent
  method work;
- an input/method change that needs execution reconsidered;
- qualification withdrawal that changes current admissibility while retaining
  the original historical record;
- presentation-only changes under the existing separation rule.

Two results with equal numbers or equal `PASS` states but different premises
must retain different derivation/context identities. Reuse numerical outputs
only through existing permitted rules; re-admit them in the applicable context.
Unknown dependency materiality must remain explicit.

Done when an honest derivation replays, semantic tampering is rejected even
after recomputing outer digests, and changed uses have source-linked reasons.
Compare new context checks against clean recomputation. Do not claim automatic
receipt reuse across a fixed-question boundary the current lineage rejects;
use separately authored revisions for that comparison.

*Status — implemented.* `avila-core derivation verify` replays the derivation
inside the compiler: it re-hashes the embedded context body against
`context_sha256`, validates the evaluator/profile/schema the record names,
compares the embedded context field-for-field with the context freshly bound
from the supplied documents, re-runs admission and verdict under it, and
compares every recorded application premise-for-premise — plus
`derivation_sha256`, `context_sha256`, and `campaign_sha256` (an omitted
campaign identity on a verdict-producing record is `not_checked`, not
silently skipped). The Python verifier's `verify-derivation` subcommand is
the independent replay: stdlib-only, it recomputes document identities, the
context identity, artifact observations and receipt identities from
`--artifact`/`--receipt` bytes, the claims↔snapshot binding, evaluator and
profile metadata, and every application via the ported lowering and verdict
rules; a forged conclusion fails even with an honestly recomputed outer
digest, and a campaign report must carry the replayed verdicts, admission
states, artifact checks, and boundary fields — `--campaign-report` compares
content, not just hash. Lifecycle refusal and envelope quarantine replay as
independent gates: an expired-and-revoked qualification verifies with both
reasons recorded. Bound material the caller does not supply is
`not_checked`, never `verified`. `explain_derivation_changes` (compiler) and
`derivation-diff` (verifier, a matching port) name the premise kinds behind
each changed use; equal verdicts under different premises stay distinct —
pinned by `derivations/case-000*` fixtures where the observed run's
`bounded.lt.within` verdict reports `premises_changed` on
`evaluation_context`.

**RA-06 — Complete the rejection tests, integration checks, and handoff.**

Dependencies: RA-05. Add tests at the boundary being claimed, including positive
counterparts proving that valid use is still possible.

| Attempt or event | Required check |
| --- | --- |
| Construct/mutate a checked compiled value, package, admission, or bound verdict from a client crate | Compile-fail test for the intended API boundary; a report DTO alone is not the protected witness. |
| Bypass with `Default`, conversion, unchecked deserialization, a mutable accessor, or a digest set | No unchecked route to a stronger witness; exercise the supported public entry points. |
| Use handles from two live contexts or transplant a receipt/qualification | Explicit identity/scope refusal. |
| Omit a parent, duplicate a claim, supply an invalid interval, or replace bounded evidence with nominal evidence | Existing admission/verdict distinctions and causal diagnostics are preserved. |
| Withdraw qualification or change policy | Recompute affected current uses and preserve the historical result and quarantine rules. |
| Change a threshold | Fresh evaluation agrees with the applicable exact calculus; execution reuse obeys its own dependencies and lineage rules. |
| Obtain identical values from different premises | Derivation provenance remains distinct. |
| Forge a serialized derivation and recompute its outer hash | Independent rule replay rejects the bad inference, not merely its old digest. |
| Omit verification material or name an unsupported profile/rule | Explicit unavailable/refused outcome, never silently verified. |
| Fail, time out, or cancel a process; return permitted partial output | Correct terminal/partial states; no unsupported admission or fabricated verdict. |
| Edit a draft with unresolved parameters or change presentation instructions | Preserve partial authoring and technical/presentation separation. |
| Exhaust checking resources or introduce a rule/dependency cycle | Deterministic bounded refusal; no fallback to success. |

Use external-crate compile-fail tests, or `compile_fail` doctests paired with
positive compilation checks so unrelated import errors cannot satisfy the test.
The current [source-pattern authority test](../../crates/avila-core-compiler/tests/authority_boundaries.rs)
is supplementary; it does not establish the language-enforced guarantee.
If using doctests, explicitly run them in CI: the existing all-targets command
does not run doctests. Prefer the existing test stack; justify a new dependency.

Run targeted checks while iterating. Before completion, from the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets --locked --no-fail-fast
cargo test --workspace --doc --locked
cargo build --locked -p avila-core-app -p avila-core-cli
```

From `verifier/`, run:

```bash
python3 -m unittest test_verifier -v
```

Add the slice's focused tests and new public-interface smoke checks. Consult
[CI](../../.github/workflows/ci.yml) for existing platform and verifier checks;
preserve those checks. Use normal runner fixtures with synthetic executables
for execution testing. Do not require external solvers or re-run expensive
scientific campaigns merely to prove a type/API refactor. New finding codes
need catalog entries, bounded repairs, and emitting fixtures.

Record the implemented ADR, changed boundaries, exact commands/results,
canonical compatibility evidence, named remaining limitations, and the next
increment here. Report environmental failures separately from code failures.
Mark each RA item complete only when its acceptance conditions hold. Changes
to code require these checks; this documentation-only handoff does not claim
they have been run.

*Status — implemented.* Rejection coverage lives at the boundary it claims:
`crates/avila-core-compiler/tests/context_binding.rs` pins fabricated
digest sets (a `check_bytes` digest for bytes no artifact attests reports
`not_checked`, never `verified`), mixed contexts (registry mismatch at
bind, `CORE-E7401` across two live contexts), claims naming a different
`compiled_snapshot_sha256` refused at `bind` before any admission runs,
a forged embedded context body and a forged evaluator rejected with the
outer digests honestly recomputed, an omitted `campaign_sha256` reported
`not_checked`, tampered claims and expired qualification replaying to
`not_evaluated`, `MAX_RULE_APPLICATIONS` exhaustion as `CORE-E7501`, and
the positive counterparts; `compile_fail` doctests pin `CompiledContract`,
`CanonicalTypedQuantity`, `Compilation::contract`, `CheckedAdmissions`,
and `VerifiedCasePackage` against downstream fabrication. The runner's
`run_observations_bind_checked_artifacts_and_receipts` pins the
execution-to-verdict chain: package artifacts under the supplied root and
committed receipt documents enter the observation set, and the resulting
derivation carries `artifact_observation`/`receipt` premises with state
`checked`; the (env-gated) full-execution test asserts the workspace
`derivation.json` binds fresh outputs and receipts. Python-side tests pin
the honest replays, the forged-conclusion-with-recomputed-digest refusal,
the forged context body and evaluator, the wrong-snapshot claims binding,
a campaign report whose verdict contradicts the replayed conclusion
(`derivation.campaign.content` mismatch), the omitted-campaign-identity
`not_checked`, the mixed-context mismatch, the combined
expired-and-revoked qualification replay (`derivations/case-000-mixed-
qualification.*` committed fixtures), and the `not_checked` behavior for
missing artifact bytes and a missing campaign report. New codes
`CORE-E7401` and `CORE-E7501` are catalogued in
`docs/architecture/DIAGNOSTICS.md` with emitting tests.

Validation commands run from the repository root on this worktree:

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace --all-targets --locked --no-fail-fast` — all
  suites pass (compiler 100 lib + fixture/diagnostic/context tests, runner
  224 lib tests, evidence 41, kernel 22, CLI, app; no failures).
- `cargo test --workspace --doc --locked` — 7 doctests pass, including the
  five `compile_fail` boundary tests.
- `cargo build --locked -p avila-core-app -p avila-core-cli` — clean.
- `python3 -m unittest test_verifier` — 139 of 140 pass; the only failure is
  `test_every_committed_case_snapshot_recomputes` for the untracked
  work-in-progress directory `examples/cases/case-010-matmul-rank/`, which
  this task was directed not to modify; its pinned snapshot predates or
  diverges from the committed lowering and the failure is unrelated to
  this slice (reported separately as pre-existing worktree state).

Named remaining limitations: the replay-compared campaign report remains
the digest-only evaluation — the run additionally emits
`derivation.json` + `campaign-report-observed.json` bound to the bytes it
checked, and promoting the observed report to the committed comparison is
a separate versioned decision; the verifier replays the rule vocabulary
the slice uses and reports anything outside it `not_checked`; `receipt`
premises name which receipt bytes were re-hashed without re-verifying a
receipt's own signature or semantics, which keep their own boundaries. See
ADR-0026 §Limitations.

**Follow-on implementation increments have specific entry conditions.**

| Increment | Work | Entry condition and completion evidence |
| --- | --- | --- |
| Broader typed workflow and obligations | Extend explicit prerequisite/guard checking to more existing roles and methods; add compositional domain rules and bounded resolution. | RA-01–06 complete; representative cases demonstrate missing compositional checks; any general obligation engine satisfies S-034 or an explicit revised decision. Preserve conformance and source-linked refusals. |
| Incremental analysis and authoring | Introduce tracked pure queries and immutable analysis snapshots; expose shared partial analysis through UI/MCP and, if justified, LSP. | A release-build workload shows repeated semantic computation matters. Clean/incremental agreement includes boundaries and derivations. Choose a query engine only after the measurement. |
| Stronger execution profiles | Enforce declared reads/effects, executable identity through launch, and confidential material-environment identity; account for libraries, interpreters, and remote data. | A platform-specific threat model and versioned execution/reuse contract. Tests falsify each claimed guarantee. Preserve historical receipt interpretation. |
| Concurrent execution authority | Consume local launch/commit authority and enforce durable attempt transitions, fencing, retry deduplication, and crash recovery. | A supported concurrent workflow needs it and the scheduler/storage gates are met. Adversarial stale-writer, cancellation, and recovery tests establish the promised scope. `Drop` alone is insufficient. |
| Formal properties and wider independent replay | Model a finite rule subset and verify stated preservation/invalidation properties; extend replay alongside each rule. | Written semantics and the implementation slice are stable enough to state the theorem precisely. Report proved scope and implementation correspondence separately. |

Do not substitute speculative infrastructure for the ready slice. These entry
conditions concern the larger work; they do not require permission pauses for
ordinary implementation choices in RA-01–06.

**Working agreement for the implementation agent.** Preserve unrelated worktree
changes. At preparation time the existing untracked
`examples/cases/case-010-matmul-rank/` belongs to separate work; do not modify,
delete, stage, or use it to infer authorization for this task. The proposal and
this handoff are task documentation. Do not reset the worktree, publish, deploy,
resume experiments, modify scientific requirements, or re-pin packages to hide
differences. Continue through ready dependencies; when a concrete external
block remains, document it and complete independent work.

**Copyable instruction for SWE-2:**

> Work in `/home/connoravila/Documents/Avila-Labs/project-north-star`.
> Read `AGENTS.md`, `CONTRIBUTING.md`,
> `docs/roadmap/proposals/2026-09-24-rust-informed-semantic-architecture.md`,
> and `docs/roadmap/RUST_ARCHITECTURE_HANDOFF.md`.
> Implement RA-01 through RA-06 in dependency order. Deliver one complete
> context-bound path from checked semantic inputs and execution observations
> through admission to a replayable verdict derivation, including changed-use
> explanations and independent Python verification. Do not stop at a plan or
> private-field refactor. Use the existing compiler/kernel/evidence/runner
> boundaries and normal query/run operations. Preserve exact comparisons,
> four-state verdicts, incomplete drafts, historical identities, scientific
> attribution, and unrelated worktree changes. Keep portable compatibility
> unless an explicit ADR and versioned format change require otherwise.
> Follow the handoff's rejection tests and validation commands, including
> compile-fail coverage and doctests. Update its RA statuses with implementation
> and validation evidence. Continue autonomously through ready work without
> routine confirmation. Treat the follow-on table as a separate backlog whose
> entry conditions must be met. Return a concise review handoff with changed
> boundaries, checks, compatibility evidence, and remaining limitations.
