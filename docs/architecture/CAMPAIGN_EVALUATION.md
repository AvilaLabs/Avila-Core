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
or package identities, evaluate policy snapshots, or invalidate anything. It
does evaluate qualification positions already carried by claims. Review and
presentation records are not claims-document inputs. No verdict it produces is
scientific truth, certification, or regulatory approval; every verdict names
the boundary it holds under.

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
  models the kernel reduces (an output claim may also carry the producing
  capability's evaluated qualification envelope; a bounded or enclosure
  requirement whose admitted evidence is outside its envelope, or of
  unknown position, is `NOT_EVALUATED` under `CORE-A4401` before the kernel
  is asked, see ADR-0008): `exact`, `interval`, `coverage_interval`,
  `worst_case`, or `unquantified`. An unquantified non-quantity claim may
  preserve a categorical value as evidence; that value is never lowered into
  the numeric verdict kernel (ADR-0011).

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
| A6, type level | the claim model is permitted by the output, its shape satisfies the model, quantities scale in the role's kind, bounds are ordered, a nominal lies inside its interval, coverage is in `(0, 1]`, a categorical value is nonempty and appears only on a non-quantity unquantified role, and the media type matches | `CORE-E7201` |
| cardinality | exactly one claim per output slot; a duplicate quarantines every claim for the slot | `CORE-E7301` |

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

Technical verdicts are review-independent. Campaign evaluation does not read a
review record, human acknowledgement, or presentation disposition, so none can
create, suppress, or alter `PASS`, `FAIL`, `INCONCLUSIVE`, or
`NOT_EVALUATED`. Qualification can still make bounded evidence
`NOT_EVALUATED` when the method is outside its recorded envelope; that is an
evidence-applicability rule, not professional review.

A `nominal` basis is evaluated only when the contract's execution policy
permits it; the compiler refuses the contract otherwise with `CORE-A4201`.

## Optional presentation gates

When a contract configures one, `avila-core run` resolves the connected
agent's practical dossier against the claims it generated and emits a
`presentation_gates` entry after campaign evaluation. The entry binds the compiled
snapshot and campaign identities; each presented artifact's source, evidence
id, digest, and media type; the policy, routing dispositions, and instructions;
and a canonical `request_sha256`. The compiled `presentation_gate` is
`awaiting_agent`; the realized dossier is `ready_for_agent` only when every
source is present, otherwise `awaiting_evidence` with the missing sources
listed. Omitting the gate is valid and produces no pending state.

The runner does not execute the connected agent or ingest its routing record.
CASE-001's scripted designer passes the exact ready request to its hash-bound
practicality agent and records an unsigned
`avila.core/staged-review-record/v0.1-draft` beside the campaign. The record may
`present_to_user`, `request_changes`, or `abstain`; it is routing history, not
admitted evidence, approval, or a technical verdict.

## Identity

The report carries the compiled snapshot identity, the canonical identity of
the claims document, every admission and verdict, and a `campaign_sha256`
over the canonical report body. Two evaluations of the same three documents
produce the same report and the same identity.

## Fixtures

`fixtures/semantic-core/campaigns/campaign-cases.v1.json` pins twelve cases:
the three bounded outcomes, exact unit scaling, a missing parent, a model the
output does not permit, inverted bounds, a duplicate claim, a snapshot
mismatch, qualification outside the envelope, and two cases proving that an
optional presentation gate cannot alter PASS or FAIL. The harness in
`crates/avila-core-compiler/tests/campaign_fixtures.rs` executes them and
pins every campaign identity.

## What a protocol study looks like here

A frozen protocol with gates and falsifiers is a contract: the gates are
requirements, the register is the declared inputs, and the checker is a
capability type. Executing the protocol produces the claims document; the
checker's re-derivation is the admission step; the ledger entry is the
campaign report; the manifest is its identity. Execution receipts now exist
for the steps a case declares, and they double as the memoization table:
an unchanged step is reused, a changed one is rerun with its change named by
class, and a requirement change re-evaluates over reused evidence
(ADR-0007). What remains is capability identity beyond an executable digest,
the signed half of A2, and the change classes a receipt cannot see.

## First composed internal case

[CASE-000](../../examples/cases/case-000-actinv-aftermatter/README.md) applies
this slice to a synthetic ACTINV 1.0.1 → Aftermatter R0 chain. Fourteen
input attestations and six output claims admit under the type-level rules,
every output claim extracted by the case runner from the results it executes:
the ACTINV problem, inventory, and decay metadata from the frozen R0 builder,
and the two Class A fractions plus the route result from Aftermatter.
Both bounded fraction claims have upper bounds below their frozen limits, so
the campaign returns `PASS / bounded.lt.within` for both requirements. No
human, professional, or agent review is needed to establish those technical
verdicts.

The case deliberately retains the Aftermatter route result as unquantified:
all three modeled routes are unresolved, and the current numeric requirement
language does not treat categorical route state as a requirement value.
CASE-000 therefore records
both what the executable slice can establish and the next vertical gaps
without pretending the upstream bytes, packages, or qualification have
been verified. The case runner makes the byte boundary visible: nine
artifacts are re-hashed from the Aftermatter checkout and five ACTINV
data-release artifacts from that checkout's data directory when the
`actinv-data` root is supplied, and every root left unsupplied stays
`not_checked`, as do the ACTINV release builds under the `actinv-release`
root. With all three roots and both bound executables supplied, every fresh
result reproduces its frozen artifact byte for byte and both fresh receipts
match the committed ones.
