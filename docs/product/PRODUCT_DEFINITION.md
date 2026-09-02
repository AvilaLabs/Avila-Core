# Product definition

## One sentence

Avila Core turns a bounded technical question into a strict computational
oracle that people and agents can iterate against, plus a portable package
showing exactly what can—and cannot—be concluded.

## The product promise

A requester should be able to state:

> Establish whether requirement R remains satisfied for this system, under
> these inputs, assumptions, tolerances, uncertainty sources, methods, and
> qualification rules.

Core should then:

1. reject missing or structurally invalid information;
2. compile the contract into canonical, typed records and report semantic or
   policy findings without silently repairing the source;
3. show an admissible plan before expensive work begins;
4. select policy-admitted implementations of the needed capability types;
5. execute them in dependency order in the permitted environment;
6. preserve input identity, configuration, software, data, logs, outputs, and
   lineage;
7. admit only evidence that satisfies the declared policy and applicability
   boundary;
8. apply the named requirement semantics;
9. return `PASS`, `FAIL`, `INCONCLUSIVE`, or `NOT_EVALUATED`; and
10. export a package another party can inspect and independently verify.

The requester receives an answer boundary and evidence, not merely a successful
job or a polished report.

A Core verdict is a deterministic derivation from admitted records under a
named semantic profile. It is not, by itself, proof that every premise is true,
that a model is adequate outside its qualification, or that a regulator must
accept the conclusion. It does not require professional review to exist.

## Core 1.0 target

Version 1.0 is a long-term product threshold, not the first public release. It is
expected to require a narrow, named domain and all of the following:

### Contracting

- versioned contract schema and migration policy;
- versioned semantic profiles replayable for the evidence-retention period;
- exact canonical quantities, nominal quantity kinds, parameter domains,
  tolerances, uncertainty claims, assumptions, and requirement semantics that
  cannot be interpreted ambiguously;
- typed facts with named providers and source requirements;
- contract lifecycle separated from campaign execution state;
- reusable contract templates governed by owners;
- draft, acceptance, retirement, and amendment workflows; and
- preflight checks that distinguish missing inputs from scientific
  inadmissibility.

### Capability ecosystem

- documented SDK and conformance suite;
- at least one complete end-to-end capability chain in the supported domain;
- at least two independently maintained implementations of one material
  capability type;
- explicit qualification scope, validation evidence, limitations, owner, and
  lifecycle for every admitted capability;
- compatibility negotiation and deterministic provider selection; and
- support for external executables and services without rewriting them in Rust.

### Execution

- reproducible local execution and one controlled remote execution backend;
- private or air-gapped deployment path;
- resource limits, cancellation, resumability, and failure recovery;
- immutable execution receipts and content-addressed artifacts;
- deterministic planning, cache policy, and selective reruns; and
- isolation appropriate for untrusted capability packages.

### Evidence and verdicts

- complete machine-readable lineage from requirement to raw inputs;
- human-readable and agent-readable evidence package;
- independently usable verifier that does not require an Avila account;
- signed manifests, receipts, qualification records, and package roots;
- explicit numerical error and modeled uncertainty treatment;
- package-level admissibility that distinguishes a well-typed method from a
  sufficiently qualified method for this contract;
- `PASS`, `FAIL`, `INCONCLUSIVE`, and `NOT_EVALUATED` semantics tested against
  adversarial boundary cases;
- exact comparison in authoritative evaluation, with presentation rounding
  unable to change a verdict;
- change-impact analysis that invalidates every affected claim; and
- no implicit claim of regulatory acceptance.

### Agent and user workflow

- requester, designer agent, method owner, capability provider, and
  administrator roles;
- fixed requirement identity while an agent searches and explicit amendments
  when requirements change;
- machine-readable findings, exact margins, iteration logs, and selective
  reruns;
- an optional instructed connected-agent presentation gate that can return a
  candidate or present it to the user without altering Core's verdict;
- optional user acknowledgement kept separate from technical evaluation; and
- readable explanations of every verdict and limitation.

### Operations and commercial readiness

- organization identity, authorization, audit, retention, backup, and recovery;
- a defined vulnerability, incident, and update process;
- transparent license and evidence-ownership terms;
- an enterprise deployment and support model;
- contract-level or organization-level pricing that does not depend on verdict
  favorability or compute waste; and
- demonstrated repeat use without hidden founder-operated steps.

## The first supported product slice

The current hypothesis is a shutdown-dose evidence contract for an irradiated
component. It would compose transport, activation, dose, uncertainty, and
requirement evaluation. This is a discovery target, not a supported use case.

The slice is acceptable only if evidence shows:

- the question or benchmark is difficult and valuable enough;
- each method has an identifiable owner, validation evidence, and
  qualification path;
- the interfaces carry the information needed without semantic loss;
- the autonomous loop rejects known shortcuts and finds a candidate that
  satisfies every stated gate; and
- the end-to-end workflow reproduces suitable references under an independent
  verifier.

If those conditions fail, Core should retain the mechanism and choose a better
first contract.

## What ships before 1.0

Core should release useful layers as they become honest:

- schema and contract-editor previews;
- local plan validation;
- capability SDK and conformance tools;
- evidence package viewer and verifier;
- experimental adapters with explicit non-production labels;
- controlled reference campaigns; and
- private design-partner pilots.

Version numbers indicate interface maturity, not scientific applicability. A
stable executable still requires a sufficiently specified contract, methods,
data, and qualification records for each claimed use.

## Out of scope for 1.0

- broad coverage of every engineering discipline;
- an Avila replacement for established scientific solvers;
- automatic discovery of new physical laws or methods;
- regulatory certification supplied by Avila;
- universal conversion between arbitrary model formats;
- unattended execution of untrusted code without isolation;
- claims that all uncertainty can be bounded tightly or efficiently; and
- a public provider marketplace before private routing is proven.

## Product-quality test

Core is valuable only when a user can say:

> My agent explored the design space, Core told it exactly what failed, and the
> final candidate satisfied the stated gates with evidence I can independently
> verify.

Ease of use without that sentence is insufficient. Rigor without improved cycle
time is also insufficient.
