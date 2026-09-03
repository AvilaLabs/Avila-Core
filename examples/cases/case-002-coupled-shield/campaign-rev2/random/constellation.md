# Engineering constellation

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.

## Requirement states

| requirement | pass | fail | inconclusive | not_evaluated | times binding |
| --- | ---: | ---: | ---: | ---: | ---: |
| SHIELD-R1-screen | 45 | 208 | 0 | 0 | 231 |
| SHIELD-R2-neutron | 7 | 3 | 2 | 241 | 0 |
| SHIELD-R3-photon | 0 | 12 | 0 | 241 | 12 |
| SHIELD-R4-mass | 253 | 0 | 0 | 0 | 0 |
| SHIELD-R5-thickness | 253 | 0 | 0 | 0 | 10 |
| SHIELD-R6-activation | 10 | 2 | 0 | 241 | 0 |

## Sensitivity: d(log nominal)/d(thickness_cm) by material

- **SHIELD-R1-screen**: borated_polyethylene=-0.1074, concrete=-0.0864, iron=-0.1605, lead=-0.1080, polyethylene=-0.1077, water=-0.1008
- **SHIELD-R2-neutron**: borated_polyethylene=-0.0057, concrete=-0.0066, iron=+0.0000, lead=+0.0000, polyethylene=-0.0026, water=+0.0057
- **SHIELD-R3-photon**: borated_polyethylene=-0.0033, concrete=-0.0031, iron=+0.0000, lead=+0.0000, polyethylene=+0.0006, water=+0.0027
- **SHIELD-R4-mass**: borated_polyethylene=+0.0165, concrete=+0.0267, iron=+0.0827, lead=+0.1228, polyethylene=+0.0157, water=+0.0142
- **SHIELD-R5-thickness**: borated_polyethylene=+0.0200, concrete=+0.0229, iron=+0.0423, lead=+0.0245, polyethylene=+0.0199, water=+0.0202
- **SHIELD-R6-activation**: borated_polyethylene=-0.0078, concrete=+0.1278, iron=+0.0000, lead=+0.0000, polyethylene=-0.0018, water=-0.0003

## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only

No candidate passed every requirement Core evaluated, so there is no feasible Pareto set yet.

## Best feasible candidate(s)

None: no candidate in this campaign passed every requirement Core evaluated.
