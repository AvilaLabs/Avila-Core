# CASE-010 control comparison — contract path vs language path

Date: 2026-09-26. Status: self-review evidence; not independently reviewed.

The seeded-error battery built for `matmul-rank` was run through the
*contract* path (`avila-core run` on `examples/cases/case-010-matmul-rank`,
completed at `cb28a51`) as the control group for the language path
(`examples/language/libraries/matmul-rank.v1.json`, committed `ae3a5cb`).
The question was not "does the language refuse" but "does the contract path
accept something silently that the language refuses."

## Results

| seeded error | contract path | language path |
|---|---|---|
| Candidate verified against the wrong tensor format (2x2x2 schoolbook decomposition supplied for the 3x3x3 question) | **silent pass, all four requirements** — `[PASS] MM-R1-fails`, `[PASS] MM-R2-rank-bound` (8 ≤ 23), and `MM-R3-adds` flips its honest FAIL to `[PASS]` (4 ≤ 57) because the smaller tensor needs fewer adds. The emitted report declares `"tensor": "2x2x2"` in plaintext; no claim binds it. | `coverage` — `subject does not carry required scenario matmul-2x3x3-gf2` at `requirements[MM-R1]`, plan refused before execution |
| Verification ran over GF(3) while claiming GF(2) | cannot express field identity. The verify script is digest-pinned (substitution is caught at package integrity), but the field is inside the script's bytes: a GF(3) variant reports `fails: 130` on npz-333 yet still emits `"field": "GF(2)"` — the field string is hardcoded at `brent_verify.py:107`. An author-committed GF(3) script passes integrity and its report cannot be challenged. | `precondition_refuted` — `scenario.operating_domain` [3,3] ⊄ `material.applicability` [2,2], refused before execution |
| Equation system shares provenance with the producing search | cannot express — `input:candidate` binds artifact bytes; there is no provenance relation between inputs | `obligation_refuted` — recorded source edges are shared across the `over` set; the attested `independent` premise does not launder the refutation |
| Rank bound bound to the `fails` slot (same unit `1`, wrong quantity) | **silent pass end-to-end** — `MM-R2-rank-bound` evaluates `fails=0 ≤ 23` as PASS; after the author accepts the mutation (regenerated claims + campaign report + receipt), every gate is `[MATCH]` green. `kind: core.count` covers both slots. | `type_mismatch` — `requirement kind violation_count ≠ subject kind term_rank` at `requirements[MM-R2]` |
| Forged report bytes (adds 68→50, faking the MM-R3 pass) | refused at package integrity — `[FAILED] 2/3 external artifacts verified`, nothing downstream ran | refused at observation binding (receipt/input/output digests, `observation_foreign`) |
| Withdrawn/superseded verifier | capability-qualification lifecycle exists (`CORE-A4101` superseded, `CORE-A4603` revoked) at *capability* granularity; a verify-script revision is handled by contract amendment, not run-time refusal | `lifecycle_refused` at method bind under supplied lifecycle material (`method:matmul-rank/brent-verify@1=withdrawn`) |

## What the delta is

- **Two silent accepts, both at authoring-semantics granularity**: tensor
  format and quantity kind are *content* — the contract path binds bytes and
  a single undifferentiated `core.count` kind, so a wrong-format verification
  and a mis-slotted requirement both evaluate clean. This is the genuine
  soundness gain: refusals at the right stage against errors a reviewer would
  plausibly miss.
- **One vocabulary absence**: provenance between inputs. The contract path
  cannot express "the search verified itself" at all.
- **One honesty ceiling on both sides**: the language's `precondition_refuted`
  fires only when the field is *declared* — an author who claims GF(2) while
  shipping GF(3) arithmetic passes both paths. Declarations are checkable;
  opaque executable internals are not. Digest binding is byte-honesty, not
  semantic-honesty.
- **Parity at integrity**: forged report bytes are refused by both (digest
  chains). The residual attack — regenerating a self-consistent forged
  package (report + claims + campaign report + repinned manifest) — is
  defeated only by the *external* anchor: `--expect-manifest`, ADR-0015
  signatures, or version control. Neither path anchors internally.
- **Lifecycle granularity differs, not exists-vs-not**: the contract path has
  supersession/revocation at capability-qualification records; the language
  path refuses per-method under supplied lifecycle material. The language's
  is finer (no new contract revision needed) but the claim "contract can't
  express lifecycle" would be wrong — it is coarse, not absent.

## Over-refusal / burden, measured on this example

- Legitimate programs refused: none (the positive program passes
  conditionally, `conditional on format-current, independent-verification`).
- Declaration burden: the library is 83 lines; each program ~120 lines
  including entities, premises, and the union-scoped `independence` premise.
  The scope-equality requirement (premise `at` must equal the full relation
  union) cost real authoring iterations — already named in the EL-05
  assessment.
- Independent replay: the positive evaluation replays clean under
  `verifier/language_verify.py` (fixture committed); the contract-path runs
  above replay under `verify-case` for the committed package.

## Bounded conclusion

On this real domain the language path adds *semantic* checks the contract
path structurally lacks — format/field/provenance/kind are declared,
checkable vocabulary rather than prose in a statement field. The contract
path's guarantees (byte identity, receipt binding, lifecycle of
qualification records, committed-expectation replay) are real and are not
superseded; they operate one layer down. The combined evidence says the
language layer is additive protection, not replacement — and its ceiling is
declared semantics, which is exactly the boundary it claims.
