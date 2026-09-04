# CASE-008 — Passive mode-selective quench response

CASE-008 found a mechanism worth one higher-fidelity study. In a reduced
three-circuit transient on the public NCSX coil geometry, adding equal passive
resistor branches shared between the CASE-007 winding circuits reduced the
worst magnetic-shape excursion by **1.614×** and the worst distributed-force
shape excursion by **1.685×** relative to the fixed independent-dump control.
All ten exploratory Core requirements pass.

This is not a quench-protection design. Inductance, resistance, time, voltage,
and current are normalized; the quench resistance is prescribed rather than
calculated from conductor heating or propagation.

## The mechanism

Three equal private dump resistors of scale `p` and three equal pair-shared
resistors of scale `b` give the loop resistance matrix

```text
R = p I + b Laplacian(K3).
```

Its common-current eigenmode sees resistance `p`; its two differential modes
see `p + 3b`. The repair sets `p = 1.45` and `b = 0.483333…`, so differential
current errors are damped at `2.9`, twice the common-mode rate, without changing
the private dump setting. The topology is passive in this mesh-circuit model.

The transient uses a regularized-Neumann three-loop inductance matrix derived
from the 18 coil filaments and integrates
`L dx/dt + (R_protection + R_quench(t)) x = 0`. Magnetic and force metrics
remove the common current mode, so the gate measures the asymmetric disturbance
rather than treating an orderly whole-magnet discharge as a shape error.

## The recorded Core iteration

The formal loop kept one manifest, compiled snapshot, model, comparator, and
ten thresholds fixed. Its two records are in
[`search/attempts.jsonl`](search/attempts.jsonl).

1. `independent-dump-r0` used `p = 1.45` with no shared branches. It met every
   burden, passivity, symmetry, and numerical gate, but reached only **1.232×**
   magnetic and **1.322×** force reduction, so Core reported two failures.
2. `mode-selective-dump-r1` named the first attempt as its parent. Core derived
   the only substantive candidate change at
   `/network/differential_to_common_mode_ratio`, from `1` to `2`. It then
   reported ten passes.

| Fine-grid gate | Required | Independent root | Mode-selective child |
| --- | ---: | ---: | ---: |
| magnetic-shape reduction | ≥ 1.5× | 1.2315× FAIL | **1.6139× PASS** |
| force-shape reduction | ≥ 1.5× | 1.3222× FAIL | **1.6850× PASS** |
| normalized initiating-circuit I²t / control | ≤ 1.05 | 0.9513 PASS | **1.0444 PASS** |
| peak modeled element voltage / control | ≤ 1.05 | 1.0416 PASS | **1.0416 PASS** |
| final energy fraction | ≤ 1e-6 | 7.1e-11 PASS | **2e-12 PASS** |
| reference/fine primary-metric change | ≤ 0.01 | 0.00093 PASS | **0.00311 PASS** |
| time-step change | ≤ 0.001 | 0.000003 PASS | **0.000010 PASS** |
| inductance condition number | ≤ 2 | 1.7683 PASS | **1.7683 PASS** |
| three-fault relative spread | ≤ 1e-9 | 0 PASS | **0 PASS** |
| passive network mapping | valid | valid | **valid** |

The I²t margin is narrow: the child is only 0.56 percentage point below its
cap and is worse than the independent root. Stronger differential damping
keeps the initiating current closer to the healthy circuits for longer. That
tradeoff is exactly why a conductor-resolved next gate could reject this
candidate even though the reduced mechanism passes.

## What this does not establish

The 20 mm filament regularization is not a winding-pack self-inductance model.
The I²t proxy is not a hot-spot temperature. The reported voltage is a
normalized resistive-element drop, not terminal-to-ground or turn-to-turn
voltage. Switches, diodes, power supplies, insulation, quench detection,
propagation, eddy currents, structures, stress, and plasma response are absent.

Standard quench analysis couples inductance, resistance, current decay, and
thermal properties; multi-coil systems also require mutual-inductance and
induced-voltage treatment. Those boundaries are described in the
[CERN quench simulation paper](https://cds.cern.ch/record/527184/files/rpph096.pdf)
and a [multi-coil superconducting-magnet study](https://www.osti.gov/servlets/purl/1807282).
Coupled-coil and pulse-based protection concepts also predate this case. The
result supplies no novelty or patentability conclusion.

## Reproduce it

Obtain SIMSOPT at the bound commit, then replay the committed receipt:

```bash
git clone https://github.com/hiddenSymmetries/simsopt.git /path/to/simsopt
git -C /path/to/simsopt checkout c648630cfc5625863b291709c17015bcdcba13af
```

```bash
cargo run -p avila-core-cli -- run \
  examples/cases/case-008-mode-selective-quench \
  --source-root magnetic-compliance=examples/capabilities/magnetic-compliance \
  --source-root simsopt=/path/to/simsopt \
  --source-root case=examples/cases/case-008-mode-selective-quench
```

For a fresh run, add
`--capability python3-numpy=/usr/bin/python3 --no-reuse`. To replay the native
lineage from an empty log, run the independent candidate with
`--attempt independent-dump-r0 --log attempts.jsonl`, then the mode-selective
candidate with `--attempt mode-selective-dump-r1 --parent-attempt
independent-dump-r0`; both runs also need the corresponding `--input
candidate=...` path.

Run the local checker tests with:

```bash
python3 -m unittest \
  examples/capabilities/magnetic-compliance/test_ncsx_circuit_quench_gate.py
```

The [protocol](PROTOCOL.md) fixes the reduced equations, comparison, candidate
box, gates, and stop/advance boundary.
