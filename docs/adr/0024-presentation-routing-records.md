# ADR-0024: Presentation-routing records

- Status: proposed 2026-09-19

## Context

A9 ratifies the smallest record in the admission family:

> If a presentation-routing record is attached, it binds the exact
> post-campaign dossier and an allowed disposition. Its absence or
> disposition never changes artifact admission.

R9 already exists in the compiler: a `ReviewDeclaration` declares the
gate's `presented_input_slots`, `decision_output_slot`, and closed
`allowed_dispositions` (`present_to_user | request_changes | abstain`);
a `ReviewPolicyBinding` pins the agent's eligibility policy by digest.
The `campaign.practical-review.pass/fail` fixtures exercise the gate.

What is missing is the *record the gate produces*: the document that
says which dossier was routed and which disposition came back. Two
`admission.*` fixture rows name it — `admission.A9` and
`admission.presentation.cannot-edit-artifact` — and without it a
routing made in a chat window and a routing made under a pinned agent
policy produce identical evidence: none.

The same posture as every record family: **the engine does not route —
it verifies the recorded routing.** The record binds exact digests; the
checks are mechanical; the record's presence, absence, or disposition
never touches an artifact, an admission, or a verdict.

## Proposed decision

### 1. The committed `staged_review_record` is the routing record

The record the gate produces already has a committed family:
`avila.core/staged-review-record/v0.1-draft` — the gate declaration's
`decision_role` is literally `core.presentation.routing-record`, and
`examples/cases/case-001-shield-search/reviews/reference.json` is a
committed instance bound in the package as a `staged_review_record`
document. The proposal names it the routing record because that is what
the contract's own vocabulary calls it; no second record type is
introduced.

The record binds everything A9 requires:

- `review_request` — the exact materialized request the reviewer
  answered, verbatim, with `request_sha256` digesting its canonical form
  with that member absent. A re-edited gate or a different dossier
  produces a different `request_sha256` and can never silently inherit
  an old routing.
- `disposition` — the recorded routing outcome; must be a member of the
  request's `allowed_dispositions`, whose closed vocabulary is enforced
  by the declaration, not by the record.
- `reviewer` — the stated attribution (`{ role, identity }`), a claim,
  exactly as ADR-0021 attestations carry.
- `record_sha256` — the document's digest over its canonical form with
  that member absent, the same rule `request_sha256` uses.
- `rationale`, `actions`, `limitations`, `attestation` — inert text;
  data, never instructions.

### 2. One check family, notice-severity, never a gate

When a package binds a `staged_review_record`, the runner checks it
against the materialized gates at campaign-evaluation time and the
verifier checks it independently:

- `review_request.request_sha256` must recompute over the embedded
  request *and* equal the materialized gate's — a record bound to a
  mutated dossier or a different campaign is quarantined
  (`CORE-X6501`): the *record* is marked invalid, and the artifact,
  admission state, and verdict it was attached to are byte-for-byte
  unchanged. A9 is explicit: a bad routing record invalidates the
  routing, never the evidence.
- `reviewer_eligibility_policy` must equal the gate's bound policy —
  a routing performed under a different policy is `CORE-X6501`.
- `disposition` must be a member of `allowed_dispositions` — `CORE-X6501`.
- `record_sha256` must recompute over the document — a record rewritten
  after binding is `CORE-X6501`.
- A record answering a `step_id` the contract declares no presentation
  gate for is `CORE-X6501` — a forged claim about a nonexistent review.
- A record answering a gate the contract *declares* but this campaign
  did not materialize (a rejected campaign produces none) is
  unresolvable: skipped without a finding, exactly as the verifier
  treats a binding no committed run carries — not forged, not checked.
- `CORE-X6502` — the record is structurally malformed (does not parse,
  missing bytes, a foreign schema version).

All findings are notices or quarantines on the *record*. An absent
record is a clean state — the surrounding workflow simply has no
recorded routing to show. A verified record marks its gate `recorded`
in the run report and the log line — the arrival a pending `respond_by`
deadline (ADR-0021 `CORE-X6401`) was waiting on; a quarantined record
does not resolve the deadline.

### 3. What the record is not

- **Not technical evidence.** The claims schema already says it:
  "Practical-review routing records are deliberately separate because
  they are not technical evidence and cannot alter a verdict." A
  routing record never enters `claims`, `campaign_sha256`, admission,
  or a verdict input.
- **Not a campaign state.** Routing is a surrounding agent workflow;
  SC-13 states and ADR-0021 transitions own campaign position. A
  `request_changes` disposition records that the reviewer asked — it
  does not move the campaign.
- **Not proof the reviewer is competent.** The record binds *what was
  routed and returned*, under *which pinned policy*. Whether the
  agent's judgment was sound is outside every check — exactly as a
  selection record proves the selection was recorded, not that it was
  wise.

## Boundary

- **Human review uses the same record.** The gate's `reviewer_role`
  vocabulary is `agent` today; a `human` reviewer is a vocabulary
  addition, not a second record type.
- **Multi-step dossiers** (several campaigns presented together) would
  need a `dossier` list; v1 binds one campaign report.
- **The `waiting`-gate deadline** (ADR-0021 `CORE-X6401`) records that
  no routing arrived in time; this record is the arrival itself.
- **No UI.** How a dossier is rendered for the reviewer is the
  surrounding application's concern; the record binds digests, not
  pixels.

## Implements

- ADR-0006 A9 (SC-11) and the R9 gate (SC-6).
- Fixture rows: `admission.A9`,
  `admission.presentation.cannot-edit-artifact`, and the record half of
  `admission.non-artifact-records`.

## Implementation review (for ratification)

Delivered in commit `008a4cf`. The committed `staged_review_record`
family already IS the routing record — its `decision_role.id` is
literally `core.presentation.routing-record` and case-001 ships a
committed instance — so the draft was amended to bind that family
rather than fork a parallel `routing_record` schema.
`case_run/routing.rs` checks the A9 bindings over package-bound
records: `request_sha256` self-consistency and materialized-gate
equality, `reviewer_eligibility_policy` equality, `allowed_dispositions`
membership, `record_sha256` self-consistency. `CORE-X6501` (semantic
mismatch) and `X6502` (structural failure) quarantine the record at
notice severity — the artifact, admission, and verdict stay
byte-identical (A9's own invariant). `respond_by` and `routing` are
excluded from `request_sha256`; a recorded answer resolves a pending
gate deadline while a quarantined one does not.

Divergences from the proposal text:

- The new `routing_record` document type the draft proposed was not
  created — the existing staged-review family was bound instead (§1–2
  amended).
- A record answering a declared-but-unmaterialized gate (a rejected
  campaign produces none) is skipped without a finding — "not forged,
  not checked," matching the verifier's posture.
- No CLI path mints a record; the surrounding agent workflow writes
  it and the engine verifies.

Ratification questions:

- Accept binding the existing `staged_review_record` family as the
  routing record rather than introducing a dedicated schema?
- Case-001's committed record now quarantines against today's
  materialization — it bound an older dossier. Accept that as honest
  history (the record is stale, not the check wrong)?
- Accept notice-severity quarantine (record fails, artifact survives)
  as the sole failure mode?
