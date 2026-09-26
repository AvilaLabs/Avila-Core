# EL-02 implementation review

Reviewed the uncommitted implementation on `main`, based on `d05e27a`,
against the r3 [language specification](../../architecture/ENGINEERING_LANGUAGE.md)
and [EL-02 work order](../ENGINEERING_LANGUAGE_HANDOFF.md#el-02--implement-shared-analysis-and-obligation-generation).

**EL-02 needs revision before acceptance. Keep EL-03 pending.** The shared
entry point and example coverage are useful, but several central guarantees
still depend on trusting document declarations. The actual API accepts false
postconditions, promotes a nominal payload to an enclosure, loses witness
support, and produces different values under equal semantic identities.

## Verification and reproduction

Independently ran:

```bash
cargo test -p avila-core-compiler --test language_fixtures --locked -j 1
cargo build -p avila-core-cli --bin avila-core --locked -j 1
python3 docs/roadmap/reviews/2026-09-25-engineering-language-el02-review/reproduce.py --check-depth
```

All 10 provided language tests passed, including the comparison of all 24
authored programs. The CLI was rebuilt from the reviewed source. The
[reproduction script](2026-09-25-engineering-language-el02-review/reproduce.py)
calls its thin `language analyze` client of `analyze_program`; it does not
run engineering cases. It writes mutated documents and complete analysis
responses into `/tmp/avila-core-el02-review/results/`. Library mutations
are repinned before judging acceptance, so the failures do not rely on
ignoring an unrelated pin-mismatch finding.

The [captured observations](2026-09-25-engineering-language-el02-review/observations.json)
record the reviewed behavior. Each CLI child is limited to 384 MiB of
virtual memory and five CPU seconds, with an eight-second timeout and core
dumps disabled. The optional depth probe reproduces a process stack overflow
under those limits. No workspace-wide test sweep, benchmark, solver, or
engineering runner was launched during this review.

Reviewed source SHA-256 values:

| File under `crates/avila-core-compiler/src/language/` | SHA-256 |
| --- | --- |
| `analyze.rs` | `a3091222e529ea3749043371c0329ba91b272f9e867c15bdb8eec37196421aa8` |
| `document.rs` | `ea473d1cebaa0e19581d22e81d04e79977b5f38ee072a63103d09ff3986bb21f` |
| `eval.rs` | `e7142d91cc27dca5f9cbfe4d240afffbc6f565851047f4378e030db9b59e0d5b` |
| `model.rs` | `d6806518d2a1e872f09aef3e2a13f3de2ba992902b8b3112abda6bc5d8f35fd5` |
| `projection.rs` | `2358107f7c21dbde42cc34e91394a7b3de4867948dac24bddc3d0223f71354b0` |

## Findings

### 1. P1 — Primitive applications manufacture checked postconditions and output types

In [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs),
lines 1376–1450 evaluate a primitive body, discard its inferred type, and
mark every `ensures` entry discharged solely because the implementation
kind is `primitive`. Lines 1502–1505 then install the signature's output
kind and claim without checking the body against them.

`false-postcondition` changes `scaled-sum`'s body to
`interval.sub(reading_a, reading_b)`. The API returns a clean, ready plan
and established `[-1/10, 3/10]`, while reporting that
`output = calibration * (reading_a + reading_b)` was checked and discharged.
That relation requires `[187/100, 231/100]`. `wrong-body-type` changes the
body to `calibration`: a dimensionless exact `11/10` is relabeled as a
scenario-carrying dose-rate enclosure. An unrecognized function and checker
in `ensures` are also reported discharged (`unrecognized-postcondition`).

**Required correction:** elaborate library bodies and every postcondition
against the declared signature and supported check vocabulary; validate
inferred kind, claim, relations, and unit before establishing an output.
Discharge a postcondition only through its successful supported check.
Validate unused declarations as well as applied ones. Also cover
`undeclared-method-assumption`: an undeclared name in a method's `assumes`
currently produces a clean plan instead of `undeclared_proposition`.

### 2. P1 — Equal semantic identities do not determine equal analysis semantics

In [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs),
`load_inputs` inserts into the environment without a uniqueness check
(line 783), and external outputs use `method.ensures.first()` (line 1396).
Both collections are declared sets in the projection. The library indexes
at lines 461–479 also silently overwrite duplicate method/product keys.

`duplicate-input-a` and `duplicate-input-b` contain the same two conflicting
definitions of `reading_a`, in reversed set order. Both are clean and ready
with the **same program semantic hash**, but `combined_rate` changes from
`[572/25, 121/5]` to `[187/100, 231/100]`. Single assignment was never enforced.
`ensures-order-a/b` use two differently ordered postconditions with the
**same library semantic hash**; displacement changes from `[1/10, 1/5]` to
`100`. The corresponding declared clearances are `[3/10, 2/5]` and
`[-199/2, -199/2]`.

**Required correction:** reject conflicting identities and shadowed names
before building maps. Give every declared set order-independent meaning,
including multi-witness and multi-postcondition handling; sorting only the
hash input is insufficient. Semantic admission must precede publishing an
admitted semantic identity. `forward-reference-identity` currently returns
one even after the sequential-reference check reports malformed input.
Test permutation properties through `analyze_program`, not just projection.

### 3. P1 — A payload can claim a stronger type without establishing it

In [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs),
lines 758–795 decode the input type and numeric binding independently.
`value_of` (line 1964) maps both exact and nominal payloads to the same
numeric variant. No admission check connects payload claim or unit to the
declared quantity type; imports have the same separation.

`nominal-payload-enclosure-type` changes only `reading_a.binding.value` to
a nominal `1`, leaving its declared enclosure type intact. The API reports
clean/ready and computes the enclosure `[99/50, 11/5]`. This bypasses the
very nominal-to-enclosure refusal EL-02 was required to enforce.
`wrong-input-unit` supplies `kg` for a dose-rate binding and is also clean.

**Required correction:** construct admitted typed values through one checked
boundary. Enforce the closed claim acceptance relation, valid value shape,
declared kind vocabulary, and the profile's unit rules for inputs and imports.
A typed field must not override a weaker payload. Unsupported unit conversion
must remain explicit rather than silently reinterpreting the numeric value.

### 4. P1 — Application context checks accept incompatible relations and domains

In [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs),
lines 1317–1324 merge operand relation maps with first-entry wins. The
external expression evaluator's errors are discarded at lines 1396–1401.
`check_domain_containment` (lines 1747–1798) compares endpoints without
validating interval ordering, units, or the referenced field names.

`conflicting-extra-relation` gives `length` a second material identity while
`coefficient` retains the original one. These are incompatible carried
relations under §7.A. The API instead returns clean/ready, chooses one
material, and silently loses the declared output value. Both
`wrong-domain-unit` (a kilogram operating domain against Kelvin applicability)
and `inverted-operating-domain` (`[400,300]`) discharge containment and
produce clean, ready plans.

**Required correction:** unify the complete operand relation union before
projection, check the output's variable substitution, and retain expression
failures as findings. Domain propositions require admitted, compatible
intervals and resolved field paths. Distinguish missing/open premises from
checked false containment.

### 5. P1 — Unattributed premises and unreplayed certificates enter as established

In [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs),
lines 1689–1711 discharge independence by finding matching proposition text,
scope, and members; they do not require admissible attribution. The general
assumption discharge at line 1873 likewise does not establish the witness's
authority. The import path at lines 1556–1575 unconditionally establishes any
well-shaped numeric payload with an explicit assumption list.

`unattributed-independence` deletes `established_by` from the witness and
still discharges independence with a clean plan. `unreplayed-certificate-import`
supplies a certificate with an all-zero digest, an invented check name, and
no replayable payload. Its imported enclosure is established, unconditional,
and plan-ready, with **zero runtime obligations**.

**Required correction:** enforce distinct admission paths for attributed
assertions and checked certificates. Missing attribution cannot discharge;
unsupported certificate replay must refuse or remain an explicit unmet
obligation. Validate scoped proposition parameters and witness admissibility
at every discharge site, including independence. Do not let an author mint
established support merely by choosing a `kind` string.

### 6. P1 — Independence witnesses lose their support, and provenance is split across inconsistent stores

In [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs),
independence discharge only emits a report (line 1702). The result at lines
1510–1518 unions operand sources and signature assumptions, excluding the
obligation witness. Provenance checks read `self.edges` (line 1650), which
is populated for inputs and imports but not for `infer` or `apply` results.

The unmodified `positive-measurement-pass` already demonstrates the loss:
`combined_rate` omits both `separate-instrumentation` and `indep-memo-7`,
although its independence witness explicitly depends on them. The provided
fixture test checks residual support only on the clearance rows, so it misses
this. `derived-provenance` uses an inferred reading whose recorded source
remains disjoint from `reading_b`; the analyzer incorrectly reports provenance
incomplete because the derived binding is absent from its separate edge map.

**Required correction:** represent discharged premises as support-bearing
values and merge their assumptions and complete attribution/dependency edges
into every dependent conclusion. Have provenance checks consume that same
graph, including derived bindings and transitive ancestors. Assert the full
measurement witness support and test both shared and disjoint derived sources.

### 7. P1 — Analysis is not total or bounded over admitted expression strings

The parser in [eval.rs](../../../crates/avila-core-compiler/src/language/eval.rs)
recurses through `parse_factor → parse_expr` at lines 116–124 without a depth
or work counter. The new language module contains none of §11's term, goal,
candidate, dependency, or witness-depth budget enforcement, and no `budget`
finding. A JSON nesting limit does not limit nesting inside expression strings.

`bounded-deep-expression-child` wraps `reading_a` in 10,000 pairs of
parentheses, about 20 KiB of expression text. The CLI aborts with exit `-6`
and `fatal runtime error: stack overflow, aborting`, instead of returning an
analysis refusal. This was reproduced in the limited child described above.

**Required correction:** specify concrete finite profile bounds and enforce
them before recursive descent/evaluation and during graph/search work. Return
a distinct budget outcome, preserving the distinction from unsupported rules
and invalid arithmetic. Add bounded regressions at the shared API boundary.

### 8. P2 — Blocking causes do not propagate to dependent bindings

In [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs),
`blocked` is indexed by the directly affected binding (line 1482). Neither
`infer` nor ordinary application carries those causes forward, and requirement
analysis consults only the final subject's key (line 2104).

`open-obligation-dependent` removes the measurement independence witness,
adds `dependent = interval.add(combined_rate, reading_a)`, and requires the
new binding. The whole plan is refused by the upstream finding, but the
dependent is reported `established` and its requirement `pending`, losing the
known unmet-obligation cause. Conversely, `unreachable-hole` adds an unused
hole to a valid clearance program and incorrectly refuses planning; §6 says
an unreachable hole remains inspectable without blocking the requirement.

**Required correction:** propagate all blocking premise causes through the
dependency graph and compute requirement reachability for planning. Keep open,
refuted, contradicted, unavailable, and runtime-pending distinctions intact.
Exercise chains through both `apply` and `infer`, with an unaffected sibling
requirement and an unreachable hole as positive counterparts.

### 9. P2 — Requirement admission omits constraints and rejects supported forms

`RequirementDecl` in [document.rs](../../../crates/avila-core-compiler/src/language/document.rs)
(line 463) requires `scope`, has no `scenario`, and uses unconstrained strings
for comparison. `requirement_reports` in
[analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs)
(line 2072) never validates the limit or comparison and checks scope by literal
equality (line 2154), rather than the specified acceptance lattice.

`malformed-requirement` uses comparison `potato` and limit `not-a-number`;
it is clean and ready. `scope-refinement` asks a steady-state subject to serve
scope `any` and is incorrectly refused. `requirement-scenario` supplies a
valid matching scenario identity and is refused as an unknown field.

**Required correction:** admit only supported comparisons and exact, compatible
limits; implement scope refinement and scenario identity demands from §§2/8.
Keep numeric verdict evaluation at its later stage. Also reconcile the new
analysis `verdict: not_evaluated` representation with §1's explicit separation
of analysis findings and requirement results; document any agreed semantic
change through the required ADR/spec process instead of changing fixture
interpretation silently.

### 10. P2 — Unknown lifecycle states are treated as usable

In [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs),
lines 1844–1865 only identify `expired` and `withdrawn`; every other nonconflicting
string becomes `usable`. `unknown-lifecycle-state` supplies the typo
`library:thermal-expansion@1=withdrawnn` and receives a clean, ready analysis.

**Required correction:** validate the closed lifecycle state and key grammar
at the shared API boundary. Retain each applicable entry's actual state and
the superseded notice as §10.2 requires; the current `usable` report loses
those distinctions. Test invalid states alongside the existing conflict and
union-of-refusals cases.

### 11. P2 — The returned artifact does not yet carry the promised executable plan or source graph

`PlanStepReport` in [analyze.rs](../../../crates/avila-core-compiler/src/language/analyze.rs)
(line 228) contains only `bind`, `operation`, and a display `detail` string.
`plan_steps` (line 2047) drops argument bindings. `ObligationReport` stores
postconditions/checks as prose, without a typed rule expression or operand
references. Findings have a textual `at` field (line 95) but no document
identity plus byte span; binding sources are flat authored edge labels,
without the application/premise links required for source-linked explanations.

For example, the returned measurement plan says only
`{bind: combined_rate, operation: apply, detail: scaled-sum}`. The thermal
plan similarly omits its argument mapping, executable/effect binding, and
structured runtime check payloads. Runner integration would have to reconstruct
these semantics from the original documents and prose rather than consume a
lowering that preserves them.

**Required correction:** finish the bounded, structured analysis/plan boundary
required by EL-02 and the r3 closure note: resolved argument bindings, supported
rule/check expressions, obligation-to-invocation links, and source spans and
dependency links. Execution and independent replay remain EL-03/EL-04; this
finding does not request implementing those milestones early.

## Instructions to hand to SWE-2

Revise EL-02 against this review before beginning EL-03. Start with a real
semantic admission phase and checked internal representations: construct
typed values, admissible witnesses, and discharged obligations only after
their invariants have been established. Then make generic application and
inference consume those representations and preserve their support. Avoid
fixes keyed to fixture IDs or method names.

Use the reproduction cases above as API regression inputs, deriving expected
outcomes from the specification. Add positive counterparts for supported
claims, attributed assertions, valid certificates when supported, compatible
scopes, and disjoint derived sources. Compare complete support and structured
obligations, not only finding presence, arithmetic values, or whether some
runtime obligation exists. Add order-invariance and budget properties.

Record the finding-by-finding closure, any proposed specification adjudication,
and validation results. Keep existing behavior under its current profile and
leave the unrelated `case-010-matmul-rank/` draft untouched. Return the revision
for independent review before claiming EL-02 complete or proceeding to EL-03.

This review added only its report and reproduction material. The implementer's
source, fixtures, and handoff changes remain unmodified and uncommitted.
