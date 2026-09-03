# Engineering constellation

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.

## Requirement states

| requirement | pass | fail | inconclusive | not_evaluated | times binding |
| --- | ---: | ---: | ---: | ---: | ---: |
| SHIELD-R1-screen | 5 | 122 | 0 | 0 | 118 |
| SHIELD-R2-neutron | 0 | 6 | 0 | 121 | 3 |
| SHIELD-R3-photon | 0 | 6 | 0 | 121 | 3 |
| SHIELD-R4-mass | 127 | 0 | 0 | 0 | 0 |
| SHIELD-R5-thickness | 127 | 0 | 0 | 0 | 3 |
| SHIELD-R6-activation | 5 | 1 | 0 | 121 | 0 |

## Sensitivity: d(log nominal)/d(thickness_cm) by material

- **SHIELD-R1-screen**: borated_polyethylene=-0.1047, concrete=-0.0821, iron=-0.1522, lead=-0.0961, polyethylene=-0.1062, water=-0.0986
- **SHIELD-R2-neutron**: borated_polyethylene=-0.0002, concrete=-0.0157, iron=+0.0276, lead=+0.0000, polyethylene=-0.0026, water=+0.0029
- **SHIELD-R3-photon**: borated_polyethylene=-0.0010, concrete=+0.0023, iron=-0.0314, lead=+0.0000, polyethylene=+0.0008, water=+0.0012
- **SHIELD-R4-mass**: borated_polyethylene=+0.0208, concrete=+0.0270, iron=+0.0960, lead=+0.1408, polyethylene=+0.0218, water=+0.0161
- **SHIELD-R5-thickness**: borated_polyethylene=+0.0234, concrete=+0.0251, iron=+0.0543, lead=+0.0269, polyethylene=+0.0267, water=+0.0220
- **SHIELD-R6-activation**: borated_polyethylene=-0.0008, concrete=+0.2900, iron=+0.0947, lead=+0.0000, polyethylene=-0.0059, water=-0.0089

## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only

No candidate passed every requirement Core evaluated, so there is no feasible Pareto set yet.

## Best feasible candidate(s)

None: no candidate in this campaign passed every requirement Core evaluated.
