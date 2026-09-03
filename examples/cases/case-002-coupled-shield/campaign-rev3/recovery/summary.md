# Surrogate-assisted shielding configuration search

30 candidates screened over 3 round(s); 9 sent to transport; stopped because 6 consecutive transports with no improvement in the best transported worst-case real margin.
Seeded with 0 prior observations (0 transported) from 0 logs; 0 rows skipped.

| candidate | layers | worst real margin | fully feasible |
| --- | --- | ---: | --- |
| c-0006 | 120 cm polyethylene | -7.915 | False |
| c-0002 | 80 cm polyethylene | -74.8 | False |
| c-0000 | 65 cm polyethylene | -307.2 | False |
| c-0010 | 85 cm polyethylene | -44.58 | False |
| c-0011 | 60 cm polyethylene | -516.2 | False |
| c-0008 | 50 cm polyethylene | -1296 | False |
| c-0022 | 100 cm polyethylene | -20.52 | False |
| c-0026 | 85 cm polyethylene + 5 cm lead | -38.53 | False |
| c-0025 | 80 cm polyethylene + 5 cm lead | -62.61 | False |

Transport ran on 9 finalist(s); 0 passed every requirement Core evaluated.
An `inconclusive` verdict means the statistical interval straddles the limit; a `not_evaluated` one means the requirement's evidence was withheld, out of its qualification envelope, or never run — never a hidden PASS or FAIL.

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.
