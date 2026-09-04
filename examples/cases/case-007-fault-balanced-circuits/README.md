# CASE-007 — Fault-balanced winding circuits

CASE-007 found a small but real candidate worth one more feasibility gate. An
exhaustive search assigned the 18 public NCSX coils to three interleaved
winding circuits. On the fine mesh, losing any candidate circuit reduced the
worst magnetic-shape departure by **1.364×** and the worst distributed-force
departure by **1.123×** relative to an equal-count, type-segregated control.
All seven Core requirements pass.

This is not yet a quench-protection design. It is evidence that circuit
partition is a design variable worth testing with real circuit dynamics.

## The iteration

Physical index `3c + t` identifies symmetry copy `c` and base-coil type `t`.
The final three circuits are:

```text
A:  0  4  7 11 14 15
B:  1  3  8 10 12 17
C:  2  5  6  9 13 16
```

Each contains two coils of every type, carries exactly one third of the total
absolute nominal ampere-turns, and is a symmetry transform of the other two.

The first exhaustive proposal optimized magnetic departure alone. It improved
that proxy by **1.437×**, but improved force departure by only **1.050×** and
therefore missed the fixed 1.10 force gate. That feedback changed only the
search objective: a second pass minimized the worse normalized magnetic or
force departure over the same 121,500 architectures. It produced the final
partition without changing the comparator, fault, constraints, or thresholds.

| Fine-grid gate | Required | Result |
| --- | ---: | ---: |
| magnetic-departure reduction | ≥ 1.25× | **1.3638×** |
| force-departure reduction | ≥ 1.10× | **1.1226×** |
| sampled peak-force ratio | ≤ 1 | **1.0000** |
| path overshoot / endpoint | ≤ 1.01 | **1.0000** |
| reference/fine change | ≤ 0.02 | **0.00114** |
| fault-group relative spread | ≤ 1e-9 | **0** |
| exact balanced partition | valid | **valid** |

The three candidate fault cases are numerically identical by symmetry. The
result is also stable from the reference to fine discretization.

## What it does not show

The complete-loss candidate still has a normalized surface-normal field of
**0.0601**, versus **0.0139** nominal. It reduces the control's fault damage; it
does not preserve a confinement-quality field. The force calculation is a
filament force-density proxy, not support stress.

The model omits mutual inductance, resistance, dump voltage, detection and
switch delay, quench propagation, hot spots, eddy currents, integer turns,
winding packs, structures, and plasma response. The thresholds were chosen
during reference-grid exploration, so the result is not a preregistered
confirmation.

Balanced quench circuits and arranging superconducting coils to reduce
unbalanced force have substantial prior art, including
[US6717781B2](https://patents.google.com/patent/US6717781) and
[US20130106545A1](https://patents.google.com/patent/US20130106545A1/en).
Meanwhile, the Stellaris reactor study explicitly notes that partial-coil
quench would create highly asymmetric loads but assumes near-simultaneous
quench in its present analysis
([Fusion Engineering and Design, 2025](https://doi.org/10.1016/j.fusengdes.2025.114868)).
That combination leaves room for a useful open geometry-aware optimization
method, but this screen supplies no patentability conclusion.

## Reproduce it

Obtain SIMSOPT at the bound commit:

```bash
git clone https://github.com/hiddenSymmetries/simsopt.git /path/to/simsopt
git -C /path/to/simsopt checkout c648630cfc5625863b291709c17015bcdcba13af
```

Replay the committed receipt:

```bash
cargo run -p avila-core-cli -- run \
  examples/cases/case-007-fault-balanced-circuits \
  --source-root magnetic-compliance=examples/capabilities/magnetic-compliance \
  --source-root simsopt=/path/to/simsopt \
  --source-root case=examples/cases/case-007-fault-balanced-circuits
```

Add `--capability python3-numpy=/usr/bin/python3 --no-reuse` for a fresh run.
Run the local checker tests with:

```bash
python3 -m unittest \
  examples/capabilities/magnetic-compliance/test_magnetic_compliance.py \
  examples/capabilities/magnetic-compliance/test_ncsx_passive_ceiling.py \
  examples/capabilities/magnetic-compliance/test_ncsx_codesign_gate.py \
  examples/capabilities/magnetic-compliance/test_ncsx_fault_partition_gate.py
```

The [protocol](PROTOCOL.md) fixes the exact model, metrics, feedback rule, and
stop/advance boundary.
