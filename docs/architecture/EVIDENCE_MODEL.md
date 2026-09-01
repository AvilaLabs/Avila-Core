# Evidence model

**Status:** conceptual model plus a minimal draft Rust type.

## Principle

Core does not make an answer trustworthy by putting it in a database. It makes
the chain inspectable: what was claimed, by which method, from which immutable
inputs, under which policy and applicability boundary, with which reviews and
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
review records ────────────────────────────────────────────────┤
                                                               ▼
                                                     requirement verdict
                                                               │
                                                               ▼
                                                        package root
```

An edge states a specific relationship such as `used`, `generated_by`,
`qualified_by`, `reviewed_by`, `derived_from`, or `evaluates`. Production design
should map compatible concepts to W3C PROV and package contextual metadata in a
standard-friendly form such as RO-Crate where practical.

## Artifact versus evidence

An artifact is bytes plus media type and identity. It becomes evidence for a
claim only when Core also knows:

- who or what produced it;
- from which admitted parents;
- under which method, environment, and policy;
- which validator accepted its structure and semantics;
- its applicability and limitations; and
- whether required reviews admit it.

A hash establishes byte identity, not truth. A signature establishes that a key
signed bytes, not that the scientific claim is correct. An exit code establishes
process behavior, not requirement satisfaction.

## Record classes

The draft model includes:

- input;
- plan;
- execution receipt;
- output;
- review;
- verdict; and
- log.

Likely production additions include contract approval, registry snapshot,
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

## Requirement verdict

A verdict record must name:

- exact requirement and contract digests;
- semantic profile and evaluator implementation identity;
- status;
- observed value or admitted bound where applicable;
- units and comparison semantics;
- numerical and uncertainty rationale;
- every supporting and contradicting evidence ID;
- policy evaluation and required review records;
- boundary and limitations;
- evaluator capability identity; and
- creation and invalidation state.

No provider output is itself the final verdict. The evaluator consumes admitted
evidence under the contract’s encoded semantics.

The verdict establishes only that its state follows from the admitted records
under the named authorities, policy, semantic profile, and rules. It does not
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
- rejected or withdrawn review;
- discovered defect or security compromise; and
- a more conservative dependency discovered after completion.

Selective reuse is allowed only when a typed dependency and reviewed reuse rule
show that the changed object cannot affect the evidence. “The files look similar”
is not a reuse rule.

## Package contents

A portable package should contain or securely reference:

- contract, requirements, assumptions, and approvals;
- input manifest and permitted redactions;
- registry snapshot and selected capability packages/manifests;
- execution plan, invocations, logs, receipts, and outputs;
- validation, qualification, uncertainty, and numerical-error evidence;
- policy decisions, reviews, and countersignatures;
- requirement verdicts and limitations;
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
permitted redaction, and proofs of exact identity. Reviewers need enough access
to evaluate the claim; a digest of inaccessible data may preserve identity but
does not establish credibility.
