# Engineering constellation

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.

## Requirement states

| requirement | pass | fail | inconclusive | not_evaluated | times binding |
| --- | ---: | ---: | ---: | ---: | ---: |
| SHIELD-R1-screen | 66 | 61 | 0 | 0 | 98 |
| SHIELD-R2-neutron | 3 | 2 | 1 | 121 | 1 |
| SHIELD-R3-photon | 0 | 6 | 0 | 121 | 5 |
| SHIELD-R4-mass | 127 | 0 | 0 | 0 | 0 |
| SHIELD-R5-thickness | 127 | 0 | 0 | 0 | 23 |
| SHIELD-R6-activation | 1 | 5 | 0 | 121 | 0 |

## Sensitivity: d(log nominal)/d(thickness_cm) by material

- **SHIELD-R1-screen**: borated_polyethylene=-0.1013, concrete=-0.0785, iron=-0.1310, lead=-0.1015, polyethylene=-0.1015, water=-0.0945
- **SHIELD-R2-neutron**: borated_polyethylene=-0.0001, concrete=+0.0044, iron=-0.0350, lead=+0.0000, polyethylene=-0.0021, water=+0.0232
- **SHIELD-R3-photon**: borated_polyethylene=-0.0014, concrete=-0.0014, iron=-0.0121, lead=+0.0000, polyethylene=+0.0016, water=-0.0085
- **SHIELD-R4-mass**: borated_polyethylene=+0.0145, concrete=+0.0227, iron=+0.0749, lead=+0.0880, polyethylene=+0.0130, water=+0.0126
- **SHIELD-R5-thickness**: borated_polyethylene=+0.0166, concrete=+0.0179, iron=+0.0152, lead=+0.0150, polyethylene=+0.0165, water=+0.0165
- **SHIELD-R6-activation**: borated_polyethylene=-0.0320, concrete=+0.0096, iron=+0.0078, lead=+0.0000, polyethylene=+0.0143, water=+0.0438

## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only

No candidate passed every requirement Core evaluated, so there is no feasible Pareto set yet.

## Best feasible candidate(s)

None: no candidate in this campaign passed every requirement Core evaluated.
