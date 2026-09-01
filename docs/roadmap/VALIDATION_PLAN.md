# Validation plan

Core is a strategic hypothesis with substantial technical risk. Validation must
precede horizontal platform construction.

## Hypotheses

### H1 — expensive resolution gap

For a recurring technical question, the principal avoidable cost lies in
assembling, checking, rerunning, and reviewing evidence across tools—not solely
in solver execution.

Evidence required:

- time study of at least three completed decisions;
- identifiable handoffs, rework, and review loops;
- buyer willingness to pay to shorten the full loop; and
- no existing deployed product that already solves it adequately.

### H2 — reusable evidence contract

A substantial portion of the question, method, policy, and evidence structure can
be reused across cases without hiding important case-specific judgment.

Evidence required:

- two cases instantiated from one template;
- explicit differences and eligibility rules;
- lower scoping effort on the second case; and
- professional agreement that the template remains valid.

### H3 — portable package improves review

An independent reviewer can understand and assess the campaign faster from the
Core package than from current scripts, folders, and reports.

Evidence required:

- blinded or structured comparison where feasible;
- reviewer time, questions, missing-evidence findings, and confidence rationale;
- successful offline integrity verification; and
- documented remaining manual judgment.

### H4 — provider-neutral capability boundary

A second provider can implement a capability type without changing the contract
or downstream consumer, while preserving explicit method differences.

Evidence required:

- independent implementation using public SDK documentation;
- shared conformance cases;
- deterministic provider selection under policy; and
- review of whether outputs are truly substitutable for the context of use.

### H5 — selective reruns create compounding speed

Core can correctly retain unaffected evidence and rerun the minimum admissible
subgraph after representative changes.

Evidence required:

- a catalog of input, method, data, policy, and review changes;
- expected invalidation graph signed off by method owners;
- no stale verdict surviving a relevant change; and
- measured cost and time saved against full rerun.

### H6 — outcome-independent economics work

Customers will pay and providers will participate when compensation depends on
policy-compliant resolution rather than favorable verdict or compute volume.

Evidence required:

- a paid contract permitting all three completion verdicts;
- provider commercial feedback;
- gross-margin model across verdict states; and
- no pressure to disguise failure or inconclusiveness.

### H7 — semantic compilation prevents material rework

The compiler catches consequential ambiguity, type mismatch, inadmissibility,
and stale-dependency errors before expensive execution, while two conforming
implementations reach the same result from the same immutable snapshot.

Evidence required:

- a defect corpus drawn from real contracts rather than invented syntax cases
  alone; the diagnostic-contract harness measures the synthetic half today,
  where every mechanically repairable mistake is fixed from the compiler's own
  repairs in one round;
- measured failures caught before execution and reviewer clarification avoided;
- complete boundary vectors for exact quantities, predicates, admission, verdict
  calculus, and invalidation;
- an independent implementation of the written rules that passes the normative
  vectors; and
- explicit separation between compiler correctness and scientific adequacy of
  the admitted methods.

## Interview groups

Interview each role separately before proposing the product:

- engineers and researchers who assemble the workflow;
- method owners who defend applicability;
- reviewers, quality staff, customers, or regulators who consume evidence;
- managers who fund delay and rework;
- software, data, lab, and consulting providers; and
- IT/security staff responsible for the execution environment.

Ask for the last real example, artifacts, elapsed time, failure, and approval
path. Avoid asking whether a hypothetical platform “sounds useful.”

## First-pilot discovery packet

Capture:

1. the exact decision and owner;
2. current process map and elapsed time at each handoff;
3. inputs, formats, tools, data, scripts, people, and licenses;
4. acceptance requirements and ambiguity;
5. uncertainty and numerical treatment;
6. review checklist and evidence actually requested;
7. known failure cases and prior rework;
8. security, export, retention, and deployment constraints;
9. total direct and delay cost; and
10. a representative but non-sensitive case for prototyping.

## Benchmark design

The reference suite must include:

- known acceptable and known unacceptable cases;
- a deliberately inconclusive boundary case;
- malformed and missing inputs;
- incompatible nominal quantity kinds, exact unit-scaling boundaries, malformed
  canonical decimals/rationals, duplicate keys, and non-normalized Unicode;
- applicability facts with missing, stale, and unauthorized sources;
- policy conflicts, inadmissible candidates, and deterministic selection ties;
- unqualified, expired, revoked, and substituted capabilities;
- stochastic repeatability and convergence cases where applicable;
- corrupted artifacts and invalid signatures;
- dependency changes that should and should not invalidate a verdict;
- process crashes, partial outputs, timeouts, and hostile archives; and
- an independent manual reconstruction of at least one verdict.

Core fails validation if only the happy path works.

## Success metrics

Primary:

- question-to-reviewed-verdict elapsed time;
- expensive runs prevented by semantic preflight and their false-block rate;
- total cost of resolution;
- professional labor hours;
- reviewer time and clarification loops;
- percentage of evidence reused correctly after change;
- number and severity of missing-evidence findings; and
- repeat contract purchase.

Guardrails:

- false `PASS` or `FAIL` count;
- disagreement between conforming implementations for the same semantic profile;
- stale verdicts after invalidating changes;
- policy bypasses;
- claims lacking complete lineage;
- unpriced Avila labor;
- data leaving the permitted boundary; and
- user interpretation of Core as certification or replacement for professional
  responsibility.

Any verdict that does not follow from its recorded semantic profile and admitted
evidence is a release-stopping event for the affected scope. A scientifically
inadequate premise that passed its declared policy is a separate, equally
serious qualification or governance failure; the distinction must be preserved
in the incident record rather than hidden by the compiler's correctness.

## Kill or pivot criteria

Reconsider the first vertical if:

- the painful work is mostly one-time and not reusable;
- existing platforms already satisfy the buyer at acceptable cost;
- the reviewer does not value the evidence package;
- tool licenses prevent practical execution or evidence portability;
- method interfaces lose essential semantics;
- professional qualification cannot be owned credibly;
- Avify-style bounds are too conservative or expensive for the chosen question;
- each case requires permanent bespoke consulting; or
- customers will not pay independently of a favorable verdict.

These findings would not necessarily invalidate the Core mechanism. They would
invalidate the selected wedge or business model.
