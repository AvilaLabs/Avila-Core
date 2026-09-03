# Engineering constellation

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.

## Requirement states

| requirement | pass | fail | inconclusive | not_evaluated | times binding |
| --- | ---: | ---: | ---: | ---: | ---: |
| SHIELD-R1-screen | 20 | 33 | 0 | 0 | 39 |
| SHIELD-R2-neutron | 0 | 8 | 0 | 45 | 0 |
| SHIELD-R3-photon | 0 | 8 | 0 | 45 | 8 |
| SHIELD-R4-mass | 53 | 0 | 0 | 0 | 0 |
| SHIELD-R5-thickness | 53 | 0 | 0 | 0 | 6 |
| SHIELD-R6-activation | 8 | 0 | 0 | 45 | 0 |

## Sensitivity: d(log nominal)/d(thickness_cm) by material

- **SHIELD-R1-screen**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=-0.0204, polyethylene=-0.1021, water=+0.0000
- **SHIELD-R2-neutron**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0000, polyethylene=-0.0801, water=+0.0000
- **SHIELD-R3-photon**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0000, polyethylene=-0.0328, water=+0.0000
- **SHIELD-R4-mass**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0409, polyethylene=+0.0175, water=+0.0000
- **SHIELD-R5-thickness**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0039, polyethylene=+0.0181, water=+0.0000
- **SHIELD-R6-activation**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0000, polyethylene=-0.0103, water=+0.0000

## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only

No candidate passed every requirement Core evaluated, so there is no feasible Pareto set yet.

## Best feasible candidate(s)

None: no candidate in this campaign passed every requirement Core evaluated.
