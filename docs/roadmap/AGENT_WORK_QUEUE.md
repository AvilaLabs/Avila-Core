# Agent work queue: make design history usable

Recorded: 2026-09-15. Status: a bounded implementation handoff derived from the
existing Stage 0 mechanisms and the owner's preference for design revisions.
This does not advance a product gate, resume a paused experiment, or authorize
publication. Suitable for SWE-2 or another coding agent; no model-specific
capability or quality claim is assumed.

## Target outcome

In the real desktop workbench, an engineer can open a recorded campaign,
select a candidate attempt, understand its exact parent and changed inputs,
inspect requirement transitions and margin deltas, and plan or run a new
candidate with the existing runner. The simulated Product Preview is visual
reference only. It is not a source of engineering values or production logic.

This first slice presents **recorded candidate attempts** as the foundation for
design history. It does not pretend that today's run-bound attempt record is
already the full independent design-revision/assessment model in DH-01.

## Starting evidence and constraints

Checked against code at HEAD `3e6dd8f` plus the uncommitted product documentation
and preview from this conversation:

- `avila-core-runner/src/attempt.rs` implements parent-bound candidate state,
  typed diffs, and comparison records under fixed manifest/snapshot identities.
- `avila-core-runner/src/query.rs` provides `core_constellation` and
  `core_attempt`. They describe saved records, with explicit verification limits.
- `avila-core-app/src/tools_view.rs` already exposes these queries through
  generic forms and record rendering. Improve that experience rather than
  rebuilding its query engine.
- `avila-core-app/src/case_view.rs` already supports supplied free inputs and
  Plan/Run, but its `CaseRunOptions` construction leaves attempt lineage at the
  default. This is a concrete connection to make.
- The executable semantic profile reports 91 kernel vectors, 68 compiler
  fixtures, and 15 campaign fixtures. These counts are not a production or
  scientific qualification claim.
- EXP-005 remains paused in the status ledger. S-034 still gates prepared
  sessions, a transactional store, an obligation graph, and a scheduler on
  measured need. This queue does not override either decision.

Read `AGENTS.md`, `CONTRIBUTING.md`, `docs/product/PRODUCT_EXPERIENCE.md`, and
`docs/product/DESIGN_HISTORY.md` before implementation. Recheck current source;
this handoff is a starting point, not permission to ignore subsequent changes.

## Ready work, in order

### CQ-01 — Reconcile the operational status documents

**Dependency:** none. **Initial status:** ready.
**Status:** done 2026-09-15.

Reconciled against HEAD `3e6dd8f`; executable evidence:
`avila-core semantic-profile` reports 103 vectors, 68 compiler fixtures, 15
campaign fixtures; `avila-core tools list --json` reports eleven tools.

- `README.md`: "ten shared read-only queries" → eleven.
- `docs/architecture/ARCHITECTURE.md`: evidence crate's "no signature system,
  lineage validator, or independent verifier" corrected (ADR-0015 signatures,
  runner lineage validation, and `verifier/` exist; a package writer still
  does not); CLI surface updated for the query tools, `keys`/`sign`, and MCP;
  the workbench's "two modes" corrected to four (Cases browser, case
  workbench, specimen compiler, Tools workspace); the step-14 note now
  distinguishes the existing independent verifier from the absent packager;
  "planner" no longer listed as a current Rust component.
- `docs/architecture/EVIDENCE_MODEL.md`: "a verifier that does not import or
  trust the runner's own code remains open" corrected — the Python verifier
  does exactly that; its named-outs stay named.
- `docs/roadmap/ROADMAP.md`: "signatures ... remain absent" corrected to
  document-level ADR-0015 signing with its remaining gaps named.
- `docs/roadmap/STAGE_0_STATUS.md`: added a reconciled-as-of marker; dropped
  "signatures" from the Execution row's open boundary; the Application row
  now names the case browser and Tools workspace and keeps a dedicated
  design-history view open; the immediate queue is split into landed work
  and open items with their gates.
- `docs/product/USER_EXPERIENCE.md`: the implemented slice now names the
  case browser and Tools workspace; the scaffold section is scoped to the
  specimen mode.

Remaining limitation: the ledger describes code at `3e6dd8f`; later items in
this queue change the application row again.

Reconcile current-tense implementation descriptions in the roadmap, architecture,
README, and status ledger against the code and executable reports. Distinguish
current support, remaining generalization, and historical decisions. Do not
rewrite old ADRs as though later features existed when they were accepted.

Concrete discrepancies to check include signatures and the independent verifier
being described as absent, stale query counts, the application now having a local
case browser and Tools workspace, and completed work remaining in the immediate
queue. Add an as-of marker and direct evidence references where appropriate.

**Done:** current summaries agree; completed items are distinct from ready work;
research and scientific gaps remain visible. Record unavailable verification as
unavailable. Do not infer a stage transition from test counts.

### CQ-02 — Give recorded attempts a dedicated history view

**Dependency:** CQ-01. **Initial status:** ready after dependency.
**Status:** done 2026-09-15, commit `f064ee4`.

`crates/avila-core-app/src/history_view.rs` adds a History workspace over
`core_constellation` + `core_attempt`: explicit log open/browse/drop, roots
and children tree, untracked legacy rows, attempt detail (identities,
candidate state, typed changes, recorded verdicts, lineage/signature
status), source path + sha256 strip, and the recorded-only boundary
statement. `core_constellation` items gained `findings` and
`attempt_request` at the shared-query boundary. Validation:
`cargo test -p avila-core-app` — 30 tests including malformed/tampered-log
regression and narrow-window selection; screenshots
`target/history-view.png`, `history-detail.png`, `history-legacy.png`
exercised the real case-009 log and an untracked log.

Build a thin egui history view over the existing shared query operations. Open
one explicit log, show roots and children, select an attempt, and display its
candidate state, typed changes, findings where present, and recorded outcomes.
Retain source file/record identities and the query verification boundary.

Use the real workbench's styles and reusable components; use the Product Preview
for interaction inspiration. Do not copy its sample records, simulated timers,
assessments, or baseline state into the real path.

**Done:** a user can navigate a real parent/child record and an untracked legacy
row. Missing ancestry remains unknown. Malformed or tampered history surfaces the
shared operation's error instead of silently dropping records. Switching logs
clears stale selection/results. Long records and narrow windows remain usable.
Add focused interaction and malformed-record regression coverage.

### CQ-03 — Make parent comparisons legible

**Dependency:** CQ-02. **Initial status:** ready after dependency.
**Status:** done 2026-09-15, commit `10538b0`.

The attempt detail now renders the recorded comparison as a changed-result
view: bound-parent identity and parent record hash (labeled "bound parent —
not an approved baseline"), all verdict transitions among the four states,
exact parent/child margins with Core's delta and unit (sign-coloured, never
recomputed), and every declined comparison with its stated reason.
Validation: `comparison_renders_all_states_deltas_and_reasons` covers all
four verdict states, mixed-sign deltas, and every unavailability reason;
`unavailable_reason_names_every_core_value` pins the reason table. The real
case-009 specimen (`copper-ratio4-r1` vs `copper-ratio2-r0`) shows the
PASS→FAIL transition and 15 exact margin comparisons.

Present the selected attempt's existing authoritative parent comparison as a
clear changed-input and changed-result view. Preserve four-state verdicts,
canonical units, exact deltas, and explicit unavailability reasons. Label the
comparison target as its parent; a user-selected reference is not an approved
baseline. Do not create arbitrary cross-contract comparison semantics here.

**Done:** existing records demonstrate both improved and worsened results,
including a PASS-to-FAIL transition. Missing/incompatible margins do not become
zero. The UI performs no independent arithmetic or verdict derivation. Where a
shared query already supplies the value, consume it; fix a demonstrated shared
query gap at its owner boundary rather than parsing log text inside the UI.

### CQ-04 — Connect a new candidate to the existing lineage runner

**Dependency:** CQ-03. **Initial status:** ready after dependency.
**Status:** done 2026-09-15, commit `5280293`.

Machine setup gained a Design attempt section (attempt ID, parent attempt,
candidate input over the declared free inputs) that assembles the same
`AttemptLineageRequest` as the CLI `--attempt` flags and previews the exact
intent — parent, tracked input and its supplied file, and the missing-log
requirement — before any plan or run. History's "Plan a child of this
attempt" prefills parent + candidate input only when the open case declares
that input and the viewed log is the workbench's log (or none is set). The
report overview shows the recorded attempt row and comparison counts.
Validation: `attempt_fields_assemble_one_explicit_lineage_request`,
`the_setup_panel_shows_the_intended_lineage_before_execution`,
`parent_prefill_is_offered_only_when_the_relationship_can_hold`, and the
end-to-end `planned_and_executed_attempts_form_a_queryable_lineage`
(real case-003: missing-log refusal, root run, plan with all-verdicts
child_missing, executed child, duplicate-ID and missing-parent refusals,
lineage read back through `core_constellation`/`core_attempt`).

Extend the real case workbench's existing candidate input and Plan/Run setup to
supply `AttemptLineageRequest`: explicit attempt ID, optional parent ID, and
nominated candidate input. Let selection in history prefill parent identity only
when the selected case/log makes that relationship valid. Keep exact inputs and
intended parent visible before execution. Delegate all authoritative validation
to the runner. Preserve the existing CLI and no-lineage workflows.

**Done:** with an existing small local checker specimen, a user can select a
parent, supply a modified candidate file, inspect the current plan, run it, and
see the actual child and comparison in history. Exercise duplicate ID, changed
question identity, missing log, failed execution, and input validation errors.
Read the existing plan-only behavior and preserve it: planning must not invent
completed execution or a child assessment. Do not overwrite the parent's input,
package, or saved result. A missing executable is actionable, not a fake success.

No schema editor or solver installation flow is required for this slice. Reuse
existing fixtures and capabilities rather than building another scientific demo.

### CQ-05 — Measure the local workflow and specify the next model increment

**Dependency:** CQ-04. **Initial status:** ready after dependency.
**Status:** done 2026-09-16.

`experiments/exp-003/` holds the harness: `workload.json` (frozen CASE-003
identities), `measure.sh` (release build, manifest capture, timed phases),
`README.md` (phases, availability, limits), and
`results/run-20260916T033739Z/result.json` (recorded evidence). On the
measured machine, verify and reuse phases ran 22–67 ms wall over 3
repetitions each; fresh execution is recorded UNAVAILABLE because the pinned
capability executable (`sha256:b8d828…`) does not resolve and `skfem` is
absent — no substitution. Agent orchestration is explicitly out of scope.
The ADR proposal is `docs/adr/0019-design-revisions-assessments-and-named-references.md`
(now accepted and implemented; see the ADR-0019 increment entry below):
revision/assessment/named-reference records in the existing log, explicit
contract amendments, derived migration from attempt logs,
shared-operation ownership, and the five unresolved choices resolved in
the ADR's decision section.

Two bounded deliverables:

1. Prepare a reproducible release-build measurement harness and frozen workload
   for the local CASE-003 path described by EXP-003. Before measurements, record
   machine, build, workload identities, cache policy, repetitions, and available
   executables. Separate verify/reuse, fresh execution, and any manual setup.
   Run the locally available non-agent portion; mark unavailable portions
   explicitly. Do not silently substitute a different workload or claim this
   completes the full agent/unattended experiment. Do not make model calls,
   download large solver data, change pinned binaries, or resume EXP-005.
2. Draft an ADR for DH-01–03: independent design revisions, assessments, named
   references, attributable baselines, and explicit contract amendments. Include
   migration from current attempt logs, multiple assessments of one revision,
   unassessed revisions, failure states, and ownership of shared operations.
   Compare the smallest viable choices; record unresolved choices plainly.
   This is a proposal deliverable, not authorization to implement a new store
   or silently replace the accepted attempt semantics.

**Done:** another agent can repeat the available measurements, the limits of the
measurement are stated, and the proposed next implementation has concrete
acceptance examples. Link the artifacts and proposal from this queue.

## Review point after the ready queue

### ADR-0019 increment — implemented 2026-09-16

The persistent revision/assessment model (DH-01–03) is implemented per the
accepted ADR. ADR-0019's five choices were resolved in its decision section
before implementation; S-034's sequencing is unchanged (attempt lineage
remains the run record; revisions and assessments are records in the same
log, not a new store).

**What landed:**

- `crates/avila-core-runner/src/history.rs` — the unified log view
  (`LogView`) and four record kinds: `design_revision`, `assessment`,
  `named_reference`, `contract_amendment`, each inside a
  `log-record/v0.1-draft` envelope with optional ADR-0015 signature.
  Append operations (`create_revision`, `append_assessment`,
  `set_reference`, `record_amendment`) run through the same lock +
  revalidation critical section as run rows. `validate_log` checks every
  record kind fail-closed.
- `attempt.rs` — `AttemptLineageRequest` gains `revision_id` and
  `amendment_id`; admission refuses a child citing an amendment, an
  unresolved revision, a candidate/fixed-identity mismatch with the cited
  revision, a changed-manifest root without an amendment, and an
  amendment whose stated identities disagree with the run's.
- `case_run/log.rs` — run rows carry `revision_id` / `assessment_id` /
  `amendment_id`; a revision-bound run's assessment row is appended after
  the run row lands, citing the exact run line's sha256 with verdicts
  copied verbatim.
- `query.rs` — `core_revision`, `core_assessment`, `core_reference` join
  the shared catalog; `core_constellation` and `core_history` project all
  record kinds with `record_kind`, the verbatim `record` payload, and
  `record_sha256`. Revision-less run rows read as derived revisions and
  derived assessments named by their attempt id.
- CLI — `avila-core revision create`, `reference set|show`, and `amend`
  verbs; `run` accepts `--revision` and `--amendment`; `tools call`
  reaches the three new queries.
- Workbench — the history view lists the four record kinds in recorded
  order with their verbatim payloads; the attempt setup panel accepts a
  revision id and (roots only) an amendment id; `tools_view` has an entry
  point for every shared tool.
- `schemas/` — `log-record`, `design-revision`, `assessment`,
  `named-reference`, and `contract-amendment` v0.1-draft JSON schemas.

**Validation evidence:**

- `cargo test --workspace` green (runner suite 141 tests incl. 14
  history-module tests: revision admission, duplicate ids, exact-parent
  binding, child-change derivation, amendment identity checks, reference
  supersession chains, legacy-row derivation, tampered-assessment and
  forged-id fail-closed validation).
- Two end-to-end tests drive `execute_case` with `revision_id` set: the
  bound run names its revision and appends an assessment row citing the
  run line's sha256; an unknown or mismatched revision is refused before
  execution.
- `design_history_records_are_queryable_with_their_exact_identities`
  exercises one log holding all four record kinds plus a legacy run
  through `core_revision`, `core_assessment`, `core_reference`, and
  `core_constellation`, including the cross-amendment comparison edge.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` clean.
- CLI smoke-verified end to end: `revision create` → `reference set` →
  `amend` → amended-root `revision create` → `reference show` /
  `tools call core_revision` / `constellation` over the real log.

**Limits:** named references are per-log only (no cross-log resolution);
assessment comparison is always derived on read — nothing stored claims a
delta; the workbench displays record payloads verbatim and owns no
domain logic. A revision's `intent`, reference `rationale`, and amendment
`rationale` are inert attribution text.

### Local workflow starters and easier capability setup — implemented 2026-09-16

The named next product increment landed in three commits: `dcfa513`
(hash-only probing in the runner plus the `avila-core capabilities` verb),
`e368b54` (workbench probing controls), and `abd7079` (bundled-root
resolution, setup prefill, and per-example needs lines).

**What landed:**

- `crates/avila-core-runner/src/case_run/probe.rs` — shared hash-only
  probing: `probe_capability` compares a candidate file's bytes to a bound
  executable digest and reports `verified`, `mismatch`, or `missing`;
  `scan_dir` offers the regular files directly inside one directory
  (deterministic order, bounded at 256 entries, no recursion) to every
  declared capability; `candidates_on_path` searches `PATH` for a capability
  name and its shortened `-`-prefixed forms (e.g. `python3`, `python3.14`
  for `python3-numpy`) while refusing an empty name. Probing never executes
  a candidate; a check or run re-verifies the chosen bytes against the
  package's pinned identities.
- `avila-core capabilities CASE` — accepts a case directory or a
  `package.json` path, refuses undeclared capability names and unreadable
  scan paths, and emits a `capability-probe/v0.1-draft` JSON report naming
  each capability's expected digest, candidate paths, per-candidate state,
  and whether any candidate verified. Options: `--candidate NAME=PATH`,
  `--scan DIR`, `--on-path`.
- Workbench machine setup — each program row can check a typed path,
  **Search PATH**, or **Scan folder…**; hashing runs on a background thread
  and the panel states that probing does not execute candidates. A verified
  discovered match is adopted only through the explicit **Use this program**
  action; typed or remembered paths are never silently replaced, and a path
  changed since the last probe is called out.
- Workflow starters — a bundled example's declared source roots resolve
  against the shipped trees (`capabilities/<name>`, `libraries/<name>`,
  then `<name>`; `case` names the case's own folder) and empty setup rows
  open prefilled, while programs stay operator-supplied and restored or
  typed locations win. Each example card states what the case still asks of
  the machine — bundled folders, folders the operator must supply (e.g.
  `simsopt`), and the program count — as manifest metadata, not
  verification.

**Validation evidence:**

- `cargo test --workspace` green — runner suite 146 tests including probe
  coverage for digest verification, mismatch/missing states, path
  deduplication, bounded deterministic scans, shortened-name `PATH`
  matching, and empty-name refusal; CLI suite adds named candidates, scans,
  missing state, undeclared-capability and unreadable-scan refusals, and
  manifest-path input; app suite 39 tests including typed-probe, folder
  scan, apply-verified, panel controls, bundled prefill, restored-path
  preservation, no-capability-prefill, and needs-line coverage.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean; `cargo build --workspace` clean.
- CLI smoke-verified against the real case-003 package: `--on-path`
  located `python3.14` for `python3-numpy` and reported the honest
  digest mismatch.
- Real workbench screenshots exercised the rendered result: prefilled
  `case`/`thermal` rows with empty program rows, the per-row
  **Search PATH** / **Scan folder…** controls, and the example cards'
  needs lines.

**Limits:** probing establishes byte identity only — it does not execute,
sandbox, or qualify a program, and a verified probe is not a run
authorization. Bundled-root offers resolve only for cases inside this
build's shipped examples tree; separately distributed builds still open a
case folder and supply every root. External roots such as `simsopt` remain
explicitly the operator's. The needs line previews the manifest and
verifies nothing.

### Bound plans and blocked states — implemented 2026-09-17

The "Deterministic planning and blocked states" build gate's evidence
landed: a content-identified bound plan inside every `run --plan` report
plus state fixtures pinning each decision.

**What landed:**

- `crates/avila-core-runner/src/case_run/plan.rs` — the
  `avila.core/bound-plan/v0.1-draft` document: `manifest_sha256` and
  `compiled_snapshot_sha256` identities, a plan-level
  `ready`/`blocked`/`refused`/`unavailable` status, ordered step
  decisions, and an `unresolved` list naming the roots and capabilities
  the plan still needs. `BoundPlan::unavailable` covers runs rejected
  before execution planning and packages declaring no executions.
- `Runner::bound_plan` — derives each step's decision from the plan-mode
  report the same `run_all` produced, then binds the operator supplies the
  report's `planned` never checks: the declared capability's bytes are
  hashed at the supplied path (never executed) and the environment keys
  its adapter and package declaration require are checked by name. A step
  reported `planned` becomes `execute` only when every supply verifies;
  otherwise it is `blocked` with named reasons — `inputs_unverified`,
  `capability_not_supplied`, `capability_missing`, `capability_mismatch`,
  `environment_not_supplied` — plus the missing key names. `reused` maps
  to `reuse_committed` (a verified committed receipt needs no executable),
  `refused` carries its finding codes, and compiled steps without a
  declared execution are `not_executed` with their reason.
- `schemas/bound-plan.v0.1-draft.schema.json` — the published schema;
  real `--plan` output validates against it.
- Human rendering: a `planned` step the bound plan blocks prints
  `[BLOCKED] step would execute but is blocked: <reasons>` instead of the
  misleading "would execute", and the outcome gains a `Bound plan:` line
  naming the status and unresolved supplies.
- State fixtures: six adversarial tests over the synthetic package pin
  `reuse_committed`, `execute` (plus byte-for-byte determinism across two
  bindings), `blocked` on unsupplied/missing/mismatched capability bytes,
  `blocked` on unverified inputs and a missing environment key,
  `refused` for an execution outside the compiled workflow, and
  `unavailable` for a manifest-pin rejection; `case_run::tests` adds the
  real-fixture contrast — CASE-004's report says `planned` while the bound
  plan says `blocked: capability_not_supplied`, and its committed receipt
  reuses without the executable.

**Validation evidence:**

- `cargo test --workspace` green; `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` clean.
- CLI smoke-verified on the real CASE-004 package in four states
  (reuse→ready, no-reuse→blocked/capability_not_supplied, wrong
  executable→capability_mismatch, no roots→inputs_unverified with named
  roots), and on CASE-003 for the rejected-early `unavailable` path.
- The emitted document validates against the published schema.

**Limits:** a bound plan is a projection of the run report for one set of
operator supplies — it is not persisted as its own artifact and it does
not schedule, rank, or estimate anything; those belong to the planning and
selection roadmap track. A `ready` plan is not a run authorization, and an
executed run still re-verifies every byte the plan hashed.

### Contract preflight editing — implemented 2026-09-17

The specimen compiler is now an authoring surface: JSON-level preflight
editing over the contract and registry, driven by the same
`compile_documents` the case runner uses.

**What landed:**

- `Specimen` is an editable draft — contract and registry buffers, the
  paths they came from, a dirty flag, and the latest check result. It
  loads the embedded specimen as the starting template.
- A **Sources** workspace: **Open contract…** loads a contract file plus
  its `registry.json` sibling; the buffers edit as monospace JSON;
  **Check** recompiles on demand; **Save** writes both buffers back to
  their files (enabled only when opened from disk and dirty); **Reset to
  specimen** discards the draft. A status line reports COMPILED/REJECTED
  with the blocking-finding count.
- **Findings** gained apply buttons: a repair candidate whose exact edits
  the compiler can state shows **Apply: <candidate>** and applies the RFC
  6902 patch to the buffer, then the check re-runs. Candidates the
  compiler can only name still render as labels. The case view's findings
  stay read-only.
- `apply_edits`/`apply_edit` — an RFC 6902 applier over
  `serde_json::Value::pointer_mut` for `replace`, `add` (object key or
  array index/`-`), and `remove`.
- `--specimen WORKSPACE` opens the mode at a named workspace (a
  screenshot dev aid).

**Validation evidence:**

- `cargo test --workspace` green; app suite 43 tests including draft
  open/edit/check/save/reset, the RFC 6902 vocabulary, a one-click repair
  that clears its own CORE-S1101 finding on the real specimen, and the
  honest-draft fixture unchanged.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Real screenshots exercised the Sources workspace (toolbar, check status,
  editable buffers) and Findings (repair candidates render; the
  specimen's judgment-only findings honestly show no Apply button).

**Limits:** this is JSON-level editing, not structured form authoring —
the fields' meaning stays in the contract schema and its findings; a
repair apply re-serializes the buffer (formatting is not preserved);
`registry.json` must sit beside the opened contract; a malformed buffer
shows the compiler's message instead of a report. Plan approval,
structured question-first authoring, and pilot-participant testing
remain.

### Capability threat model and conformance design — implemented 2026-09-17

`docs/architecture/CAPABILITY_THREAT_MODEL.md` is the adversarial review the
gate asked for, tied to the boundary as implemented rather than the target
platform's:

- A mechanism table: each threat, the actual defense (digest-pinned
  executables, confined path resolution, `env_clear` staging, symlink-refusing
  output collection, process-group kill, free-input schema validation,
  ADR-0015 signature checks, invocation-identity reuse), and the named
  adversarial test pinning it.
- A residual-risk list kept honest: no sandbox, check-to-exec TOCTOU,
  supplied environment values deliberately outside invocation identity
  (ADR-0013 — a changed value cannot invalidate reuse), no resource
  accounting, host compromise, document-level signature coverage.
- A conformance *definition*: a conformance vector is an ordinary case
  package binding a candidate executable to a type's declared boundary over
  owner-fixed inputs; the suite is adversarial (wrong slot, malformed claim,
  determinism violation, out-of-domain parameter, wrong media type), and
  conformance is scoped to `type@major`, never global. The SDK, signed
  provider packages, and the suite itself stay Stage 3.

The status row moved to "Design documented": the review exists and names
what's unfixed; the conformance suite is a named later track, not implied
by this slice.

**Validation evidence:** every test name cited in the mechanism table was
verified against `execute/adversarial_tests.rs` and `execute/mod.rs`; no
code changed (documentation-only increment).

### Campaign supervision roll-up — implemented 2026-09-17

The named gap "campaign supervision beyond one run" now has a slice:
`core_constellation` returns a `supervision` section over the filtered
items — run-state counts (`evaluated`/`planned`/`rejected`), step-state
tallies, the latest recorded verdict per requirement, the newest record
per case, and an `attention` list naming every non-evaluated run with its
finding codes. The roll-up respects `case_id`/`id` filters and stays
strictly recorded-only.

The History view renders it under the source strip: state badges, step
tallies, latest verdicts, and the attention list. CLI and MCP get the
same section through the shared query; the human renderer shows it
unmodified.

**Validation evidence:**

- `cargo test --workspace` green; a new query test pins the roll-up —
  including that a rejected record lands in `attention` and becomes the
  case's latest record without moving a requirement's latest verdict,
  and that an attempt filter scopes the roll-up.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` clean.
- CLI smoke on `case-002`'s 127-line campaign log returns the full
  roll-up; a real screenshot exercises the History strip.

**Limits:** supervision reads a single log — a cross-campaign or
cross-case dashboard is not implemented; "waiting for input / running"
are live states the log cannot record, so the strip is a recorded-state
reading, not a progress monitor.

### Package-root semantics + redaction named owner-gated — implemented 2026-09-17

EVIDENCE_MODEL.md gains two sections resolving the gate's remaining
locally-doable half:

- **Package-root semantics**, documented against the implemented rules:
  documents confined to the canonicalized package root (`..`/symlink
  escapes refuse before reads), artifacts digest-bound through
  operator-resolved source roots (location is not identity), the manifest
  digest as package identity, and check-states-not-errors for absent
  bytes. A new test,
  `a_relocated_package_verifies_at_the_same_identity`, pins the
  content-not-location claim.
- **Redaction and retention (owner-gated)**: the mechanism exists — a
  redacted artifact keeps its digest identity and reads as
  `not_checked`/`missing` — but which artifacts may be redacted before a
  package may be called publishable, and what retention obligations
  apply, are policy decisions trading confidentiality against
  verifiability. Named owner-gated rather than unimplemented.

The status row now reads Partial with the three pieces named honestly:
package-root semantics documented and pinned, the independent verifier
exercised (named-outs open), redaction/retention rules owner-gated.

**Validation evidence:** `cargo test --workspace` green including the new
relocation test; `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets -- -D warnings` clean.

### Verifier named-outs: staged-review verification — implemented 2026-09-17

The independent verifier's "presentation-gate realisation" named-out is
resolved for its committed half: a new section 11 in
`avila_core_verify.py` verifies every `staged_review_record` document —
recomputing `request_sha256`/`record_sha256` (canonical body minus the
digest field, the rule `campaign_sha256` shares), re-deriving each
presented-evidence entry from claims.json exactly as the run realises it,
checking readiness against the recorded missing list, reviewer role,
disposition vocabulary, and the eligibility-policy digest against the
bound `review_policy` document.

One honest subtlety the section surfaced: CASE-001's committed record
binds a campaign/snapshot identity that *no committed record carries* —
the run it names was never committed. The verifier reports that as
`not_checked` (unresolvable reference), never `mismatch`, and a new test
pins the distinction.

The other named-out — compiled-snapshot recomputation — was closed in the
next increment: `verifier/avila_core_lower.py` independently lowers the
committed contract+registry into the compiled-body identity, proved
against all 68 compiler fixtures' pinned snapshots and every committed
case.

**Validation evidence:** 56 verifier tests / 163 subtests pass, including
five new `TestStagedReviewMutations` tests (edited presented-evidence
digest re-stamped consistently is still named; out-of-vocabulary
disposition named; stale record digest named; unresolvable campaign
binding named `not_checked`; positive path clean). `verify-case
case-001` runs the section under the example trust root with exit 0, and
a new CI step exercises it there.

**Next product increment:** Stage-0 status row updates for the verifier
gate, then the final report of what remains owner- or outside-gated.

### Categorical verdict vectors + relocatable export — implemented 2026-09-17

Two increments that grew out of the ledger pass.

**Categorical vectors.** `verdict-calculus.v1.json` gains two mixed-state
numeric vectors (`not_evaluated.mixed-admitted-quarantined`,
`not_evaluated.mixed-admitted-invalidated`) and a ten-vector
`categorical_vectors` section covering every `equals`/`in_set` outcome and
edge — missing, quarantined, mixed, duplicate, category-missing, invalidated.
The corpus is now 103 vectors (was 91). The vectors surfaced a real
divergence: the independent verifier evaluated the *admitted half* of a
mixed evidence set where the kernel reports `not_evaluated` — a
non-admitted claim settles nothing. Both verifier evaluators now follow
the kernel's ordering; an admitted claim missing its category reports
`not_evaluated.category_missing` instead of a false `fail`; and `in_set`
is vector-proven on both implementations rather than an inferred rule.

**Relocatable export.** `avila-core export CASE --source-root name=DIR --out
DIR` verifies a package completely, then gathers it into one directory —
manifest and documents verbatim, artifacts under `roots/<source_root>/`,
plus a content-identified `export-report.json`
(`export-report/v0.1-draft`) whose digests are re-measured on the copied
bytes. Export refuses a partially verified package and writes nothing.
The bundle verifies anywhere: `run --plan` on it reproduces the bound
plan, and the independent verifier re-derives package identity, artifact
digests, and verdicts against `roots/<name>`.

**Validation evidence:** kernel harness executes all 12 new vectors;
`semantic-profile` reports 103; 57 verifier tests / 175 subtests; three
export tests pin gather/relocate-verify/refuse-empty; smoke run on the
real CASE-004 bundle verified end-to-end by the Python verifier.

**Limits:** the export report is an export record, not a manifest
document — it carries no evidence weight. Package *installation* of
foreign bundles (a catalog operation) remains a reserved track.

### Compiled-snapshot recomputation — implemented 2026-09-17

The independent verifier's last named-out is closed: a new
`verifier/avila_core_lower.py` independently lowers the committed
contract+registry into the compiled body's identity and `verify-case`
reports `claims.compiled_snapshot_recomputation` as `verified` (digest
equal), `mismatch` (digest differs or the pair would not compile), or
`not_checked` (a construct outside the port's covered subset, named).

The port reimplements the lowering that decides the compiled body's
contents — canonical document identities, registry-reference validation
(invalid sources, purpose checks), slot resolution (explicit and
single-match auto-binding with role/media checks), the deterministic
topological order, parameter and material-factor lowering (all five
typed-value families with domain bounds), determinism/seed rules,
presentation-gate resolution, requirement lowering into canonical units,
claim-model sufficiency, basis coverage, and categorical requirements —
plus the `deny_unknown_fields`/required-field/enum refusals, since a pair
the compiler rejects has no legitimate snapshot. It deliberately does not
reproduce the finding vocabulary (codes, pointers, repairs) a rejected
compile reports; that text carries no weight in the snapshot.

**Validation evidence:** every one of the 68 compiler fixtures' pinned
`snapshot_sha256` values reproduces exactly (and every rejected fixture is
detected as non-compiling), and every committed example case's recorded
snapshot recomputes; three new mutation tests prove the check names an
edited contract field, an edited recorded digest, and a non-compiling
contract as `mismatch`. 62 verifier tests pass.

This is the Stage-3 "second implementation" seeded early: the verifier now
implements the semantic kernel *and* the compiler's lowering result in an
independent codebase.

### Real-contract diagnostic defect corpus — implemented 2026-09-17

`fixtures/semantic-core/defects/` pins 23 realistic defects seeded one at a
time into verbatim copies of the committed CASE-001 contract+registry —
typo'd and missing fields, unresolvable metrics and bindings, role/media
mismatches, unadmitted units and limit-kind mismatches, parameter and seed
violations, incomplete review declarations, basis and coverage violations,
unknown purposes and capability types, a self-dependency, and two
registry-side invariant breaks (a dropped role, a rewritten canonical-unit
factor). Each fixture records the mutation as JSON pointers plus the
compiler's exact status, findings (code, class, owner, primary pointer,
repair applicability), and — where compilation still succeeds — the
snapshot identity.

One fixture intentionally compiles: dropping a redundant explicit binding
lets single-candidate auto-binding resolve the slot, and the snapshot
differs from the base pair's only through the source-document digest —
pinning that defect location does change identity even when structure does
not.

**Validation evidence:** `generate.py` produced the corpus by invoking the
real compiler; the Rust fixture harness replays all 23 against
`compile_documents`; the independent lowering port reproduces every
rejection (and the compiled snapshot), which required porting the kernel's
canonical-unit factor invariant it previously skipped. 63 verifier tests
pass.

### Deterministic capability discovery — implemented 2026-09-17

The "package discovery" half of planning-and-selection's open row:
`CaseRunOptions.capability_dirs` / `--capability-dir DIR` scans a directory
once and binds the file whose digest equals the manifest's pinned
`executable_sha256`. Selection is deterministic *because the pin is the
selector* — any match is byte-identical to the declared capability, so no
name matching or ordering policy is needed. Directories scan in declared
order, entries in file-name order, first digest match wins; an explicit
`--capability` supply always wins and is authoritative (a supplied wrong
file refuses even when the catalog holds the right bytes); every resolved
path is hash-verified again at use. The bound plan records
`capability_source` (`supplied`/`catalog`) — a kind, not a path, so plan
identity stays portable.

**Validation evidence:** four new adversarial fixtures cover catalog
selection with a decoy present, explicit-supply precedence, no-match
blocking, and source recording on the bound plan; the schema lists
`capability_source`; smoke-tested end-to-end on a re-pinned CASE-004 copy
(`decision: execute`, `capability_source: catalog`).

**Limits:** discovery is one directory level deep and digest-only — there
is no ranking among *different* admissible implementations (that is the
still-open multi-implementation selection policy), no cross-registry
package discovery, and no cost/duration estimates.

### Question-first form authoring — implemented 2026-09-17

The specimen compiler gained a **Question** workspace: structured form
fields over the question layer of the contract — the bounded question,
contract id/revision/status, execution-policy flags, the assumptions list,
and every requirement's statement, purpose (registry choices), metric
(none / contract input / step output, with the output-slot dropdown fed by
the step's declared capability type in the registry), comparison, limit
and tolerance quantities (kind and unit dropdowns from the registry), and
basis. Requirements add and remove in place. Edits write the same JSON
buffer Sources edits — the authoritative compiler is still the only check,
and nothing persists until Save.

**Validation evidence:** the workspace renders against the bundled
specimen (screenshot-verified); helper tests pin that every specimen kind
admits units, every step exposes its declared output slots, the specimen's
requirement metric resolves to real choices, and a malformed buffer is
reported rather than panicking.

**Limits:** the form covers the *question* the workflow answers — workflow
steps, inputs, bindings, parameters, seeds, and review declarations stay
JSON-level in Sources, as do categorical requirements. There is no
schema-driven generic form generator; each field is written against the
contract shape explicitly.

A public catalog, package installation, browser client, cloud/HPC scheduling,
organization governance, autonomous search orchestration, and comprehensive
semantic invalidation remain larger roadmap tracks. They are not implicit tasks
for a worker that reaches the end of this queue. Neither are solver physics,
qualification bounds, package re-pinning, or evidence re-blessing.

## Working agreement

- Inspect the initial worktree and preserve existing uncommitted documentation
  and the preview. Do not reset, stash away, or overwrite unrelated work.
- Take one item at a time. Update its status and record changed files, exact
  validation commands/results, and remaining limitations. Keep changes easy to
  review; no remote pushes, releases, or publication are part of this handoff.
- Continue through ready dependencies without routine confirmation. If one item
  needs missing external data or a genuinely unresolved semantic decision,
  record that specific dependency and complete independent ready work.
- Keep scientific authority in compiler/kernel/runner boundaries. Keep the
  preview executable separate. No fabricated results in the real workbench.
- Follow repository checks: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace --all-targets`. Use targeted tests while iterating;
  complete required checks before handoff. Report an environmental failure
  separately from a code failure. If the temporary filesystem quota recurs,
  use a dedicated ignored `target/` directory as the test `TMPDIR`.
- Capture and inspect the real egui screens and exercise the user flow. Passing
  unit tests alone is insufficient evidence for a usable history interface.
- Stop when the finite ready queue is complete. Report the next review point;
  do not turn spare time into an unbounded platform rewrite.

## Copyable worker instruction

> Work in `project-north-star`. Read `AGENTS.md`, `CONTRIBUTING.md`, and
> `docs/roadmap/AGENT_WORK_QUEUE.md`. Execute CQ-01 through CQ-05 in dependency
> order, honoring the scope and acceptance criteria. Preserve the existing
> worktree changes and keep the simulated preview separate from the real
> workbench. Use the existing query, lineage, and runner operations. Update the
> queue with evidence as each item completes. Do not resume EXP-005, re-pin
> scientific packages, change verdict/qualification rules, or implement deferred
> infrastructure. Complete the required checks and visually exercise the app.
> Continue through ready work without routine confirmation; stop at the stated
> review point with a concise handoff.
