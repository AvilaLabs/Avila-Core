# Engineering constellation

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.

## Requirement states

| requirement | pass | fail | inconclusive | not_evaluated | times binding |
| --- | ---: | ---: | ---: | ---: | ---: |
| SHIELD-R1-screen | 123 | 109 | 0 | 0 | 158 |
| SHIELD-R2-neutron | 0 | 10 | 1 | 221 | 6 |
| SHIELD-R3-photon | 0 | 10 | 1 | 221 | 4 |
| SHIELD-R4-mass | 232 | 0 | 0 | 0 | 0 |
| SHIELD-R5-thickness | 232 | 0 | 0 | 0 | 64 |
| SHIELD-R6-activation | 9 | 2 | 0 | 221 | 0 |

## Sensitivity: d(log nominal)/d(thickness_cm) by material

- **SHIELD-R1-screen**: borated_polyethylene=-0.1007, concrete=-0.0740, iron=-0.1031, lead=-0.0722, polyethylene=-0.1005, water=-0.0929
- **SHIELD-R2-neutron**: borated_polyethylene=-0.0020, concrete=+0.0148, iron=-0.0184, lead=+0.0000, polyethylene=-0.0084, water=+0.0037
- **SHIELD-R3-photon**: borated_polyethylene=-0.0055, concrete=+0.0054, iron=-0.0547, lead=+0.0000, polyethylene=+0.0009, water=+0.0031
- **SHIELD-R4-mass**: borated_polyethylene=+0.0161, concrete=+0.0354, iron=+0.1065, lead=+0.1383, polyethylene=+0.0158, water=+0.0157
- **SHIELD-R5-thickness**: borated_polyethylene=+0.0184, concrete=+0.0266, iron=+0.0159, lead=+0.0190, polyethylene=+0.0191, water=+0.0185
- **SHIELD-R6-activation**: borated_polyethylene=-0.0375, concrete=+0.0811, iron=+0.1309, lead=+0.0000, polyethylene=+0.0052, water=-0.0021

## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only

No candidate passed every requirement Core evaluated, so there is no feasible Pareto set yet.

## Best feasible candidate(s)

None: no candidate in this campaign passed every requirement Core evaluated.
