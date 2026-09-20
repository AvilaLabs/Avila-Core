# ADR-0021: State-transition records and actor attestations

- Status: proposed 2026-09-19

## Context

SC-13 ratifies two closed state vocabularies and the rule that governs
them:

- Campaign states: `planned | running | blocked | completed | cancelled |
  superseded | invalidated`.
- Step states: `pending | reused | staged | running | collecting |
  validating | admitted | quarantined | failed | skipped`.
- Every transition is an immutable record by an authorized actor.
  Resumption is a new run over the same bound plan; admitted current work
  is reused under SC-12. An amendment supersedes the campaign and cancels
  in-flight steps with receipts.

SC-9.1 ratifies the contract status vocabulary (`draft | in_review |
approved | retired`) and SC-9.4 the `supersedes` edge for semantic edits
to a non-draft contract. SC-14 ratifies that Core compiles requirements
for *attributable records* — contract attestations over an exact digest —
and that software may verify key-to-role assertions for technical records
under a named trust policy, without ever asserting the asserted fact is
true.

None of the record machinery exists. Today a contract carries a `status`
label its owner may set to anything; a campaign's position in its
lifecycle is invisible — a run produces an attempt row, but whether the
campaign is running, blocked, or completed lives nowhere. A status change,
an approval, a step moved by the wrong actor, and a deadline that lapsed
all produce identical evidence: none. Fourteen `lifecycle.*`,
`campaign.*`, and `authority.*` fixture rows name the gap.

This ADR proposes the smallest honest slice, on the same posture as
provider-selection records (ADR-0020): **the engine does not perform
transitions — it verifies recorded transitions.** A transition is a
producer record, digest-bound, checked for legality and attribution
against the named trust policy. Core never decides whether a campaign
*should* move; it decides whether the recorded move is one the vocabulary
permits and an authorized actor signed.

## Proposed decision

### 1. An `attestation` record binds an actor to a statement

`avila.core/attestation/v0.1-draft`: the authority primitive SC-14 names.
Fields:

- `attestation_id`, `schema_version`, `semantic_profile`.
- `subject`: the digest-bound target the statement is about — a contract
  `id@rev` plus its document digest, a transition record digest, or a
  package manifest digest.
- `statement`: a closed vocabulary (`approves`, `authors`, `reviews`,
  `waives`, `rescinds`) plus inert text — the engine checks the
  vocabulary, never the prose.
- `actor`: `{ actor_id, actor_kind }` where `actor_kind` is
  `person | agent | tool` — a stated attribution, exactly as ADR-0019
  treats `created_by`: a claim, never inferred.
- `role`: the technical role the actor asserts for this statement — the
  same closed vocabulary the trust root names (`requester`, `runner`)
  extended by one new entry: `policy_owner`.
- `signature`: a signature document over the attestation's bound bytes,
  verified under the supplied trust root.

An attestation never asserts that its statement is true. It asserts that
a listed key, in a listed role, signed these exact bytes. A transition
that names an attestation is only as authorized as the attestation it
names — and the trust root is only as trustworthy as the organization
that maintains it. Core establishes attribution and policy conformance,
not honesty.

### 2. A `state_transition` record moves one subject

`avila.core/state-transition/v0.1-draft`: one immutable record per move.
Fields:

- `transition_id`, `schema_version`, `semantic_profile`.
- `subject`: `{ kind, identity }` where `kind` is `contract | campaign |
  step` and `identity` is the subject's bound identity — the contract
  `id@rev` plus its source digest, or the campaign/step identity the
  package manifest names.
- `from_state`, `to_state`: members of the subject kind's closed
  vocabulary — a contract names contract statuses; a campaign names
  campaign states; a step names step states. Cross-vocabulary values are
  structurally impossible, exactly as campaign states cannot appear on a
  contract today.
- `actor_attestation`: the identity of the `attestation` record under
  which the actor signs — a record the package carries, never a
  prose name.
- `at`: the recorded instant; `rationale`: inert text.
- `step_effects`: optional per-step effects the transition records —
  which steps it cancels, which it leaves running. An amendment-driven
  campaign supersession uses this to name the in-flight steps it
  cancelled, each keeping its receipts.

The record appends to the campaign log under the same lock as attempts,
revisions, and named references — one JSONL history per campaign.

### 3. A closed legality table, checked mechanically

A transition is legal only if `(from_state, to_state)` appears in the
subject kind's table. The campaign table:

| from | to | meaning |
| --- | --- | --- |
| `planned` | `running`, `cancelled` | execution begins or the campaign is abandoned before start |
| `running` | `blocked`, `completed`, `cancelled`, `superseded` | progress halts, delivery assesses complete, abandoned, or an amendment supersedes |
| `blocked` | `running`, `cancelled`, `superseded` | the blocker clears, the campaign is abandoned, or an amendment supersedes |
| `completed` | `invalidated` | later evidence invalidates what completed |
| `cancelled` | — | terminal |
| `superseded` | — | terminal |
| `invalidated` | — | terminal |

`completed → running` is illegal: a completed campaign does not reopen; a
new run under a new campaign lineage is the honest record. Contract
status moves follow SC-9: `draft → in_review`, `in_review → draft |
approved`, `approved → retired`, and a semantic edit to a non-draft
produces a *new* draft with a `supersedes` edge — never an in-place move.
`retired` is terminal.

Step states are *derived, never recorded*: a step's state is computed
from its receipts, transitions, and the run log — `pending` until bound,
`reused`/`staged`/`running`/`collecting`/`validating` through execution,
`admitted`/`quarantined` at claim admission, `failed` on receipt failure,
`skipped` on dependency block. A `step` transition record exists only for
moves evidence cannot derive — cancellation of an in-flight step, which
produces `cancelled` while the step's receipts remain.

### 4. Actor authorization is per subject-kind, per transition class

Each table entry names the role its actor attestation must carry:
requester-signed for contract status moves and campaign
`planned→running`, `running→blocked`, `running→cancelled`; the bound
`policy_owner` attestation for `completed→invalidated`; a runner key for
system-recorded effects (step cancellation under supersession). A
transition whose attestation names the wrong role — a provider key
marking a step `admitted` — is refused exactly as an unsigned one is.

### 5. `CORE-X6xxx` names campaign-state findings

- `CORE-X6401` — a recorded deadline lapsed with no transition: the
  record shows a `waiting` gate past its `respond_by`; the runner emits
  the finding and produces **no decision** — an escalation is an event to
  record, never a verdict or a move the engine invented.
- `CORE-X6402` — an illegal transition was recorded (`completed→running`,
  a contract moved `approved→draft`); the campaign log entry is refused.
- `CORE-X6403` — the actor attestation is missing, unsigned, or names the
  wrong role for the transition class.
- `CORE-X6404` — a resumption run's reuse plan disagrees with the
  recorded state: steps recorded `admitted` were re-executed, or a step
  recorded `running` was silently re-run without a transition.

### 6. Amendment linkage

An ADR-0019 `contract-amendment` record that names a campaign's bound
contract supersedes the campaign: the runner records
`running→superseded` (or `blocked→superseded`) with `step_effects`
cancelling every in-flight step; cancelled steps keep their receipts.
Amendments that name no live campaign only supersede the contract.

## Boundary

- **Contract templates** (SC-9.2/9.5, `contract_template` documents and
  eligibility) are a different machinery — instantiation metadata, not a
  state move — and stay gated on their own design.
- **Presentation-routing records** (A9) bind a dossier to a gate; the
  `waiting`-gate deadline in clause 5 records expiry only — the routing
  record itself is separate.
- **Organization-policy attestations** (approvals spanning contracts)
  need the ADR-0020-deferred org-policy document first.
- **The legality table is deliberately small.** Timeouts, retries, and
  multi-actor approvals are client workflows that append transitions;
  the table checks legality, never orchestrates.
- **No inference.** An absent transition means the state is what the last
  record says — never a guess. A crashed runner leaves the campaign
  `running`; resumption is a new run, not a state invention.

## Implements

- ADR-0006 SC-13 (campaign and step state), SC-14 (attestations and
  actor attribution), SC-9.1/9.4 (contract status and supersession).
- Fixture rows: `lifecycle.submit-*`, `lifecycle.retired-*`,
  `campaign.illegal-transition`, `campaign.step.moved-by-wrong-actor`,
  `campaign.resume.new-run-reuses`, `campaign.human.deadline-escalation`,
  `campaign.amend-in-flight`, and the `authority.*` family.

## Implementation review (for ratification)

Delivered in commit `ea975e8`. The attestation record binds a
key+role to a digest-bound subject; the `state_transition` record
moves contract/campaign/step subjects under a closed legality table;
step states are derived from receipts, never recorded. `KeyRole`
gained `policy_owner`. `CORE-X6401` (deadline lapse), `X6402`
(illegal transition), `X6403` (wrong-role actor), and `X6404`
(resume/reuse disagreement) are emitted at the runner's record-load
boundary; campaign JSONL logs carry transitions and attestations with
optional per-line signatures verified under the supplied trust root.
`avila-core attest`/`transition` mint both record kinds.

Divergences from the proposal text: none structural — the model,
vocabularies, and legality table match the draft. `Attestation`
gained an optional `target` field under ADR-0023 (a second identity a
statement binds), schema-amended there.

Ratification questions:

- Is the deliberately small legality table the right scope — are
  timeouts/retries/multi-actor approvals correctly left to client
  workflows appending transitions rather than to the engine?
- Is `policy_owner` acceptable as a trust-root role (ADR-0023 floors
  verify under it)?
- Is the amend-in-flight semantics right — a superseding campaign
  amendment cancels in-flight steps while their committed receipts
  remain valid evidence?
