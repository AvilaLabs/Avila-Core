# Capability protocol

**Status:** early design; the registry snapshot's capability types are
executable, and a case-specific invocation with execution receipts exists for
the steps a committed case declares (ADR-0007). Package manifests, selection,
and a generic adapter protocol are not implemented.

## Purpose

A Core capability is a versioned, provider-owned contribution with named input
and output slots. Each slot has a nominal evidence-role type and slot-scoped
cardinality. The capability operates under an explicit method and qualification
boundary. It may wrap software, data access, a calculation, a verification
method, or a professional review.

The protocol must allow different providers to implement the same semantic
capability type without requiring Core to understand their internal algorithms.
Interchangeability is established by conformance and qualification—not by sharing
a label.

## Manifest identity

The `v0.1` manifest was retired with decision S-016. The current executable
record is the capability *type* in a registry snapshot: nominal input and
output slots with roles and media types, permitted output claim models, typed
parameters and domains, determinism class and material execution factors,
review semantics, governed-purpose exclusions, non-claims, and owner.

Package manifests return under SC-5 and must identify the exact implementation
and declare at least:

- content digest and provider signature;
- method owner and support contact;
- exact protocol and schema compatibility;
- platform, resource, license, and network requirements;
- parameter schema and quantity semantics;
- deterministic/stochastic behavior declaration;
- validation cases and qualification evidence roots;
- applicability predicates and limitations;
- expected output validators;
- security permissions; and
- deprecation and migration information.

## Capability types

A capability type is a semantic contract, not an executable name. It defines:

- meaning of every accepted input role;
- meaning and units of every produced output role;
- names and cardinality of the slots in which those roles occur;
- required uncertainty and numerical-error metadata;
- governed intended-use purposes explicitly excluded by each output;
- failure and partial-result semantics;
- compatibility and conformance fixtures; and
- what the type explicitly does not establish.

Types should be narrow enough to test and stable enough for multiple
implementations. A provider-specific option belongs in an implementation
parameter namespace, not the shared type. Free-text limitations remain visible
to people but never substitute for typed purpose exclusions, and lack of a typed
exclusion does not by itself establish qualification for that use.

## Proposed invocation lifecycle

```text
resolve → verify package → stage → preflight → approve → execute → collect
    → validate outputs → write receipt → promote evidence or quarantine
```

### Resolve

Policy chooses an exact capability package and execution environment from the
approved registry snapshot. Selection inputs and rejected candidates become plan
evidence.

### Verify package

Core checks schema, content digest, signature, compatibility, revocation,
qualification scope, applicability facts and their required sources, and
requested permissions before unpacking or executing.

### Stage

The runner creates a fresh workspace with read-only input objects, a writable
output directory, an invocation document, and only the declared credentials or
licenses. Paths are runner-assigned; adapters cannot choose host paths.

### Preflight and approval

The adapter may report missing prerequisites and an execution estimate. The
runner checks this report against policy. A human approval gate may bind the
immutable invocation digest before cost or data movement begins.

### Execute

The runner invokes the package through a constrained adapter interface, applies
resource and time limits, captures stdout/stderr separately, and records process,
container, scheduler, or service identity. Exit status is execution evidence, not
scientific success.

### Collect and validate

Only declared files in the output area are collected. Type-specific validators
check structure, units, completeness, identities, and consistency. Invalid
outputs are quarantined and cannot enter verdict evaluation.

### Receipt and promotion

The runner creates a receipt covering all verified inputs, the invocation,
capability package, environment, timestamps, resource use, process outcome,
logs, produced artifact hashes, and validation outcome. Admissible outputs are
then added to the evidence graph; originals remain immutable.

The implemented slice of this lifecycle is deliberately narrow: a case package
binds an exact executable by digest and declares which compiled step runs
through which named adapter; the runner stages verified bytes at
package-declared workspace paths, runs with a cleared environment and a
timeout (plus any locator keys the adapter requires, which are identity by
name and provenance by value), collects only declared outputs, writes the receipt, and verifies it
from bytes; the adapter extracts claims. Resolve, package verification beyond
the digest, preflight, approval, and generic validators are not implemented.

## Proposed invocation document

The future invocation should include at least:

```json
{
  "protocol": "avila.core/capability-invocation/v1",
  "campaign_id": "...",
  "step_id": "...",
  "capability_digest": "sha256:...",
  "contract_digest": "sha256:...",
  "inputs": [
    {
      "role": "...",
      "uri": "runner-relative://inputs/...",
      "sha256": "...",
      "media_type": "..."
    }
  ],
  "parameters": {},
  "output_directory": "runner-relative://outputs",
  "environment_policy_digest": "sha256:..."
}
```

This is illustrative and not a released schema.

## Provider selection

Selection must be deterministic for a given contract, policy, registry snapshot,
signed evaluation-time record, fact set, and cost/availability snapshot.
Admissibility is evaluated first. Optimization cannot rescue an inadmissible
candidate. Selection among admitted candidates then follows an explicit ordered
list of objectives and tie-breakers. Policy may consider:

- qualification scope and validation evidence;
- customer allow/deny lists;
- data locality and security permissions;
- compatible inputs and output semantics;
- license and total contract cost;
- time and resource availability;
- reproducibility and service history; and
- independence or diversity requirements.

Commercial payment cannot silently improve rank. A commercial objective, when
permitted at all, must be declared in the selection policy and visible in the
candidate decisions and resulting plan. Scientific margin is never a hidden
commercial optimization dimension.

## Qualification

Qualification is always scoped. A capability is not simply “verified.” Its
record must state:

- the capability digest and method version;
- context of use and permitted parameter domain;
- hardware, data, and environment conditions;
- validation and benchmark evidence;
- uncertainty and numerical limits;
- known failure modes and exclusions;
- owner, reviewer, dates, and expiration/review triggers; and
- the policy or organization recognizing the qualification.

Core can enforce and preserve this record. It cannot create domain credibility
merely by storing it.

Applicability predicates address named input slots, not an ambiguous role name.
Every non-input fact used by a predicate is typed and names its provider and
provenance requirement. A runner signature establishes who recorded a fact; it
does not grant that provider authority to assert it. Missing, stale, mismatched,
or unauthorized facts evaluate to `unknown` and fail closed.

## Human capabilities

Review, data approval, or professional judgment can be capability steps when the
contract defines:

- an immutable reference to the external reviewer-eligibility policy;
- exact evidence presented;
- governance-only dispositions and rationale fields, kept separate from
  technical requirement verdicts;
- independence and conflict rules;
- identity and signature requirements; and
- timeout, rejection, and escalation semantics.

Human input is evidence, not an undocumented exception to the workflow.
The compiler emits a pending obligation and does not decide reviewer
eligibility. Later admission can check keys, signed policy assertions, the exact
presented dossier, permitted dispositions, and separation-of-duties rules under
an organization-selected trust policy. It cannot prove a person's internal
independence, attention, expertise, or reasoning merely from a signature.
