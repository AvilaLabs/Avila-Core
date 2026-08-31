# Product definition

## One sentence

Avila Core turns a bounded technical question into a governed computational
campaign and a portable package showing exactly what can—and cannot—be
concluded.

## The product promise

A requester should be able to state:

> Establish whether requirement R remains satisfied for this system, under
> these inputs, assumptions, tolerances, uncertainty sources, methods, and review
> rules.

Core should then:

1. reject missing or structurally invalid information;
2. show an admissible plan before expensive work begins;
3. select approved implementations of the needed capability types;
4. execute them in dependency order in the permitted environment;
5. preserve input identity, configuration, software, data, logs, outputs, and
   lineage;
6. apply the declared evidence policy and requirement semantics;
7. return `PASS`, `FAIL`, or `INCONCLUSIVE`; and
8. export a package another party can inspect and independently verify.

The requester receives an answer boundary and evidence, not merely a successful
job or a polished report.

## Core 1.0 target

Version 1.0 is a long-term product threshold, not the first public release. It is
expected to require a narrow, named domain and all of the following:

### Contracting

- versioned contract schema and migration policy;
- units, parameter domains, tolerances, uncertainty sets, assumptions, and
  requirement semantics that cannot be interpreted ambiguously;
- reusable contract templates governed by owners;
- draft, review, approval, retirement, and amendment workflows; and
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
- human-readable review package;
- independently usable verifier that does not require an Avila account;
- signed manifests, receipts, reviews, and package roots;
- explicit numerical error and modeled uncertainty treatment;
- `PASS`, `FAIL`, `INCONCLUSIVE`, and `NOT_EVALUATED` semantics tested against
  adversarial boundary cases;
- change-impact analysis that invalidates every affected claim; and
- no implicit claim of regulatory acceptance.

### Professional workflow

- requester, method owner, capability provider, reviewer, and administrator roles;
- separation of authoring, execution, review, and approval where policy requires;
- annotations, requests for information, rejected evidence, and countersignature;
- readable explanations of each verdict and limitation; and
- measured acceptance by independent professionals in the supported domain.

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

The slice is acceptable only if a domain professional confirms:

- the question is common and expensive enough;
- each method has an identifiable owner and qualification path;
- the interfaces carry the information needed without semantic loss;
- a reviewer values the resulting package; and
- the end-to-end workflow can be validated against suitable references.

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
stable executable still requires a qualified contract, methods, data, and review
policy for each use.

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

Core is valuable only when a professional can say:

> I reached the reviewable answer materially faster and at lower total cost,
> without giving up the evidence or judgment my decision requires.

Ease of use without that sentence is insufficient. Rigor without improved cycle
time is also insufficient.

