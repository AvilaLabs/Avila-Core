# ADR-0025: Role attribute vocabularies, minor versions, and contract input metadata

- Status: proposed 2026-09-19

## Context

SC-7 ratifies a predicate language that already addresses facts by
slot and attribute: `input_attribute_in(slot, attribute, values...)` and
`input_attribute_in_range(slot, attribute, min, max)` are closed forms,
and "input attributes are addressed by named contract or step slot so
repeated roles remain unambiguous" is clause 2. But nothing declares
what attributes *exist*: a role carries only `role_id` and a version,
and a contract input declares `input_id`, `role`, `media_type`,
`claim_model`, `required` — no attribute vocabulary. Two `roles.*`
rows and one `change.*` row name the consequences:

- `roles.attribute-undeclared-in-predicate` — a predicate naming an
  attribute no role declares cannot be structurally refused today,
  because there is no vocabulary to check it against.
- `roles.minor-version` — `VersionedRef` carries `major` only; a
  `role@1` slot offered a `role@1` carrying extra optional attributes
  has no compatibility model to accept it under.
- `change.input_metadata.non-dependence` — the reuse-rule `(step,
  input_slot)` edge scope is implemented, but an attribute the runner
  never consults has no field to live in; today every input byte is
  consulted.

This ADR proposes the vocabulary extension those three rows need —
all of it is declaration machinery, and none of it changes what a
predicate can say.

## Proposed decision

### 1. Roles declare an attribute vocabulary

`RoleDefinition` gains an optional `attributes` map:

```json
"roles": {
  "actinv.decay-data": {
    "version": 1,
    "validator": "…",
    "attributes": {
      "chain_format": { "value_type": { "kind": "string", "allowed_values": ["ensdf", "livechart"] } },
      "nuclide_count": { "value_type": { "kind": "integer", "min": 1 } }
    }
  }
}
```

Attribute declarations reuse `ParameterDefinition`'s `value_type`
machinery exactly — `kind`, `min`, `max`, `allowed_values` — the same
typed domains capability parameters and (proposed) template
parameters already use. An attribute may be `required` (default) or
`optional`.

Two structural checks close the predicate gap:

- `CORE-T2701` — a predicate's `input_attribute_in(_range)` names an
  attribute the bound role does not declare. Attribute-bearing
  predicates live on qualification records — package documents, not
  compiler inputs — so the refusal lands at record load, before any
  envelope is evaluated: the SC-7 form is legal syntax, but the
  attribute it addresses does not exist in the role's declared
  vocabulary. The check resolves each named slot through the
  exercised steps' bindings to the contract input's role, then the
  role's `attributes` map; a role declaring no vocabulary cannot
  refuse a name, and a record whose pair is unexercised binds no slot
  to check.
- `CORE-T2702` — a bound input's declared attributes violate the
  role's vocabulary: an undeclared attribute name, a value outside
  its declared domain, or a `required` attribute absent.

### 2. `VersionedRef` gains `minor`; compatibility is declared

`VersionedRef` becomes `{ id, major, minor? }` — `minor` defaults to
0 and remains a nonnegative integer. Compatibility rule (R2's
extension):

- Same `id`, same `major`, offered `minor >= required minor`:
  compatible — a minor bump adds optional attributes only, so a newer
  producer still feeds an older slot.
- Offered `minor < required minor`: `CORE-T2101` — the slot asked for
  attributes the producer's version predates.
- A required-attribute addition is a `major` bump by definition: a
  `role@2` never satisfies a `role@1` slot (unchanged R2 semantics).

The rule is mechanical and stated in the registry — a `role@1.3`
definition that adds a *required* attribute is `CORE-R3501`
(registry-incomplete), so the compatibility claim stays honest at the
declaration layer rather than being re-derived at every use.

### 3. `input_metadata` carries non-consulted attributes

`ContractInput` gains an optional `input_metadata` map — attributes
the package carries for humans and routing but the runner never
consults:

```json
{ "input_id": "fns-spectrum", "role": "…",
  "input_metadata": { "source_lab": "LLNL", "retrieved_at": "…" } }
```

Semantics, matching SC-12.2's fail-closed default:

- `input_metadata` is **excluded from the consulted-input digest**:
  changing it never invalidates, never condemns an edge, never enters
  a reuse check. The `(step, input_slot)` edge scope reuse rules
  exempt is the same boundary — metadata sits outside it by
  construction, not by exemption.
- Metadata is **not evidence**: it never enters claims, admission, or
  a verdict input. It is declared to be *not consulted* — the honest
  inverse of an attribute a predicate may reference. A field the
  workflow actually depends on belongs in declared attributes, not
  metadata; the distinction is enforced by `input_attribute_in`
  predicates refusing `input_metadata` names (`CORE-T2701` — the
  predicate vocabulary sees only declared attributes).
- Metadata values are strings, numbers, or booleans — display facts,
  never artifacts.

## Boundary

- **Attributes are declared, never inferred.** An input file's
  contents are opaque; the contract author declares which attributes
  the input carries. Predicates check declarations, not file bytes.
- **`input_metadata` is not a second attribute channel.** If a
  predicate may reference it, it is a declared attribute; if the
  runner may not consult it, it is metadata. The compiler refuses
  overlap: a name present in both is `CORE-T2702`.
- **Minor-version semantics are conventional.** The "adds optional
  attributes only" rule is this registry's contract, not a claim about
  semantic versioning generally.
- **Obligations reports and preflight records** (the remaining
  `admission.*` record types) are separate machinery — a report over
  declared obligations, and a pre-execution check record — not covered
  here.

## Implements

- ADR-0006 SC-7 clause 2 (slot-addressed input attributes) and R2
  (nominal identity, extended to minor versions); SC-12.2 (the
  fail-closed invalidation default `input_metadata` sits outside).
- Fixture rows: `roles.attribute-undeclared-in-predicate`,
  `roles.minor-version`, `change.input_metadata.non-dependence`.
