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
`avila-core semantic-profile` reports 91 vectors, 68 compiler fixtures, 15
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
(status: proposed): revision/assessment/named-reference records in the
existing log, explicit contract amendments, derived migration from attempt
logs, shared-operation ownership, and five unresolved choices stated
plainly.

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

The next product increment is the full persistent revision/assessment model,
followed by local workflow starters and easier capability setup. The ADR from
CQ-05 should make that increment concrete. Resolve its identity and migration
choices and record any change to S-034's sequencing before implementation.

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
