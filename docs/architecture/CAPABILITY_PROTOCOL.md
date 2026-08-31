# Capability protocol

**Status:** early design; only manifest parsing and planning exist.

## Purpose

A Core capability is a versioned, provider-owned contribution that accepts
defined evidence roles and produces defined evidence roles under an explicit
method and qualification boundary. It may wrap software, data access, a
calculation, a verification method, or a professional review.

The protocol must allow different providers to implement the same semantic
capability type without requiring Core to understand their internal algorithms.
Interchangeability is established by conformance and qualification—not by sharing
a label.

## Manifest identity

The draft manifest currently records:

- schema version;
- globally unique capability ID;
- semantic capability type;
- name, provider, and implementation version;
- maturity and qualification record;
- accepted and produced evidence roles;
- process execution declaration or a fail-closed unavailable reason; and
- metadata.

Production manifests also need:

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
- required uncertainty and numerical-error metadata;
- failure and partial-result semantics;
- compatibility and conformance fixtures; and
- what the type explicitly does not establish.

Types should be narrow enough to test and stable enough for multiple
implementations. A provider-specific option belongs in an implementation
parameter namespace, not the shared type.

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
qualification scope, and requested permissions before unpacking or executing.

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
and cost/availability snapshot. Policy may consider:

- qualification scope and validation evidence;
- customer allow/deny lists;
- data locality and security permissions;
- compatible inputs and output semantics;
- license and total contract cost;
- time and resource availability;
- reproducibility and service history; and
- independence or diversity requirements.

Commercial payment cannot silently improve rank. Any paid preference must be
prohibited or visible as an inadmissible policy dimension.

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

## Human capabilities

Review, data approval, or professional judgment can be capability steps when the
contract defines:

- required role and authority;
- exact evidence presented;
- allowed decisions and rationale fields;
- independence and conflict rules;
- identity and signature requirements; and
- timeout, rejection, and escalation semantics.

Human input is evidence, not an undocumented exception to the workflow.

