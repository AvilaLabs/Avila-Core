# Engineering-language example programs (EL-01)

Authored before any checker exists. The normative rules are in
[docs/architecture/ENGINEERING_LANGUAGE.md](../../docs/architecture/ENGINEERING_LANGUAGE.md)
(profile `avila.core/language/0.1-draft`, ADR-0027); these documents are
specification artifacts — `expectations.json` records what the rules say
each program must produce, so the first implementation is judged against
the spec rather than the other way around.

## Layout

- `libraries/` — method libraries (`avila.core/method-library/v0.1-draft`):
  `thermal-expansion` (the charter's toy model), `measurement-scaling`
  (the second library, pinning independence-premise behavior), and
  `uncanonical-units` (a deliberately inadmissible library — a kind with
  no `canonical_unit`, exercising the unit-admission gate).
- `programs/` — authored programs (`avila.core/language-program/v0.1-draft`).
- `expectations.json` — per-program specified analysis/plan/verdict
  outcomes, the `identity_cases` semantic-projection requirements
  (positional operand order is identity-bearing; declared sets are not;
  user identifiers named `note`/`label`/`description` are preserved),
  plus the adversarial rows that live at the execution/replay boundary
  and therefore have no program file.

## The programs

| Program | Specified result |
| --- | --- |
| `clearance-pass` | clean analysis; on execution `EL-R1` PASS (`[3/10,2/5] ≥ 1/4`) |
| `clearance-inconclusive` | `EL-R1` INCONCLUSIVE (limit `7/20` inside the enclosure) |
| `clearance-fail` | `EL-R1` FAIL (`2/5 < 9/20`) |
| `clearance-not-evaluated` | `EL-R1` NOT_EVALUATED — `temperature_change` declared `unavailable` |
| `clearance-heuristic-unusable` | `EL-R1` NOT_EVALUATED — the method declares no postcondition; its nominal output cannot serve a bounded requirement |
| `positive-assumption-discharge` | PASS conditional on `fixture-symmetric@(bracket@2)` — the unconditional witness drops `uniform-temperature-change`; the conditional witness trades `linear-expansion-model` for its own support |
| `positive-measurement-pass` | `EL-R2` PASS (`231/100 ≤ 5/2`) — `independent(reading_a, reading_b)` discharged by an attested premise |
| `cyclic-witnesses` | `cyclic_witness` findings (mutual + self-dependent); discharge refused, assumptions stay residual, verdict still passes conditionally |
| `invalid-geometry-mismatch` | blocking type finding naming `bracket@1` vs `bracket@2` |
| `invalid-scope-mismatch` | unmet coverage obligation — steady-state subject under a transient requirement |
| `invalid-scenario-mismatch` | scenario identity failure — `field-survey-1` ≠ `field-survey-2` despite equal `steady-state` scope |
| `invalid-conflicting-assumptions` | contradiction finding; dependent uses blocked |
| `invalid-nominal-enclosure` | claim mismatch — a `nominal` input cannot satisfy an `enclosure` slot |
| `invalid-applicability-domain` | refuted precondition — scenario domain `⊄` material applicability |
| `invalid-product-kind` | unsupported product — no `kind_products` row for `length × temperature` |
| `invalid-relation-absent` | `relation_absent` — a relation-less import cannot fill a slot requiring `scenario` and `material` |
| `invalid-import-missing-assumptions` | malformed import — residual assumptions must be declared |
| `invalid-nominal-arithmetic` | unsupported — a `nominal` operand makes `interval.mul` inapplicable; there is no nominal arithmetic |
| `invalid-shared-source-independence` | `provenance_disjoint` refuted by the shared calibration edge; `independent` stays open — neither blocks silently |
| `invalid-independence-unknown` | independence open — disjoint recorded provenance, but no attested premise |
| `invalid-provenance-conflict` | `premise_conflict` — asserting `provenance_disjoint` against a recorded shared edge contradicts the record itself |
| `invalid-mixed-units` | inadmissible library — a kind with no `canonical_unit` (`malformed`); the additive rules also refuse mixed-unit operands (`type_mismatch`); `sum` stays unestablished |
| `unfinished-missing-input` | authoring hole — `coefficient` declared but never bound |
| `unfinished-goal-hole` | open hole reachable from the requirement |
| `unfinished-ambiguous-methods` | named ambiguity — three methods match the goal's type, no silent choice |

Programs are inspectable when incomplete: every "refused" row still
produces a full analysis record naming the finding.
