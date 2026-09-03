# First-pilot discovery packet

**Candidate:** shutdown-dose evidence chain for an irradiated component  
**Status:** discovery hypothesis; not selected, supported, or qualified  
**Packet owner:** unassigned  
**Last update:** 2026-09-02

This packet turns the Stage 0 questions into one auditable decision record. The
current candidate comes from the product strategy; it is retained only if
benchmark, workflow, and user evidence support it. This packet may guide
product discovery, but no interview or professional review is a prerequisite
for engineering progress or a Core verdict.

Use coded identifiers and sanitized summaries here. Store detailed notes and
restricted artifacts in their approved system of record as described in the
[discovery information boundary](README.md#information-boundary).

## Internal pressure-test specimen

[CASE-000](../../examples/cases/case-000-actinv-aftermatter/README.md) binds a
synthetic ACTINV 1.0.1 → Aftermatter R0 chain, executes both computational
steps under verified execution receipts, and runs the generated claims through
the current Core compiler and campaign evaluator. It exists to expose the
smallest real semantic and evidence gaps: capability identity stops at an
executable digest, receipts are unsigned, categorical route states are not
requirement values, and qualification evidence is incomplete. Its two bounded
requirements now produce technical `PASS` without a review stage.

CASE-000 is not a completed decision loop, participant observation,
representative customer case, pilot selection, or evidence for H1–H6. It does
not replace any unknown field or open box below.

## Evidence notation

| State | Meaning |
| --- | --- |
| `observed` | Confirmed in an artifact, completed workflow, or direct measurement |
| `asserted` | Stated by a participant but not independently observed |
| `inferred` | Analyst interpretation that still requires confirmation |
| `unknown` | No adequate evidence recorded |

Every non-unknown answer names a source reference, participant role, and date.

## Candidate decision card

| Field | Current answer | State | Source / owner |
| --- | --- | --- | --- |
| Organization | Unanswered | unknown | — |
| Bounded technical question | Establish whether a shutdown-dose requirement remains satisfied for a declared irradiated component and boundary | inferred | Strategy candidate; requester confirmation required |
| Decision that follows | Unanswered | unknown | Decision owner required |
| Decision owner | Unanswered | unknown | — |
| Recurrence or case volume | Unanswered | unknown | Buyer/requester required |
| Cost of delay | Unanswered | unknown | Buyer required |
| Current end-to-end elapsed time | Unanswered | unknown | Completed-case time study required |
| Method owner for each material step | Unanswered | unknown | — |
| Evidence consumer or independent verifier | Unanswered | unknown | — |
| Optional agent practicality policy | Unanswered | unknown | Not required for Core use |
| Representative non-sensitive case | Unanswered | unknown | — |
| Deployment and data boundary | Unanswered | unknown | IT/security required |
| Buyer and budget authority | Unanswered | unknown | — |

## Optional interview register

Keep detailed notes outside this repository. Add one row per role-separated
conversation, even when several roles belong to the same organization.

| ID | Date | Participant role | Organization code | Completed case examined | Artifact references | Sanitized findings reference |
| --- | --- | --- | --- | --- | --- | --- |
| — | — | — | — | — | — | — |

Useful perspectives, when available:

- requester/engineer who assembles the work;
- method owner for each material method;
- independent verifier or evidence consumer;
- manager or buyer funding delay and rework;
- relevant software, data, laboratory, or consulting provider; and
- IT/security owner for the execution environment.

### Evidence-first interview protocol

Use the last completed case, not a hypothetical future platform, as the spine
of a 30–45 minute conversation.

1. **Boundary and handling:** confirm the participant's role, the information
   classification, what may be summarized, and the coded case identifier.
2. **Decision:** ask what exact question had to be answered, who owned the
   resulting decision, what outcome was reached, and what happened next.
3. **Timeline reconstruction:** walk from the first request to the evidenced
   answer. Record each handoff, queue, tool, input, output, rerun, and owner.
4. **Artifact walk-through:** ask the participant to identify the artifacts
   actually used. Record only approved coded references here.
5. **Failure and rework:** ask where ambiguity, missing evidence, tool mismatch,
   stale inputs, or clarification changed the elapsed time or result.
6. **Verification and presentation:** ask what had to be reconstructed and
   which practical concerns were checked outside the technical requirements.
7. **Economics:** separate hands-on labor, solver/compute cost, third-party cost,
   queue time, delay cost, and the cost of a wrong or late answer.
8. **Reuse and change:** ask what repeated from the previous case, what could
   have repeated but did not, and which changes forced a rerun.
9. **Commitment test:** ask whether a representative non-sensitive case,
   reference data, and bounded evaluation period can be made available. This
   helps product validation but does not gate repository work.

Role-specific probes:

- requester/engineer: scoping burden, manual glue, retries, and unavailable
  inputs;
- method owner: applicability, qualification evidence, uncertainty treatment,
  limitations, and invalidation authority;
- verifier/evidence consumer: evidence checklist, reconstruction effort,
  clarification loops, and unacceptable shortcuts;
- buyer/manager: delay cost, repeat frequency, budget authority, and willingness
  to pay for a non-favorable but admissibly resolved result;
- provider: interface stability, licensing, support boundary, and
  outcome-independent compensation; and
- IT/security: data movement, identity, secrets, isolation, retention, export,
  and deployment restrictions.

Avoid asking whether Core “sounds useful.” Close by reading back the observed
workflow and explicitly separating participant assertions from analyst
inference.

## Completed decision-loop register

Record at least three completed decisions. A hypothetical future workflow does
not establish H1.

| Case ID | Decision date | Question class | Outcome | Elapsed time | Labor | Compute/direct cost | Review loops | Rework/defect | Source |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| — | — | — | — | — | — | — | — | — | — |
| — | — | — | — | — | — | — | — | — | — |
| — | — | — | — | — | — | — | — | — | — |

For each case, distinguish queue time, hands-on time, solver runtime, review
time, clarification delay, and rerun time. Record missing or rejected evidence,
not only successful handoffs.

## Current-state workflow map

The hypothesized chain is geometry/materials/exposure through transport,
activation/inventory, shutdown-dose, uncertainty treatment, requirement
evaluation, optional practicality routing, and presentation. Replace that hypothesis with the evidence-backed
current process.

| Step | Named owner | Inputs and identity | Tool/method/version | Output and unit/uncertainty | Environment/license | Touch time | Queue time | Handoff/routing | Known failures |
| --- | --- | --- | --- | --- | --- | ---: | ---: | --- | --- |
| Contract/question framing | Unanswered | Unanswered | Human process | Unanswered | Unanswered | — | — | Unanswered | Unanswered |
| Transport | Unanswered | Unanswered | Candidate only | Unanswered | Unanswered | — | — | Unanswered | Unanswered |
| Activation/inventory | Unanswered | Unanswered | Candidate only | Unanswered | Unanswered | — | — | Unanswered | Unanswered |
| Shutdown-dose method | Unanswered | Unanswered | Candidate only | Unanswered | Unanswered | — | — | Unanswered | Unanswered |
| Uncertainty/bound treatment | Unanswered | Unanswered | Candidate only | Unanswered | Unanswered | — | — | Unanswered | Unanswered |
| Requirement evaluation | Unanswered | Unanswered | Current manual/software process | Unanswered | Unanswered | — | — | Unanswered | Unanswered |
| Optional practicality/presentation gate | Unanswered | Unanswered | Connected agent or omitted | Routing record or direct presentation | Unanswered | — | — | Unanswered | Unanswered |

Add, remove, or split steps to match reality. Project names such as NCTForge,
ACTINV, or Avify are possible implementations, not evidence that the step is
required, qualified, licensed, or owned.

## Contract boundary

### Question and decision

- Exact bounded question: unknown.
- Decision enabled by the answer: unknown.
- User or system that consumes the technical result: unknown.
- Allowed completion states (`PASS`, `FAIL`, permitted `INCONCLUSIVE`): unknown.
- Conditions that must yield `NOT_EVALUATED`: unknown.

### Requirements and assumptions

| ID | Requirement/assumption | Owner | Exact limit/boundary | Allowed uncertainty basis | Optional presentation check | Source |
| --- | --- | --- | --- | --- | --- | --- |
| — | — | — | — | — | — | — |

### Inputs and interfaces

| Interface | Authoritative owner | Format/media | Quantity kind and unit | Uncertainty representation | Identity/provenance | Missing/stale behavior |
| --- | --- | --- | --- | --- | --- | --- |
| Geometry | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |
| Materials/composition | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |
| Exposure/history | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |
| Tolerances | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |
| Requirement | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |

## Method and validation matrix

| Method/capability type | Named owner | Context of use | Qualification/validation evidence | Explicit limitations | Candidate implementations |
| --- | --- | --- | --- | --- | --- |
| Transport | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |
| Activation/inventory | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |
| Shutdown-dose | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |
| Uncertainty/bound | Unanswered | Unanswered | Unanswered | Unanswered | Unanswered |
| Requirement evaluation | Unanswered | Unanswered | Unanswered | Unanswered | Core kernel is only a software candidate |

Core cannot invent ownership or validation evidence; those records must be
explicit. It does not require the owner to hold a professional credential.

## Independent-verification and presentation inventory

Record what another implementation must verify and which optional practical
checks should occur before a finalist is presented.

| Evidence item/check | Required? | Current producer | Identity/version requirement | Verification or routing action | Common deficiency | Source |
| --- | --- | --- | --- | --- | --- | --- |
| Raw/input manifest | Unknown | Unknown | Unknown | Unknown | Unknown | — |
| Method and configuration | Unknown | Unknown | Unknown | Unknown | Unknown | — |
| Qualification/applicability basis | Unknown | Unknown | Unknown | Unknown | Unknown | — |
| Execution logs/receipts | Unknown | Unknown | Unknown | Unknown | Unknown | — |
| Numerical/uncertainty treatment | Unknown | Unknown | Unknown | Unknown | Unknown | — |
| Requirement comparison | Unknown | Unknown | Unknown | Unknown | Unknown | — |
| Independent reconstruction/check | Unknown | Unknown | Unknown | Unknown | Unknown | — |
| Optional user acknowledgement | No by default | Unknown | Unknown | Record separately from verdict | Conflated with technical PASS | — |

## Data, deployment, security, and licensing

| Question | Answer/state | Source/owner |
| --- | --- | --- |
| What data may leave the customer environment? | unknown | IT/security |
| Is an air-gapped or on-premises path required? | unknown | IT/security |
| What export-control or retention rules apply? | unknown | Legal/security |
| Which tools permit automated invocation? | unknown | Tool owner/legal |
| Which artifacts, manifests, adapters, and derived evidence may be redistributed? | unknown | Tool owner/legal |
| What credentials or license servers are required at runtime? | unknown | IT/tool owner |
| What isolation is required for each adapter? | unknown | Security |
| What information may appear in a portable package? | unknown | Data owner |

## Change and invalidation catalog

The method owner and validation records must state which changes invalidate
which outputs. Do not infer scientific non-dependence from file similarity.

| Change class | Example from completed work | Evidence affected | Full rerun today? | Candidate minimal rerun | Authority for reuse rule | Source |
| --- | --- | --- | --- | --- | --- | --- |
| Input bytes | Unanswered | Unanswered | Unknown | Unknown | Unanswered | — |
| Input metadata | Unanswered | Unanswered | Unknown | Unknown | Unanswered | — |
| Parameter/tolerance | Unanswered | Unanswered | Unknown | Unknown | Unanswered | — |
| Method/software/data version | Unanswered | Unanswered | Unknown | Unknown | Unanswered | — |
| Environment/hardware | Unanswered | Unanswered | Unknown | Unknown | Unanswered | — |
| Qualification/policy | Unanswered | Unanswered | Unknown | Unknown | Unanswered | — |
| Agent policy or presentation change | Unanswered | Routing only | No technical rerun | Re-route presentation | User/organization policy | — |

## Reference-case and benchmark design

The candidate needs one representative non-sensitive case plus negative and
boundary cases fixed by the requirement owner and reference records.

| Case | Required state | Reference authority | Available? | Acceptance comparison |
| --- | --- | --- | --- | --- |
| Known acceptable | `PASS` within the declared boundary | Unanswered | No evidence recorded | Exact agreed result/bound |
| Known unacceptable | `FAIL` within the declared boundary | Unanswered | No evidence recorded | Exact agreed result/bound |
| Boundary/insufficient evidence | `INCONCLUSIVE` or `NOT_EVALUATED` as agreed | Unanswered | No evidence recorded | Agreed rule and reason |
| Malformed or missing input | Fail closed | Unanswered | No evidence recorded | Agreed diagnostic/admission state |
| Relevant change | Correct invalidation and rerun | Unanswered | No evidence recorded | Declared and tested impact graph |

Baseline and pilot measurements:

- question-to-evidenced-candidate elapsed time;
- hands-on setup and maintenance labor;
- compute and third-party direct cost;
- agent iteration and clarification loops;
- missing-evidence findings and defect/rework events;
- correctly reused evidence after change; and
- every false `PASS`, false `FAIL`, stale verdict, or policy bypass.

## Hypothesis evidence board

| Hypothesis | State | Evidence required for this candidate | Recorded evidence | Decision |
| --- | --- | --- | --- | --- |
| H1 expensive resolution gap | untested | Three completed-loop time studies and buyer willingness | None | pending |
| H2 reusable evidence contract | untested | Two cases from one governed template with lower second-case scoping effort | None | pending |
| H3 portable package improves independent verification | untested | Second implementation, offline integrity check, and remaining limitation record | None | pending |
| H4 provider-neutral boundary | untested | Second implementation and conformance evidence | None | later-stage; note interface blockers now |
| H5 selective reruns compound speed | untested | Signed expected invalidation graph and measured savings | None | later-stage; capture changes now |
| H6 outcome-independent economics | untested | Buyer/provider feedback and a contract permitting non-PASS outcomes | None | pending |
| H7 semantic compilation prevents rework | synthetic evidence only | Real contract defects caught before execution and clarification avoided | Mutation harness only | pending real corpus |

## Candidate decision

No candidate advances because the architecture is interesting. Record one of
the following only after the evidence board is reviewed.

- `advance`: H1–H3 have credible support; owners, reference data, licenses, and
  deployment boundary are available; no kill criterion is active.
- `hold`: a bounded missing item has an owner and decision date.
- `pivot`: the mechanism may remain useful, but this question or chain fails a
  wedge criterion.
- `stop`: the resolution gap or feasible responsibility boundary is not present.

Current decision: **hold — discovery has not started**.

Decision record fields:

- decision and date: pending;
- participants and roles: pending;
- evidence references reviewed: pending;
- strongest supporting evidence: pending;
- strongest falsifier or unresolved risk: pending;
- next bounded action, owner, and due date: pending.

## Stage 0 exit commitment

All boxes remain open until a source reference is recorded.

- [ ] One bounded benchmark and candidate space are fixed.
- [ ] A named requester owns the requirements and stopping rule.
- [ ] Every material method has a named owner, applicability boundary, and limitations.
- [ ] At least one known shortcut is encoded as a negative case.
- [ ] A representative non-sensitive case is available.
- [ ] The deployment and data boundary is agreed.
- [ ] Candidate tool licensing is feasible.
- [ ] Baseline measurement can be performed.
- [ ] The final or exhaustion package can be replayed independently by software.
- [ ] The pilot is non-safety-critical and does not imply qualification or
      certification.

Only after these commitments should the packet produce a vertical engineering
backlog. That backlog begins with the smallest bound plan, adapters, controlled
receipts, package identity, offline package, and typed invalidation needed by
this exact case.
