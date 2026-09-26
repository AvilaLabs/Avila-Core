# EL-02 implementation review

A self-review pass over the analyzer (`avila_core_compiler::language`) at
`1b445df`, in the convention of the earlier spec reviews — adversarial, aimed
at paths the 24-program corpus does not exercise. It supplements, and does
not replace, the independent review every EL milestone still awaits; the
verifier-side EL-04 audit (`8632b88`) had already hardened replay parity —
this pass questioned the Rust semantics themselves.

## Finding 1 — unit gap under kinds without `canonical_unit` (fixed)

`quantity_kind.canonical_unit` is `Option` in the document grammar. Admission
checks `value.unit == canonical_unit` only when a canonical unit exists, so a
kind declaring none admitted any unit — and the additive rules (P-ADD/P-SUB)
check kind equality, not unit equality, inheriting `left.unit` unchecked.
The multiplicative rule is kind-directed (a unit string is never consulted)
and the requirement comparator compares exact bounds, so nothing anywhere
rejected mixed units. Demonstrated: `[1,2]mm + [10,20]cm` under one unitless
`rate` kind silently bound `[11,22]` — wrong in either unit — with an
`established` state.

Every corpus kind declares a canonical unit, which is why the corpus never
saw it: the check existed, the declaration that makes it applicable was
optional.

Fixed in three places:

- `LibraryChecker` refuses a kind with no `canonical_unit` (`malformed`,
  `quantity_kinds[kind]`). §13's grammar already pairs every kind with its
  unit; the option was lenient decode, not intent.
- `RuleFailure::UnitMismatch` — P-ADD/P-SUB now require coincident units
  and report `type_mismatch` (`body[i].implementation`), so even inside a
  refused analysis the wrong value is never derived.
- `poison` now carries the declared kind's canonical unit instead of `""`.
  Without this, a poisoned operand (already `unestablished`) produced a
  spurious `mm` vs `""` type-mismatch on the next step — the unestablished
  state join already made the arithmetic moot; the fake unit keeps the
  record quiet and honest.

Spec §7.A now states the shared-unit premise explicitly; `CORE-E8010`'s
catalog text covers it. The verifier got both gates plus
`library_admission_ok` parity — `EvalFailure("type_mismatch")` on mixed
units, unitless kinds unadmitted. Fixture:
`examples/language/libraries/uncanonical-units.v1.json` +
`examples/language/programs/invalid-mixed-units.program.json` (findings:
`malformed` at `quantity_kinds[rate]`, `type_mismatch` at
`body[0].implementation`, `library_pin_mismatch`; `sum` stays
`unestablished`).

## Verified — paths the corpus does not reach

- **Semantic projection coverage is total.** I enumerated every field of
  `LibraryDocument`/`ProgramDocument` against `projection.rs`: all
  non-annotation fields project (schema_version, profile, every relation
  key, `produces`, `infer.arguments` as positional sequence, `import`'s
  present-vs-absent `assumptions`, requirement `scope` vs `scenario` — the
  identity-vs-kind distinction is preserved). Undeclared fields fail at
  both serde (`deny_unknown_fields`) and projection — two gates, neither
  launderable.
- **Duplicate identifiers are refused at admission** for `inputs`,
  `premises`, `assumptions`, `requirements`, and `body` bind names
  (sequential single-assignment). The first-wins canonical binding in
  `load_inputs` is unreachable in admitted documents, so declared-set
  order-insensitivity cannot fork semantics under one identity.
- **Judgement call worth an independent eye:** `ProvenanceDecl.statement`
  is an annotation — two programs differing only in the asserted claim's
  text share identity. Consistent (the machine never consumes claim text;
  `party` is semantic), but it is a decision about what a source
  assertion *means*, and it should be a conscious one.
- **`bad` short-circuits before eval; `refuted` poisons before witness
  support merges.** Slot-admission failure and precondition refutation
  cannot reach the arithmetic rules — verified by reading, and the
  ordering now matters more since the unit gate fires inside eval.
- **Exact claim arithmetic is consistent:** exact×exact cannot produce a
  non-point interval (exact operands are points), so the
  `claim == Exact && lower == upper` demotion arm is unreachable for
  genuine exact products — correct, not dead code worth removing.

## Remaining limits

- This is self-review. The unit gap is exactly the class of bug a
  corpus-driven audit misses: the corpus never constructed a unitless
  kind, so every green fixture also passed under the buggy semantics.
  Adversarial review needs inputs the author did not think to write.
- `canonical_unit` is still `Option` at the serde layer — the refusal is
  a checker finding, so a future profile could legitimately admit
  unitless kinds; the eval gate (coincident units required) is the part
  that must stay regardless of profile.
- No new finding codes were introduced: `type_mismatch` (`CORE-E8010`)
  covers the unit gate; `malformed` (`CORE-E8001`) covers the admission
  rule. `TEST_PINNED` entries for E8001/E8002 removed — fixtures now
  exercise them.

## Result

- One soundness bug found and fixed in both implementations, with fixture,
  spec clause, and catalog text updated.
- `cargo fmt`, `clippy -D warnings`, all 23 test binaries, 59 verifier
  tests, and all 9 fixture evaluations green.

EL-02 remains **in review** — this pass closed the finding it surfaced;
independent review is still owed.
