# ADR-0008: Qualification envelopes

**Status:** accepted 2026-09-02 (decision S-025); the applicability context
is persisted on the assessment and claim by ADR-0018

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

## Refinement: output-slot scope (S-039)

A capability step commonly produces more than one output from a single
execution — the shielding and thermal screens each emit a nominal dose or
hotspot estimate alongside exact areal-mass and thickness figures computed by
arithmetic over the bound materials table, not by the estimate's model. Before
this refinement a qualification record covered every output slot a step
produced, or none; there was no way to state that the geometry arithmetic is
qualified while the estimate it sits beside is not, other than binding no
record at all and leaving both unqualified.

1. **A record may name the output slots it covers.** The optional
   `covered_output_slots` field lists them; omitting it covers every output
   slot the bound capability produces, exactly as before this field existed.
   The field is validated non-empty when present — an empty list would state
   a record that covers nothing, which is never the intent.
2. **The runner attaches the envelope only to a covered claim.** Before this
   refinement every claim a step produced carried the same assessment. Now
   `promote` checks each extracted claim's output slot against the bound
   record's `covered_output_slots` (when named) before attaching the
   evaluated `ClaimQualification`; an uncovered claim carries none at all,
   indistinguishable from a step with no qualification bound. The envelope is
   still evaluated once per step, from the same applicability facts, so a
   covered and an uncovered claim from the same execution can differ only in
   whether the assessment is attached, never in what it says.
3. **An uncovered claim can still satisfy only a nominal-basis requirement.**
   Nothing about admission or the campaign's qualification rules changes: a
   bounded or enclosure requirement over an uncovered claim is `CORE-A4402`
   under `require_qualification`, or carries the informational `CORE-A4403`
   otherwise, exactly as if the capability had no qualification at all. A
   nominal requirement over the same claim is unaffected, as always.
4. **The screen adapters now report the facts their own envelopes need.**
   CASE-001 and CASE-002's shielding screen reuses the slab-transport fact
   extraction under its own adapter id (so a scope's `source_requirement`
   validator selects between the screen and transport, which read the
   identical candidate and source bytes independently); CASE-003's thermal
   screen and finite-element adapters share a fact extraction that now also
   reports the plate's total thickness, which the finite-element qualification
   did not need but the screen's geometry envelope does.

## Consequences of the refinement

- Schema `qualification.v0.1-draft` gains `covered_output_slots`; the parser
  refuses a present-but-empty list.
- CASE-001, CASE-002, and CASE-003 each gain a qualification record for their
  screen capability, scoped to the mass/areal-mass and thickness output
  slots only, binding no validation evidence and declaring an envelope no
  wider than the case's own search box (layer count, listed materials, and a
  thickness bound at least as wide as the case's own requirement limit). Each
  case's `execution_policy.require_qualification` is now `true`; every
  bounded and enclosure requirement's evidence carries a satisfied envelope,
  and `CORE-A4403` no longer appears in any of the three committed campaign
  reports.
- A candidate with four layers (`candidates/outside-envelope-four-layers.json`
  in CASE-001) demonstrates the scope working the other way: the screen still
  computes a mass and a thickness, but the layer count term is `false`, so
  both bounded requirements are `NOT_EVALUATED` under `CORE-A4401` while the
  nominal screen-dose requirement evaluates normally.
- The unit test `covered_output_slots_scopes_which_claims_the_record_covers`
  and the runner adversarial test
  `a_qualification_scoped_to_one_output_slot_leaves_the_others_unqualified`
  move with this refinement.
