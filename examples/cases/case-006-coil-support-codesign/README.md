# CASE-006 — Field-preserving coil/support co-design

CASE-006 tried the most direct escape route identified by
[CASE-005](../case-005-passive-compliance-ceiling/): change the NCSX coil
geometry and the ideal passive support together, while preserving the target
surface field. The result is **stop this candidate family**.

The loop found a useful search technique, but not a viable 1.5× concept. A
normal-field near-nullspace search improved the fine-grid ceiling from
**1.1347× to 1.1573×**—about **2.0%**—while the required result remained
**1.5×**.

```text
117 low-order coil Fourier variables
                 ↓
nominal-field Jacobian → weakest SVD directions
                 ↓
candidate generation → fixed magnetic/mechanical gates
                 ↓
coarse winner → fine-grid failure → conservative rollback
                 ↓
Avila Core: 8 validity PASS, reduction FAIL → stop
```

## What was tried

The deterministic exploratory search used the three unique public SIMSOPT
NCSX modular coils, Cartesian Fourier modes 0–6, and stellarator symmetry. It
searched 117 coefficient offsets only through the 18 directions that least
changed the nominal surface-normal field. Four iterations evaluated 649
candidates with a fixed seed.

The comparison cannot move with the design:

- the denominator is the original NCSX uncoupled support;
- the field scale, currents, plasma boundary, stiffness trace, load basis,
  compliance bound, and condition-number bound remain fixed;
- the candidate receives the unrealistically favorable global full-matrix
  passive stiffness ceiling from CASE-005; and
- displacement, coil length, curvature, field strength, and normal-field
  error are bounded.

## Compiler feedback

The coarse-grid winner appeared admissible at **1.1975×**, but the fine grid
raised its normal-field error to **1.0353×** the original, beyond the fixed
1.02 limit. The loop responded by rolling the same coefficient vector back to
half amplitude. That repaired candidate passes every validity gate:

| Gate | Limit | Fine result | Verdict |
| --- | ---: | ---: | --- |
| reduction vs original control | ≥ 1.5× | 1.1573× | **FAIL** |
| normal-field error ratio | ≤ 1.02 | 1.0082 | PASS |
| surface field-strength ratio | 0.98–1.02 | 1.0001 | PASS |
| combined geometry utilization | ≤ 1 | 0.9455 | PASS |
| ideal worst compliance | ≤ 2 | 1.8470 | PASS |
| ideal stiffness condition | ≤ 10 | 3.0060 | PASS |
| reference/fine difference | ≤ 15% | 1.7602% | PASS |
| derivative-step difference | ≤ 2% | 0.0000288% | PASS |

The geometry utilization includes a 10.0 mm RMS displacement, 22.9 mm
maximum displacement, 0.544% maximum coil-length change, and 1.040× maximum
curvature ratio.

As a screen-grid diagnostic, extrapolating the raw direction far outside the
envelope did not approach the target: it peaked near **1.257×** at fourfold
amplitude, with invalid field and geometry, then worsened by sixfold
amplitude. This does not prove another geometry cannot work. It does show that
spending more time polishing this local mode-0–6 NCSX perturbation is not
justified by its trend.

## What Core contributed

Core made the failed iteration informative instead of ambiguous. It kept the
original control fixed, caught coarse-grid overfitting, exposed exactly which
gate failed, bound the repair and search history, and then showed that the
repaired design passes eight checks but misses the only performance threshold
by **0.3427×**. A language model or optimizer cannot silently redefine that
away.

## Reproduce it

Obtain SIMSOPT at the exact commit bound by CASE-005:

```bash
git clone https://github.com/hiddenSymmetries/simsopt.git /path/to/simsopt
git -C /path/to/simsopt checkout c648630cfc5625863b291709c17015bcdcba13af
```

Replay the committed receipt without executing the checker:

```bash
cargo run -p avila-core-cli -- run \
  examples/cases/case-006-coil-support-codesign \
  --source-root magnetic-compliance=examples/capabilities/magnetic-compliance \
  --source-root simsopt=/path/to/simsopt \
  --source-root case=examples/cases/case-006-coil-support-codesign
```

Add `--capability python3-numpy=/usr/bin/python3 --no-reuse` for a fresh run
(about 26 seconds in the reference environment). Run the independent checks
with:

```bash
python3 -m unittest \
  examples/capabilities/magnetic-compliance/test_magnetic_compliance.py \
  examples/capabilities/magnetic-compliance/test_ncsx_passive_ceiling.py \
  examples/capabilities/magnetic-compliance/test_ncsx_codesign_gate.py
```

## Scope

This is an exploratory local search, not a global geometry proof or an
independent confirmation. It omits free-boundary equilibrium, forces, stress,
clearance, winding packs, quench behavior, cost, safety, and construction.
The ideal full stiffness matrix is still a mathematical ceiling rather than a
real support. Nothing here establishes novelty, patentability, or practical
fusion performance, and the calculation is not endorsed by SIMSOPT
contributors or NCSX institutions.
