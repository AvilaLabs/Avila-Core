# Surrogate-assisted shielding configuration search

160 candidates screened over 8 round(s); 24 sent to transport; stopped because 10 consecutive transports with no improvement in the best transported worst-case real margin.
Seeded with 883 prior observations (105 transported) from 11 logs; 0 rows skipped.

| candidate | layers | worst real margin | fully feasible |
| --- | --- | ---: | --- |
| c-0001 | 5 cm iron + 20 cm polyethylene + 5 cm concrete | -6310 | False |
| c-0000 | 10 cm concrete + 5 cm lead + 5 cm concrete | -2.73e+04 | False |
| c-0010 | 25 cm polyethylene + 25 cm borated_polyethylene + 20 cm concrete | -281.5 | False |
| c-0031 | 25 cm polyethylene + 5 cm iron + 85 cm water | -6.611 | False |
| c-0027 | 25 cm borated_polyethylene + 85 cm water | -8.007 | False |
| c-0008 | 25 cm polyethylene + 85 cm water | -12.88 | False |
| c-0011 | 5 cm lead + 50 cm polyethylene + 10 cm water | -311 | False |
| c-0038 | 5 cm lead + 50 cm polyethylene + 5 cm water | -487.2 | False |
| c-0042 | 85 cm borated_polyethylene + 25 cm concrete | -0.2142 | False |
| c-0075 | 90 cm polyethylene + 25 cm concrete | -1.212 | False |
| c-0055 | 100 cm borated_polyethylene + 20 cm concrete | 0 | True |
| c-0021 | 45 cm concrete + 50 cm borated_polyethylene | -23.35 | False |
| c-0080 | 35 cm water + 70 cm concrete | -26.79 | False |
| c-0097 | 85 cm polyethylene + 5 cm lead + 25 cm borated_polyethylene | 0.9996 | True |
| c-0081 | 80 cm borated_polyethylene + 25 cm polyethylene + 5 cm iron | 0.4911 | True |
| c-0089 | 5 cm concrete + 80 cm borated_polyethylene + 30 cm polyethylene | -14.1 | False |
| c-0087 | 5 cm concrete + 80 cm borated_polyethylene + 25 cm polyethylene | -16.68 | False |
| c-0116 | 10 cm concrete + 45 cm borated_polyethylene + 65 cm polyethylene | -14.9 | False |
| c-0120 | 85 cm borated_polyethylene + 25 cm polyethylene + 5 cm iron | 0.885 | True |
| c-0126 | 80 cm borated_polyethylene + 30 cm concrete | -0.8228 | False |
| c-0132 | 80 cm borated_polyethylene + 25 cm polyethylene + 10 cm concrete | -1.307 | False |
| c-0140 | 25 cm concrete + 20 cm polyethylene + 10 cm lead | -1274 | False |
| c-0141 | 5 cm lead + 85 cm borated_polyethylene + 25 cm polyethylene | -3.463 | False |
| c-0121 | 5 cm water + 80 cm borated_polyethylene + 25 cm concrete | -1.889 | False |

Transport ran on 24 finalist(s); 4 passed every requirement Core evaluated.
An `inconclusive` verdict means the statistical interval straddles the limit; a `not_evaluated` one means the requirement's evidence was withheld, out of its qualification envelope, or never run — never a hidden PASS or FAIL.

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.
