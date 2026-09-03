# CASE-002 campaign summary

Every number below is read from a Core campaign log; the summarizer constructs no verdict.

## Practice baselines

| candidate | all PASS | verdicts (margin) |
| --- | --- | --- |
| 96 cm polyethylene | no | R1 pass (5.145), R2 fail (-11.37), R3 fail (-25.08), R4 pass (597.6), R5 pass (4), R6 pass (1) |
| 96 cm polyethylene + 5 cm lead | no | R1 pass (7.309), R2 fail (-11.28), R3 pass (1.195), R4 pass (30.1), R5 fail (-1), R6 pass (0.9999) |
| 10 cm iron + 76 cm polyethylene + 5 cm lead | no | R1 pass (5.474), R2 fail (-13.74), R3 fail (-0.4881), R4 fail (-568.9), R5 pass (9), R6 fail (-3.277) |

## Search arms

| arm | screened only | transported | all-PASS | first all-PASS at transport # | best all-PASS (mass) | refused runs | INCONCLUSIVE verdicts |
| --- | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| recovery | 45 | 8 | 0 | - | - | 0 | 0 |
| learning | 221 | 11 | 0 | - | - | 0 | 2 |
| random | 121 | 6 | 0 | - | - | 0 | 0 |

## Verdict histograms over transported candidates

**recovery**: R1: pass 8; R2: fail 8; R3: fail 8; R4: pass 8; R5: pass 8; R6: pass 8

**learning**: R1: fail 4, pass 7; R2: fail 10, inconclusive 1; R3: fail 10, inconclusive 1; R4: pass 11; R5: pass 11; R6: fail 2, pass 9

**random**: R1: fail 5, pass 1; R2: fail 6; R3: fail 6; R4: pass 6; R5: pass 6; R6: fail 1, pass 5

## Control sweep

17 grid points transported; 0 all-PASS.

## Recovery check

{"applicable": false, "reason": "the sweep has no all-PASS point"}
