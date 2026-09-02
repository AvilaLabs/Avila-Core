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

This boundary cannot execute a capability, qualify a method, determine whether
a reviewer is eligible, fulfill a review, admit evidence, or emit a requirement
verdict. A successful compilation means only that the source is internally
composable under the implemented semantic rules and pinned registry snapshot.

## Current pipeline

```text
authoritative JSON reader, every value refusal
              │
              ▼
embedded-schema shape check, every violation
              │
              ▼
typed decode + semantic-profile check
              │
              ▼
registry integrity and nominal identities
              │
              ▼
parameter and reproducibility declaration checks
              │
              ▼
slot resolution + role/media checks
              │
              ▼
role-separated review obligation checks
              │
              ▼
derived dependency graph + cycle check
              │
              ▼
requirement metric + exact kind/unit lowering
              │
              ▼
claim-model/basis satisfiability
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
- no compatible source emits `CORE-R3101` with a `constrained_choice` repair
  naming every way the slot could be fed under the pinned snapshot:
  `declare_input:<role>` or `add_step:<capability type>/<output slot>`; the
  list is computed from the snapshot alone and is not a qualification claim;
- multiple compatible sources emit `CORE-R3102` and require an explicit
  constrained choice; and
- an authored binding is checked independently for nominal role identity and
  accepted media type.

Workflow order in the source file is presentational. Data bindings create the
dependency graph. Self-bindings, unknown step outputs, and cycles are rejected;
the compiler never repairs a graph by reordering or dropping work. A step whose
capability type is absent from the snapshot is reported once, at its
`capability_type`; bindings and requirement metrics that name its outputs are
suppressed rather than reported as nonexistent sources, because they would
resolve as soon as the type is supplied. A reference to a known step names the
outputs that step actually declares.

Implicit resolution currently considers all declared contract inputs and step
outputs in the supplied snapshot. That rule is intentionally simple and total.
Future scope or aggregation semantics must revise the semantic profile and its
fixtures rather than quietly changing candidate discovery.

## Parameter semantics

Capability types own parameter declarations. Each declaration names whether
the value is required and one of five closed value families: boolean, signed
64-bit integer, exact number, text, or typed quantity. Integer and exact-number
types may declare inclusive or exclusive minimum and maximum bounds. Text may
declare a finite choice set. Quantity bounds name a quantity kind and are
lowered through that kind's exact unit registry before comparison.

Authored exact numbers are strings, never binary floating-point JSON numbers.
Authored quantities are objects containing an exact string `value` and a
`unit`. Successful compilation replaces raw parameter JSON with tagged typed
values; exact numbers are reduced to canonical rationals and quantities are
stored in the kind's canonical unit. Undeclared parameters, wrong value
families or quantity kinds, and out-of-domain values are blocking findings.
Core does not coerce strings into booleans, round numbers, choose an enum value,
or invent a default.

The exact string `not_defined` is reserved across every parameter family. In a
draft it emits `CORE-S1301` as a `missing` finding owned by the requester. Once
the contract is `in_review`, `approved`, or `retired`, it is an `invalid`
finding. Missing required parameters follow the same status rule. Missing and
invalid findings both block compilation, and the marker never enters compiled
IR. This lets an unfinished draft state its incompleteness without allowing an
incomplete value to masquerade as executable configuration.

## Requirement lowering

A requirement's `limit` and, for an `equal` comparison, its `tolerance` are
lowered through the metric role's quantity kind into that kind's canonical
unit. Kind and unit mismatches are reported at the exact field. An `equal`
comparison without a tolerance is `CORE-T2104` as a `missing` finding, because
the verdict calculus could never evaluate it; a tolerance on any other
comparison is `CORE-T2104` as `invalid`, because it would silently mean
nothing. A negative tolerance is refused. A `coverage` basis must be a
canonical decimal in `(0, 1]` and may appear only on a `bounded` basis; the
kernel repeats that check at verdict time, but a requirement that can never be
evaluated must not compile. A `nominal` basis is a visible weakening: it
compiles only when the execution policy sets `permit_nominal_basis`, and the
finding `CORE-A4201` otherwise carries the edit that would permit it.

## Type-level reproducibility

Every capability type declares one determinism class: `deterministic`,
`seeded_stochastic`, or `nondeterministic`. It also declares every execution
factor that the type considers material to invocation identity. Material
factors use the same closed typed-value and domain machinery as parameters and
must all be explicitly bound on each step. Their canonical typed values are
retained in compiled IR.

A `seeded_stochastic` step must bind a nonempty opaque seed, which is retained
in invocation identity. A seed on either of the other classes is rejected
because silently treating irrelevant configuration as identity would obscure
the type's actual reproducibility claim. Nondeterminism is forbidden when the
contract omits an execution policy; a nondeterministic type compiles only when
the contract explicitly lists every produced evidence role under
`permitted_nondeterministic_roles`. Permission is scoped by nominal role and
does not make the capability deterministic, scientifically qualified, or
acceptable under an organization's policy. Its compiled class remains
`nondeterministic`, and downstream execution memoization must remain disabled.

This is the type-level half of R8 only. Exact packages may declare additional
material environment, hardware, ABI, validator, and implementation factors.
Those factors cannot be checked until package binding exists, so successful
static compilation is not a package-level reproducibility judgment.

## Review obligations

R9 represents review as a capability type, but gives it no privileged power
over technical claims. Every review type names each required input in the
exact dossier, emits only one nominal decision record with the `unquantified`
claim model, declares a closed set of governance dispositions, and says
whether its reviewer is an `accountable_person` or an `agent`. None of those
dispositions is a technical `PASS` or `FAIL`.

The role closes the authority surface. An accountable-person capability must
remain nondeterministic and may allow `approve_for_use`, `reject_for_use`,
`request_changes`, or `abstain`; it may not merely recommend another review.
An agent capability may allow only `recommend_for_accountable_review`,
`request_changes`, or `abstain`. Offering an agent approval or rejection
authority is a compile-time refusal. Every agent use must also bind at least
one nonempty practical instruction.

Each workflow use binds a reviewer-eligibility policy by id, revision, and
lowercase SHA-256 identity. The policy bytes, identities, credentials, and
trust roots are deliberately not compiler inputs. Independence is nevertheless
explicit in the contract: either `none`, preserving the visible weakening, or
a nonempty set of minimum person/organization separations from named campaign
parties. The same binding slot is used for an agent policy, where it pins the
permitted implementation and instructions rather than professional authority.

Successful compilation records `pending_external_review` for a person or
`pending_agent_review` for software, along with the exact resolved evidence
sources, decision role and media type, permitted dispositions, policy identity,
independence constraints, and instructions. These are obligations for later
routing or admission. They are not evidence that a review occurred or that any
reviewer was legitimate. Only the accountable-person obligation participates
in campaign `PASS` gating; an agent stage can send work back but cannot fulfill
that obligation or alter a verdict.

The runner now materializes a content-identified request from each obligation
and the evidence actually present. Draft schema
`avila.core/staged-review-record/v0.1-draft` records an unsigned agent routing
result over that request. The authoritative, signed accountable fulfillment
record remains intentionally unimplemented. It must eventually bind at least
the compiled snapshot and review-step identity, exact realized dossier
identities, one allowed disposition and rationale, eligibility-policy identity,
signer and organization-scoped eligibility assertions, independence inputs and
result, canonical payload identity, signature, and supersession or revocation
state. Neither record may contain a field that overrides a technical verdict.

`CORE-R3401` is reserved for an authored review obligation that is structurally
incomplete or contradictory. Once a valid obligation has compiled, an absent
accountable decision withholds `PASS` at campaign evaluation; an absent agent
record changes no technical verdict. A bad signature, failed eligibility check,
or failed independence check remains a future admission refusal. Keeping those
outcomes distinct prevents static composition, software routing, human action,
and organizational trust from collapsing into one misleading status.

## Governed purposes and typed exclusions

Every requirement names one governed, versioned purpose representing its
intended use. The supplied immutable registry snapshot defines those purpose
identities, owners, and human descriptions. Purpose identity is nominal: the
compiler performs no hierarchy traversal, prefix matching, synonym expansion,
or language-model interpretation. `screening@1` and `screening_research@1` are
different purposes even when their names look related.

A capability type may place `excluded_purposes` on each output slot. When that
output is selected as a requirement metric, an exact match between the
requirement purpose and an exclusion emits `CORE-T2601`. Unknown purposes,
unsupported major versions, duplicate registry definitions, unknown
exclusions, and repeated exclusions also fail closed. Successful compiled
requirements retain the exact purpose identity.

Free-text `non_claims` remain important explanatory material, but they are not
machine-enforced exclusions and the compiler never guesses a purpose from
prose. Conversely, absence from `excluded_purposes` is not a positive
qualification claim; R10 establishes only that the producing type did not make
this exact explicit refusal. Package qualification, applicability, admission,
and organization policy remain later checks.

Contract inputs currently carry no trusted producer-type exclusion declaration.
Their actual package or evidence-record limitations must be checked during
binding and admission, and cross-contract reuse must preserve those limitations
rather than laundering an output into an unrestricted input.

## Findings

Every finding carries:

- a stable code;
- `missing`, `invalid`, `unsatisfied`, `inadmissible`, or `notice` class, of
  which only `notice` does not block compilation;
- an accountable owner, always a product role: `requester`, `policy_owner`, or
  `registry_owner`;
- a document plus JSON Pointer location;
- related locations when applicable; and
- typed repair applicability, a label per alternative, and, when the
  compiler can state the exact bytes, the RFC 6902 JSON Patch that realizes
  each alternative.

Consumers match these fields, never explanatory wording. Every code is
explained in the [diagnostic catalog](DIAGNOSTICS.md), which the CLI serves
through `avila-core explain`. Findings are sorted
deterministically and independent root causes are reported in one pass. The
current slice suppresses checks whose premise could not be constructed, such
as a unit check after the metric source itself is missing.

Two findings are notices rather than blockers. `CORE-R3601` marks a contract
input that no step binds, and `CORE-R3602` marks a non-review step whose
outputs feed neither another step nor a requirement. Both describe declared
work that would never enter the campaign's evidence, which matters to a
product whose thesis is minimum sufficient computation. Notices are computed
only when the contract is otherwise compilable, because an unfed declaration
is usually a consequence of a blocking resolution failure reported elsewhere.
A compiled report may therefore carry notices; a rejected report never does.

Source-layer refusals are located and reported in one pass too. The
authoritative reader tracks the JSON Pointer of the value it is reading, so a
binary float, `null`, duplicate key, non-NFC string, or unsafe integer is
reported at that value rather than at the document root; when a document is
refused, a diagnostic read records every such refusal instead of stopping at
the first. The compiler then checks the document's shape against the embedded
`v0.2-draft` schema, reporting every unknown property, missing required
property, wrong value family, wrong tagged variant, and non-canonical number at
its own pointer. That stage enforces exactly what typed decoding would refuse;
every rule the decoder would accept, such as cardinality, uniqueness, minimums,
empty identifiers, and identity formats, stays with the semantic pass that owns
it so the finding carries its semantic code and accountable owner. A wrong
variant or enumerated value carries a `constrained_choice` repair naming the
admitted values. A number that is well formed but not canonical, such as
`"100.0"` or `"2/4"`, carries a `mechanically_safe` repair naming its unique
canonical form; the compiler still refuses it rather than rewriting authored
bytes. Only a syntax error ends the pass early; it points at the value being
read and keeps the line and column in its message.

## The diagnostic contract

A finding is only useful if a tool that has never seen the document can act
on it. That is enforced, not assumed: the harness in
`crates/avila-core-compiler/tests/diagnostic_contract.rs` takes every compiled
fixture, applies each of fourteen authoring mistakes with a deterministic
generator, and checks every resulting report. Compiling twice must yield
identical reports. Every pointer must anchor in the mutated document, either
exactly, as the documented logical slot location `/workflow/{i}/inputs/{slot}`
under a step whose type declares that slot, or as a `missing` finding whose
parent or grandparent exists. Every `mechanically_safe` repair must have one
candidate and remove its finding when applied verbatim. Every
`constrained_choice` on a substitutable value must remove its finding when its
first candidate is applied, and every feeding candidate on an unfed slot must
name a role or producer the snapshot defines. Finally, a fixer that applies
only the compiler's own repairs must reach a compiled snapshot within two
rounds for every mistake that is mechanically repairable at all: an unknown
field, a non-canonical number, a wrong enumerated value, or an unknown unit.

The harness prints a summary per mistake: cases, findings per case, how many
were fixed mechanically, and rounds to green. On the current corpus every
mistake produces about one finding, so a single mistake does not cascade, and
every mechanically repairable mistake is fixed in one round. That is the first
measurement of the thesis that compiler feedback lets an agent repair a
contract without re-reading everything. It is measured over synthetic
fixtures; the defect corpus from real contracts that the validation plan
requires does not exist yet.

Repairs are typed edits. Each alternative carries its RFC 6902 patch when the
compiler can state the exact bytes: replace a value with a canonical form or a
listed choice, remove a misplaced tolerance, an unknown field, or an unused
declaration, rename a misspelled property to a known one, add a binding for a
listed source, or add the input or step that would feed an unfed slot. The
harness's fixer applies the first alternative of every blocking finding with
no knowledge of individual codes, which is exactly what an agent's mechanical
loop should do; only a value the compiler cannot know, such as a missing
tolerance, stays with the requester.

## Compiled identity

Successful output is `avila.core/compiled-contract/v0.2-draft`. It contains:

- the canonical SHA-256 identities of both source documents;
- the semantic profile and compiler implementation identity;
- resolved bindings in deterministic topological order;
- typed parameter values lowered to canonical exact representations;
- type-level determinism, seeds, material execution factors, and the effective
  contract nondeterminism policy;
- role-separated pending review obligations, exact presented evidence,
  external eligibility-policy identities, explicit independence constraints,
  and agent instructions;
- governed requirement-purpose identities and output-level explicit
  purpose exclusions;
- exact requirement limits, and equality tolerances, lowered to canonical
  units; and
- a `snapshot_sha256` over the canonical compiled body, excluding the digest
  field itself.

Changing either source document, even in a semantically visible ordering field,
changes its canonical source identity. Changing only whitespace or JSON object
key order does not. The compiled identity deliberately includes the compiler
implementation version in addition to the semantic profile; reproducibility
and semantic compatibility are related but not interchangeable claims.

## Implemented and deferred rules

This initial slice implements the currently representable portions of SC-1,
SC-2, SC-3, SC-4, and SC-6 R1–R10. In particular it covers canonical source
identity, exact within-kind unit scaling, unique slot resolution, nominal
role-major compatibility, type-level claim-model/basis satisfiability, media
compatibility, graph shape, metric binding, limit kind/unit compatibility,
typed parameter/domain enforcement, type-level reproducibility bindings, the
structural review roles and authority limits, and nominal purpose-exclusion
checking.

It does not yet represent or decide:

- package-level actual claim-model, coverage, and bound-side sufficiency (the
  binding half of R3);
- package-declared environment, hardware, ABI, validator, and implementation
  factors (the binding half of R8);
- actual reviewer identities, credentials, signatures, eligibility-policy
  evaluation, independence evaluation, and signed decision admission (the
  fulfillment half of R9);
- capability packages, qualification, policy, full package-level admission, or
  selection; or
- campaign execution and evidence lifecycle.

Those are blockers on the path to an accepted `0.2` profile. The present
`v0.2-draft` records may change as those semantics are added.
