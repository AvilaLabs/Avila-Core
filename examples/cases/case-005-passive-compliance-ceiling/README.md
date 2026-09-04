# CASE-005 — Passive magnetic-compliance ceiling

CASE-005 decides whether it is scientifically defensible to start an iterative
support-topology campaign after [CASE-004](../case-004-magnetic-compliance/)
failed. It replaces the analytic eight-coil surrogate with the public SIMSOPT
NCSX modular-coil configuration and gives the support an intentionally
unrealistic advantage: it may be **any** symmetric positive-definite 54 by 54
stiffness matrix, including arbitrary nonlocal and cross-direction coupling.

The answer is **no: do not start the passive linear topology loop**. The
fine-grid mathematical ceiling is **1.1347×** against an inherited target of
**1.5×**.

```text
SIMSOPT NCSX coils + plasma boundary
                    ↓
  54-DOF rigid-translation sensitivity S
                    ↓
       H = SᵀS and closed-form global optimum
                    ↓
 best uncoupled control vs every full SPD stiffness matrix
                    ↓
        Avila Core: 5 validity PASS, ceiling FAIL → stop
```

## Why this is a ceiling

For a symmetric positive-definite stiffness matrix `K`, a unit generalized
load produces displacement `K⁻¹f`. Over the complete orthogonal load basis,
the squared RMS incremental normal-field error is

```text
J²(K) = tr(H K⁻²) / n,     H = SᵀS,     tr(K) = n.
```

For fixed stiffness eigenvalues, the trace inequality minimizes this objective
when `K` and `H` share eigenvectors, pairing the greatest stiffness with the
most sensitive mode. The remaining scalar optimization is convex; its
stationary condition gives

```text
kᵢ ∝ hᵢ^(1/3).
```

The checker constructs that global optimum directly. A realizable passive
linear topology is a subset of the full-matrix space and therefore cannot
exceed its reduction factor under the same discretized model. The control is
the best admissible member of the same uncoupled directional grid used in
CASE-004; allowing a continuously optimized control would only lower the
reported ceiling further.

## Result

| Gate | Inherited limit | Result | Verdict |
| --- | ---: | ---: | --- |
| fine-grid linear ceiling | ≥ 1.5× | 1.1347× | FAIL |
| ideal worst compliance | ≤ 2× isotropic | 1.8465× | PASS |
| ideal stiffness condition number | ≤ 10 | 2.9809 | PASS |
| reference-grid linearization error | ≤ 10% | 1.3256% | PASS |
| reference/fine ceiling difference | ≤ 15% | 1.6011% | PASS |
| derivative-step difference | ≤ 2% | 0.0000300% | PASS |

At the reference resolution, exact displaced-geometry Biot–Savart evaluation
realizes 1.1517×, close to its 1.1528× linear prediction. The fine sensitivity
spectrum spans only 5.15× and has no mode below one tenth of its largest
singular value. There is useful anisotropy, but not remotely enough to support
a 50% direction-neutral improvement at equal stiffness trace.

## What the feedback means

An optimizer, language model, or human designer cannot produce a passing
passive linear support by searching this formulation more creatively. To
continue, at least one scientific assumption—not a threshold—must change:

1. **Change the magnetic sensitivity itself.** Jointly design coil geometry
   and structural compliance while preserving plasma-boundary field quality.
   This is the route most aligned with the original passive idea, but it needs
   a real SIMSOPT/VMEC objective and independent holdout equilibria.
2. **Change the mechanism class.** Active correction or nonlinear/preloaded
   structures escape the linear stiffness ceiling, but require new energy,
   stability, failure, force, and safety gates to prevent a trivial solution.
3. **Change the load question.** Optimizing for a validated operational load
   ensemble rather than every unit direction may expose more opportunity, but
   the present files do not supply reactor structural loads.

Each is a new contract and hypothesis. Lowering the 1.5× target, changing the
load basis after seeing the result, or letting a candidate alter the checker
would turn the loop into gate gaming rather than discovery.

[CASE-006](../case-006-coil-support-codesign/) subsequently tested a bounded
version of the first route. Its field-preserving Fourier search produced only
a 2.0% fine-grid improvement over this fixed-geometry ceiling and still
failed the inherited target, so it does not justify a larger campaign.

## Reproduce it

The two SIMSOPT inputs are bound at commit
`c648630cfc5625863b291709c17015bcdcba13af` but are not copied into this
repository. Obtain that exact public tree:

```bash
git clone https://github.com/hiddenSymmetries/simsopt.git /path/to/simsopt
git -C /path/to/simsopt checkout c648630cfc5625863b291709c17015bcdcba13af
```

Verify all artifacts and replay the committed receipt without executing the
checker:

```bash
cargo run -p avila-core-cli -- run \
  examples/cases/case-005-passive-compliance-ceiling \
  --source-root magnetic-compliance=examples/capabilities/magnetic-compliance \
  --source-root simsopt=/path/to/simsopt \
  --source-root case=examples/cases/case-005-passive-compliance-ceiling
```

Force a fresh calculation (about 18 seconds in the reference environment):

```bash
  … --capability python3-numpy=/usr/bin/python3 --no-reuse
```

The bound capability is CPython 3.14.4 with NumPy 2.3.5. Independent unit
checks for the field kernel and the ceiling construction run with:

```bash
python3 -m unittest \
  examples/capabilities/magnetic-compliance/test_magnetic_compliance.py \
  examples/capabilities/magnetic-compliance/test_ncsx_passive_ceiling.py
```

## Provenance and scope

The coil coefficients come from SIMSOPT's `src/simsopt/configs/NCSX.dat`; the
plasma-boundary coefficients come from
`tests/test_files/input.NCSX_c09r00_halfTeslaTF`. The package binds both files,
the upstream commit declaration, checker, field kernel, model, interpreter,
result, claims, receipt, and campaign report. See the repository's
[third-party notices](../../../THIRD_PARTY_NOTICES.md).

SIMSOPT identifies this configuration as NCSX modular coils without the
circular coils. This calculation is an independent unqualified probe: it is
not endorsed by SIMSOPT contributors, PPPL, or NCSX institutions, and it is
not structural validation, an equilibrium calculation, or evidence of
practicality, novelty, patentability, or safety.
