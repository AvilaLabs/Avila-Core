# Surrogate-assisted shielding configuration search

120 candidates screened over 6 round(s); 6 sent to transport; stopped because no improvement in best worst-case margin for 5 rounds.

| candidate | layers | worst real margin | fully feasible |
| --- | --- | ---: | --- |
| c-0007 | 80 cm borated_polyethylene | -55.45 | False |
| c-0034 | 85 cm borated_polyethylene | -31.68 | False |
| c-0050 | 55 cm water + 10 cm water + 25 cm borated_polyethylene | -51.54 | False |
| c-0058 | 65 cm water + 5 cm borated_polyethylene + 10 cm iron | -164.9 | False |
| c-0090 | 15 cm water + 20 cm concrete + 55 cm borated_polyethylene | -31.11 | False |
| c-0111 | 95 cm polyethylene | -25.71 | False |

Transport ran on 6 finalist(s); 0 passed every requirement Core evaluated.
An `inconclusive` verdict means the statistical interval straddles the limit; a `not_evaluated` one means the requirement's evidence was withheld, out of its qualification envelope, or never run — never a hidden PASS or FAIL.

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.
