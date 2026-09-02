# ADR-0009: Staged agent review

**Status:** accepted 2026-09-02 (decision S-026)

## Context

The generative loop calls for a practical review stage between expensive
technical evaluation and the accountable person who may approve a result for
use. The compiler already represented accountable review as a capability with
an exact dossier and governance dispositions, but it could not distinguish a
person exercising authority from software routing work. Reusing the existing
review declaration without that distinction would either give an agent
`approve_for_use` authority or make its absence withhold an otherwise valid
technical `PASS`. Both collapse roles that Core is meant to keep separate.

## Decision

1. **Every review capability declares its reviewer role.** The registry value
   is either `accountable_person` or `agent`, and the compiled obligation keeps
   it. An accountable-person obligation is `pending_external_review`; an agent
   obligation is `pending_agent_review`.

2. **Authority is closed by role.** An accountable person may be offered
   `approve_for_use`, `reject_for_use`, `request_changes`, and `abstain`, but
   not `recommend_for_accountable_review`. An agent may only be offered
   `recommend_for_accountable_review`, `request_changes`, and `abstain`; the
   compiler refuses any agent capability that can approve or reject for use.
   An agent review must also carry nonempty, explicit instructions in the
   contract. These restrictions are compiled semantics, not UI convention.

3. **Only accountable review governs a technical `PASS`.** Campaign evaluation
   continues to withhold `PASS` while an accountable-person obligation is
   outstanding and still emits contradictory `FAIL` evidence immediately.
   Agent stages are routing work: whether pending or completed, they neither
   create nor suppress a technical verdict and can never fulfill an
   accountable-person obligation.

4. **The runner materializes the exact request but does not execute review.**
   After campaign evaluation it resolves each compiled review dossier to the
   evidence identifiers, digests, and media types actually present. The
   request also binds the compiled snapshot, campaign, step, role, permitted
   dispositions, policy, independence declaration, and instructions. It is
   `ready_for_review` only when the whole dossier is present; otherwise it is
   `awaiting_evidence` with the missing sources named. `request_sha256`
   content-identifies the request body.

5. **The first agent record is explicitly non-accountable.** Draft schema
   `avila.core/staged-review-record/v0.1-draft` embeds the exact request, the
   agent implementation digest, routing disposition, rationale, actions, and
   limitations. It is unsigned and marked `unverified`. It is not a campaign
   admission, a signed review fulfillment, or an approval. CASE-001 binds a
   deterministic scripted reviewer and practical policy; the script verifies
   its own and the candidate's dossier identities, the policy, request, and
   campaign digests, the instructions, and the authority limits before it
   emits a record.

## Boundary

Core binds the policy bytes but does not determine that their author is
legitimate or that an agent's practical rules are correct. The CASE-001 script
performs specimen consistency checks, not organization-scoped admission.
There is no accountable-person obligation or accountable decision in
CASE-001, so no candidate in the case is approved for use. A signed
accountable fulfillment record, eligibility and independence evaluation,
revocation, and admission remain deferred.

The staged record currently lives beside the claims document rather than in
it. Consequently it cannot influence campaign evaluation even structurally.
Later admission of agent records may preserve their routing history, but must
not grant them authority or make them prerequisites for a technical verdict.

## Consequences

- The review declarations in existing fixtures and CASE-000 explicitly name
  `accountable_person`; their compiled identities and downstream expectations
  are re-blessed.
- CASE-001 adds `practical-review`, a hash-bound policy and reviewer script,
  an exact request in every run report, and a committed record showing the
  failing reference candidate returned to the designer.
- The scripted designer reviews every transport finalist, records the result,
  returns `request_changes` candidates to the loop, and places only
  `recommend_for_accountable_review` candidates in the person queue. It never
  treats that queue as approval.
- The workbench distinguishes agent routing stages from accountable-person
  review and displays request readiness, dossier identity, and instructions.
