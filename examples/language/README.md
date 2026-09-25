# Engineering-language example programs (EL-01)

Authored before any checker exists. The normative rules are in
[docs/architecture/ENGINEERING_LANGUAGE.md](../../docs/architecture/ENGINEERING_LANGUAGE.md)
(profile `avila.core/language/0.1-draft`, ADR-0027); these documents are
specification artifacts — `expectations.json` records what the rules say
each program must produce, so the first implementation is judged against
the spec rather than the other way around.

## Layout

- `libraries/` — method libraries (`avila.core/method-library/v0.1-draft`):
  `thermal-expansion` (the charter's toy model) and `measurement-scaling`
  (the second library, pinning independence-premise behavior).
- `programs/` — authored programs (`avila.core/language-program/v0.1-draft`).
- `expectations.json` — per-program specified analysis/plan/verdict
  outcomes, plus the adversarial rows that live at the execution/replay
  boundary and therefore have no program file.

## The programs

| Program | Specified result |
| --- | --- |
| `clearance-pass` | clean analysis; on execution `EL-R1` PASS (`[3/10,2/5] ≥ 1/4`) |
| `clearance-inconclusive` | `EL-R1` INCONCLUSIVE (limit `7/20` inside the enclosure) |
| `clearance-fail` | `EL-R1` FAIL (`2/5 < 9/20`) |
| `clearance-not-evaluated` | `EL-R1` NOT_EVALUATED — `temperature_change` declared `unavailable` |
| `clearance-heuristic-unusable` | `EL-R1` NOT_EVALUATED — the method declares no postcondition; its nominal output cannot serve a bounded requirement |
| `invalid-geometry-mismatch` | blocking type finding naming `bracket@1` vs `bracket@2` |
| `invalid-scope-mismatch` | unmet coverage obligation — steady-state subject under a transient requirement |
| `invalid-conflicting-assumptions` | contradiction finding; dependent uses blocked |
| `invalid-nominal-enclosure` | claim mismatch — a `nominal` input cannot satisfy an `enclosure` slot |
| `invalid-applicability-domain` | refuted precondition — scenario domain `⊄` material applicability |
| `invalid-import-missing-assumptions` | malformed import — residual assumptions must be declared |
| `invalid-shared-source-independence` | unmet independence premise — shared calibration edge |
| `unfinished-missing-input` | authoring hole — `coefficient` declared but never bound |
| `unfinished-goal-hole` | open hole reachable from the requirement |
| `unfinished-ambiguous-methods` | named ambiguity — two applicable methods, no silent choice |

Programs are inspectable when incomplete: every "refused" row still
produces a full analysis record naming the finding.
