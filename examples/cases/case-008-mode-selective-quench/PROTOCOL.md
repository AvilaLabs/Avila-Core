# CASE-008 protocol: passive mode-selective quench response

Status: exploratory feasibility protocol. The model, limits, and the two
reported settings were selected during interactive exploration on 2026-09-04.
They were frozen before the formal two-attempt Core lineage, but the lineage is
not a statistical holdout or preregistration.

Historical correction: NCSX modular coils were cryoresistive copper. The
prescribed “quench resistance” below is a hypothetical normalized disturbance
on NCSX geometry, not a model of the NCSX conductor. CASE-009 carries the
mechanism into a dimensional copper fault/discharge model.

## Question

Can a passive three-loop dump network damp differential circuit-current modes
strongly enough to reduce both magnetic-shape and inter-coil
filament-force-shape excursions by at
least 1.5 times relative to an independent-dump control, while remaining
within five percent of that control's normalized I²t and peak modeled
resistive-element voltage?

## Fixed winding and inductance model

- Use the 18 public SIMSOPT NCSX modular-coil filaments, currents, symmetry,
  reference mesh, and fine mesh bound by the package.
- Use the CASE-007 circuit groups `(0,4,7,11,14,15)`,
  `(1,3,8,10,12,17)`, and `(2,5,6,9,13,16)`.
- Scale each filament by its signed nominal ampere-turn ratio under a common
  circuit current.
- Calculate every single-filament mutual term with the midpoint Neumann
  integral, replacing distance `r` with `sqrt(r² + a²)` at fixed `a = 0.02 m`.
  Sum these terms into a three-circuit matrix and divide by mean diagonal.
- Refuse a matrix that is not numerically positive definite.

This softened-filament matrix preserves geometry-dependent mutual coupling for
the screen. It is not a dimensional self-inductance or winding-pack model.

## Fixed transient

Start all normalized loop currents at one and remove the power-supply voltage.
For each initiating circuit in turn, add only to that loop a quench resistance
that rises linearly from zero to three by normalized time `0.4`. Activate the
protection network at time `0.12`. Integrate

```text
L dx/dt + (R_protection + R_quench(t)) x = 0
```

to time `10` with event-split fixed-step RK4. Use step `0.0025` for reference
and fine results and repeat the fine result at `0.00125`.

The control has three independent private unit resistors. A candidate supplies
`p`, the equal private resistance, and `rho`, the differential/common modal
resistance ratio. Three equal pair-shared resistors have
`b = p(rho - 1)/3`, making

```text
R_protection = p I + b Laplacian(K3).
```

Accept only `0.25 <= p <= 2` and `1 <= rho <= 4`, so all modeled resistances
are nonnegative. The checker evaluates exactly one supplied candidate and does
not search.

## Fixed metrics

- Magnetic departure is the RMS surface-normal field left after subtracting
  the instantaneous best common-current scaling.
- Inter-coil filament-force departure is the length-weighted RMS force change
  left after subtracting the common-current-squared nominal force pattern;
  singular filament self/hoop force is omitted.
- I²t is the time integral of the initiating loop's normalized current squared.
- Element voltage is the greatest normalized resistive drop across the quench,
  a private resistor, or a pair-shared resistor.
- Residual energy is `xᵀ L x` at the end divided by its initial value.
- Every primary value is the worst of the three possible initiating circuits.

## Frozen gates

1. Fine magnetic-departure reduction versus control is at least `1.5`.
2. Fine inter-coil filament-force-departure reduction versus control is at
   least `1.5`.
3. Fine initiating-circuit I²t ratio versus control is at most `1.05`.
4. Fine peak modeled element-voltage ratio versus control is at most `1.05`.
5. Fine final energy fraction is at most `0.000001`.
6. The largest primary-metric change from reference to fine geometry is at
   most `0.01`.
7. The largest primary-metric change on halving the fine time step is at most
   `0.001`.
8. Fine inductance condition number is at most `2`.
9. Relative spread across symmetry-related initiating circuits is at most
   `0.000000001`.
10. The candidate maps to the declared passive resistor network.

## Formal feedback rule

The root uses `p = 1.45`, `rho = 1`. If the two shape gates fail while the
burden and numerical gates pass, hold `p` fixed and test `rho = 2` as a child.
Do not change the contract, model, control, or limits. Core must bind both
attempts to the same manifest and compiled snapshot and derive their candidate
diff. Stop after the child.

## Advance boundary

An all-pass child justifies one dimensional higher-fidelity study with a
winding-pack inductance model, a stated conductor and operating current,
temperature-dependent resistance and heat capacity, quench propagation,
detection and switching, terminal/ground/turn voltage, and structural and
plasma-response checks. Anything less than all-pass stops this mechanism in
the present box. Neither outcome is hardware, safety, novelty, patentability,
or commercial evidence.
