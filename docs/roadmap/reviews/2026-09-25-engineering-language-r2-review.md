# EL-01 r2 follow-up specification review

Follow-up: [r3 closure review](2026-09-25-engineering-language-r3-closure.md).
The findings below describe the reviewed r2 working tree.

Reviewed the uncommitted r2 working tree based on `ce35f22`, including the
specification, both libraries, all 23 programs, and their expectations.
Specification file byte SHA-256 at review:
`32b33def9da55a65b6ebfb75be7b8265425dffb6934de870ac642829e653955f`.
This follows the [first specification review](2026-09-25-engineering-language-spec-review.md).

**Decision: EL-02 should remain pending.** Scenario identity is now separate
from scope, multiplication has declared kind rules, and A3 preserves witness
support and explicitly refuses circular discharge. The undeclared proposition
and `claim: any` were corrected. The remaining findings below concern actual
rule/fixture disagreement and counterexamples to the new identity and
independence rules; successful JSON/hash validation does not resolve them.

No checker implementation or historical EL-00 verification was exercised.

## 1. [P1] Sorting positional arguments makes different verdicts share an identity

[Specification §10.1](../../architecture/ENGINEERING_LANGUAGE.md#101-two-identities-per-document),
lines 581–585, says every array is order-insensitive and no array carries
positional semantics. But
[invalid-product-kind](../../../examples/language/programs/invalid-product-kind.program.json),
line 51, already encodes primitive arguments as a positional array.

A counterexample uses the same form for the supported `interval.sub` rule:

```json
{"rule":"interval.sub","arguments":[{"ref":"initial_clearance"},{"ref":"length"}]}
```

Reverse the two references without changing anything else. With
`initial_clearance = 1/2 mm`, `length = enclosure [100,100] mm`, and a
requirement `>= 1/4 mm` without a scope constraint, the first program computes
`[-199/2,-199/2]` and must FAIL; the second computes `[199/2,199/2]` and must
PASS. Their full document hashes differ, but the specified semantic projection
gives both programs exactly:

`sha256:0882bf29ca923ae7b2a650e9dbd8e1d30ad0235ab6a0fe162555b1bb25310fb5`.

This is a collision introduced by the projection, not by SHA-256. It defeats
the promised connection between semantic identity, reuse, and replay.

**Closure:** define normalization by schema position. Preserve operand order,
or encode operands with semantic roles such as `lhs`/`rhs`. Enumerate the
collections that really are sets. Test subtraction reversal and a permitted
reordering separately. State when the sequential-reference authoring check
runs relative to identity/reuse; sorting `body` must not admit a forward
reference that §4 refuses.

## 2. [P1] Global annotation-key removal deletes semantic declarations

[Specification §10.1](../../architecture/ENGINEERING_LANGUAGE.md#101-two-identities-per-document),
lines 574–580, strips annotation names at every object level. User-defined
identifiers in maps can have those same names; the language does not reserve
them. These keys name declarations, not annotation fields.

Reproduction: rename `uniform-temperature-change` and its `assumes`
references to the legal proposition identifier `note`. Make two copies of
the library. In the first, declare `note.params = [geometry, scenario]`; in
the second, use `[material, scenario]`. Both parameter sets can instantiate
in `linear-expansion`, but they establish different scoped assumptions.

The projection removes the entire `propositions.note` declaration in both
copies. Both therefore receive:

`sha256:2a78d5fda47b2d9dcdfd570c2afb71c2d7f9967b5abd387dd4d8a1a3324608d3`.

Changing a proposition's parameters is explicitly supposed to change its
semantic identity. Similar collisions are possible in user-named input and
entity maps.

**Closure:** remove annotations only at schema-defined annotation positions,
preserving every declaration key and its semantic fields. Add identifier
cases named `note`, `label`, and `description`, including a semantic change
inside each declaration. A reserved-name policy would require an explicit
authoring refusal before hashing; silent removal is not a refusal.

## 3. [P1] The positive clearance program still drops an undeclared relation

[Specification §2.2](../../architecture/ENGINEERING_LANGUAGE.md#22-value-types-and-the-relation-map),
lines 106–117, retains extra argument relations, unions operand maps, and
requires an explicit `projects` declaration for removal. In the actual
[thermal library](../../../examples/language/libraries/thermal-expansion.v1.json),
lines 130–157, `clearance-difference` has no `projects` declaration and its
output contains only geometry and scenario.

The positive program passes a displacement carrying
`{geometry: bracket@2, scenario: thermal-soak-steady, material: al-6061-t6}`.
Subtracting it from the initial clearance retains all three keys under P-SUB.
The declared output silently loses `material`. The rule therefore refuses
the method application while the expectation still says clean/ready/PASS.
The heuristic signature has the same undeclared removal for these arguments.

**Closure:** carry material through the output, or specify and justify a
projection consistent with the meaning chosen in §2.2. Keeping material is
the direct representation of this example's dependence on expansion of that
material. Update the library, readable derivation, expectations, and pins
together. Validate complete relation maps at every application, including
extra relations supplied by actual arguments; validating result kinds alone
misses this failure.

## 4. [P2] Shared provenance does not refute measurement independence

[Specification E4b](../../architecture/ENGINEERING_LANGUAGE.md#7-primitive-inference-rules),
lines 429–435, now correctly requires affirmative support for independence,
but it also derives refuted independence from a shared source edge and
rejects an independence premise as contradictory. Sections 9 and 12 repeat
that implication. It does not follow, even with complete recorded provenance.

For a direct counterexample, let `Z` be uniform on `{0,1,2,3}`,
`X = Z mod 2`, and `Y = floor(Z/2)`. Both values depend on the same source
`Z`. Nevertheless each `(X,Y)` pair has probability `1/4`, equal to the
product of its marginals, so X and Y are independent. A shared administrative
or document source is an even weaker basis for refuting independence.

**Closure:** keep graph overlap and measurement dependence separate in both
directions. Shared edges can refute `provenance_disjoint`; they cannot alone
refute `independent`. A method may explicitly require disjoint provenance as
an additional admission policy, so the shared-source example can remain
blocked for that reason. Otherwise independence stays unresolved until its
own supported evidence determines it. Reserve `premise_conflict` for an
actual contradiction of the same proposition. Update the negative examples
and preservation argument to match that distinction.

## 5. [P2] Mixed nominal/enclosure arithmetic still lacks a defined value

[Specification §7.A](../../architecture/ENGINEERING_LANGUAGE.md#7-primitive-inference-rules),
lines 310–334, restricts the additive rule's premise to exact/enclosure
operands, then says a nominal operand produces a nominal result. The product
rule also claims nominal propagation. No numeric rule defines composition
of a representative scalar with an enclosure, and §3 still prohibits an
implicit enclosure-to-nominal conversion.

For example, the supported kind product
`dimensionless_factor × dose_rate → dose_rate` does not determine a single
representative result for nominal `2` times enclosure `[1,3]`. Returning
nominal `2`, `4`, or `6` requires an additional modeling choice; returning an
interval does not match §3's nominal representation. Removing `claim: any`
has not settled this newly stated mixed arithmetic behavior.

**Closure:** restrict these primitives to their fully defined exact/enclosure
operands, or define separate nominal signatures and any explicit
representative-selection operation. Give mixed operands a specified refusal
or a complete value rule, with a fixture. Keep the formal premises and prose
in agreement.

## 6. [P2] Lifecycle entries need a rule for overlapping scopes

[Specification §10.2](../../architecture/ENGINEERING_LANGUAGE.md#102-documents-and-bindings),
lines 622–634, permits both library-wide and method-specific entries but
does not define their composition into the single `method.lifecycle` premise.

Supply `library:thermal-expansion@1 = withdrawn` alongside
`method:thermal-expansion/linear-expansion@1 = active`. Both are permitted
entries. Whether the method-specific state overrides the library state or
the library withdrawal blocks the method is unspecified. Duplicate entries
with contradictory states and multiple simultaneous refusal reasons likewise
have no defined resolution.

**Closure:** define applicable scopes, uniqueness/conflict handling, and
combination of their states. Preserve applicable library and method refusals
unless an explicit policy establishes an override. Add overlapping-scope,
duplicate/conflicting-entry, and missing-material cases to the replay matrix.
Do not let iteration order choose authority; §10.1 treats this material as a
set. The historical/current-context distinction can remain as written.

## Reproduction and validation

The bounded review script is available in this workspace at
`/tmp/avila-core-el01-r2-review/reproduce.py`. From the repository root:

```bash
python3 -B /tmp/avila-core-el01-r2-review/reproduce.py
```

It uses the existing Python verifier's canonicalizer and a literal
transcription of r2's projection; it is not an engineering-language checker.
It saves both subtraction programs and both conflicting-identity libraries
under the same temporary directory. The reproductions are also described
above so their construction does not depend on retaining that directory.

Checks completed:

- All 26 JSON documents parse under the canonicalizer: 23 programs, two
  libraries, and expectations. Program/expectation coverage matches exactly.
- Both library document hashes, both semantic hashes, and all 23 program pins
  match. This verifies consistency of the current projection, not its safety.
- The two identity collisions reproduce with differing full document hashes.
- Inspection of the actual positive-program relation maps finds the missing
  `material` projection at `clearance-difference`.
- Exact arithmetic agrees with the thermal and measurement bounds. The
  shared-source independence counterexample satisfies all four joint/marginal
  probability equalities using exact fractions.

No builds, benchmarks, or engineering runs were launched. The review script
completed in under one second in this session. Existing revision files and
the unrelated draft case were not edited during the review.

## Next SWE-2 revision

Correct these rules and fixtures before EL-02. Make identity projection aware
of schema roles; include semantic-collision counterexamples, complete
relation-map checks, and the missing premise/lifecycle cases. Return the
revised normative decisions and validation results for review. Preserve the
already corrected scenario identity and witness-support rules.
