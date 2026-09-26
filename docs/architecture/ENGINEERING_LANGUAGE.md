# The engineering-language fragment — normative specification

- Governing decision: [ADR-0027](../adr/0027-engineering-language-fragment.md)
- Charter: [engineering-language charter](../roadmap/proposals/2026-09-25-engineering-language-charter.md)
- Profile: `avila.core/language/0.1-draft`
- Status: normative for the EL experiment; specifies a language that is not
  yet implemented. If an implementation and this document disagree, the
  disagreement is reported and adjudicated — neither side silently wins.
- Example programs: `examples/language/` — authored against this document
  alone; `expectations.json` names each program's specified result.
- Revision: r3 — revised after the
  [EL-01 specification review](../roadmap/reviews/2026-09-25-engineering-language-spec-review.md)
  and the
  [r2 follow-up review](../roadmap/reviews/2026-09-25-engineering-language-r2-review.md).

This document defines the smallest fragment that exercises the whole
architecture: related types, propagated assumptions, uncertainty
composition, method obligations, execution binding, and independently
replayed conclusions — over the charter's synthetic thermal-expansion /
clearance model and a second measurement library.

## 1. Judgments and the program lifecycle

A **program** is a document declaring: named entities (geometry revisions,
scenarios, materials), typed inputs with sources, explicit assumptions,
established premises, method applications bound to names, optional open
goals, and requirements. A **method library** is a document declaring
typed method signatures that compose the primitive rules of §7. Neither
contains an implementation hook that can mint a checked result.

The lifecycle has four phases; each emits a record, not a verdict:

```text
program + library
  → analyze  → analysis record: typed bindings, propagated assumptions,
               generated obligations, open holes, blocking findings,
               candidate sets
  → plan     → execution plan (only when analysis is plan-ready):
               invocations, input bindings, required observations,
               placed runtime obligations
  → execute  → observation set (the runner's domain): per invocation,
               observed output values, artifact digests, receipt digests
  → evaluate → derivation + requirement verdicts (the four states of
               campaign evaluation, unchanged in meaning)
```

`analyze` is a pure, total function over the program, the library, and the
profile. It always returns an analysis; an incomplete or inconsistent
program produces an analysis naming its holes and blockers, never a crash
or a partial success dressed as complete. `plan` requires all static
obligations discharged; `evaluate` requires every runtime obligation
discharged by an observation bound to the exact invocation that produced
it, or the requirement reports `not_evaluated` with the unmet obligation
named.

**No fifth verdict.** A program that cannot reach a plan emits analysis
findings, not a requirement result; a requirement whose premises cannot be
established evaluates `not_evaluated`. Blocking-at-analysis and
not-evaluated-at-runtime are different stages, both honest.

## 2. Sorts, types, and the claim dimension

### 2.1 Entities, relations, and scopes

Programs name finite identities:

| Sort | Written | Equality |
| --- | --- | --- |
| geometry revision | `g ::= name@rev` | identity equality |
| scenario | `s ::= name` (an entity carrying a declared scope kind) | identity equality |
| material / applicability identity | `m ::= name@rev` | identity equality |
| quantity kind | `k` from the library's declared kind vocabulary | kind equality |
| source / party | `p` | identity equality |

**Scenario identity and scope kind are different data.** A scenario is an
entity (`thermal-soak-steady`); its declared `scope` is a classification
drawn from a small lattice shipped by the profile — `steady-state`,
`transient`, and `any`, where `transient` and `steady-state` are
incomparable and both refine `any`. Signature variables over `scenario`
bind scenario *identity* and unify by equality: `hot-soak` and
`cold-soak`, both `steady-state`, are different entities and cannot
satisfy one variable. Scope acceptance applies only where a *scope kind*
is demanded — a requirement's `scope` field, or a signature
`scope_check` precondition — never to identity parameters.

### 2.2 Value types and the relation map

A **quantity type** is

```text
qty k ⊳ c   over   ρ
```

- `k` — quantity kind (equality required);
- `c` — **claim model**: `exact | nominal | enclosure` (§3);
- `ρ` — the **relation map**: a *partial* map over
  `{geometry, scenario, material}`. Each present key is bound to a
  concrete entity or to a signature variable.

**An absent key means the value is not parameterized by that relation** —
it carries no such claim, and no later use may silently attach one. Absent
is not a wildcard and not "unknown":

- a slot that names a parameter the argument does not carry **refuses**
  with `relation_absent` (an omitted relation cannot be relabeled at the
  consumer — e.g. a geometry-only clearance cannot fill a slot that
  requires a scenario-carrying displacement);
- a slot never inspects parameters it does not name; extra parameters the
  argument carries remain on the value;
- an expression's result carries the **union** of its operands' relation
  maps — any parameter present in an operand is present in the result
  (§7.A), so a method cannot produce a value whose declared map omits a
  relation its inputs carried unless the signature declares the
  projection;
- a signature **may** declare `projects: [param…]`, an authored semantic
  claim that the output genuinely does not depend on that relation; the
  output's map then omits it. Projection narrows the type, never the
  provenance: dependency edges recorded under the projected relation stay
  attached to the value (§8.3).

Method signatures quantify over entity variables:

```text
method m : ∀ ρ-vars . (x₁:τ₁, …, xₙ:τₙ) requires φ₁…φᵢ  ensures ψ₁…ψⱼ
                    assumes A   effects F   projects P?  ⇒  y:τ_out
```

where each `τ` binds a subset of the declared variables. Applying the
method unifies the argument types with the slots (§5) and produces the
signature's obligations instantiated at the applied entities (§6). A
signature's output must bind every relation key it declares to an entity
present in the operand union or its own `variables` — an output cannot
declare a relation the application never saw.

### 2.3 Restriction, not conversion

Two distinct rules, never conflated:

- **Equality parameters** (every key of `ρ`, the kind `k`, and the claim
  model, unless widened by §7.C3) unify exactly at every application
  boundary. Two different geometry revisions — or two different scenarios
  sharing a scope kind — are different *subjects*.
- **Domain restriction** moves a proposition from a larger declared domain
  to a contained one (§7.R2) — a weakening that is sound because the claim
  was already asserted over the whole. There is no generalization rule;
  no rule moves a claim to a domain the premises do not cover.

A change of geometry or coordinate frame is a *transport*: it is a method
like any other, requiring its own declared relation and justification.
This fragment ships no transport primitive; an attempted transport is an
unsupported obligation, never an identity.

## 3. Claim models and uncertainty composition

A claim model says what a value asserts about the physical subject:

| Model | Asserts | Used by |
| --- | --- | --- |
| `exact` | the quantity equals the stated rational under the stated premises | exact arithmetic; any enclosure slot after widening (§7.C3) |
| `enclosure` | the quantity lies in the stated rational interval `[lo, hi]` under the stated premises | bounded comparisons; interval composition |
| `nominal` | the stated rational is a representative estimate — *no bound claimed* | display, nominal-only consumers; cannot feed a bounded comparison |

They do not form one confidence ranking, and there is no universal claim
parameter: the set `{exact, nominal, enclosure}` is closed. The only
built-in widening is `exact → enclosure` (a point is a degenerate
interval). There is **no** rule producing an enclosure from a nominal —
that is the adversarial "promotion" case, refused at the consumer's type
check because a nominal does not carry the bound the slot requires.
Enclosure → nominal weakening is likewise absent: choosing a
representative point is a modeling decision that must be a declared
method, not a silent coercion.

Uncertainty dependencies ride on every derived value: a set of
distinguished *source edges* (§8.3). Two results computed from premises
sharing a source edge refute `provenance_disjoint` — and only that
proposition; whether a method's `independent` obligation is established
is a separate question defined by §7.E4, decided by attestation, not by
the recorded graph.

## 4. Terms

The authored program grammar (canonical JSON encoding in §10; a readable
notation used here):

```text
program      ::= entities ; inputs ; assumptions ; premises ; body ; requirements
body         ::= step*
step         ::= let x = apply m(args)
               | let x = infer P(args)                -- primitive inference (§7)
               | let x = import c(args)               -- imported evidence/assertion (§9)
               | let x = hole τ                       -- open authoring binding
               | let x = goal τ                       -- open binding, method search
assumption   ::= { asserts | denies } prop at ρ by p    -- explicit assumption term
premise      ::= prop at ρ established_by src [under Δ] -- established support (§9)
requirement  ::= require id: subject rel limit [scope q | scenario s]
args         ::= references to inputs and earlier step results; a step
                 result may only be referenced after the step that binds it
```

`infer` names a primitive rule directly (e.g. `infer interval.mul(a,b)`);
`apply` instantiates a library method; `import` names an externally
produced value with its declared provenance, assumptions, and (optionally)
a certificate the kernel can check. `hole` is an explicit unfinished
binding of a stated type — the program admits it is not complete. `goal`
is a `hole` that additionally requests bounded candidate search over the
pinned library: the analysis lists applicable methods for it (possibly
zero or more than one — ambiguity is named, never resolved by choice) and
the binding stays open until the author writes an `apply`.

**Propositions are scoped.** A proposition reference is a pair
`(id, ρ)` — a predicate identifier and the relation map it is stated
over, restricted to the `params` the proposition declares in the library's
`propositions` map. Equality
of propositions is equality of identifier *and* bound map:
`uniform-temperature-change@(bracket@2, thermal-soak-steady)` is not the
`uniform-temperature-change` of another scenario. Proposition identifiers
used by `assumes`, `requires`, `premises`, or `assumptions` must resolve
to the library's `propositions` map or to a profile built-in
(`independent`, `provenance_disjoint`); an undeclared identifier is a
finding (`undeclared_proposition`), not a fresh symbol.

`premise` declares an established proposition with attribution: the
asserting party or certificate (`established_by`), its scope `at ρ`, and
the residual assumptions `under Δ` it itself rests on. A premise can
discharge an obligation or a propagated assumption (§7.A3) by
scoped-proposition equality. Profile built-in predicates carry an explicit
argument set participating in that equality — `independent` is written
`{"proposition": "independent", "arguments": {"over": [...]}, "at": {...}}`
and is scoped by the shared relation params of its members.

Scoping: `let` bindings are sequential and single-assignment. A name
shadowed is a static error (ambiguity is a finding, not a resolution).

## 5. Unification and the application rule

Application is the language's central judgment:

```text
Γ ⊢ aᵢ : τᵢ'      τᵢ' ≼ τᵢ[vars ↦ applied entities]   for each input i
────────────────────────────────────────────────── (T-APPLY)
Γ ⊢ apply m(a) : τ_out  ⊣ obligations(m, applied)
```

`≼` is *compositional acceptance*, defined component-wise:

- **quantity kind `k`**: kind equality — `length ⋠ temperature` even
  where units coincide (a unit calculation never confers an engineering
  interpretation);
- **relation map `ρ`**: for every key the slot names, the argument carries
  that key and its entity unifies by identity — `g`, `s`, `m` are
  *identities* (`hot-soak` ≠ `cold-soak` despite equal scope). A missing
  required key is `relation_absent` (§2.2). Keys the slot does not name
  pass through unconstrained;
- **claim `c`**: `exact ≼ exact`, `exact ≼ enclosure`, `enclosure ≼
  enclosure`, `nominal ≼ nominal`; every other pair refuses.

A signature slot declaring `claim: enclosure` accepts `enclosure` or
`exact` (widened to `[x,x]`); `claim: exact` and `claim: nominal` accept
exactly their own model. There is no `any` claim and no implicit claim
ranking — the four pairs above are the entire acceptance relation.

Unification chooses nothing: a method identifier selects library data,
candidate sets are enumerated by declared applicability, and ambiguity —
two or more applicable methods, none a strict refinement of the others —
is an unresolved obligation requiring an authored selection. The checker
never picks the scientifically convenient method, never relaxes a
requirement, and never invents a method to close a goal.

## 6. Obligations

A method signature's obligations instantiate at application time. Each
has a *stage* where it must be discharged:

| Obligation | Stage | Discharge |
| --- | --- | --- |
| precondition `φ` on arguments or relations (`domain_containment`, `scope_check`, `independence`, `provenance_disjoint`) | analysis | a checked primitive or an established premise by scoped-proposition equality; `provenance_disjoint` additionally discharges/refutes from recorded edges (§7.E4a); otherwise an open hole — or refuted, when recorded facts contradict it |
| postcondition `ψ` stating the output relation (e.g. `out = c·ΔT·L`) | analysis when the method is primitive-composed; runtime when it is externally executed | the named check replayed over the (observed) values through the declared rules |
| runtime observation `ω` (the output was produced by this invocation, with these input digests) | execution | an observation bound to the invocation — receipt digest + output digests + value |
| assumption `a` carried by the signature | propagated onto the conclusion | discharged only by an established premise or checked inference witnessing `a` (§7.A3); else conditional |
| independence obligation `independent(S)` | analysis | only a scoped, attributable premise or certificate (§7.E4); never by recorded provenance alone |

A postcondition can never be dropped during lowering: lowering rewrites
the program's *surface*, not its obligations. An externally executed
method's `ψ` becomes a runtime check naming the primitive rules that
verify it — the rule sequence is fixed in the signature, so an adapter
cannot claim a certificate for a different relation than the one declared.

**Goal holes.** A `hole τ` or `goal τ` is an authoring state: `analyze`
reports it with its type and the obligations suspended on it; `plan`
refuses while any hole reachable from a requirement remains. A hole
unreachable from any requirement is reported but does not block —
incomplete programs are inspectable.

**Budgets and unsupported constructs.** An obligation the fragment has no
rule for (a transport, a statistical combination, an undeclared kind
product) is reported `unsupported` with its source location — an open
state, not a pass. Every inference engine counter is finite (§11); a
budget exhausted mid-search is an explicit `budget` outcome, distinct
from `unsupported` and from success.

## 7. Primitive inference rules

Premises are stated in the judgment; every rule's conclusion carries the
union of its premises' residual assumptions and the union of their source
edges, minus only assumptions a listed premise *discharges* under §7.A3.
Each rule names the authority that establishes its premises.

### A. Kinded exact and interval arithmetic (authority: kernel)

Additive rules require a shared kind, a shared unit, and compatible
relation maps — every kind declares a `canonical_unit` and admission
normalizes each value to it, so operands of one kind always coincide in
unit and the rules perform no conversion:

```text
a, b : same kind k ; same unit ; claims ∈ {exact,enclosure} ;
       ρₐ ∪ ρ_b consistent                                     [P-ADD/P-SUB]
─────────────────────────────
a ± b : kind k, claim = weakest-of(exact→enclosure), over ρ = ρₐ ∪ ρ_b
  +: [a₁+b₁, a₂+b₂]   −: [a₁−b₂, a₂−b₂]
```

Relation-map consistency: where both operands carry a key, its entities
are identical; the union keeps every carried key. Two `exact` operands
yield `exact`; any `enclosure` operand yields `enclosure`. **`nominal`
has no arithmetic**: there is no defined composition of a nominal
representative with an enclosure (which representative would the result
carry?), so a nominal operand leaves the rule *inapplicable* — an
`unsupported` finding naming the operand, not a synthesized value.
Nominal values exist as attributed assertions and as unchecked method
outputs only; nothing computes with them.

Multiplication is **kind-directed**: the product is defined only where
the library declares a row in `kind_products` —
`{lhs_kind, rhs_kind, result_kind, result_unit}`. The row names the
engineering meaning of the product; a unit string is not consulted.

```text
a : kₐ, b : k_b ; (kₐ × k_b → k_c, u_c) ∈ kind_products ; ρ consistent [P-MUL]
─────────────────────────────
a × b : kind k_c, unit u_c, over ρ = ρₐ ∪ ρ_b
  enclosure × enclosure: [min(S), max(S)], S = {a₁b₁, a₁b₂, a₂b₁, a₂b₂}
```

`P-MUL` applies componentwise to `exact` (degenerate intervals) and
`enclosure` operands and to nothing else — a `nominal` operand leaves it
inapplicable for the same reason as the additive rules. An absent
`kind_products` row is an
`unsupported` finding (`length × temperature` cannot silently mint a
torque). There is no division rule in this fragment. Expressions in
`ensures` are binary trees evaluated pairwise under these rules; an
expression elaborates only if every node's product/additive rule and the
final kind equal the declared output kind.

Containment is established *conditional on the premises*: if `a` truly
lies in `[a₁,a₂]` and `b` in `[b₁,b₂]`, the product lies in the computed
enclosure. The rule makes no empirical claim about the premises — those
stay attributed to whoever asserted them. Repeated use of one enclosure
operand treats each use independently (interval-arithmetic semantics);
shared-source correlation is the independence obligation's concern, not
the arithmetic's.

### B. Domain rules (authority: compiler)

```text
P holds over domain D ;   D' ⊆ D  declared compatible  [R2: specialize]
─────────────────────────────
P holds over D'
```

Specialization records the domain edge; no reverse rule exists.

```text
subject carries scenario s ; scope(s) ≼ scope-required                [scope]
─────────────────────────────
usable at the required scope
```

A steady-state result asked to serve a transient requirement fails this
check — `steady-state ⋠ transient` — and produces an explicit coverage
obligation, not a silent widening. A subject carrying no `scenario` key
at all cannot establish scope; the coverage obligation stays open.

### C. Claim-model rules (authority: kernel/compiler)

```text
x : exact over ρ                                       [C3: widen]
─────────────────────────────
x : enclosure [x,x] over ρ
```

The inverse direction does not exist. `nominal` has no widening in any
direction: promotion to enclosure is the adversarial case and refuses at
the consuming slot.

### D. Evidence and assumption rules (authority: kernel for discharge, runner for observation)

```text
E established under assumptions Σ_E                    [E1: use]
─────────────────────────────
E usable as premise; residual assumptions = Σ_E
```

```text
apply m ⇒ out:τ with signature assumptions A           [A2: propagate]
─────────────────────────────
out carries assumptions Σ_args ∪ inst(A)
```

Signature assumptions instantiate at the applied entities: a signature
`assumes` entry `uniform-temperature-change` whose declared params are
`{geometry, scenario}` becomes
`uniform-temperature-change@(applied g, applied s)`. A signature entry
whose declared params are absent from the application's relation map is
a library authoring finding, never silently dropped.

```text
C supported by (Σ_C, E_C) ;  a ∈ Σ_C
premise W establishes a' with a' ≡ a (scoped equality), support (Σ_W, E_W)
a ∉ Σ_W  and  W is not cyclic for a                                  [A3: discharge]
─────────────────────────────
C supported by ((Σ_C ∖ {a}) ∪ Σ_W,  E_C ∪ E_W)
```

Discharge keeps the *witness's* residual assumptions and dependency
edges — proving `a` under `{b}` retains `{b}` on the conclusion. A
witness is **cyclic for `a`** when its support, transitively through
other premises' support, contains `a`: `W` establishing `a` under `{a}`
self-depends; `W₁` under `{b}` combined with `W₂` establishing `b` under
`{a}` is a mutual cycle. Cyclic witnesses are inadmissible — recorded as
a `cyclic_witness` finding, treated as absent, and `a` remains an
undischarged assumption. Discharge can never manufacture an
unconditional claim from conditional support.

```text
S a declared evidence set                                     [E4a: provenance]
─────────────────────────────
provenance_disjoint(S): discharged when recorded source-edge sets are
pairwise disjoint; refuted when any pair shares a recorded edge;
open while recorded provenance is incomplete

independent(S):                                              [E4b: independence]
─────────────────────────────
discharged only by a scoped premise W ≡ independent(S) or a checked
certificate; otherwise open. Graph state neither discharges nor refutes
it — shared recorded edges are not evidence against statistical
independence.
```

The two propositions are different claims. `provenance_disjoint` is a
property of the recorded dependency graph and is decidable there.
`independent` is the measurement-independence claim a method may require
— it needs an affirmative, attributed basis *in either direction*: two
source identifiers that are merely *different* leave it open, and two
identifiers that are the *same* leave it open too. A shared calibration
certificate does not by itself establish dependence — for `Z` uniform
on `{0,1,2,3}`, `X = Z mod 2` and `Y = floor(Z/2)` are independent
despite sharing source `Z` — so shared
provenance refutes `provenance_disjoint` only. A method that treats
common calibration as disqualifying declares `provenance_disjoint` as
its own requirement (an admission policy), separately from
`independent`. `premise_conflict` (§9) is reserved for a premise
asserting a proposition the record itself refutes — e.g. asserting
`provenance_disjoint` over members with a shared recorded edge.

```text
context Σ ⊇ {a, denies a} at equal scope                       [X1: contradiction]
─────────────────────────────
contradiction finding; every use depending on either is blocked
```

Contradicted premises block — they do not make arbitrary conclusions
provable.

### E. Observation and binding rules (authority: runner/kernel boundary)

```text
invocation ι ran with input digests D_in, produced output digests D_out,
receipt ρ ; receipt bytes re-hash to ρ's declared digest               [O1: observe]
─────────────────────────────
observation(ι, D_in, D_out, ρ) — a bound fact, not yet a conclusion
```

```text
observation(ι, …) bound to application site α ; the signature's declared
check replays over observed values and holds                           [O2: discharge]
─────────────────────────────
postcondition obligation of α discharged
```

A receipt transplanted from another invocation or a recomputed hash over
foreign bytes fails `O1` at digest comparison; nothing downstream can
discharge `α`'s obligation. A missing receipt or output leaves the
obligation open and the requirement `not_evaluated` — pre-execution
refusal belongs to malformed plans, absent runtime observations to
`not_evaluated`.

### F. Requirement rules (authority: kernel; same meanings as campaign evaluation)

```text
subject : enclosure [lo,hi] ; limit : exact                          [V-*]
─────────────────────────────
≥:  lo ≥ limit → pass ; hi < limit → fail ; else inconclusive
≤:  hi ≤ limit → pass ; lo > limit → fail ; else inconclusive
```

Non-enclosure subjects (nominal, or unestablished) report `not_evaluated`
with the unmet premise named. The four-state vocabulary is exactly the
existing one; no new verdict state is introduced.

## 8. Propositions, dependencies, and requirements

### 8.1 What a requirement consumes

A requirement `require R: x ≥ l` is a use of the proposition "`x`'s value
is ≥ `l`" under all of `x`'s propagated premises. Its result is one of
`pass | fail | inconclusive | not_evaluated`, each with the derivation
applications that produced it — the same `context.bind → admissions →
verdicts` shape as ADR-0026, so existing replay machinery reads it
unchanged.

A requirement may declare `scope` (a scope kind) or `scenario` (an
identity), or both. `scenario` requires the subject's `scenario` relation
to be that entity; `scope` applies the §7.B scope check — a subject
without a `scenario` key cannot establish either and leaves the coverage
obligation open.

### 8.2 The proposition states, kept separate

| State | Meaning | Rule |
| --- | --- | --- |
| **established** | a checked inference discharged it, or it entered as attributed evidence | usable; carries attribution |
| **assumed** | declared by `assumptions` or propagated from a signature | conclusions are conditional on it; it is *listed*, never silently dropped |
| **open** | an obligation no rule/premise yet discharges (hole, unsupported, budget, unproven independence) | blocks planning of dependents; `not_evaluated` at runtime |
| **refuted** | recorded facts contradict it (shared provenance, failed containment) | a named finding; dependents cannot use it |
| **contradicted** | both `a` and `denies a` present at equal scope | blocks all dependent uses |

The analysis record reports each with source locations; the derivation's
premises carry the same state truth downstream.

### 8.3 Dependency edges

Every established/assumed proposition records the edges it rests on:
`input-digest`, `source-party`, `method-application`, `assumption`,
`premise`, `observation`, `certificate`. These edges are what
`derivation diff` and change-explanation consume: a changed input maps to
the applications whose premise edges touch it. Edges persist under type
projection — a value whose declared relation map omits `scenario` still
records the scenario edges of the inputs it was derived from.
`provenance_disjoint` (§7.E4a) reads this graph; distinct digests with a
common ancestor edge are one source.

## 9. The trust model

| Layer | Establishes | Never |
| --- | --- | --- |
| kernel | arithmetic correctness, kind/relation/claim compatibility, containment under premises, the verdict rule's output | that premises describe the world |
| runner | an invocation occurred with these input/output digests; a receipt document was issued at this instant | that the executable computed the declared relation |
| checked certificate | a re-computable proof the declared postcondition holds for the observed values (the named check replays) | more than its postcondition |
| named party | an *assertion* — attributed, scoped, conditional; admissible only where it does not contradict recorded facts | a checked result; assertion ≠ certificate |
| author | the program's declared assumptions, goals, and requirements | discharge of an assumption by writing it |

Importing an assertion preserves its party, scope, and declared
assumptions; importing a certificate additionally requires its replayable
payload. A signature establishes *who said it* — never that what they said
is true. An adapter's self-declared "pure" flag changes nothing (charter:
unrestricted external processes do not become pure by declaration).

An `import` must declare its full residual assumption set explicitly —
`assumptions: []` asserts the imported value is unconditional; an absent
field is malformed (the author has not stated the premise set at all).
At replay, the declared set is compared against the source record's
propagated assumptions: an assumption dropped in transit is a mismatch on
the `import` premise, not a vanished premise.

An assertion is **inadmissible** where it contradicts what the context
already records about that same proposition: a premise asserting
`provenance_disjoint(S)` over members whose recorded edges intersect, or
a scoped proposition denied by a recorded contradiction, produces a
`premise_conflict` finding rather than discharging anything. Asserting
`independent(S)` over shared-provenance members is *admissible* — shared
edges refute only `provenance_disjoint` (§7.E4). Assertions extend the
graph; they cannot overwrite it.

## 10. Canonical identity, context binding, and change dependencies

### 10.1 Two identities per document

Every language document has **two** declared identities:

- `document_sha256` — canonical JSON of the full document: a byte-level
  provenance handle (this exact record was supplied).
- `semantic_sha256` — canonical JSON of the document's **semantic
  projection**: the meaning-bearing content only.

The semantic projection is fixed by this profile and is **schema-directed**
— it walks the declared schema of each document kind, not the keys of
whatever document happens to arrive:

- **Annotation fields are removed only at declared annotation
  positions.** The annotation set is `{label, note, reason, statement,
  description, title, gloss}`, and it applies where the schema declares
  it: `title`/`description` on document metadata; `statement`/`note`/
  `reason` on `source` and `established_by` provenance objects; `reason`
  on an input's `binding` when its state is `unavailable`; `gloss`
  on `quantity_kinds.*` and `propositions.*` declaration values;
  `label`/`note`/`reason` on method, obligation (`requires`, `ensures`,
  `effects`), input, assumption, premise, requirement, and finding
  entries. **Keys of user-defined maps are identifiers, never
  annotation**: `propositions.note` is a proposition named `note` and is
  preserved with its `params`; an input or entity named `label` or
  `description` is likewise preserved. A field bearing an annotation
  name at a position the schema does not declare is **malformed** — an
  authoring finding, not a stripped field — so silent removal cannot
  launder a declaration.
- **Arrays are normalized by schema position.** Declared *sets* are
  sorted by canonical element encoding: `methods`, `kind_products`,
  `requires`, `ensures`, `assumes`, `effects`, `params`, `over`,
  `inputs`, `assumptions`, `premises`, `requirements`, and premise-level
  `assumptions` lists. Declared *sequences* preserve order: program
  `body` (sequential bindings — reordering can turn a forward reference
  into a resolved one), `infer`/`apply` positional `arguments`
  (reversing `sub(a,b)` yields a different value), and any tuple-like
  array the schema does not list as a set. An array at an undeclared
  position is schema-invalid, not guessed.
- **Admission precedes identity.** Semantic identity is defined only for
  documents admitted under the schema — including §4's
  sequential-reference check on `body`. The projection never repairs an
  inadmissible document into a hashable one, and no reuse decision is
  made on the identity of a document that would not analyze.

Programs pin a library by `semantic_sha256`; contexts record the
library's `semantic_sha256` as identity and may record `document_sha256`
as provenance. Renaming a precondition's `label`, reordering `methods`,
or rewriting a `gloss` therefore produces an identical semantic identity
and identical derivations; adding a `requires` entry, changing a kind,
changing declared `params`, or reordering positional operands changes
the semantic identity — a new library in every sense that matters.
Presentation edits can neither alter a conclusion nor manufacture a
changed premise (§8.3), and the promise runs no further: byte-level
document identity legitimately changes with presentation.

### 10.2 Documents and bindings

Documents: `avila.core/language-program/v0.1-draft`,
`avila.core/method-library/v0.1-draft`, and the produced
`avila.core/language-analysis/v0.1-draft`,
`avila.core/execution-plan/v0.1-draft` — canonicalized by the existing
JSON canonicalization.

Lowering to the derivation machinery reuses ADR-0026 unchanged: the
evaluation context binds `{profile, evaluator, compiled snapshot,
library identity, claims/evidence identity, execution policy,
qualifications, lifecycle, observations, receipts}`; each §7 rule
application is a `RuleApplication` with named premises, states, and
conclusion. `context_sha256`/`derivation_sha256` give the same identity
guarantees; `explain_derivation_changes` answers "which uses changed"
over the dependency edges of §8.3.

**Profile negotiation.** Program and library each carry a `profile`
field; binding requires both to equal the invoked profile, and the context
record carries it as `semantic_profile`. A program or library naming a
different profile — including `avila.core/semantic/0.2-draft` — is refused
at bind, not reinterpreted. Profile identifiers are never stripped or
relabelled on import.

**Lifecycle material.** The context's `lifecycle` set is *supplied
material* (like qualifications): entries keyed `library:<name>@<rev>` or
`method:<library>/<id>@<rev>` with state `active | superseded | expired |
withdrawn` (and `superseded_by` where applicable).

*Applicable entries.* For `apply m` of library `L@r` the applicable
entries are the `library:L@r` entry and the `method:L/m@r` entry —
both scopes, not whichever an iteration finds first. The
`method.lifecycle` premise records every applicable entry's state.

*Combination.* States combine by union of refusals: if **any**
applicable entry is `expired` or `withdrawn`, the application's
conclusions are unusable and dependents are `not_evaluated.<state>` —
and when several refusals apply, the premise names *all* of them (a
method-specific `active` never overrides a library-level refusal; there
is no precedence, only accumulation). `superseded` records a notice,
not a refusal. `active`/`superseded` alone discharges the premise; no
applicable entry at all records state `absent` — usable, visible, and
not an implicit `active`. Two entries at the *same* key with different
states are malformed material — a `lifecycle_conflict` refusal at bind,
never resolved by iteration order; same-key duplicates with equal states
collapse (set semantics). The pinned program and library documents never
change: a narrowed, expired, or withdrawn method changes *current*
evaluations only, while historical derivations replay under their own
recorded context and stay intact.

Lowering preserves meaning stage by stage: `analyze` may reorder for
display but never drop a premise; `plan` may schedule but must carry every
runtime obligation to its invocation; `execute` may substitute observed
values for symbolic ones only inside a dischargeable check. A lowering
step that cannot preserve an obligation must refuse, not omit.

**Presentation-only changes.** Per §10.1: editing annotation fields or
reordering declared-set arrays yields identical semantic identities and identical
derivations — document digests may differ, and that is honest.

Source maps: every term, relation key, obligation, and finding carries
byte spans into the authoring document. Editor-facing explanations are
built from these spans, not reconstructed prose.

## 11. Finite limits

All bounds are parameters of the profile; exceeding any is a bounded
`CORE-E…`-style refusal — explicit, not silent.

| Limit | Purpose |
| --- | --- |
| `MAX_TERMS` (program steps + subterms) | elaboration depth |
| `MAX_APPLICATIONS` (rule applications per derivation) | the ADR-0026 bound |
| `MAX_GOALS` (open holes) | authoring surface |
| `MAX_NUMERIC_WORK` | the kernel's 128-bit rational budget |
| `MAX_CANDIDATES` | method-search size; ambiguity beyond remains an authored choice |
| `MAX_DEPENDENCY_EDGES` | change/dependency graph |
| `MAX_WITNESS_DEPTH` | assumption-discharge chain length before `cyclic_witness`/`budget` |

## 12. Preservation argument (by cases over §7)

Claim: for each primitive rule, if its premises hold under their stated
attributions, its conclusion holds under the union of the premises'
residual assumptions — i.e., a checker's acceptance never manufactures a
premise it did not check.

- **P-ADD/P-SUB/P-MUL**: rational arithmetic and componentwise interval
  enclosure are containment-sound by construction of rational intervals;
  the conclusion inherits exactly the premises' assumption set. The
  *kind* of a product is declared data (`kind_products`), not derived:
  the rule guarantees containment of the computed interval, not that the
  declared kind is physically right — correctness of a product row is an
  unproved library obligation (named, attributed, and visible in the
  derivation). Union semantics for relation maps is monotone (no operand
  relation is lost), and `projects` declarations are the only removal —
  visible in the signature.
- **R2 (specialize)**: `D' ⊆ D` means every point of `D'` is a point of
  `D`, so a claim over `D` holds over `D'`. Direction matters; the
  converse is absent. *Trusted:* domain comparison is implemented as
  declared containment, not ad-hoc.
- **scope check**: `≼` on the declared lattice is a finite order check;
  refusal when the subject lacks a scenario is forced (there is no
  scenario to inspect — open, not assumed).
- **C3 (widen)**: `[x,x]` is a degenerate interval; the claim changes
  only in presentation. There is no `enclosure → exact` and no nominal
  promotion; the refusal is the theorem.
- **A2/A3 (propagation/discharge)**: conclusions carry the union of
  premises' assumptions, and discharge substitutes the witness's support
  for the discharged assumption. Soundness: the resulting support proves
  the conclusion *without* `a` only if the witness's support does not
  itself need `a` — which the acyclicity check enforces exactly.
  Cyclic witnesses are inadmissible rather than "empty support".
  *Unproved:* the cycle check must traverse the full premise graph —
  `MAX_WITNESS_DEPTH` bounds it, and exhaustion is a `budget` outcome,
  not silent acceptance.
- **E4a/E4b**: `provenance_disjoint` is decidable over the recorded graph
  and nothing more is claimed. `independent` requires an affirmative
  premise or certificate and is *never* decided by the graph — shared
  edges refute only disjointness (`X = Z mod 2`, `Y = floor(Z/2)` are
  independent while sharing `Z`), so an assertion of `independent` over
  shared provenance is admissible, and `premise_conflict` fires only
  when the asserted proposition is itself the one the record refutes.
  *Unproved:* recorded-graph completeness depends on authors/libraries
  declaring provenance — `provenance_disjoint` is sound relative to the
  recorded graph, not omniscient.
- **O1/O2 (observe/discharge)**: binding is by digest equality to the
  invocation's recorded inputs/outputs; a foreign receipt or recomputed
  outer hash cannot satisfy the digest premises. *Trusted:* the runner's
  observation record and the digest functions.
- **V-***: the four-state comparison over `[lo,hi]` vs exact `limit` is
  decidable and total; `not_evaluated` covers everything else.
- **Identity projections (§10.1)**: the projection is schema-directed —
  annotation fields drop only at declared positions (map keys and
  undeclared positions are never silently stripped), declared sets sort
  while declared sequences (`body`, positional `arguments`) keep order,
  and admission precedes hashing. Every semantic field survives the
  projection, so equal semantic identities of admitted documents imply
  equal rule inputs — and derivation equality implies replay
  equivalence. *Unproved:* the annotation positions and set/sequence
  classification are declarations fixed by this profile; a schema edit
  that misclassifies a position changes identity semantics — caught by
  review of the schema, not inferred from documents.

**What is deliberately not claimed:** that declared physical premises are
true, that a certificate's postcondition is the *right* postcondition for
the engineering question, that a declared kind-product names the right
engineering meaning, that declared provenance is complete, or that a PASS
under this profile is scientific validation. The fragment guarantees
conditional soundness — premises in, conclusion justified — nothing more.

## 13. The experiment instantiation (fixture expectations)

Library `thermal-expansion@1` declares (readable form; canonical JSON in
`examples/language/libraries/`):

```text
kinds:  length (mm), temperature (K), thermal_expansion_coefficient (1/K),
        strain (1)
kind_products:
  thermal_expansion_coefficient × temperature → strain        (unit 1)
  strain × length                   → length                  (unit mm)

linear-expansion : ∀ g s m .
    (length:            qty length ⊳ {exact|enclosure} {geometry: g},
     coefficient:       qty thermal_expansion_coefficient ⊳ {exact|enclosure} {material: m},
     temperature_change: qty temperature ⊳ {exact|enclosure} {scenario: s})
  requires  domain_containment(s.operating_domain, m.applicability)
  ensures   output = coefficient * temperature_change * length
            — elaborates (c×ΔT): strain {m,s}; ×L: length {m,s,g}; equals output kind
            (check: interval_arithmetic)
  assumes   uniform-temperature-change@{g,s}, linear-expansion-model@{m,s}
  projects  — (output declares {geometry,scenario,material} — the full union)
  effects   process_spawn, write_output
  external  executable synthetic/linear-expansion@1

clearance-difference : ∀ g s m .
    (initial_clearance: qty length ⊳ {exact|enclosure} {geometry: g},
     displacement:      qty length ⊳ {exact|enclosure} {geometry: g, scenario: s, material: m})
  ensures   output = initial_clearance - displacement  (check: interval_arithmetic)
  assumes   —
  primitive interval.sub(initial_clearance, displacement)  ⇒ output {g,s,m}
            — material stays on the result: the answer depends on which
              material expanded
```

`clearance-heuristic` carries the same corrected signature (its output
stays `nominal`, so it still cannot serve a bounded requirement).

Under the charter's inputs — `L=100 mm` exact, `c=1/10000 K⁻¹` exact,
`ΔT=[10,20] K` enclosure, `clearance=1/2 mm` exact — the rules, not an
implementation, fix the expectations:

```text
c·ΔT                 = [1/1000, 1/500] strain  over {m,s}        (P-MUL row 1)
(c·ΔT)·L             = [1/10, 1/5] mm length   over {g,s,m}      (P-MUL row 2)
displacement          = [1/10, 1/5] mm          (observed ≡ checked at runtime)
remaining_clearance  = [1/2,1/2] − [1/10,1/5] = [3/10, 2/5] mm over {g,s,m} (P-SUB)

require ≥ 1/4 mm   → pass          (3/10 ≥ 1/4)
require ≥ 7/20 mm  → inconclusive  ([3/10,2/5] straddles 7/20)
require ≥ 9/20 mm  → fail          (2/5 < 9/20)
require with a missing runtime observation → not_evaluated
```

Residual assumptions on `remaining_clearance`:
`{uniform-temperature-change@(bracket@2,thermal-soak-steady),
  linear-expansion-model@(al-6061-t6,thermal-soak-steady)}` — scoped to
the applied entities, propagated unchanged through
`clearance-difference`. A declared premise can discharge either: an
unconditional witness removes it; a witness under `{fixture-symmetric}`
leaves `fixture-symmetric@(bracket@2)` residual. The
domain-containment precondition is a static obligation discharged by
`[253,313] ⊆ [223,423]`; an out-of-domain scenario refutes it and blocks
the plan.

Second library (`measurement-scaling@1`): `scaled-sum` computes
`calibration × (reading_a + reading_b)` via `dimensionless_factor ×
dose_rate → dose_rate` and requires two separable obligations:
`independent(reading_a, reading_b)` — discharged only by an attested
premise or certificate, and never decided by graph state (§7.E4b) — and
`provenance_disjoint(reading_a, reading_b)` — an admission policy
discharged or refuted by recorded edges (§7.E4a). Four fixed outcomes
exercise the split: shared calibration edge → disjointness refuted
(independence untouched); distinct edges with no premise → independence
open; a scoped assertion by a named party → discharged, with that
premise's support and attribution retained; asserting `provenance_disjoint`
over shared edges → `premise_conflict`. No statistical-combination rule
exists to close the gap — that is the generality test: same rules, no
case-specific branch.
