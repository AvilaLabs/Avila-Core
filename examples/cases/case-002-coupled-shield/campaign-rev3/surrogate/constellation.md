# Engineering constellation

The surrogate only proposes candidates and predicts their margins; it never constructs, edits, or substitutes for a Core verdict. Every status, rule, limit, and margin in this file is copied from Avila Core's own --json report or --log campaign log.

Seeded with 883 prior observations (105 transported) from 11 logs; 0 rows skipped.

## Requirement states

| requirement | pass | fail | inconclusive | not_evaluated | times binding |
| --- | ---: | ---: | ---: | ---: | ---: |
| SHIELD-R1-screen | 129 | 56 | 0 | 0 | 120 |
| SHIELD-R2-neutron | 12 | 10 | 2 | 161 | 8 |
| SHIELD-R3-photon | 6 | 17 | 1 | 161 | 12 |
| SHIELD-R4-mass | 185 | 0 | 0 | 0 | 0 |
| SHIELD-R5-thickness | 185 | 0 | 0 | 0 | 43 |
| SHIELD-R6-activation | 14 | 10 | 0 | 161 | 2 |

## Sensitivity: d(log nominal)/d(thickness_cm) by material

- **SHIELD-R1-screen**: borated_polyethylene=-0.0363, concrete=-0.0395, iron=-0.1173, lead=-0.0676, polyethylene=-0.0364, water=-0.0294
- **SHIELD-R2-neutron**: borated_polyethylene=-0.0253, concrete=-0.0365, iron=-0.0793, lead=-0.0289, polyethylene=-0.0249, water=-0.0127
- **SHIELD-R3-photon**: borated_polyethylene=-0.0133, concrete=-0.0183, iron=-0.0955, lead=-0.1660, polyethylene=-0.0090, water=-0.0054
- **SHIELD-R4-mass**: borated_polyethylene=+0.0045, concrete=+0.0106, iron=+0.0703, lead=+0.1173, polyethylene=+0.0036, water=+0.0033
- **SHIELD-R5-thickness**: borated_polyethylene=+0.0063, concrete=+0.0097, iron=+0.0251, lead=+0.0223, polyethylene=+0.0063, water=+0.0064
- **SHIELD-R6-activation**: borated_polyethylene=+0.0144, concrete=+0.0556, iron=+0.1530, lead=+0.3777, polyethylene=+0.0129, water=+0.0203

## Pareto set: mass vs. primary (worst bounded) dose metric, feasible candidates only

| candidate sha256 | mass | primary dose |
| --- | ---: | ---: |
| sha256:039432d3be3582b1e4d853491ea862e1832dfed352e8a879c1ac4b0870da97be | 1428 | 4.025 |
| sha256:a2aefec71c9695f892d513d52e6738f0d8178ca4521c0fe5750210152d727caf | 1460 | 2.255 |
| sha256:f8e8fe8cca5009be25b80a0d99d25d71dcb2ba072ec19579781fd8d5f958fe7b | 1478 | 2.04 |
| sha256:8e0e51f8e431ffd8c81bd346ce4487ac136a8d2fc93f4186e6d2e27428ee2a35 | 1616 | 1.966 |

## Best feasible candidate(s)

- sha256:8e0e51f8e431ffd8c81bd346ce4487ac136a8d2fc93f4186e6d2e27428ee2a35: primary dose 1.966, mass 1616.5
- sha256:f8e8fe8cca5009be25b80a0d99d25d71dcb2ba072ec19579781fd8d5f958fe7b: primary dose 2.04, mass 1478.5
- sha256:a2aefec71c9695f892d513d52e6738f0d8178ca4521c0fe5750210152d727caf: primary dose 2.255, mass 1460.0
- sha256:039432d3be3582b1e4d853491ea862e1832dfed352e8a879c1ac4b0870da97be: primary dose 4.025, mass 1428.5
