# Semantic compiler

## Boundary

`avila-core-compiler` is the first executable boundary between authored records
and an immutable campaign description. It accepts exactly two byte documents:

1. an evidence contract under
   `avila.core/evidence-contract/v0.2-draft`; and
2. one explicit registry snapshot under
   `avila.core/registry-snapshot/v0.2-draft`.

Both documents must name `avila.core/semantic/0.2-draft`. The compiler does not
read an ambient registry, inspect installed software, discover a provider, or
fill a missing value from machine state. The same bytes and compiler version
therefore produce the same report.

This boundary has no authority to execute a capability, qualify a method,
admit evidence, or emit a requirement verdict. A successful compilation means
only that the source is internally composable under the implemented semantic
rules and pinned registry snapshot.

## Current pipeline

```text
strict authoritative JSON reader
              │
              ▼
schema + semantic-profile check
              │
              ▼
registry integrity and nominal identities
              │
              ▼
slot resolution + role/media checks
              │
              ▼
derived dependency graph + cycle check
              │
              ▼
requirement metric + exact kind/unit lowering
              │
              ▼
immutable compiled snapshot or REJECTED report
```

The authoritative reader rejects duplicate keys, `null`, binary floating-point
JSON numbers, unsafe JSON integers, and non-NFC strings before typed decoding.
Input bytes need not already have canonical key order or whitespace. Core
canonicalizes the accepted values before computing each source identity.

## Resolution and graph semantics

Capability types declare named input and output slots. A source is either a
named contract input or a named step output. For each required input slot:

- one compatible source is resolved;
- no compatible source emits `CORE-R3101`;
- multiple compatible sources emit `CORE-R3102` and require an explicit
  constrained choice; and
- an authored binding is checked independently for nominal role identity and
  accepted media type.

Workflow order in the source file is presentational. Data bindings create the
dependency graph. Self-bindings, unknown step outputs, and cycles are rejected;
the compiler never repairs a graph by reordering or dropping work.

Implicit resolution currently considers all declared contract inputs and step
outputs in the supplied snapshot. That rule is intentionally simple and total.
Future scope or aggregation semantics must revise the semantic profile and its
fixtures rather than quietly changing candidate discovery.

## Findings

Every finding carries:

- a stable code;
- `missing`, `invalid`, `unsatisfied`, `inadmissible`, or `notice` class;
- an accountable owner;
- a document plus JSON Pointer location;
- related locations when applicable; and
- typed repair applicability and candidates when a bounded repair exists.

Consumers match these fields, never explanatory wording. Findings are sorted
deterministically and independent root causes are reported in one pass. The
current slice suppresses checks whose premise could not be constructed, such
as a unit check after the metric source itself is missing.

## Compiled identity

Successful output is `avila.core/compiled-contract/v0.2-draft`. It contains:

- the canonical SHA-256 identities of both source documents;
- the semantic profile and compiler implementation identity;
- resolved bindings in deterministic topological order;
- exact requirement limits lowered to canonical units; and
- a `snapshot_sha256` over the canonical compiled body, excluding the digest
  field itself.

Changing either source document, even in a semantically visible ordering field,
changes its canonical source identity. Changing only whitespace or JSON object
key order does not. The compiled identity deliberately includes the compiler
implementation version in addition to the semantic profile; reproducibility
and semantic compatibility are related but not interchangeable claims.

## Implemented and deferred rules

This initial slice implements the currently representable portions of SC-1,
SC-2, SC-4, SC-6 R1, R2, R4, R5, and R6. In particular it covers canonical
source identity, exact within-kind unit scaling, unique slot resolution,
nominal role-major compatibility, media compatibility, graph shape, metric
binding, and limit kind/unit compatibility.

It does not yet represent or decide:

- uncertainty-model and comparison-basis sufficiency (R3);
- typed parameter domains and placeholders (R7);
- seed and execution-factor completeness (R8);
- human-review signatures (R9);
- purpose/non-claim conflicts (R10);
- capability packages, qualification, policy, admission, or selection; or
- campaign execution and evidence lifecycle.

Those are blockers on the path to an accepted `0.2` profile. The present
`v0.2-draft` records may change as those semantics are added.
