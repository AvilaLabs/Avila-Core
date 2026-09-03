# Conventional-practice baselines

Unshielded dose rate: 187200 uSv/h. Design target (limit 10 uSv/h / factor 2): 5 uSv/h.

These are conventional-practice comparison arms, sized by the same unqualified removal-cross-section method the screen uses, not a learned or optimized design. --run reports only Core's own verdicts; nothing here constructs one.

## practice-removal-poly

removal-cross-section method, design factor 2: polyethylene alone sized to attenuate the unshielded dose (187200 uSv/h) to limit/2 (5 uSv/h).

Layers: 96 cm polyethylene

| requirement | status | rule | margin |
| --- | --- | --- | ---: |
| SHIELD-R1-screen | pass | nominal.le.within | 5.145 |
| SHIELD-R2-neutron | fail | bounded.le.exceeds | -12.21 |
| SHIELD-R3-photon | fail | bounded.le.exceeds | -24.73 |
| SHIELD-R4-mass | pass | bounded.le.within | 1098 |
| SHIELD-R5-thickness | pass | bounded.le.within | 24 |
| SHIELD-R6-activation | pass | nominal.le.within | 1 |

## practice-removal-poly-pb

removal-cross-section method, design factor 2, identical polyethylene thickness as practice-removal-poly, plus the usual capture-gamma rule: a fixed 5 cm lead layer after the moderator to absorb neutron-capture gammas the removal calculation does not model.

Layers: 96 cm polyethylene + 5 cm lead

| requirement | status | rule | margin |
| --- | --- | --- | ---: |
| SHIELD-R1-screen | pass | nominal.le.within | 7.309 |
| SHIELD-R2-neutron | fail | bounded.le.exceeds | -10.29 |
| SHIELD-R3-photon | pass | bounded.le.within | 1.197 |
| SHIELD-R4-mass | pass | bounded.le.within | 530.1 |
| SHIELD-R5-thickness | pass | bounded.le.within | 19 |
| SHIELD-R6-activation | pass | nominal.le.within | 0.9999 |

## practice-fe-poly-pb

classic iron/polyethylene/lead sandwich: a fixed 10 cm iron first layer (cheap inelastic removal of the highest-energy neutrons), polyethylene sized by the removal-cross-section method at the same design factor 2 for the remaining attenuation the iron and lead do not supply, and the same fixed 5 cm lead capture-gamma layer last.

Layers: 10 cm iron + 76 cm polyethylene + 5 cm lead

| requirement | status | rule | margin |
| --- | --- | --- | ---: |
| SHIELD-R1-screen | pass | nominal.le.within | 5.474 |
| SHIELD-R2-neutron | fail | bounded.le.exceeds | -12.89 |
| SHIELD-R3-photon | fail | bounded.le.exceeds | -0.2798 |
| SHIELD-R4-mass | fail | bounded.le.exceeds | -68.9 |
| SHIELD-R5-thickness | pass | bounded.le.within | 29 |
| SHIELD-R6-activation | fail | nominal.le.exceeds | -3.279 |

