# Engineering language charter

Date: 25 September 2026. Status: proposed design charter following the owner's
agreement on the language direction; not an implemented semantic profile or a
claim of soundness. Repository planning baseline: `b1587ad`.

The [implementation handoff](../ENGINEERING_LANGUAGE_HANDOFF.md) defines the
next work. The [earlier proposal](2026-09-24-rust-informed-semantic-architecture.md)
and [ADR-0026](../../adr/0026-context-bound-verdict-derivations.md) supply useful
implementation foundations. Their completion does not establish this charter.

## Product commitment

Avila Core should be a language for constructing engineering arguments. An
engineer describes a system, the question being asked, candidate methods, and
the premises available. The compiler determines what those operations mean,
which compositions are admissible, what work remains, and what conclusions
follow from completed work.

Types, assumptions, uncertainty, method interfaces, execution, changes, and
authoring tools must participate in one semantic model. A new file syntax or
additional wrappers around reports would not establish this property. JSON,
forms, a textual language, and agent proposals can all be frontends to the
same model.

The proposed organizing guarantee is:

> Under a specified model and explicit trusted premises, accepted engineering
> operations preserve the meaning of their inputs and establish only
> conclusions justified by those premises and the language's inference rules.

This is conditional soundness within a defined fragment. It does not establish
that a physical model describes the world correctly. Measurements, provider
assertions, model assumptions, and qualification judgments retain their
attribution and scope. Proof of a transformation's arithmetic does not prove
the physical premises of that transformation.

## Language commitments

### Values describe a physical subject and a claim

A semantic type can relate a value to an immutable design revision, a scenario,
a spatial or temporal domain, a quantity kind, and a claim interpretation.
Method signatures share variables for these relationships: inputs that must
describe the same geometry must actually unify on that geometry.

The first fragment uses finite identities and exact interval domains. It does
not require arbitrary dependent theorem proving. Equality and permitted domain
restriction are separate rules. A claim covering a domain can be specialized
to a contained domain when its interpretation permits that operation; it
cannot silently be generalized to a larger one. A change of geometry or
coordinate system requires an explicit supported transport operation.

Types are part of Core's engineering language. They must not require a new
Rust generic type or a hard-coded branch for each role, method, or case.

### Assumptions are dependencies with consequences

Every operation propagates its material assumptions and external trust
dependencies. A checked inference may discharge an assumption; a formatting
operation or an adapter's status field may not. An explicit assumption creates
a conditional claim, never a proof that the assumption holds.

Keep three things distinct: established propositions, declared assumptions,
and unresolved obligations. Contradicted premises block their use. Supported
consistency checks must reject contradictory contexts rather than using a
contradiction to establish arbitrary engineering conclusions. Unsupported
consistency questions remain unresolved.

The compiler records source locations and dependency edges for each of these
states. This is how the editor can explain why a change matters and how an
independent checker can reconstruct a conclusion's actual boundary.

### Uncertainty has rules of composition

Exact quantities, nominal estimates, enclosures, and statistical claims have
different meanings. They do not form one universal confidence ranking. The
initial executable fragment supports exact values, nominal values, and rational
interval enclosures with explicit model premises. Other interpretations remain
describable as unsupported for an inference that cannot handle them.

Interval arithmetic establishes containment under its input premises. It does
not establish those premises' empirical credibility. Dependencies on shared
sources remain visible. A method requiring an independence premise must obtain
one with specified scope and attribution; distinct artifact names or digests
do not establish independence. The prototype need not implement statistical
inference to demonstrate this refusal.

### Method signatures create obligations

A method declares related input/output types, assumptions, preconditions,
postconditions, relevant effects, and the supported justification for its
output. Applying a method creates obligations from that signature.

The compiler discharges supported static obligations, retains unresolved
authoring holes, and places supported runtime obligations at the exact point
where they must be checked. A missing postcondition cannot disappear during
lowering. A result becomes usable only after the checks required for its use.

Candidate method discovery operates over a pinned finite library. Several
applicable methods remain an explicit choice. The compiler must not silently
choose scientific assumptions, relax a requirement, or invent a method in
order to close a goal. Search is bounded; ambiguity and exhausted budgets are
distinct from successful checking.

### Execution implements the checked program

The bound plan identifies inputs, implementations, parameters, effects,
observations, and required checks. Every lowering step preserves the claim's
meaning, prerequisites, and remaining assumptions. Execution observations are
bound to the actual invocation and actual outputs before they discharge any
runtime obligation.

File identity, fresh execution, receipt verification, cache acceptance,
environment isolation, and scientific adequacy establish different properties.
An unrestricted external process does not become pure because it declares
itself pure. If an effect boundary cannot be enforced, the remaining reliance
must stay explicit in the program's trust boundary and reuse decision.

Only existing permitted reuse rules, or a new specified and checked transport
rule, may authorize reuse. Historical conclusions retain their original
context. Current applicability is evaluated against explicitly supplied
material and time; there is no ambient clock inside the pure checker.

### Libraries extend the language through defined interfaces

Ordinary domain libraries compose existing primitives and declare method
signatures. They cannot register arbitrary Rust/Python callbacks that mint
authoritative proof states. New primitive inference rules expand the trusted
semantics and require a specification, implementation, adversarial tests, and
independent verification coverage.

External methods can supply checked certificates or attributed assertions.
Those routes remain distinguishable. A human or agent may propose either;
neither can relabel an assertion as a checked mathematical result. Signatures
establish attribution within their trust policy, not the truth of every
statement the signer makes.

## The first complete language experiment

Use a synthetic thermal-expansion/clearance model with two composed method
applications. Its equations are declared toy semantics, not a qualified
engineering method:

```text
displacement = expansion_coefficient * length * temperature_change
remaining_clearance = initial_clearance - displacement
```

Length and clearance belong to a geometry revision. Temperature change and the
method use belong to a named scenario. Material identity and applicability
domain are bound. The relation carries explicit assumptions of uniform
temperature change and the declared linear expansion model. Imported input
bounds remain attributed premises.

Illustrative synthetic inputs are length `100 mm`, coefficient
`1/10000 K^-1`, temperature change `[10, 20] K`, and initial clearance
`1/2 mm`. Exact interval composition yields displacement `[1/10, 1/5] mm`
and remaining clearance `[3/10, 2/5] mm`. These are mathematical fixture
expectations, not recorded run results or physical validation.

For the requirement `remaining_clearance >= limit`, limits `1/4 mm`,
`7/20 mm`, and `9/20 mm` exercise PASS, INCONCLUSIVE, and FAIL under the
existing bounded-comparison meanings. Missing required evidence exercises
NOT_EVALUATED or pre-execution refusal at the appropriate stage. Different
limits are separate authored test programs; a run must never alter its own
requirement to obtain a pass.

The experiment must expose the complete path:

```text
authored program and goal
  -> related types, assumptions, and uncertainty
  -> static obligations, authoring holes, and runtime obligations
  -> bound execution and observed results
  -> checked inference under explicit premises
  -> four-state requirement result and independent replay
```

Edits to geometry, scenario, input bounds, method, policy, or assumptions must
produce attributable changes through those same rules. A schema accepting
these fields while adapters interpret them independently is insufficient.

## How we judge the experiment

The defining demonstration is one rule system handling correct composition,
incompatible composition, incomplete authoring, execution failure, altered
premises, and permitted reuse. A second synthetic method library must exercise
the same rules without adding a branch for its case or method identity.

Every claimed guarantee needs a written rule, the authority that establishes
its premises, a positive example, a counterexample, and a replay check. Tests
support the implementation; they do not replace a semantic specification or
constitute a general soundness proof. Write a preservation argument for the
finite primitive set and name its trusted assumptions.

The existing CLI, UI, MCP, numeric kernel, and runner remain reusable assets.
The experiment needs its own explicit profile and entry path so that existing
documents retain their meaning. Production migration follows an assessed
experiment and a compatibility decision, not an automatic rewrite of all cases.

## Later consequences to test

The same language could eventually support interactive goal completion,
incremental checking, a method package ecosystem, and certificates covering
regions of design space. A regional certificate would establish a proposition
for every member of an explicitly described region under named assumptions;
point samples cannot be promoted into that guarantee.

Those are consequences to investigate after composition works. Persistent
sessions, a database, a distributed scheduler, general SMT search, an LSP
server, and new surface syntax are not prerequisites for this first experiment.
The prototype still needs a shared analysis operation with source-linked
diagnostics suitable for future editor use.

## Research basis

These sources motivate the design; their guarantees do not automatically
transfer to Core:

- [RustBelt](https://plv.mpi-sws.org/rustbelt/popl18/paper.pdf) gives types a
  semantic interpretation and derives obligations for safe library interfaces
  over a Rust subset. This motivates specifying Core's primitives and trust
  boundary before claiming composition soundness.
- [F* effect refinements](https://fstar-lang.org/tutorial/book/part4/part4_pure.html)
  connect computation types, preconditions, and postconditions through
  verification-condition generation. Core's proposed method obligations are
  a restricted engineering application of that idea.
- [Jif](https://www.cs.cornell.edu/jif/doc/jif-3.3.0/overview.html) checks
  information-use restrictions through program composition. It motivates
  automatic propagation of relevant assumptions and trust dependencies here.
- [Lean](https://lean-lang.org/theorem_proving_in_lean4/Introduction/)
  separates construction of arguments from checking explicit proofs. Core's
  planner and agents should similarly remain outside proof authority.
- [Modelica connection semantics](https://specification.modelica.org/maint/3.6/connectors-and-connections.html)
  generate mathematical equations from component connections. This motivates
  engineering composition that creates obligations with defined meaning.
- [Abstract interpretation](https://www.di.ens.fr/~cousot/COUSOTpapers/POPL77.shtml)
  provides a basis for sound approximation over possible computations. Using
  it for design regions would require a specified model and justified abstract
  operations, not just additional caching.
