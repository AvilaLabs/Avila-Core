# Surrogate-assisted shielding configuration search

120 candidates screened over 6 round(s); 6 sent to transport; stopped because no improvement in best worst-case margin for 5 rounds.

| candidate | layers | worst real margin | fully feasible |
| --- | --- | ---: | --- |
| c-0007 | 95 cm borated_polyethylene | -9.654 | False |
| c-0025 | 5 cm concrete + 100 cm polyethylene | -31.88 | False |
| c-0058 | 5 cm concrete + 5 cm borated_polyethylene + 100 cm polyethylene | -24.7 | False |
| c-0061 | 10 cm iron + 100 cm polyethylene | -18.24 | False |
| c-0084 | 5 cm iron + 105 cm polyethylene | -23.55 | False |
| c-0076 | 80 cm concrete + 15 cm water | -25.72 | False |

Transport ran on 6 finalist(s); 0 passed every requirement Core evaluated.
An `inconclusive` verdict means the statistical interval straddles the limit; a `not_evaluated` one means the requirement's evidence was withheld, out of its qualification envelope, or never run — never a hidden PASS or FAIL.

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.
