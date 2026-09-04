# CASE-009 — NCSX copper asynchronous-discharge gate

CASE-009 stops the delayed resistor-mesh mechanism from CASE-008 in its
present form. On a dimensional cryoresistive-copper model, the ratio-two mesh
made the worst magnetic and inter-coil filament-force departures larger, not
smaller: its reduction factors were **0.771×** and **0.714×** against the
equal private-dump control.
Both required at least 1.5×.

This case also corrects the physical framing of CASE-008. Historical NCSX
modular coils were flexible copper windings precooled with liquid nitrogen,
not superconducting coils; the bound conceptual source does not establish the
copper grade. CASE-009 therefore models converter loss and controlled
discharge, not quench propagation.

## What was tested

The dimensional resistance, self-inductance, conductor, current, and thermal
values come from one coherent `m45r00` conceptual-design table. The three
historical coil-type circuits M1/M2/M3 are retained. SIMSOPT NCSX filaments
paired with a `c09r00`-named boundary supply only field, force,
centerline-length, and mutual-inductance surrogates; the checker explicitly
reports that these revisions are not proven equivalent.

At fault onset, one converter loses voltage while the other two hold their
initial loss compensation. At the declared detection time, all sources are
bypassed and the private and pair-shared resistors connect together. The sweep
covers all three failed converters, delays of 0/25/50/100/120 ms, copper
resistivity scales of 0.85/1/1.15, and heat-capacity scales of 0.95/1/1.05.

The comparator has three chosen 0.05 ohm private dump resistors. The root adds shared
branches for protection-resistance network ratio 2. The child tests the maximum
allowed ratio 4 without changing the model, comparator, thresholds, manifest,
or compiled contract.

## Results

| Worst-case gate | Required | Ratio-2 root | Ratio-4 child |
| --- | ---: | ---: | ---: |
| magnetic-shape reduction | ≥ 1.5× | **0.7705× FAIL** | **0.5434× FAIL** |
| inter-coil force-shape reduction | ≥ 1.5× | **0.7142× FAIL** | **0.5058× FAIL** |
| selected-onset prehistory + fault I²t | ≤ 530 MA²s | 414.65 PASS | 407.19 PASS |
| lumped winding temperature | ≤ 125 K | 124.89 PASS | 124.26 PASS |
| absolute circuit current | ≤ 24 kA | 23.682 PASS | 23.682 PASS |
| provisional modeled component voltage | ≤ 2 kV | 1.798 PASS | **2.228 FAIL** |
| shape-metric resolution change | ≤ 1% | 0.694% PASS | 0.694% PASS |
| off-diagonal mutual-inductance change | ≤ 0.5% | 0.033% PASS | 0.033% PASS |
| thermal validation energy error | ≤ 2% | 1.400% PASS | 1.400% PASS |

All other current-reversal, residual-energy, energy-balance, inductance,
geometry-length, passivity, and sweep-coverage gates pass for the root. The
thermal baseline internally reproduces the three published energy/temperature
pairs to within 1.400% in energy and 0.291 K in temperature. The checker also
propagates the NIST heat-capacity fit's ±5% error and a chosen ±15% resistivity
envelope through 135 fault/delay/material comparisons.

The limiting root cases occur after the network is active. Core records the
primary cause as `post-activation-shape-redistribution`: on these unequal
historical type circuits, the shared mesh drives a current trajectory that
increases the declared field or inter-coil force departure. Raising the ratio
from 2 to 4
improves I²t and temperature slightly, but worsens both shape margins and adds
a voltage failure. It does not prove every intermediate ratio fails; it does
show that the predeclared upper-bound endpoint repair is anti-responsive, so
further one-parameter tuning stops under this protocol.

## What Core contributed

Core compiled 17 fixed requirements, bound every executable input—code,
SIMSOPT data, model, candidate, and result identities—and reproduced the
reference result from a verified receipt. The PPPL and NIST publications are
cited but are not themselves content-bound artifacts. Core also recorded the
two candidate attempts under one manifest and compiled
snapshot. For the child it derived the two-field candidate diff, verified the
exact parent log row, reported the voltage PASS→FAIL transition, and calculated
15 exact parent-to-child margin deltas. The record is
[`search/attempts.jsonl`](search/attempts.jsonl).

That is useful negative evidence: the dimensional model did not merely return
“fail”; it separated the selected-onset conditional thermal/electrical results
from the failed field/inter-coil-force mechanism and showed that the obvious
one-parameter endpoint repair moved the critical margins in the wrong
direction.

## Reproduce it

Obtain SIMSOPT at the bound commit:

```bash
git clone https://github.com/hiddenSymmetries/simsopt.git /path/to/simsopt
git -C /path/to/simsopt checkout c648630cfc5625863b291709c17015bcdcba13af
```

Replay the committed reference receipt:

```bash
cargo run -p avila-core-cli -- run \
  examples/cases/case-009-ncsx-copper-discharge \
  --source-root magnetic-compliance=examples/capabilities/magnetic-compliance \
  --source-root simsopt=/path/to/simsopt \
  --source-root case=examples/cases/case-009-ncsx-copper-discharge
```

Add `--capability python3-numpy=/usr/bin/python3 --no-reuse` for a fresh
execution. The [protocol](PROTOCOL.md) fixes the model and stop boundary;
[source notes](SOURCES.md) separate the two NCSX revisions and summarize the
prior-art boundary.

The force surrogate omits singular filament self/hoop force. This is not a
worst-time-in-pulse protection envelope, local-hotspot, grounding, insulation,
switch, structural, plasma, safety, manufacturability, novelty, patentability,
or commercial analysis.
