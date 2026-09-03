# CASE-002 campaign summary

Every number below is read from a Core campaign log; the summarizer constructs no verdict.

## Practice baselines

| candidate | all PASS | verdicts (margin) |
| --- | --- | --- |

## Search arms

| arm | screened only | transported | all-PASS | first all-PASS at transport # | best all-PASS (mass) | refused runs | INCONCLUSIVE verdicts |
| --- | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| llm | 15 | 8 | 5 | 2 | ? (3.6 kg) | 0 | 0 |

## Verdict histograms over transported candidates

**llm**: R1: pass 8; R2: fail 3, pass 5; R3: pass 8; R4: pass 8

## Control sweep

12 grid points transported; 10 all-PASS.
Optimum by mass: ? at 18 kg.

## Recovery check

{"applicable": false, "reason": "no recovery arm"}
