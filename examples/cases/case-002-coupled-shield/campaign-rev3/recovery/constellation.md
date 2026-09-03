# Engineering constellation

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.

Seeded with 0 prior observations (0 transported) from 0 logs; 0 rows skipped.

## Requirement states

| requirement | pass | fail | inconclusive | not_evaluated | times binding |
| --- | ---: | ---: | ---: | ---: | ---: |
| SHIELD-R1-screen | 6 | 34 | 0 | 0 | 30 |
| SHIELD-R2-neutron | 1 | 8 | 0 | 31 | 7 |
| SHIELD-R3-photon | 1 | 8 | 0 | 31 | 2 |
| SHIELD-R4-mass | 40 | 0 | 0 | 0 | 0 |
| SHIELD-R5-thickness | 40 | 0 | 0 | 0 | 1 |
| SHIELD-R6-activation | 9 | 0 | 0 | 31 | 0 |

## Sensitivity: d(log nominal)/d(thickness_cm) by material

- **SHIELD-R1-screen**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=-0.0318, polyethylene=-0.0392, water=+0.0000
- **SHIELD-R2-neutron**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=-0.0042, polyethylene=-0.0288, water=+0.0000
- **SHIELD-R3-photon**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=-0.0552, polyethylene=-0.0127, water=+0.0000
- **SHIELD-R4-mass**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0506, polyethylene=+0.0076, water=+0.0000
- **SHIELD-R5-thickness**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0090, polyethylene=+0.0098, water=+0.0000
- **SHIELD-R6-activation**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.1614, polyethylene=-0.0038, water=+0.0000

## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only

No candidate passed every requirement Core evaluated, so there is no feasible Pareto set yet.

## Best feasible candidate(s)

None: no candidate in this campaign passed every requirement Core evaluated.
