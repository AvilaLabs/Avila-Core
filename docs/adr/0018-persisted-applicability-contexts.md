# ADR-0018: Persisted applicability contexts in evidence claims

- Status: accepted for the local case runner; the evidence-claims schema
  remains a draft
- Date: 2026-09-11
- Refines: ADR-0008 clauses 2-3

## Context

ADR-0008 clause 2 has each adapter report the kernel's applicability
context — the extracted facts with their declared sources, and each staged
input's media type and identity — and clause 3 evaluates the bound
qualification record's scope over it. The assessment was then attached to
the step's report and claims, but the context itself was discarded: a
claim recorded each scope term's authored predicate text and its
evaluation result, never the fact values that produced those results. The
independent verifier (S-041, S-042) could therefore check only that the
recorded assessment was internally consistent — that its stated terms
matched the record's scope and its state aggregated its own term results —
and named the missing facts explicitly as the reason an envelope verdict
could not be re-derived from outside.

## Decision

1. **The assessment carries the context it was evaluated over.**
   `EnvelopeAssessment` and `ClaimQualification` gain a required `context`
   field holding the exact applicability context `evaluate_envelope` was
   given — the kernel's `ApplicabilityContext` as JSON: `facts`,
   `inputs`, and the optional `params` and `environment`. The
   evidence-claims v0.2-draft schema refines in place to require it, as
   ADR-0011 and ADR-0017 refined the same unreleased schema before.

2. **Provenance is preserved, not upgraded.** Every fact still carries its
   declared source class, source identity, validator, and receipt
   reference; every input still carries its media type and SHA-256 as slot
   attributes. Persisting the context binds the recorded assessment to the
   recorded facts; it does not make the adapter's readings of the input
   bytes true. A runner signature authenticates the record, not the
   physics.

3. **The context binds the step's planned invocation.** Facts cite
   `plan:<invocation_sha256>` and context inputs name the staged input
   digests, so a context attached to a claim whose step receipt plans a
   different invocation, or stages different input bytes, is detected by
   an independent check rather than silently accepted.

4. **The verifier re-derives, not trusts.** The standard-library
   verifier's qualification section now re-evaluates each scope term over
   the persisted context and compares per-term results and the aggregate
   state against the recorded ones, and binds the context's input
   identities and plan reference to the step's committed receipt. A
   qualification-carrying claim with no context is a mismatch, not a
   trusted record. What remains outside its profile, by name: whether the
   adapter extracted the facts correctly from the bytes — that belongs to
   the capability's own validation evidence, which the record names by
   identity only.

## Boundary

The context is a record of what the adapter asserted about verified bytes
under a named invocation. Re-derivation proves the recorded assessment
follows from those assertions; it does not validate the extraction, the
adapter, or the physics behind the envelope. A changed candidate or source
document changes the facts, the plan identity, and the assessment
together — nothing here weakens SC-12 invalidation.

## Consequences

- `schemas/evidence-claims.v0.2-draft.schema.json` requires
  `qualification.context`; the two semantic-core campaign fixtures that
  carry qualifications were updated with consistent contexts.
- CASE-001 and CASE-003 were re-blessed (fresh execution, regenerated
  claims and campaign report, re-signed receipts and manifest) with every
  verdict, margin, and receipt output identity unchanged.
- CASE-002's committed claims remain in the pre-ADR-0018 shape: its
  pinned ACTINV executable no longer resolves on this machine, so its
  claims cannot be regenerated. The verifier names every one of its
  qualified claims' missing contexts as a mismatch until the case is
  deliberately re-pinned and re-blessed.
- The independent verifier's named gap is closed: `UNSUPPORTED_NOTES`
  drops `qualification_facts`, and mutation tests pin that an edited fact,
  a missing context, an edited input identity, and a context lifted from
  a sibling step are each reported.
