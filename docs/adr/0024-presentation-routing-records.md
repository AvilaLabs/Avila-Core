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

### 1. A `routing_record` document binds dossier and disposition

`avila.core/routing-record/v0.1-draft`, a package document. Fields:

- `record_id`, `schema_version`, `semantic_profile`.
- `gate`: `{ step_id, review_declaration_sha256 }` — the exact compiled
  gate the record answers, pinned by digest so a re-edited gate can
  never silently inherit an old routing.
- `dossier`: `{ campaign_report_sha256, claims_sha256 }` — the exact
  post-campaign dossier digests. The dossier is what the reviewer saw;
  binding it by digest is what makes a mutated dossier detectable.
- `disposition`: one member of the gate's `allowed_dispositions` — the
  closed vocabulary is enforced by the declaration, not by the record.
- `agent_policy`: `{ policy_id, sha256 }` — the pinned agent
  eligibility policy the routing was performed under, matching the
  `ReviewPolicyBinding`'s `reviewer_eligibility_policy`.
- `actor`: the stated attribution (`{ actor_id, actor_kind }`), a
  claim, exactly as ADR-0021 attestations carry.
- `at`: the recorded instant; `rationale`: inert text.

### 2. One check, notice-severity, never a gate

When a package carries a `routing_record`, the verifier and the runner
check it against the committed material:

- `dossier.campaign_report_sha256` must equal the committed campaign
  report's digest — a record bound to a mutated dossier is quarantined
  (`CORE-X6501`): the *record* is marked invalid, and the artifact,
  admission state, and verdict it was attached to are byte-for-byte
  unchanged. A9 is explicit: a bad routing record invalidates the
  routing, never the evidence.
- `disposition` must be a member of the gate's `allowed_dispositions` —
  a disposition the declaration did not allow is `CORE-X6501` again.
- `gate.review_declaration_sha256` must equal the compiled gate's
  digest, and `agent_policy.sha256` must equal the bound policy's — a
  record answering a different gate or performed under a different
  policy is `CORE-X6501`.
- `CORE-X6502` — the record is structurally malformed (unknown fields,
  a missing digest, a non-canonical form).

All findings are notices or quarantines on the *record*. An absent
record is a clean state — the surrounding workflow simply has no
recorded routing to show.

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
