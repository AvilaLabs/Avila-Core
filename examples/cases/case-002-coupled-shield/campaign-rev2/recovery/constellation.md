# Engineering constellation

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.

## Requirement states

| requirement | pass | fail | inconclusive | not_evaluated | times binding |
| --- | ---: | ---: | ---: | ---: | ---: |
| SHIELD-R1-screen | 12 | 18 | 0 | 0 | 24 |
| SHIELD-R2-neutron | 1 | 4 | 0 | 25 | 2 |
| SHIELD-R3-photon | 2 | 3 | 0 | 25 | 3 |
| SHIELD-R4-mass | 30 | 0 | 0 | 0 | 0 |
| SHIELD-R5-thickness | 30 | 0 | 0 | 0 | 1 |
| SHIELD-R6-activation | 4 | 1 | 0 | 25 | 0 |

## Sensitivity: d(log nominal)/d(thickness_cm) by material

- **SHIELD-R1-screen**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=-0.0215, polyethylene=-0.0999, water=+0.0000
- **SHIELD-R2-neutron**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0163, polyethylene=-0.0322, water=+0.0000
- **SHIELD-R3-photon**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=-0.0498, polyethylene=-0.0001, water=+0.0000
- **SHIELD-R4-mass**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0340, polyethylene=+0.0129, water=+0.0000
- **SHIELD-R5-thickness**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.0023, polyethylene=+0.0182, water=+0.0000
- **SHIELD-R6-activation**: borated_polyethylene=+0.0000, concrete=+0.0000, iron=+0.0000, lead=+0.2201, polyethylene=-0.0832, water=+0.0000

## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only

No candidate passed every requirement Core evaluated, so there is no feasible Pareto set yet.

## Best feasible candidate(s)

None: no candidate in this campaign passed every requirement Core evaluated.
