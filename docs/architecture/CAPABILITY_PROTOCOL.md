# Capability protocol

**Status:** early design; the registry snapshot's capability types are
executable, and a case invocation with execution receipts exists for the steps
a committed case declares (ADR-0007). In addition to built-in adapters, a
package may now bind the narrow external-checker descriptor in ADR-0011.
Signed capability manifests, selection, sandboxing, and the broader adapter
lifecycle described below are not implemented. This is a different signature
from the one ADR-0015 implements: ADR-0015 lets a requester sign a case
package's manifest and a runner sign its execution receipts and campaign
log lines, so `run --trust-root FILE` can refuse a rewritten package or an
unsigned reused receipt; it says nothing about a capability *provider*
signing the capability package itself, which is what "provider signature"
below still describes as unimplemented.

## Purpose

A Core capability is a versioned, provider-owned contribution with named input
and output slots. Each slot has a nominal evidence-role type and slot-scoped
cardinality. The capability operates under an explicit method and qualification
boundary. It may wrap software, data access, a calculation, a verification
method, or an optional agent presentation stage.

The protocol must allow different providers to implement the same semantic
capability type without requiring Core to understand their internal algorithms.
Interchangeability is established by conformance and qualification—not by sharing
a label.

## Manifest identity

The `v0.1` manifest was retired with decision S-016. The current executable
record is the capability *type* in a registry snapshot: nominal input and
output slots with roles and media types, permitted output claim models, typed
parameters and domains, determinism class and material execution factors,
optional presentation semantics, governed-purpose exclusions, non-claims, and owner.

An evidence role two of those slots share also carries two independent,
optional statements of what fills it: `validator`, a free-text identifier of
the owner-named checker (documentation only, never invoked), and
`input_schema`, an embedded JSON Schema in the exact restricted subset the
compiler's shape validator supports (`type`, `properties`, `required`,
`additionalProperties`, `items`, `enum`, `const`, `oneOf`, the
canonical-decimal `pattern`, and `description`; no `$ref`). A role's
`input_schema` is checked for that subset at registry compile time
(`CORE-R3501`); a free input filling the role is checked against it at run
time, before staging (`CORE-X1301`, see [Stage](#stage)). Material vocabulary
(which strings a `material` field accepts) stays the capability's own job,
never the schema's.

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

Before anything is staged, every free input the operator supplied
(`run --input NAME=PATH`) is checked against its role's declared
`input_schema`, when the role declares one (see [Manifest
identity](#manifest-identity)). A designer's malformed candidate is refused
with `CORE-X1301` at the exact JSON Pointer, rather than surfacing later as an
adapter traceback or a silently defaulted qualification fact.

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
timeout (plus any locator keys a built-in adapter requires, which are identity
by name and provenance by value), collects only declared regular-file outputs,
writes the receipt, and verifies it from bytes. An adapter extracts claims.

For a conventional command-line checker, the package may supply a separately
hashed `external_checker_adapter` document. Its document id and internal
`adapter_id` must equal the execution's adapter id. The descriptor fixes the
capability type, complete input-slot set, literal/input/output argument vector,
output paths and media types, timeout, limitations, and JSON Pointer mappings
to exact, interval, numeric unquantified or closed-set categorical claims. Its raw-byte digest enters the
invocation and memoization identity. Core invokes the executable directly
without a shell, accepts only authoritative JSON for extraction, and never
interprets the checker-specific category. The executable remains the domain
validator; the descriptor is only a transport boundary.

Numeric extraction preserves the producer's uncertainty model (ADR-0017):

| Model | JSON Pointer fields | Numeric claim |
|---|---|---|
| `exact` | `pointer` | One exact quantity |
| `interval` | `lower_pointer`, `upper_pointer`, optional `nominal_pointer` | Declared lower/upper bounds, with a nominal only when supplied |
| `unquantified` | `pointer` | A nominal estimate with no quantified uncertainty |

Each numeric descriptor fixes a `unit`. Each selected number must be a safe
JSON integer or canonical exact-number string; the whole extraction output
must be authoritative JSON. An optional pointer is omitted rather than null.
Bounds are assertions by the producing method. Existing claim admission checks
their model, order, units and nominal containment; qualification and the kernel
still determine whether they can support the authored requirement. An estimate
does not become exact merely because it is serialized without floating-point
JSON. See the visibly synthetic descriptor/output pair under
`fixtures/external-checkers/` for the transport shape.

Exact and categorical claims may also declare `optional: true`
(ADR-0016): a pointer that resolves to no value leaves the claim absent
rather than failing extraction, so a checker that ran correctly and
produced no result of that kind is reported as missing evidence to
dependent requirements instead of as a rejected step. A present but
malformed value still fails extraction, and the absent slot names are
recorded in the step's report.

Package-integrity re-hashing of large operator-supplied artifacts (bulk
nuclear-data libraries, vendored release binaries) is the dominant cost of a
verify-only invocation and repeats on every run. `avila-core run --hash-cache
PATH` (S-038) lets the operator name a JSON file where a verified digest is
recorded against a file's canonical absolute path, size, modification time,
and, where available, device and inode. A subsequent run whose artifact
still matches that exact stamp reuses the recorded digest instead of
re-reading the bytes, and the per-artifact integrity state says so
(`verified_cached`, distinct from `verified`) rather than silently claiming
the stronger state. The cached digest is still compared against the
manifest's bound identity exactly as a freshly computed one would be, so a
disagreement still fails closed. This is convenience over an
already-established boundary, not a new one: it trusts that operator-owned
artifact roots are not modified while preserving size and modification
time, which is weaker than reading bytes and is documented as such in
`SECURITY.md`. Package documents and any artifact resolved from inside the
case package directory are never eligible, cache or no cache. The cache is
off unless supplied and is never used by the case bless or verify tooling
under `examples/cases/tools/`.

Resolve beyond these local declarations, package verification beyond content
digests, preflight, approval, resource isolation, signatures, network policy,
and generic validators are not implemented.

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
- owner, dates, expiration triggers, and validation refresh conditions;
- the policy or organization recognizing the qualification; and
- the output roles it covers, when narrower than everything the capability
  produces — a capability with several outputs may be qualified on some and
  not others, and a claim on an uncovered output carries no qualification at
  all, so it can still satisfy only a nominal-basis requirement (ADR-0008).

Core can enforce and preserve this record. It cannot create domain credibility
merely by storing it.

Applicability predicates address named input slots, not an ambiguous role name.
Every non-input fact used by a predicate is typed and names its provider and
provenance requirement. A runner signature establishes who recorded a fact; it
does not grant that provider authority to assert it. Missing, stale, mismatched,
or unauthorized facts evaluate to `unknown` and fail closed.

## Optional presentation capabilities

A connected-agent practicality check can be a capability step when the contract
defines:

- an immutable reference to the agent policy and implementation identity;
- the exact post-campaign dossier presented;
- `present_to_user`, `request_changes`, and `abstain` as the closed routing
  dispositions;
- explicit practical instructions; and
- identity, timeout, refusal, and retry semantics.

The compiler emits an optional `presentation_gate` in state `awaiting_agent`.
The runner materializes the exact request only after campaign evaluation. The
record is routing history, not technical evidence, and its absence or outcome
cannot change a verdict. Human acknowledgement or organization-specific
approval may be recorded separately but is never a universal Core gate.
