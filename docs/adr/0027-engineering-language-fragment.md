# ADR-0027 — Experimental engineering-language fragment `avila.core/language/0.1-draft`

- Status: Accepted for the scoped experiment; the language is specified,
  not implemented at this revision
- Date: 25 September 2026
- Basis: [language charter](../roadmap/proposals/2026-09-25-engineering-language-charter.md),
  [EL implementation handoff](../roadmap/ENGINEERING_LANGUAGE_HANDOFF.md),
  [ADR-0026](0026-context-bound-verdict-derivations.md) and its review
  follow-up at `b1587ad`
- Normative specification:
  [docs/architecture/ENGINEERING_LANGUAGE.md](../architecture/ENGINEERING_LANGUAGE.md)
- Example programs: `examples/language/` (authored before the checker;
  the specification, not an implementation, determines their results)
- Revision: specification r3 incorporates the
  [EL-01 specification review](../roadmap/reviews/2026-09-25-engineering-language-spec-review.md)
  and the
  [r2 follow-up review](../roadmap/reviews/2026-09-25-engineering-language-r2-review.md)
  — kind-directed multiplication, scenario identity vs scope,
  witness-based assumption discharge, the independence/provenance split,
  explicit relation-map semantics, schema-directed identity projection,
  and lifecycle scope combination. The decision stands unchanged.

## Context

The owner agreed to make language design the next milestone: Core's type
system, method interfaces, execution, revision handling, and authoring tools
should grow from one engineering language whose guarantee is conditional
soundness — under a specified model and explicit trusted premises, accepted
operations preserve the meaning of their inputs and establish only
conclusions justified by those premises and the inference rules.

RA-01–RA-06 built the context-bound evaluation and replayable-derivation
machinery this experiment depends on; the review at `aa733af` found five
boundary gaps and `b1587ad` closed them ([verification record](../roadmap/reviews/2026-09-25-adr-0026-followup-verification.md)).
That machinery is reused, not claimed as the language.

## Sequencing change

S-034 deferred semantic-obligation work behind infrastructure gates and
follow-on milestones. This ADR records the owner's directed change: a finite
language experiment (EL-01 through EL-05 in the handoff) precedes that
infrastructure work, because the experiment is what determines whether the
shared rules deserve the infrastructure. This change does not mark any
S-034 gate satisfied and does not authorize a session system, database,
scheduler, solver library, or general reasoning platform. The prototype
keeps finite terms, an acyclic method graph, rational interval arithmetic,
and bounded search.

## Decision

1. **A new experimental semantic profile**: `avila.core/language/0.1-draft`.
   It is a separate name, not a revision of `avila.core/semantic/0.2-draft`:
   the fragment has different terms, types, and inference rules, and existing
   records under the released profile keep their meaning exactly as stored.
   No migration, deserialization, or flag can give an existing record
   stronger guarantees under the new profile. Entry is opt-in per document
   (`profile` field) and per invocation.

2. **The language is specified before it is implemented.** The linked
   specification fixes the term forms, the related-type parameters
   (geometry revision, scenario identity, material/applicability identity,
   quantity kind, claim model), the proposition states
   (established / assumed / open / refuted / contradicted — each blocking
   or conditioning uses as specified), the primitive inference
   rules and their premises, the static-versus-runtime obligation split,
   the trust model for imported assertions and checked certificates, the
   canonical identity and context binding, the finite limits, and the
   case-by-case preservation argument. Programs in `examples/language/`
   are authored against that specification alone — a later checker either
   matches the specified results or the specification was wrong, which is
   reported, not silently repaired.

3. **The first experiment is the charter's synthetic thermal-expansion /
   clearance model** with two composed method applications, a declared toy
   semantics, and limits `1/4`, `7/20`, `9/20` mm exercising PASS,
   INCONCLUSIVE, FAIL plus a NOT_EVALUATED variant. The second synthetic
   library (`measurement-scaling`) exists at specification level to pin the
   independence-premise behavior before any implementation is tempted to
   special-case it.

4. **Authority stays in the existing boundaries.** The kernel owns exact
   arithmetic and the primitive rules; the compiler owns analysis,
   obligation generation, and the canonical identities; the runner owns
   execution observations; the Python verifier independently replays.
   Libraries are data: they compose primitives and declare signatures and
   cannot mint proof authority.

## Consequences

- The specification's "done" is testable without a checker: each committed
  program carries its specified expected outcome, derived by hand from the
  rules. Disagreement at implementation time is a spec or code defect, not
  a policy choice.
- An implementation that cannot honor a rule must report
  `unsupported`/`not_checked`/a bounded refusal — never synthesize a premise.
- The fragment deliberately excludes: dependent theorem proving, statistical
  combination formulas, persistent sessions, an LSP server, a new surface
  syntax, and regional certificates. Their entry conditions remain in the
  charter's follow-on list.
- New diagnostic codes and document schemas introduced at implementation
  time get catalogued under the existing rules (CONTRIBUTING) with emitting
  fixtures.

## Alternatives considered

- **Extend `avila.core/semantic/0.2-draft` in place.** Rejected: campaign
  evaluation's contract/registry/claims semantics would acquire obligations
  it was never specified to produce, and existing records could be read
  under stronger-looking rules. A separate profile keeps both honest.
- **A textual surface first.** Rejected per ACORE_SYNTAX's exploratory
  status: JSON is the canonical interchange; the spec's term grammar is a
  readable presentation, not a required parser.
- **Implement, then write the rules.** Rejected: the charter's whole point
  is that the rules — not an implementation's incidental behavior — define
  correct composition. The examples exist to falsify the rules before code
  can bless them.
