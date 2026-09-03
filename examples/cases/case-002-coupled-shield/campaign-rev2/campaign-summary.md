# CASE-002 campaign summary

Every number below is read from a Core campaign log; the summarizer constructs no verdict.

## Practice baselines

| candidate | all PASS | verdicts (margin) |
| --- | --- | --- |
| 96 cm polyethylene | no | R1 pass (5.145), R2 fail (-12.21), R3 fail (-24.73), R4 pass (1098), R5 pass (24), R6 pass (1) |
| 96 cm polyethylene + 5 cm lead | no | R1 pass (7.309), R2 fail (-10.29), R3 pass (1.197), R4 pass (530.1), R5 pass (19), R6 pass (0.9999) |
| 10 cm iron + 76 cm polyethylene + 5 cm lead | no | R1 pass (5.474), R2 fail (-12.89), R3 fail (-0.2798), R4 fail (-68.9), R5 pass (29), R6 fail (-3.279) |

## Search arms

| arm | screened only | transported | all-PASS | first all-PASS at transport # | best all-PASS (mass) | refused runs | INCONCLUSIVE verdicts |
| --- | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| recovery | 25 | 5 | 0 | - | - | 0 | 0 |
| learning | 121 | 6 | 0 | - | - | 0 | 1 |
| random | 241 | 12 | 0 | - | - | 0 | 2 |

## Verdict histograms over transported candidates

**recovery**: R1: pass 5; R2: fail 4, pass 1; R3: fail 3, pass 2; R4: pass 5; R5: pass 5; R6: fail 1, pass 4

**learning**: R1: fail 1, pass 5; R2: fail 2, inconclusive 1, pass 3; R3: fail 6; R4: pass 6; R5: pass 6; R6: fail 5, pass 1

**random**: R1: pass 12; R2: fail 3, inconclusive 2, pass 7; R3: fail 12; R4: pass 12; R5: pass 12; R6: fail 2, pass 10

## Control sweep

31 grid points transported; 0 all-PASS.

## Recovery check

{"applicable": false, "reason": "the sweep has no all-PASS point"}
