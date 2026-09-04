# Evidence model

**Status:** conceptual model plus minimal draft Rust records, an offline
case-package integrity slice, an execution-receipt record verified from bytes
(ADR-0007), a narrow package-declared external-checker boundary (ADR-0011), and
an unsigned optional agent presentation-routing record (ADR-0010).

## Principle

Core does not make an answer trustworthy by putting it in a database. It makes
the chain inspectable: what was claimed, by which method, from which immutable
inputs, under which policy and applicability boundary, and with which
limitations.

## Evidence graph

Every material object is immutable and content-addressed. Typed relationships
form a directed acyclic graph:

```text
contract ───────────────┐
input data ─────────────┼──► invocation ─► execution receipt ─► output
capability package ─────┤                                      │
method qualification ──┘                                      ▼
                                                        derived evidence
policy + requirement ──────────────────────────────────────────┤
                                                               ▼
                                                     requirement verdict
                                                               │
                                                               ▼
                                                        package root
                                                               │
                                                               ▼ optional
                                                   agent presentation routing
```

An edge states a specific relationship such as `used`, `generated_by`,
`qualified_by`, `derived_from`, `evaluates`, or `routes_for_presentation`. Production design
should map compatible concepts to W3C PROV and package contextual metadata in a
standard-friendly form such as RO-Crate where practical.

## Artifact versus evidence

An artifact is bytes plus media type and identity. It becomes evidence for a
claim only when Core also knows:

- who or what produced it;
- from which admitted parents;
- under which method, environment, and policy;
- which validator accepted its structure and semantics;
- its applicability and limitations.

A hash establishes byte identity, not truth. A signature establishes that a key
signed bytes, not that the scientific claim is correct. An exit code establishes
process behavior, not requirement satisfaction.

## Record classes

The draft model includes:

- input;
- plan;
- execution receipt;
- output;
- optional presentation routing;
- verdict; and
- log.

Likely production additions include contract acceptance, registry snapshot,
capability package, method qualification, dataset identity, environment image,
validator report, policy decision, cost receipt, invalidation, amendment,
countersignature, and package manifest.

## Canonical identity

Core needs deterministic canonicalization before signatures and package roots.
Rules must define:

- a named semantic profile separate from the verifier implementation version;
- JCS-derived JSON key ordering and escaping, plus strict Unicode normalization;
- exact integer, canonical decimal, and reduced rational representation, with no
  authoritative binary floating point or non-finite values;
- omitted optional fields rather than semantically ambiguous `null` values;
- nominal quantity kinds, exact within-kind unit scaling, and explicit
  cross-kind conversion provenance;
- path and URI normalization;
- archive entry ordering and metadata;
- large or confidential artifact references; and
- hash algorithm identifiers and migration.

The current helper computes SHA-256 over supplied bytes only. It does not define
canonicalization or package identity.

The `avila-core run` slice reads a draft case manifest, confines relative
paths beneath explicitly selected roots, re-hashes package documents and
available external artifacts, and reports missing roots separately from
missing or mismatched files. For the steps a case declares it stages the
verified bytes into a fresh workspace, runs the bound executable with a
cleared environment, and writes an `avila.core/execution-receipt/v0.1-draft`
record: capability identity, staged inputs, portable invocation and its
identity, process outcome, log and output digests, runner identity, and
limitations. The receipt is re-read and re-hashed before anything is
promoted. The generated claims document is then bound to the manifest
digests and any configured agent-policy identities before campaign evaluation. This is an
integrity, execution, and replay spike, not the final portable evidence
package: it has no archive canonicalization, signed root, trust store,
redaction semantics, sandbox, or lineage-completeness proof. Environment keys an adapter requires the operator to value are recorded
by name inside the invocation identity and by value outside it; the content
a locator names is bound as a staged input. A package may declare free
inputs; a supplied one is hashed and attested, the steps it reaches have
their committed claims withheld and bind fresh outputs by receipt, and
replay is reported not applicable.

An unquantified claim for a non-quantity role may carry a nonempty categorical
`value`. Package-declared external checkers may extract that value only from a
closed set fixed in their hashed descriptor. Core preserves the category in
the claims document, case report, human summary, and attempt log, but does not
coerce it into a number or feed it to the numeric verdict kernel. Quantitative
roles reject categorical values. A contract that needs a technical verdict
must bind a separate reducible claim to an explicit requirement. The adapter
descriptor's raw-byte digest is part of the invocation identity, so changing
the extraction map invalidates receipt reuse even when the executable and
inputs are unchanged.

After campaign evaluation the runner also realizes each configured presentation
dossier from those exact claims. It content-identifies a request containing
the compiled snapshot and campaign, evidence sources and artifact digests,
agent role, policy, dispositions, and instructions, and says
whether the dossier is complete. CASE-001's external agent writes an unsigned
staged-review record over that request. The record is routing history, not an
admission or approval, and it cannot alter a
technical verdict.

## Requirement verdict

A verdict record must name:

- exact requirement and contract digests;
- semantic profile and evaluator implementation identity;
- status;
- observed value or admitted bound where applicable;
- units and comparison semantics;
- numerical and uncertainty rationale;
- every supporting and contradicting evidence ID;
- policy and qualification evaluations;
- boundary and limitations;
- evaluator capability identity; and
- creation and invalidation state.

No provider output is itself the final verdict. The evaluator consumes admitted
evidence under the contract’s encoded semantics.

The verdict establishes only that its state follows from the admitted records
under the named trust policy, semantic profile, and rules. It does not
turn those premises into physical truth or confer certification. Its complete
boundary statement is authoritative; a badge or summary is not.

## Invalidation

Core must propagate change through both direct lineage and semantic policy. A
new artifact does not overwrite an old one; it creates a new branch and records
why prior claims are no longer current.

Potential invalidation triggers include:

- changed input bytes or metadata;
- changed parameter, unit, tolerance, or uncertainty domain;
- amended requirement or comparison semantics;
- new solver, method, dataset, adapter, or environment version;
- expired, revoked, or narrowed qualification;
- changed organization evidence policy;
- discovered defect in a validation or qualification record;
- discovered defect or security compromise; and
- a more conservative dependency discovered after completion.

Selective reuse is allowed only when a typed dependency and validated reuse rule
show that the changed object cannot affect the evidence. “The files look similar”
is not a reuse rule.

The case runner implements the execution-memoization half of this: a step is
reused only when its planned invocation identity equals a committed completed
receipt's and every recorded output verifies at a bound identity, and any
difference is reported by change class and reruns the step. Authorized
non-dependence reuse rules, and invalidation by policy, qualification,
or advisory changes, are not implemented.

## Package contents

A portable package should contain or securely reference:

- contract, requirements, assumptions, and owner acceptance records;
- input manifest and permitted redactions;
- registry snapshot and selected capability packages/manifests;
- execution plan, invocations, logs, receipts, and outputs;
- validation, qualification, uncertainty, and numerical-error evidence;
- policy decisions and qualification records;
- requirement verdicts and limitations;
- optional presentation-routing and user-acknowledgement records, explicitly
  outside the verdict lineage;
- complete graph and content inventory; and
- signed package root plus verifier compatibility information.

The human view is generated from these records, not maintained as an unrelated
report.

## Independent verification

An offline verifier should report, separately:

- package structure and schema validity;
- content-hash integrity;
- signature and trust-root status;
- lineage completeness;
- policy and qualification references;
- requirement-evaluation reproducibility where the evaluator is available;
- semantic-profile support and historical rule replay;
- missing or redacted content; and
- checks not performed.

“Package valid” must never collapse these different checks into a single
unqualified promise.

## Confidentiality and portable evidence

Portability does not require publishing customer data. The package format must
support encrypted or externally retained artifacts, disclosed metadata, policy-
permitted redaction, and proofs of exact identity. Evidence consumers need
enough access to verify the claimed derivation; a digest of inaccessible data
may preserve identity but does not establish credibility.
