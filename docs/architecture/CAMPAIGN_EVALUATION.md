# Campaign evaluation

## Boundary

`avila-core evaluate` takes the same contract and registry snapshot the
compiler takes, plus one **evidence-claims** document under
`avila.core/evidence-claims/v0.2-draft`, and returns a campaign report. This
is the first executable slice of ADR-0006 SC-10 and SC-11: it admits claims
against the compiled snapshot and derives one four-state verdict per
requirement with the kernel.

The claims document binds exactly one compiled snapshot by identity. If the
supplied documents compile to a different snapshot, the whole document is
refused with `CORE-E7001`; nothing in it can be attributed to this campaign.

The slice does not read artifact bytes, verify execution receipts, signatures,
or package identities, evaluate qualification or policy snapshots, or
invalidate anything. Review decisions are recorded as unverified assertions.
No verdict it produces is scientific truth, certification, or regulatory
approval; every verdict names the boundary it holds under.

`avila-core run` composes this evaluator with a separate case-package layer.
That layer re-hashes explicitly resolved bytes, executes the steps a case
declares through named adapters over exact executables, verifies the
resulting execution receipts from bytes, generates the claims document from
package identities and fresh outputs, and binds every identity before
evaluation (ADR-0007). It does not alter the evaluator's admission semantics
or turn an unchecked, unsigned, or unqualified assertion into evidence of
truth.

## Claims

A claims document carries:

- **input attestations**: for each contract input, the artifact identity
  (`sha256:` digest and media type) that stood for it;
- **output claims**: for each executed step output, the artifact identity, an
  optional producer identity, and the uncertainty claim in one of the SC-3
  models the kernel reduces: `exact`, `interval`, `coverage_interval`,
  `worst_case`, or `unquantified`; and
- **review decisions**: for each accountable-review step, one asserted
  disposition with a rationale and a reviewer identity, marked `unverified`.

Quantities are exact strings with units; the kernel scales them into the
role's canonical unit exactly, so a claim in `Sv/s` and a limit in `uSv/h`
compare without rounding.

## Admission

Every attested input and every claim receives an explicit state, `admitted`,
`quarantined`, or `missing`, with the reason codes that decided it. The
conditions checked are the type-level subset of SC-11:

| Condition | Check | Code |
| --- | --- | --- |
| identity | the record names an input, step, and output slot the compiled snapshot has | `CORE-E7002` |
| A1, partial | the artifact identity is a well-formed `sha256:` digest; an unattested input is missing | `CORE-E7101` |
| A3 | every parent bound to the producing step is admitted; admission fails closed along the dataflow | `CORE-E7103` |
| A5 | a claim exists for the output | verdict `not_evaluated.missing` |
| A6, type level | the claim model is permitted by the output, its shape satisfies the model, quantities scale in the role's kind, bounds are ordered, a nominal lies inside its interval, coverage is in `(0, 1]`, and the media type matches | `CORE-E7201` |
| cardinality | exactly one claim per output slot; a duplicate quarantines every claim for the slot | `CORE-E7301` |
| A9, structural | a decision names a compiled review step with an allowed disposition, once | `CORE-E7401` |

A quarantined parent quarantines its descendants. That cascade is intended:
each claim's state must be explicit, and a claim computed from inadmissible
evidence is inadmissible however well formed its own values are.

## Verdicts

For each compiled requirement, the claims for its metric source enter the
kernel's verdict calculus with their admission states. A quarantined or
missing claim yields `NOT_EVALUATED` with the reason. Admitted claims are
reduced under SC-3, scaled exactly, and compared under the SC-10 tables:
`bounded.le.within`, `bounded.le.crossing`, `bounded.le.exceeds`, and the
rest.

Review is asymmetric, as SC-10 requires. Every compiled review obligation is
a required review; `PASS` is withheld as `not_evaluated.review_pending` until
an `approve_for_use` decision is present, while `FAIL` is emitted from
contradicting evidence with the review still outstanding and listed. A
decision is an unverified assertion here; eligibility, signatures, and
independence are later admission checks.

A `nominal` basis is evaluated only when the contract's execution policy
permits it; the compiler refuses the contract otherwise with `CORE-A4201`.

## Identity

The report carries the compiled snapshot identity, the canonical identity of
the claims document, every admission and verdict, and a `campaign_sha256`
over the canonical report body. Two evaluations of the same three documents
produce the same report and the same identity.

## Fixtures

`fixtures/semantic-core/campaigns/campaign-cases.v1.json` pins twelve cases:
the three bounded outcomes, exact unit scaling, a missing parent, a model the
output does not permit, inverted bounds, a duplicate claim, a snapshot
mismatch, and the three review states. The harness in
`crates/avila-core-compiler/tests/campaign_fixtures.rs` executes them and
pins every campaign identity.

## What a protocol study looks like here

A frozen protocol with gates and falsifiers is a contract: the gates are
requirements, the register is the declared inputs, and the checker is a
capability type. Executing the protocol produces the claims document; the
checker's re-derivation is the admission step; the ledger entry is the
campaign report; the manifest is its identity. Execution receipts now exist
for the steps a case declares (ADR-0007); what remains is capability identity
beyond an executable digest, the signed half of A2, and typed invalidation so
a changed input or method names exactly which verdicts no longer stand.

## First composed internal case

[CASE-000](../../examples/cases/case-000-actinv-aftermatter/README.md) applies
this slice to a synthetic ACTINV 1.0.1 → Aftermatter R0 chain. Twelve
input attestations and four output claims admit under the type-level rules;
the three classification claims are extracted by the case runner from the
Aftermatter result it executes, and the inventory claim is a recorded
attestation.
Both bounded fraction claims have upper bounds below their frozen limits, but
the campaign returns `NOT_EVALUATED / not_evaluated.review_pending` for both
requirements because no qualified-review decision is asserted.

The case deliberately retains the Aftermatter route result as unquantified:
all three modeled routes are unresolved, and the current numeric requirement
language has no categorical route-state semantics. CASE-000 therefore records
both what the executable slice can establish and the next vertical gaps
without pretending the upstream bytes, packages, qualification, or review have
been verified. The case runner makes the byte boundary visible: nine
artifacts are re-hashed from the Aftermatter checkout and five ACTINV
data-release artifacts from that checkout's data directory when the
`actinv-data` root is supplied, and every root left unsupplied stays
`not_checked`. With both roots and the bound executable supplied, the fresh
Aftermatter result reproduces the frozen artifact byte for byte and the fresh
receipt matches the committed one.
