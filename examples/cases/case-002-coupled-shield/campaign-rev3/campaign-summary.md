# CASE-002 campaign summary

Every number below is read from a Core campaign log; the summarizer constructs no verdict.

## Practice baselines

| candidate | all PASS | verdicts (margin) |
| --- | --- | --- |

## Search arms

| arm | screened only | transported | all-PASS | first all-PASS at transport # | best all-PASS (mass) | refused runs | INCONCLUSIVE verdicts |
| --- | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| llm | 19 | 10 | 6 | 1 | 105 cm borated_polyethylene + 2 cm lead (1277 kg) | 0 | 2 |
| recovery | 31 | 9 | 0 | - | - | 0 | 0 |
| surrogate | 161 | 24 | 4 | 11 | 80 cm borated_polyethylene + 25 cm polyethylene + 5 cm iron (1428 kg) | 0 | 3 |

## Verdict histograms over transported candidates

**llm**: R1: pass 10; R2: inconclusive 1, pass 9; R3: fail 3, inconclusive 1, pass 6; R4: pass 10; R5: pass 10; R6: pass 10

**recovery**: R1: fail 6, pass 3; R2: fail 8, pass 1; R3: fail 8, pass 1; R4: pass 9; R5: pass 9; R6: pass 9

**surrogate**: R1: fail 8, pass 16; R2: fail 10, inconclusive 2, pass 12; R3: fail 17, inconclusive 1, pass 6; R4: pass 24; R5: pass 24; R6: fail 10, pass 14

## Control sweep

11 grid points transported; 3 all-PASS.
Optimum by mass: 105 cm polyethylene + 5 cm lead at 1554 kg.

## Recovery check

{"applicable": true, "reached": false, "sweep_points": 11, "designer_best_mass_kg": null, "sweep_optimum_mass_kg": "3109/2"}
