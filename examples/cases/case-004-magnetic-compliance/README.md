# CASE-004 — Magnetic-compliance feasibility gate

CASE-004 asks a deliberately cheap question before any stellarator-support
project begins: can positive nearest-neighbor spring coupling redirect rigid
coil motion into magnetically less-sensitive directions, at the same total
stiffness as the best uncoupled directional support?

The answer for this analytic eight-coil surrogate is **no: stop at Gate 0**.
The [protocol](PROTOCOL.md), model, comparison, and thresholds were frozen
before the reference output was examined.

```text
analytic coils + toroidal field surface
                  ↓
       Biot–Savart sensitivity
                  ↓
 equal-budget support search ── best uncoupled vs nearest-neighbor coupled
                  ↓
 exact displaced geometry + load and numerical checks
                  ↓
        Avila Core: 4 PASS, 3 FAIL → stop
```

## Result

| Gate | Frozen limit | Result | Verdict |
| --- | ---: | ---: | --- |
| exact magnetic-error reduction | ≥ 1.5× | 0.9928× | FAIL |
| nominal-load error ratio | ≤ 1.25× control | 1.3333× | FAIL |
| worst compliance | ≤ 2× isotropic | 1.3333× | PASS |
| linearization error | ≤ 10% | 0.0144% | PASS |
| coarse/fine difference | ≤ 15% | 0.0202% | PASS |
| derivative-step difference | ≤ 2% | 0.0000088% | PASS |
| coupled stiffness share | ≥ 10% | 8.3333% | FAIL |

The sensitivity spectrum also has no singular direction below one tenth of
its largest singular value; its largest-to-smallest contrast is only 1.86.
The best admissible coupled support therefore found no useful low-sensitivity
subspace to exploit. It made the direction-neutral error slightly worse and
the calculated symmetric-load error one third worse than the control. The
resolution, derivative, and nonlinear checks are much tighter than their
limits, so this is not plausibly a discretization or linearization failure.

This result rejects only this reduced mechanism: rigid translations, an
analytic coil family, positive local ground springs, positive nearest-neighbor
coupling, and stiffness trace as the resource proxy. It says nothing decisive
about a real stellarator equilibrium, coil bending, a surrounding structure,
nonlocal or active mechanisms, mass, stress, fatigue, access, safety, novelty,
or patentability.

[CASE-005](../case-005-passive-compliance-ceiling/) subsequently repeated the
question on the public SIMSOPT NCSX modular-coil geometry and relaxed the
support to *any* full positive-definite stiffness matrix. Its global ceiling is
still only 1.1347× against the 1.5× target, so a passive linear topology loop is
not justified.

## Reproduce it

Verify the complete package and replay its committed receipt without running
the checker:

```bash
cargo run -p avila-core-cli -- run \
  examples/cases/case-004-magnetic-compliance \
  --source-root magnetic-compliance=examples/capabilities/magnetic-compliance \
  --source-root case=examples/cases/case-004-magnetic-compliance
```

Force a fresh calculation (about five seconds in the reference environment):

```bash
cargo run -p avila-core-cli -- run \
  examples/cases/case-004-magnetic-compliance \
  --source-root magnetic-compliance=examples/capabilities/magnetic-compliance \
  --source-root case=examples/cases/case-004-magnetic-compliance \
  --capability python3-numpy=/usr/bin/python3 \
  --no-reuse
```

The bound capability is CPython 3.14.4 with NumPy 2.3.5; Core rejects a
different interpreter digest. Run the checker's independent numerical tests
with:

```bash
python3 -m unittest \
  examples/capabilities/magnetic-compliance/test_magnetic_compliance.py
```

## What Core contributed

Core did not supply the physics. It made the inexpensive test decision-grade
for its limited purpose: the question and seven thresholds compile into a
fixed contract; exact inputs, checker, output, and interpreter are bound by
digest; every extracted metric is tied to the result artifact; and the fresh
run reproduces the committed output, claims, receipt, and campaign report.

During bootstrap, Core refused the first package because the output claims had
not yet been bound to an artifact identity. That feedback exposed a provenance
gap before the scientific run could be treated as evidence. No Core source
change was needed to add this domain.
