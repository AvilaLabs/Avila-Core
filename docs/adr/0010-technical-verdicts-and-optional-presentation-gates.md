# ADR-0010: Technical verdicts and optional presentation gates

**Status:** accepted 2026-09-02 (decision S-027)
**Supersedes:** ADR-0009 and the review-gating portions of ADR-0006

## Context

Core's primary use is an autonomous generative loop. A person may ask a
connected AI agent to design something, the agent may explore thousands of
candidates through Core, and Core must be able to compile and evaluate each
candidate without waiting for a professional reviewer.

The previous model conflated two different concerns:

1. whether admitted evidence satisfies the contract's technical requirements;
2. whether a candidate should be shown to the user after a practical sanity
   check.

That conflation made an absent accountable person suppress an otherwise valid
technical `PASS`. It contradicted the product's purpose and made every campaign
depend on access to a professional reviewer.

A practical check is still useful. For example, a technically compliant suit
design may place a lithium-ion battery near the wearer's crotch. Core will only
catch that issue if the contract or a capability models it. A connected agent
can be given explicit instructions to inspect such practical concerns after the
technical campaign, return a candidate to the search loop, or present it to the
user.

## Decision

1. **Technical verdicts are review-independent.** `PASS`, `FAIL`,
   `INCONCLUSIVE`, and `NOT_EVALUATED` are derived only from the compiled
   requirements, admitted claims, qualification position, and kernel rules.
   Review records, acknowledgements, and presentation policy are not kernel
   inputs and cannot create, suppress, or alter a verdict.

2. **Core never requires a professional reviewer.** A contract with no agent
   integration compiles, runs, and evaluates normally. No human or professional
   credential is required to advance a campaign or establish technical `PASS`.
   External law, regulation, or an organization's own process may separately
   require a person, but that is not a Core verdict rule and Core does not imply
   such a requirement by default.

3. **A connected-agent practicality check is an optional presentation gate.**
   A contract may configure one by binding an agent role, exact dossier, policy
   identity, closed routing dispositions, and explicit instructions. Successful
   compilation records a `presentation_gate` in state `awaiting_agent`. Omitting
   it is valid and leaves no pending state in Core.

4. **The gate controls routing, not authority.** Its dispositions are
   `present_to_user`, `request_changes`, and `abstain`. `request_changes` sends
   the candidate back to the surrounding design loop. `present_to_user` means
   only that the candidate passed the authored practical instructions. It is
   not approval, certification, professional judgment, or a technical verdict.

5. **The technical campaign runs first.** The runner materializes the gate's
   exact, content-identified dossier after campaign evaluation. The agent reads
   Core's recorded statuses and reasons, refuses presentation unless the
   configured policy allows it, and never edits or re-derives those statuses.
   The routing record remains outside evidence claims and campaign admission.

6. **User acknowledgement is separate.** A product may ask the user to
   acknowledge limitations before acting on or exporting a result. Such an
   acknowledgement is a presentation or product-policy event, not professional
   review and not a prerequisite for the technical verdict.

7. **Qualification remains evidence-scoped.** A qualification envelope can
   constrain where a method's evidence may support a bounded verdict. It is
   established by records and validation evidence under the named policy, not
   by a universal professional-review step.

## Consequences

- Evidence claims no longer contain review decisions.
- Campaign admission and the kernel no longer accept review state as input.
- `not_evaluated.review_pending`, `reviews_outstanding`, and review-attestation
  verdict fields are removed from the executable profile.
- The only compiled reviewer role in this slice is `agent`; compiled output
  calls the result a `presentation_gate`, not a review obligation.
- At adoption, CASE-000 produced two technical `PASS` verdicts with no review
  stage. Revision 3 later added an independently evaluated categorical route
  requirement; its `FAIL` likewise needs no review stage to exist.
- CASE-001 demonstrates the optional path: Core evaluates first, then a
  hash-bound instructed agent returns the failing reference candidate to the
  designer. A clean candidate in the agent tests is routed to the user.
- Future human acknowledgement or organization-specific approval features must
  live outside the technical verdict and must not reintroduce a universal
  professional dependency.

## Acceptance checks

- The same claims produce the same verdict whether a presentation gate is
  absent, awaiting an agent, or has a routing record.
- Removing a configured presentation gate changes compiled workflow identity
  but does not weaken or strengthen any technical requirement.
- An agent cannot emit a disposition outside the closed routing set.
- No claims schema, campaign report, or kernel case contains a review decision.
- Documentation and roadmap gates do not require professional review to build,
  test, or advance Core.
