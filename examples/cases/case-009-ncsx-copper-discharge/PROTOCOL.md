# CASE-009 protocol: NCSX copper asynchronous discharge

Status: exploratory dimensional falsification protocol. Source selection,
model equations, limits, and the two reported ratios were settled during
interactive work on 2026-09-04 before formal lineage capture. The lineage is
not a statistical holdout or preregistration.

## Physical and revision boundary

- Model historical NCSX modular windings as cryoresistive copper, not as
  superconductors. The bound conceptual source does not establish the grade;
  use of NIST OFHC properties is an explicit modeling assumption.
- Take the dimensional circuit values only from the conceptual `m45r00` data:
  six coils/type, 36 turns/coil, centerline lengths 7.351/7.130/6.537 m,
  143.6015802 mm² copper area, 1.05843 helical-length factor, 85 K resistivity
  2.36e-9 ohm-m, initial circuit resistances 27.58/26.75/24.52 milliohm,
  line resistance 17.20 milliohm, line inductance 0.13 mH, and self-inductances
  38.28/28.48/31.38 mH.
- Do not mix later 20/20/18-turn production-coil parameters into that model.
- Use SIMSOPT NCSX coil filaments paired with a `c09r00`-named boundary only
  as a surrogate for field, force, centerline length, and inter-type mutual
  inductance. Report the revision mismatch and require the centerline-length
  difference to remain below 1%.

## Electrical and thermal model

For circuit-current vector `i`, use

```text
L di/dt = v_source
          - [diag(R_coil(T) + R_line)
             + S(t) (p I + b B^T B)] i
```

where `B` is the oriented incidence matrix for the three pair-shared branches,
`p = 0.05 ohm`, `b = p (r_mode - 1) / 3`, and `S(t)` changes from zero to one at
the detection event. The dump value, converter-collapse fault, delay set, and
global bypass are exploratory choices, not historical NCSX protection
settings. Verify both positive definiteness and the equality between `i^T R i`
and summed private/shared branch dissipation.

Set one converter voltage to zero at fault onset. Until detection, the two
healthy converters hold only their initial copper-plus-line loss-compensation
voltage. At detection, bypass every source and connect both private and shared
dump branches. Integrate to 3 s with event-aligned fixed-step RK4 at 1 ms, then
repeat the nominal-material cases at 0.5 ms.

For each type circuit, use one adiabatic temperature:

```text
m cp(T) dT/dt = i^2 R_coil(T)
R_coil(T) = G rho_Cu(T)
```

Use NIST OFHC-copper heat capacity and an assumed copper density of 8960 kg/m3.
Repeat the full sweep at 0.95, 1, and 1.05 times the heat capacity to propagate
the fit's stated five-percent error. Use the NIST intrinsic resistivity shape
scaled to the published 85 K value, and repeat at 0.85, 1, and 1.15 times that
resistance. The resistivity interval is a chosen model envelope, not a NIST
uncertainty statement.

Before candidate evaluation, reproduce the published deposited-energy/final-
temperature mapping to within 2%. This is an internal enthalpy-curve check,
not a physical temperature-accuracy claim. If it fails, return model invalid
rather than a candidate verdict.

## Initial state and sweep

Start the fault transient from the published 0.213 s pre-high-beta state
`i = [22685, 21392, 18008] A` and `T = [108, 110, 97] K`. Include the bound
published current prehistory in total I²t by treating current as piecewise
linear between the sparse samples and integrating its square exactly. Evaluate:

- failed converter M1, M2, and M3;
- detection delay 0, 25, 50, 100, and 120 ms;
- resistivity scale 0.85, 1, and 1.15;
- heat-capacity scale 0.95, 1, and 1.05;
- 256-segment/48×32 and confirmation 512-segment/96×64 field/coil meshes; and
- reference and half transient time steps.

The control uses `r_mode = 1`, so it has the same private resistors and no shared
branches. The checker evaluates exactly one supplied candidate and performs no
search.

## Fixed gates

1. Worst magnetic-shape reduction versus control is at least 1.5.
2. Worst inter-coil filament-force-shape reduction versus control is at least
   1.5. Singular self/hoop force is omitted.
3. Total prehistory-plus-fault I²t is at most 5.3e8 A²s in every circuit. This
   conceptual-design circuit rating is a provisional screen, not a demonstrated
   fault-survival threshold.
4. Lumped winding temperature is at most 125 K.
5. Absolute circuit current is at most 24 kA.
6. No circuit current reverses.
7. Modeled component voltage is at most the provisional 2 kV screen.
8. Final magnetic-energy fraction is at most 1e-4.
9. Relative electrical energy-balance residual is at most 1e-5.
10. Across all fault/delay cases at nominal material properties, halving the
    time step changes no gated physical aggregate or per-scenario field/force
    peak by more than 0.1%; the roundoff-scale energy-balance residual retains
    its separate absolute gate.
11. Across the same nominal-material cases, the 256-to-512 confirmation
    refinement changes those metrics by at most 1%.
12. Its off-diagonal mutual-inductance Frobenius change is at most 0.5%.
13. Confirmation total-inductance condition number is at most 3.
14. c09r00/m45r00 centerline-length mismatch is at most 1%, without treating
    that result as proof of revision equivalence.
15. Thermal energy/temperature validation error is at most 2%.
16. The explicit resistor incidence and power identity are valid.
17. All declared fault, delay, resistivity, and heat-capacity cases are covered.

The 2 kV value is a provisional screen from a later production-design datum
whose voltage reference is not specified. It is not asserted as an m45r00
terminal-to-ground or insulation requirement.

## Formal lineage and stop rule

The root uses `r_mode = 2`, carrying CASE-008's protection-resistance network
ratio into the dimensional model. If it misses either shape gate while
remaining numerically valid, test `r_mode = 4`, the upper candidate bound, as one child to determine
whether that obvious endpoint repair is responsive. Hold the manifest,
compiled snapshot, control, model, and limits fixed.

Stop further one-parameter tuning under this protocol if the child still misses
a shape gate, worsens either shape margin, or creates a new absolute/numerical
failure. This two-point endpoint test is not proof that every intermediate
ratio fails. A later attempt must alter the physical dynamics—for example
through an explicitly modeled self-acting transient-only branch—and must
predict which recorded limiting case it removes.

No outcome establishes local hotspot behavior, voltage to ground or between
turns, switching and arcing, support-shell eddy currents, self/hoop force,
stress, plasma
survival, hardware safety, novelty, patentability, or commercial feasibility.
The single 0.213 s fault onset also does not establish a worst-time-in-pulse
thermal or I²t protection envelope.
