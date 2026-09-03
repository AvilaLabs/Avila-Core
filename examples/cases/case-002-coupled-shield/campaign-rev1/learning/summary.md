# Surrogate-assisted shielding configuration search

220 candidates screened over 11 round(s); 11 sent to transport; stopped because no improvement in best worst-case margin for 5 rounds.

| candidate | layers | worst real margin | fully feasible |
| --- | --- | ---: | --- |
| c-0008 | 100 cm polyethylene | -20.77 | False |
| c-0007 | 80 cm borated_polyethylene | -55.45 | False |
| c-0046 | 90 cm polyethylene | -32.89 | False |
| c-0069 | 20 cm borated_polyethylene + 20 cm borated_polyethylene + 55 cm polyethylene | -12.97 | False |
| c-0080 | 10 cm borated_polyethylene + 80 cm borated_polyethylene + 5 cm iron | -9.376 | False |
| c-0096 | 85 cm water + 5 cm iron | -63.87 | False |
| c-0101 | 85 cm borated_polyethylene + 5 cm polyethylene + 5 cm iron | -7.963 | False |
| c-0122 | 10 cm borated_polyethylene + 80 cm polyethylene + 5 cm concrete | -16.18 | False |
| c-0160 | 60 cm concrete | -978.6 | False |
| c-0162 | 55 cm concrete + 10 cm borated_polyethylene | -456.3 | False |
| c-0181 | 5 cm polyethylene + 90 cm borated_polyethylene + 5 cm iron | -11.41 | False |

Transport ran on 11 finalist(s); 0 passed every requirement Core evaluated.
An `inconclusive` verdict means the statistical interval straddles the limit; a `not_evaluated` one means the requirement's evidence was withheld, out of its qualification envelope, or never run — never a hidden PASS or FAIL.

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.
