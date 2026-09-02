# ADR-0008: Qualification envelopes

**Status:** accepted 2026-09-02 (decision S-025)

## Context

Core reserves `PASS` on a bounded requirement for evidence from a qualified
method, and the generative loop needs Core to say, before or after a run,
that a candidate lies outside the range a method has been validated for.
Until now nothing in a case bound a capability to any statement of where it
may be trusted: a `coverage_interval` claim from any executable satisfied a
bounded requirement wherever it was computed. ADR-0006 already defines the
closed applicability predicate and typed facts with provenance; the kernel
implements them; no case used them.

## Decision

1. **A qualification record is a method owner's document.** Schema
   `avila.core/qualification/v0.1-draft`: an identifier and revision, the
   owner, the adapter identifier it covers, the exact executable digest it
   covers, a statement, a `scope` that is a kernel applicability predicate,
   the quantity kinds of the facts the scope names, validation-evidence
   identities, and limitations. A case package binds it as a document with
   role `qualification`. The runner refuses the run if the record names an
   executable digest other than the one the package binds under that
   capability id, or an adapter and capability pair no execution uses.

2. **Facts come from verified inputs through the adapter.** Before a step
   runs, and before reuse is decided, the adapter reports the kernel's
   applicability context: for every staged input its media type and identity
   as attributes of that slot, the input count as a runner-measured fact,
   and whatever the adapter reads from the verified input bytes (the
   shielding transport adapter reports the source energy and geometry, the
   slab's total thickness and layer count, and each layer's material). A
   fact carries the input's identity as provenance and the adapter as
   validator; a scope term names the source class and validator it accepts,
   and a fact from any other source leaves the term unknown.

3. **The envelope is evaluated by the kernel and reported per term.** The
   runner evaluates the scope over the context with the kernel's evaluator
   under strong Kleene logic and records `inside`, `outside`, or `unknown`
   with each top-level term's result, on the step report and on every claim
   the step produces. `--plan` already shows it, so a designer can be told
   "outside" without spending the run.

4. **Outside or unknown cannot establish a bounded requirement.** In the
   campaign, a requirement with a bounded or enclosure basis whose admitted
   evidence carries an envelope that is not `inside` is `NOT_EVALUATED` with
   rule `not_evaluated.outside_qualification` or
   `not_evaluated.qualification_unknown`, reason `CORE-A4401` owned by the
   method owner, and the failed terms named on the evidence. Nominal-basis
   requirements are unaffected: a guide stays a guide. Where qualified
   review is also outstanding, the review rule is reported and the envelope
   reason is appended, so neither hides the other.

5. **An unqualified capability is still evaluated.** A claim without an
   envelope behaves as before. Enforcing "no qualification, no bounded
   verdict" is a semantic-profile change that would rewrite the campaign
   fixtures; it is deferred, and the gap is visible in every report that
   shows a bounded verdict without an envelope line.

## Boundary

The envelope is what the owner claims to have validated; Core checks the
case against the claim, not the claim against reality. Validation evidence
is named by identity only and not verified. Facts are extracted by the
case-specific adapter, not by a general fact language. The first record, for
CASE-001's transport capability, binds no validation evidence at all and
says so in its limitations: it exists so that Core can refuse candidates
beyond 120 cm and so that a real qualification has a place to go.

## Consequences

- CASE-001's registry gains an energy kind, its transport capability a
  qualification record, and its candidates an `outside-envelope` example
  whose transport verdict is `NOT_EVALUATED` although transport ran and
  produced an interval.
- Every CASE-001 claim from transport now carries the envelope, so the
  committed claims and campaign report were re-blessed.
- Catalog code `CORE-A4401`, campaign fixture
  `campaign.outside-qualification.not_evaluated`, and adversarial runner
  tests for inside, outside, and a mismatched executable move with this ADR.
