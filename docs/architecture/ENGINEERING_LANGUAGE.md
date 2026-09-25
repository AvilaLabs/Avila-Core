# The engineering-language fragment — normative specification

- Governing decision: [ADR-0027](../adr/0027-engineering-language-fragment.md)
- Charter: [engineering-language charter](../roadmap/proposals/2026-09-25-engineering-language-charter.md)
- Profile: `avila.core/language/0.1-draft`
- Status: normative for the EL experiment; specifies a language that is not
  yet implemented. If an implementation and this document disagree, the
  disagreement is reported and adjudicated — neither side silently wins.
- Example programs: `examples/language/` — authored against this document
  alone; each names its specified result.

This document defines the smallest fragment that exercises the whole
architecture: related types, propagated assumptions, uncertainty
composition, method obligations, execution binding, and independently
replayed conclusions — over the charter's synthetic thermal-expansion /
clearance model and a second measurement library.

## 1. Judgments and the program lifecycle

A **program** is a document declaring: named entities (geometry revisions,
scenarios, materials), typed inputs with sources, explicit assumptions,
method applications bound to names, optional open goals, and requirements.
A **method library** is a document declaring typed method signatures that
compose the primitive rules of §7. Neither contains an implementation
hook that can mint a checked result.

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

### 2.1 Entities and relations

Programs name finite identities:

| Sort | Written | Equality |
| --- | --- | --- |
| geometry revision | `g ::= name@rev` | identity equality |
| scenario | `s ::= name (carrying a scope kind)` | identity equality |
| material / applicability identity | `m ::= name@rev` | identity equality |
| quantity kind | `k` from the library's declared kind vocabulary | kind equality |
| source / party | `p` | identity equality |

**Scope kinds** are drawn from a small declared lattice — the fragment
ships `steady-state`, `transient`, and `any` — where `transient` and
`steady-state` are incomparable and both refine `any`. A value scoped to
`steady-state` may be consumed where `any` is required; it may not be
consumed where `transient` is required. This is the scope-mismatch rule
the adversarial matrix exercises.

### 2.2 Value types

A **quantity type** is

```text
qty k ⊳ c   over   σ = (g, s, m)
```

- `k` — quantity kind (equality required);
- `c` — **claim model**: `exact | nominal | enclosure`, specifying what
  the value claims (§3);
- `σ` — the **relation signature**: the geometry revision the quantity
  describes (`g`), the scenario scope it is scoped to (`s`), and the
  material/applicability identity it is about (`m`). Each component may be
  a concrete entity or a signature variable bound by the method.

Method signatures quantify over `σ` variables:

```text
method m : ∀ g s m . (x₁:τ₁, …, xₙ:τₙ) requires φ₁…φᵢ  ensures ψ₁…ψⱼ
                   provides assumptions A   effects F   ⇒  y:τ_out
```

where each `τ` may mention the bound variables. Applying the method
unifies the argument types with the signature (§5) and produces the
signature's obligations instantiated at the applied entities (§6).

### 2.3 Restriction, not conversion

Two distinct rules, never conflated:

- **Equality parameters** (`g`, `s`, `m`, `k`, and the claim model, unless
  widened by §7.C3) must unify exactly at every application boundary. Two
  different geometry revisions are different *subjects*, not different
  representations of one subject.
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

They do not form one confidence ranking. The only built-in widening is
`exact → enclosure` (a point is a degenerate interval). There is **no**
rule producing an enclosure from a nominal — that is the adversarial
"promotion" case, refused at the consumer's type check because a nominal
does not carry the bound the slot requires. Enclosure → nominal weakening
is likewise absent: choosing a representative point is a modeling decision
that must be a declared method, not a silent coercion.

Uncertainty dependencies ride on every derived value: a set of
distinguished *source edges* (§8.3). Two results computed from premises
sharing a source edge are *not independent*; §7.E4 governs what that
premise may claim.

## 4. Terms

The authored program grammar (canonical JSON encoding in §10; a readable
notation used here):

```text
program      ::= entities ; inputs ; assumptions ; body ; requirements
body         ::= step*
step         ::= let x = apply m(args)
               | let x = infer P(args)                -- primitive inference (§7)
               | let x = import c(args)               -- imported evidence/assertion (§9)
               | let x = hole τ                       -- open authoring binding
               | let x = goal τ                       -- open binding, method search
assume       ::= assume a [denied] by p scope σ       -- explicit assumption term
requirement  ::= require id: subject rel limit       -- requirement use (§8.1)
args         ::= position- or name-bound references to inputs and earlier
                 step results; a step result may only be referenced after
                 the step that binds it
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

Scoping: `let` bindings are sequential and single-assignment. A name
shadowed is a static error (ambiguity is a finding, not a resolution).

## 5. Unification and the application rule

Application is the language's central judgment:

```text
Γ ⊢ aᵢ : τᵢ'      τᵢ' ≼ τᵢ[σ ↦ applied entities]     for each input i
────────────────────────────────────────────────── (T-APPLY)
Γ ⊢ apply m(a) : τ_out  ⊣ obligations(m, applied)
```

`≼` is *compositional acceptance*, defined component-wise:

- `g`, `m`, `k`: identity equality; a failure names both identities —
  `geometry mismatch: bracket@1 vs bracket@2` — and is a blocking
  finding at the consumer, not a coercion.
- `s`: scope acceptance — the value's scenario scope must equal or refine
  the slot's (`steady-state` ≼ `steady-state`, `steady-state` ≼ `any`,
  `steady-state` ⋠ `transient`). A scope failure names both scopes.
- `c`: claim acceptance — `exact ≼ exact`, `exact ≼ enclosure`,
  `enclosure ≼ enclosure`, `nominal ≼ nominal`. All other pairs refuse.
- A signature slot declaring `claim: exact` accepts only exact inputs;
  `claim: enclosure` accepts `enclosure` or `exact` (widened to `[x,x]`);
  `claim: any` accepts all three, and the application's output then
  carries the *weakest input claim actually supplied* (§7.A5) — never a
  stronger one.

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
| precondition `φ` on arguments or relations (e.g. "scenario's temperature domain ⊆ material applicability domain") | analysis | a checked primitive or an established premise; otherwise an open hole |
| postcondition `ψ` stating the output relation (e.g. `out = c·L·ΔT`) | analysis when the method is primitive-composed; runtime when it is externally executed | the named primitive check replayed over the observed values |
| runtime observation `ω` (the output was produced by this invocation, with these input digests) | execution | an observation bound to the invocation — receipt digest + output digests + value |
| assumption `a` carried by the signature | propagated onto the conclusion | discharged only by a checked inference that establishes `a`; else conditional |
| independence premise `indep(S)` on a set of sources | analysis or runtime | a source-dependency certificate showing disjoint provenance; else unresolved |

A postcondition can never be dropped during lowering: lowering rewrites
the program's *surface*, not its obligations. An externally executed
method's `ψ` becomes a runtime check naming the primitive that verifies it
— the primitive is fixed in the signature, so an adapter cannot claim a
certificate for a different relation than the one declared.

**Goal holes.** A `hole τ` is an authoring state: `analyze` reports it
with its type and the obligations suspended on it; `plan` refuses while
any hole reachable from a requirement remains. A hole unreachable from any
requirement is reported but does not block — incomplete programs are
inspectable.

**Budgets and unsupported constructs.** An obligation the fragment has no
rule for (e.g., a transport, a statistical combination, an SMT-shaped
search) is reported `unsupported` with its source location — an open
state, not a pass. Every inference engine counter is finite (§11); a
budget exhausted mid-search is an explicit `budget` outcome, distinct
from `unsupported` and from success.

## 7. Primitive inference rules

Premises are stated in the judgment; every rule's conclusion carries the
union of its premises' assumptions and the union of their source edges,
minus only assumptions a listed premise *establishes*. Each rule names the
authority that establishes its premises.

### A. Exact and interval arithmetic (authority: kernel)

```text
a, b exact rationals (same kind, same σ)            [P-EXACT]
─────────────────────────────
a ⊙ b  exact,  ⊙ ∈ {+,−,×}, b≠0 for ÷

a ∈ [a₁,a₂], b ∈ [b₁,b₂] (same kind, same σ)        [P-INT]
─────────────────────────────
a ⊙ b ∈ enclosure(⊙ applied componentwise to endpoints)
  +: [a₁+b₁, a₂+b₂]   −: [a₁−b₂, a₂−b₂]
  ×: [min(S), max(S)], S={a₁b₁,a₁b₂,a₂b₁,a₂b₂}
```

Containment is established *conditional on the premises*: if `a` truly
lies in `[a₁,a₂]` and `b` in `[b₁,b₂]`, the product lies in the computed
enclosure. The rule makes no empirical claim about the premises — those
stay attributed to whoever asserted them. Interval dependencies are
conservative: repeated use of one enclosure operand treats each use
independently (interval-arithmetic semantics); shared-source correlation
is the independence premise's concern, not the arithmetic's.

### B. Domain rules (authority: compiler)

```text
P holds over domain D ;   D' ⊆ D  declared compatible  [R2: specialize]
─────────────────────────────
P holds over D'
```

Specialization records the domain edge; no reverse rule exists.

```text
assert a at σ ; scope(σ.s) ≼ scope-required            [scope check]
─────────────────────────────
usable at the required scope
```

A steady-state result asked to serve a transient requirement fails this
check and produces an explicit coverage obligation — not a silent widening.

### C. Claim-model rules (authority: kernel/compiler)

```text
x : exact at σ                                        [C3: widen]
─────────────────────────────
x : enclosure [x,x] at σ
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
out carries assumptions Σ_args ∪ A
```

```text
a ∈ Σ ; checked rule R's premises establish a          [A3: discharge]
─────────────────────────────
Σ' = Σ ∖ {a}
```

```text
evidence set S declared independent                    [E4: independence]
─────────────────────────────
discharged iff dependency edges show pairwise-disjoint source sets;
a shared source edge leaves the premise named and unmet
```

Two different artifact names or digests sharing a calibration source are
not independent; the premise stays open until an independence
*assertion* (attributed, §9) or a checked certificate supplies it.

```text
context Σ ⊇ {a, denies a}                              [X1: contradiction]
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
observation(ι, …) bound to application site α ; runtime check R
replays over observed values and holds                                 [O2: discharge]
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

### 8.2 The three proposition states, kept separate

| State | Meaning | Rule |
| --- | --- | --- |
| **established** | a checked inference discharged it, or it entered as attributed evidence | usable; carries attribution |
| **assumed** | declared by `assume` or propagated from a signature | conclusions are conditional on it; it is *listed*, never silently dropped |
| **open** | an obligation no rule/premise yet discharges (hole, unsupported, budget) | blocks planning of dependents; `not_evaluated` at runtime |
| **contradicted** | both `a` and `denies a` present | blocks all dependent uses |

The analysis record reports each with source locations; the derivation's
premises carry the same three-state truth downstream.

### 8.3 Dependency edges

Every established/assumed proposition records the edges it rests on:
`input-digest`, `source-party`, `method-application`, `assumption`,
`observation`, `certificate`. These edges are what `derivation diff` and
change-explanation consume: a changed input maps to the applications
whose premise edges touch it. Shared source edges are how §7.E4 detects
non-independence; distinct digests with a common ancestor edge are one
source.

## 9. The trust model

| Layer | Establishes | Never |
| --- | --- | --- |
| kernel | arithmetic correctness, type/claim compatibility, containment under premises, the verdict rule's output | that premises describe the world |
| runner | an invocation occurred with these input/output digests; a receipt document was issued at this instant | that the executable computed the declared relation |
| checked certificate | a re-computable proof the declared postcondition holds for the observed values (the named primitive replays) | more than its postcondition |
| named party | an *assertion* — attributed, scoped, conditional | a checked result; assertion ≠ certificate |
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

## 10. Canonical identity, context binding, and change dependencies

Documents: `avila.core/language-program/v0.1-draft`,
`avila.core/method-library/v0.1-draft`, and the produced
`avila.core/language-analysis/v0.1-draft`,
`avila.core/execution-plan/v0.1-draft` — all canonicalized by the existing
JSON canonicalization, identified by content digest.

Lowering to the derivation machinery reuses ADR-0026 unchanged: the
evaluation context binds `{profile, evaluator, compiled snapshot,
library identity, claims/evidence identity, execution policy,
qualifications, observations, receipts}`; each §7 rule application is a
`RuleApplication` with named premises, states, and conclusion.
`context_sha256`/`derivation_sha256` give the same identity guarantees;
`explain_derivation_changes` answers "which uses changed" over the
dependency edges of §8.3.

Lowering preserves meaning stage by stage: `analyze` may reorder for
display but never drop a premise; `plan` may schedule but must carry every
runtime obligation to its invocation; `execute` may substitute observed
values for symbolic ones only inside a dischargeable check. A lowering
step that cannot preserve an obligation must refuse, not omit.

**Profile negotiation.** Program and library each carry a `profile`
field; binding requires both to equal the invoked profile, and the context
record carries it as `semantic_profile`. A program or library naming a
different profile — including `avila.core/semantic/0.2-draft` — is refused
at bind, not reinterpreted. Profile identifiers are never stripped or
relabelled on import.

**Presentation-only changes.** Document ordering, notice text, source-map
spans, and display formatting contribute no dependency edges and no
premise content: editing them alone yields identical context and
derivation identities. The semantic check is the only thing replayed —
byte-level equality is not required across presentation, and no
presentation edit can alter a semantic conclusion (nor invent a changed
premise, §8.3).

Source maps: every term, type parameter, obligation, and finding carries
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

## 12. Preservation argument (by cases over §7)

Claim: for each primitive rule, if its premises hold under their stated
attributions, its conclusion holds under the union of the premises'
residual assumptions — i.e., a checker's acceptance never manufactures a
premise it did not check.

- **P-EXACT/P-INT**: rational arithmetic and componentwise interval
  enclosure are containment-sound by construction of rational intervals;
  the conclusion inherits exactly the premises' assumption set (the
  arithmetic asserts nothing more). *Trusted:* the kernel's exact-number
  implementation and its work budget.
- **R2 (specialize)**: `D' ⊆ D` means every point of `D'` is a point of
  `D`, so a claim over `D` holds over `D'`. Direction matters; the
  converse is absent. *Trusted:* domain comparison is implemented as
  declared containment, not ad-hoc.
- **C3 (widen)**: `[x,x]` is a degenerate interval; the claim changes
  only in presentation. There is no `enclosure → exact` and no nominal
  promotion; the refusal is the theorem.
- **A2/A3 (propagation/discharge)**: conclusions carry the union of
  premises' assumptions minus those a listed premise established — an
  assumption discharged twice or silently would appear as a residual
  difference in replay and fail. *Trusted:* the propagation is a pure
  function of the premise sets.
- **E4 (independence)**: disjointness over recorded source edges; a
  shared edge leaves the premise open. *Unproved:* dependency-edge
  completeness depends on authors/libraries declaring provenance — the
  rule is sound relative to the recorded graph, not omniscient.
- **O1/O2 (observe/discharge)**: binding is by digest equality to the
  invocation's recorded inputs/outputs; a foreign receipt or recomputed
  outer hash cannot satisfy the digest premises. *Trusted:* the runner's
  observation record and the digest functions.
- **V-***: the four-state comparison over `[lo,hi]` vs exact `limit` is
  decidable and total; `not_evaluated` covers everything else.

**What is deliberately not claimed:** that declared physical premises are
true, that a certificate's postcondition is the *right* postcondition for
the engineering question, that declared provenance is complete, or that a
PASS under this profile is scientific validation. The fragment guarantees
conditional soundness — premises in, conclusion justified — nothing more.

## 13. The experiment instantiation (fixture expectations)

Library `thermal-expansion@1` declares:

```text
linear-expansion : ∀ g s m .
    (length: qty length ⊳ {exact|enclosure} (g, s, m),
     coefficient: qty thermal_expansion_coeff ⊳ {exact|enclosure} (g, s, m),
     temperature_change: qty temperature ⊳ {exact|enclosure} (g, s, m))
  requires  scenario-temperature-domain ⊆ applicability(m)
  ensures   out = coefficient × length × temperature_change   (check: P-INT composition)
  assumes   uniform-temperature-change, linear-expansion-model
  effects   process-spawn, write(out)
  external  executable

clearance-difference : ∀ g s m .
    (initial_clearance: qty length ⊳ {exact|enclosure} (g, s, m),
     displacement:     qty length ⊳ {exact|enclosure} (g, s, m))
  ensures   out = initial_clearance − displacement            (check: P-INT.sub)
  assumes   —
  primitive P-INT.sub
```

Under the charter's inputs — `L=100 mm`, `c=1/10000 K⁻¹`,
`ΔT=[10,20] K`, `clearance=1/2 mm` — the rules, not an implementation,
fix the expectations:

```text
displacement         = c·L·ΔT = [1/10, 1/5] mm          (P-INT)
remaining_clearance  = 1/2 − [1/10, 1/5] = [3/10, 2/5]  (P-INT.sub)

require ≥ 1/4 mm   → pass          (3/10 ≥ 1/4)
require ≥ 7/20 mm  → inconclusive  ([3/10,2/5] straddles 7/20)
require ≥ 9/20 mm  → fail          (2/5 < 9/20)
require with a missing runtime observation → not_evaluated
```

Assumption propagation: `remaining_clearance` carries
`{uniform-temperature-change, linear-expansion-model}` from
`linear-expansion`, unchanged through `clearance-difference` (no
assumptions of its own). The declared-applicability precondition
(`temperature domain ⊆ m`'s applicability) is a static obligation
discharged by domain containment; an out-of-domain scenario leaves it
open and blocks the plan.

Second library (`measurement-scaling@1`, EL-05): a `scaled-sum` method
combining two readings with a calibration factor and a declared
`independent(readings)` premise — §7.E4 keeps the premise open for two
readings sharing a calibration source even when both are byte-checked,
and no statistical-combination rule is provided to close it. That is the
generality test: the same rules, no case-specific branch.
