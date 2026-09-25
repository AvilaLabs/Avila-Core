# EL-01 specification review

Follow-up: [r2 working-tree review](2026-09-25-engineering-language-r2-review.md).
The findings below describe the original `ce35f22` specification.

Reviewed revision: `ce35f22` (`b1587ad..ce35f22`). This review covers ADR-0027,
the language specification, both libraries, all fifteen authored programs,
and their expectations. It reviews the proposed rules, not an implemented
checker. The EL-00 verification note was read; its execution checks were not
repeated in this review.

**Decision: revise EL-01 before starting EL-02.** The numerical examples are
consistent, but the normative rules do not yet determine all their typing,
dependency, and identity behavior. Implementing now would require the checker
author to make semantic decisions that belong in the specification.

## Findings

### 1. [P1] Multiplication cannot type-check the intended examples

[Specification §7.A](../../architecture/ENGINEERING_LANGUAGE.md#7-primitive-inference-rules),
lines 254–262, requires the operands of both arithmetic rules to have the
same quantity kind and relation signature. But
[linear-expansion](../../../examples/language/libraries/thermal-expansion.v1.json)
multiplies `thermal_expansion_coefficient`, `length`, and `temperature`.
No ordering of those operands satisfies the stated same-kind rule.
`measurement-scaling` also multiplies a dimensionless factor by a dose rate.

Consequently the specified primitive rules cannot justify the positive
programs. Removing the same-kind check alone would leave the result kind and
unit unjustified: multiplication of two lengths cannot simply retain `length`.

**Closure:** specify separate addition/subtraction and multiplication typing
rules, including result kinds, canonical units, and allowed intermediate
types. Derive both libraries' expressions through those rules. Add an invalid
product/result-kind counterpart. Keep physical kind distinctions explicit
even where dimensions coincide; a unit calculation alone must not confer a
new engineering interpretation.

### 2. [P1] Scenario identity and scope acceptance conflict

[Specification §5](../../architecture/ENGINEERING_LANGUAGE.md#5-unification-and-the-application-rule),
lines 189–198, defines acceptance component-wise but checks `s` only by scope
kind. Sections 2.1 and 2.3 instead require scenario identity equality.

For example, `hot-soak` and `cold-soak` can both have scope `steady-state`.
Under the component-wise rule they satisfy each other's scenario slot,
including two inputs sharing a signature variable `s`; under §2.3 they do
not. This ambiguity permits an implementation to combine distinct operating
cases while reporting a successful related-type check.

**Closure:** distinguish scenario identity from scope classification in the
judgment, and require identity unification independently of permitted scope
acceptance. Add a same-scope/different-scenario refusal alongside the existing
steady-state/transient example, plus a same-identity positive counterpart.

### 3. [P1] Assumption discharge permits circular justification

[Specification A3](../../architecture/ENGINEERING_LANGUAGE.md#7-primitive-inference-rules),
lines 307–321, allows established evidence to retain assumptions, then removes
`a` when a checked rule's premises establish `a`. It never states that the
support establishing `a` must be free of dependence on `a`.

A claim under `{a}` and a checked derivation of `a` under `{a}` cannot produce
an unconditional claim. The written set subtraction removes `a` anyway. The
general union rule does not repair this example: unioning `{a}` with `{a}`
and then removing `a` still produces the empty set. Replaying that same rule
would reproduce the unsound discharge, rather than detect it.

**Closure:** name the witness derivation and its residual assumptions. For a
claim supported by `Sigma` and a witness for `a` supported by `Delta`, retain
`(Sigma - {a}) union Delta` and all witness attribution/dependency edges.
Claim that `a` is discharged only if the resulting support no longer depends
on `a`; reject cyclic discharge chains. Specify scoped proposition equality.
Add independent, conditional, self-dependent, and mutually dependent witness
examples. In particular, proving `a` under `{b}` must retain `{b}`.

### 4. [P1] Disjoint recorded sources automatically discharge independence

[Specification E4](../../architecture/ENGINEERING_LANGUAGE.md#7-primitive-inference-rules),
lines 325–333, discharges independence when recorded source sets are disjoint.
The preservation argument limits this to the recorded graph, but no scoped
independence assertion is required and no completeness assumption is carried
by the result.

Thus two separately named source parties/edges with no recorded common
ancestor can close the obligation merely through graph disjointness. This
contradicts the [EL-05 handoff](../ENGINEERING_LANGUAGE_HANDOFF.md#el-05--demonstrate-transfer-and-assess-the-experiment):
two source identifiers alone must leave independence unresolved. Recorded
provenance separation and independence of measurement uncertainty need
different meanings; neither an absent shared edge nor a shared administrative
party by itself decides the latter.

**Closure:** define the proposition precisely. Graph disjointness may
establish a separately named provenance property. Independence required by a
method must have a scoped, attributable premise or supported proof, retaining
its residual trust assumptions. Add three cases: shared provenance,
different identifiers with independence still unknown, and an explicit
conditional independence premise. Define how that premise interacts with
shared provenance instead of combining an `iff disjoint` rule with an
unspecified assertion override.

### 5. [P2] Missing relation parameters have no defined meaning

[Specification §2.2](../../architecture/ENGINEERING_LANGUAGE.md#2-sorts-types-and-the-claim-dimension)
defines every quantity over `(g,s,m)`, each component an entity or a bound
variable. The JSON programs and library signatures instead omit components:
the expansion input length has only geometry, the coefficient only material,
and the temperature change only scenario. The clearance output omits material
altogether.

The spec does not say whether absence means unknown, inapplicable, invariant
over that relation, an existential variable, or a wildcard. These choices
produce different application and goal-search results. Treating absence as
an unrestricted wildcard could silently attach a physical identity; treating
it as an unresolved parameter would block the positive fixtures.

**Closure:** define the type representation and elaboration of absent
components, permitted relation composition/projection, and output
substitution. Demonstrate the complete types of every intermediate value in
the positive program. Add a case preventing an omitted relation from being
used to relabel an established value. Keep dependencies even where a type
intentionally projects away a relation.

### 6. [P2] `claim: any` has no defined result rule

[Specification §5](../../architecture/ENGINEERING_LANGUAGE.md#5-unification-and-the-application-rule),
lines 201–203, says the output takes the weakest supplied claim and cites
`§7.A5`, which does not exist. Section 3 explicitly rejects a universal claim
ranking and supplies no coercion between `nominal` and `enclosure`.

For a method receiving one nominal and one enclosure argument, no defined
operation chooses that "weakest" claim or reconciles it with the declared
output type. Independent implementations would have to invent a rule.

**Closure:** remove `claim: any` from this finite fragment, or specify its
supported signatures, representation, result typing, and mixed-claim cases.
Do not turn unsupported combinations into an implicit confidence ranking.

### 7. [P2] Canonical content identity conflicts with presentation invariance

[Specification §10](../../architecture/ENGINEERING_LANGUAGE.md#10-canonical-identity-context-binding-and-change-dependencies),
lines 438–472, identifies libraries by canonical document content and binds
library identity into the evaluation context, but also promises identical
context/derivation identities after presentation changes. It provides no
semantic identity projection to reconcile these statements.

The pinned thermal library digest is the hash of the full canonical JSON.
Changing only a precondition's display `name` changes that digest. Existing
canonicalization also preserves array order; "document ordering" cannot
generally be ignored, particularly for sequential program bodies.

**Closure:** specify exact identity bodies and which fields/orderings are
semantic. Either distinguish document identity from semantic identity, or
weaken the promise to unchanged conclusions/dependencies where raw source
identity legitimately changes. Add display-only and genuinely semantic
mutation counterparts. Preserve source attribution rather than stripping
text indiscriminately.

### 8. [P2] The lifecycle row specifies library upgrades, not withdrawal

[Expectations](../../../examples/language/expectations.json), line 155, maps
narrowing, expiration, and withdrawal to a new library revision/hash. That
covers replacement, but a pinned library can expire or be withdrawn without
its immutable document changing. The specification names qualifications in
the context but gives no language rule for this current-use decision.

A new revision's existence does not by itself determine whether an old pinned
method remains usable. The required historical/current distinction therefore
has no determinate expected outcome yet.

**Closure:** specify the supplied qualification/lifecycle context, its
association with the method, and the stage and outcome of refusal or missing
material. Add a replay scenario keeping the program and library fixed while
changing current lifecycle material. Preserve historical replay under the
original context. These can remain execution/replay fixture descriptions
until EL-03/EL-04, but must state premises and expected decisions now.

## Additional consistency correction

`linear-expansion-via-table` assumes `tabulated-expansion-model`, absent from
the thermal library's proposition declarations. Declare it, or explicitly
specify that proposition identifiers need no declaration. The ambiguity
fixture should not rely on an unexplained symbol-resolution convention.

## Checks performed and limits

- Read the specification against its governing charter/handoff and every
  authored program. The counterexamples above are rule-level reasoning, not
  executions of a language checker.
- Parsed all 18 JSON documents: fifteen programs, two libraries, and
  expectations. Checked exact coverage of program filenames and every pinned
  library digest. All matched. The canonical hash calculation used the
  fixture's ASCII keys and integral JSON numbers and reproduced both pins.
- Recomputed the fixture arithmetic with Python `Fraction`: displacement
  `[1/10,1/5]`, clearance `[3/10,2/5]`, and PASS/INCONCLUSIVE/FAIL at limits
  `1/4`, `7/20`, and `9/20` respectively.
- Confirmed the display-name mutation changes the full library content hash
  and identified the undeclared proposition above.

No build, benchmark, solver run, or workspace test suite was needed for this
specification review. The unrelated draft case was left untouched.

## SWE-2 next work

Revise EL-01's specification, preservation argument, libraries, and expected
results together. Add the positive and adversarial counterparts above, and
refresh pins after library edits. State the decisions explicitly; do not
leave the checker to infer them. Return the revised rules for review before
EL-02 implementation. No new infrastructure, solver integration, or broad
language feature is needed to close these findings.
