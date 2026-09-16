# ADR-0019: Design revisions, assessments, and named references

- Status: accepted 2026-09-16; the unresolved choices are decided below
- Refines: ADR-0014 (identity-bound attempt lineage), ADR-0004 (four-state
  verdicts), ADR-0010 (verdicts independent of review), ADR-0015 (signed
  records), ADR-0018 (persisted applicability contexts)
- Implements: the DH-01–03 portion of `docs/product/DESIGN_HISTORY.md`

## Context

The accepted attempt lineage (ADR-0014) binds a run to an exact parent row
inside one campaign log: one record is simultaneously the proposed design,
the execution that produced it, and the comparison with its parent. CQ-02
through CQ-04 made that record navigable and plannable in the workbench.
Three DH requirements expose where the fused record stops:

- **DH-01** wants a design revision to exist before execution, to carry
  several executions at different seeds, fidelities, and methods, and to
  let an assessment bind evidence to exact contract, profile, and policy
  snapshots. Today's attempt cannot exist without a run, cannot be
  executed twice, and its "comparison" is recomputed on read rather than
  an assessment anyone can cite.
- **DH-02** wants named references — a baseline, a review target — that
  resolve to exact revision and assessment identities with attributable
  acceptance, so an improvement in one requirement and a regression in
  another can be navigated without electing a winner.
- **DH-03** wants the fixed-question boundary kept and a deliberate
  question change recorded as an explicit amendment with actor and
  rationale, rather than today's "start a new root and lose the link".

This ADR proposes the smallest viable separation. It is a proposal: it does
not authorize replacing the accepted attempt semantics, building a new
store, or weakening any integrity check.

## Proposed decision

### 1. Three records, each with one responsibility

- **Design revision** (`avila.core/design-revision/v0.1-draft`): an
  immutable proposed state. Fields: revision id; `parent_revision_id` and
  `parent_record_sha256` exactly as attempts bind today; the fixed
  manifest and compiled snapshot identities; the canonical candidate
  state, its artifact and state digests, and the Core-derived typed
  change set; the recorded actor attribution (`created_by`: person,
  agent, or tool, as stated by the caller — a claim, never inferred);
  optional stated intent as inert text. A revision is valid the moment it
  is appended: no execution, verdict, or assessment required.
- **Assessment** (`avila.core/assessment/v0.1-draft`): binds one
  revision's identity to the evidence one run produced. Fields:
  assessment id; `revision_id` and `revision_record_sha256`; the
  run-attempt log row identity it cites; the contract, snapshot, policy,
  and qualification identities the verdicts were derived under; the
  four-state verdicts and exact margins verbatim from that run; and the
  parent-assessment comparison Core computes, with the same explicit
  unavailability reasons as today. One revision may have many
  assessments — a rerun under a new policy snapshot, a different seed, a
  different method each append another — and an assessment never rewrites
  an earlier one.
- **Named reference** (`avila.core/named-reference/v0.1-draft`): a name
  (`baseline`, `review-target`, or a project name) pointing at an exact
  revision id and, when the name claims a result, an exact assessment id.
  Moving or creating a reference appends a record carrying the name, the
  target identities, the actor attribution, stated rationale, and the
  superseded target. A reference is a claim about significance, not a
  verdict: accepting a baseline never changes a recorded PASS/FAIL/
  INCONCLUSIVE/NOT_EVALUATED.

### 2. Amendments for deliberate question changes

`avila.core/contract-amendment/v0.1-draft` links a new lineage root to the
root it supersedes when the manifest or compiled snapshot changed on
purpose: prior root identity, new fixed identities, actor attribution,
stated rationale, and a Core-derived summary of which contract elements'
identities changed. Comparisons that cross an amendment disclose the
amendment and withhold margin deltas whose unit or limit no longer
agrees — the existing unavailability machinery already states such
reasons; the amendment adds the *why* the boundary moved.

### 3. Storage: extend the log, don't replace it

Revisions, assessments, references, and amendments are new record kinds
in the existing JSONL campaign log, appended under the same lock, lineage
revalidation, and optional log-line signature as run-attempt rows. A
run-attempt row with an attempt record gains `revision_id` and
`assessment_id` members naming the records it is evidence for. The
existing `attempt` member stays accepted forever as the legacy form.

### 4. Migration from current attempt logs

A revision/assessment store is **derived** from the attempt records
already in a log: one attempt record maps to exactly one revision plus
exactly one assessment citing its row. Migration is a one-shot,
verifiable projection — for each attempt, emit the revision record
(identical candidate state, ancestry, and fixed identities) and the
assessment record (the row's verdicts and margins under the row's own
manifest/snapshot/policy identities), then leave the original rows
untouched. A migrated log validates if and only if every derived record
re-derives the attempt's identities; the migration tool reports any row
it could not project rather than dropping it silently. Whether migration
rewrites logs in place, writes a new file, or runs lazily at read time is
an unresolved choice below.

### 5. Shared-operation ownership

The runner owns all validation and derivation: revision append
revalidation (ancestry, digests, fixed identities), assessment binding,
comparison computation, reference resolution to exact identities, and
amendment linkage. New shared query operations — tentatively
`core_revision`, `core_assessment`, `core_reference`, and an extension of
`core_constellation` — project them read-only with the same
recorded-only verification boundary. The workbench and CLI remain
clients; neither parses log text, derives changes, nor compares margins.

## Acceptance examples

- **Unassessed revision:** append a revision for a proposed candidate
  with no run; the constellation shows it without a verdict, and no
  parent's PASS is inherited by display convention.
- **Two assessments, one revision:** run the same unchanged candidate
  twice; two assessment records cite two run rows; the revision's
  earlier assessment is not rewritten and both remain individually
  addressable.
- **Named baseline:** mark `baseline` at revision R assessment A; move
  it to revision S assessment B; both events and their actor attributions
  remain, and no verdict on either assessment changes.
- **Explicit amendment:** relax a requirement limit, append an amendment
  linking old root to new root; a cross-amendment comparison shows
  verdict transitions, names the amendment, and reports the changed
  margin's delta as unavailable with the stated reason.
- **Execution failure:** a run that fails mid-execution still appends its
  row; its assessment records `not_evaluated` verdicts and the failure
  findings, distinguishable from a technical FAIL.
- **Refused lineage:** duplicate revision id, missing parent, or a
  manifest change without an amendment record is refused before any
  capability executes — the existing CORE-X1201 path.

## Smallest viable choices considered

- **Extend the JSONL log (proposed)** versus a new store. The log already
  carries locking, revalidation, signature, and query machinery; DH-06's
  10,000-candidate workload has not been measured, so replacing storage
  now would answer a question nobody has asked. A transactional store
  remains gated on S-034's measured-need rule.
- **Derive comparison on read (keep)** versus storing comparison
  results. `core_attempt` already recomputes comparisons authoritatively;
  storing them would create a second copy that can drift. Assessments
  cite the run row; comparisons stay derived.
- **References as log records** versus a side file. Log records get
  signing and ordering for free; a side file would be a second source of
  truth.
- **Amendments as explicit records** versus inferring question changes
  from manifest digests. Inference cannot carry actor or rationale, and
  DH-03 requires both.

## Unresolved choices

Decided at acceptance:

- **Migration mechanics → lazy read-time projection.** An attempt row
  that cites no revision projects a *derived* revision (its id is the
  attempt id) and a *derived* assessment (same id) citing its own row.
  No logged byte changes; legacy logs gain the revision/assessment view
  without migration, and the derived identities are exactly what named
  references and revision parents may cite. The verification rule —
  every derived record re-derives the attempt's identities — holds
  because derivation copies fields verbatim under lineage validation.
- **Reference namespace → per-log.** Names resolve within one campaign
  log. Cross-log references would need identity rules not yet specified
  and are not admitted.
- **Revision existence without a run → `avila-core revision create`.**
  A non-run append path taking the candidate file, fixed manifest and
  compiled snapshot identities, optional parent revision and amendment,
  actor (`--by`), and optional intent. Signature policy matches run
  rows: appended unsigned unless `--runner-key` signs, and a supplied
  `--trust-root` verifies the parent line's signature. Unsigned appends
  are accepted; `created_by` remains a stated claim.
- **Multiple candidate inputs:** unchanged — future work.
- **Assessment of partial failure → no distinct flag.** The assessment
  binds the cited run row's verdicts verbatim; the row's status and
  findings already distinguish an execution failure's `not_evaluated`
  verdicts from a technical FAIL.

Additional decisions settled at acceptance:

- **Row envelope.** Non-run records append as
  `{"schema_version":"avila.core/log-record/v0.1-draft","recorded_at":…,
  "record_kind":<kind>,"record":{…},"signature"?:…}` under the same
  append lock, revalidation, and optional line signature as run rows.
  Run rows gain optional `revision_id`, `assessment_id`, and
  `amendment_id` members.
- **Assessment id equals its run's attempt id.** Every assessment —
  explicit or derived — is named by the attempt row that produced its
  evidence, so the assessment namespace inherits attempt-id uniqueness
  for free and a run row's `assessment_id` is self-describing.
- **Revision/assessment resolution space.** A revision id resolves to an
  explicit revision record or to a revision-less attempt row's derived
  revision. An explicit revision id must not equal any attempt id in
  the log, keeping resolution unambiguous.
- **Attempt↔revision edge agreement.** A run citing a revision is
  refused when the two parentage edges disagree: the attempt's parent
  must resolve to the revision's `parent_revision_id` (through the
  parent's own citation or its derived id), and a root attempt may cite
  only a root revision. Their `amendment_id` edges must likewise match
  exactly.
- **Amendment is an admission rule, not a history rule.** A *root*
  attempt whose `case_id` already has roots in the log under different
  fixed identities is refused (CORE-X1201) unless the row cites an
  amendment whose new identities match and whose superseded root is an
  earlier same-case root. History validation never refuses old logs for
  lacking amendments; it only checks that cited amendments exist
  earlier with matching identities. Revision roots carry no `case_id`
  and may cite an amendment voluntarily.
- **Amendment summary is manifest-derived.** `avila-core amend`
  requires both manifest files: the prior file's hash must equal the
  superseded root's recorded manifest identity (a stated file that
  disagrees is refused), and `changed_elements` is Core's typed diff of
  the two canonical manifests. The new compiled-snapshot identity is
  caller-supplied from the new contract's compile report.
- **Assessment stores verbatim verdicts.** The assessment record copies
  the cited run row's verdicts so the record is citable alone;
  validation requires the copy to equal the cited row exactly, so the
  duplication cannot drift. Comparisons are still derived on read.

## Boundary

Records carry actor attribution as stated claims; Core never infers
intent, causality, or review sufficiency. A baseline is never an input to
verdict derivation. Amendments disclose changed boundaries; they cannot
retroactively legitimize a verdict recorded under the old boundary. The
fixed-question refusal inside one lineage is unchanged. Nothing here
advances a product gate, resumes a paused experiment, or commits to an
implementation order beyond "specify before write operations exist".
