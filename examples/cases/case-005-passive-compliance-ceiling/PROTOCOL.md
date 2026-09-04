# CASE-005 protocol: full-matrix passive-support ceiling

**Status:** confirmatory follow-up to CASE-004, run 2026-09-04.

**Registration note:** this case was not separately committed before its first
exploratory calculation. Its 1.5 reduction target and compliance,
linearization, resolution, and derivative limits are inherited unchanged from
the CASE-004 protocol, which was frozen before either CASE-004 or CASE-005 was
run. The condition-number limit was already part of CASE-004's declared search
box. Treat this as a reproducible mathematical bound, not a blinded
experimental replication.

## Question

On the declared public SIMSOPT NCSX modular-coil and plasma-boundary data, can
any symmetric positive-definite passive linear stiffness matrix with the same
trace reach 1.5 times the direction-neutral magnetic-error performance of the
best declared uncoupled directional control?

## Relaxation and control

- Each of 18 physical modular coils has local radial, toroidal, and vertical
  rigid-translation degrees of freedom: 54 in total.
- The control is exhaustively selected from the CASE-004 directional stiffness
  allocation grid, with trace 54, compliance at most 2, and condition number at
  most 10.
- The relaxed candidate may be any 54 by 54 symmetric positive-definite
  stiffness matrix with trace 54. It is computed by the global closed-form
  optimum of the linear RMS objective, not by a heuristic search.
- The complete orthogonal generalized-load basis defines the direction-neutral
  RMS metric. No probability distribution or reactor load spectrum is claimed.

The full matrix is a strict mathematical superset of the realizable passive
linear topologies contemplated for an iterative campaign. Failure of its
global optimum therefore stops that campaign for this model.

## Numerics

- Reference: 128 line segments per coil and 24 by 16 surface samples.
- Fine: 256 line segments per coil and 48 by 32 surface samples.
- Sensitivity: compare 0.2 mm and 0.1 mm central differences on the fine grid.
- Nonlinear check: recompute the displaced filament geometry for every basis
  load at a 5 mm unit-support displacement scale on the reference grid.
- Source inputs are the exact SIMSOPT files and upstream commit named in the
  bound model and package.

## Decision rule

Begin a realizable passive topology campaign only if all six contract
requirements pass. If the linear ceiling fails, do not iterate within the
passive linear support space regardless of the other results. Any exploration
of coil/support co-design, active correction, nonlinear mechanisms, or a
different load ensemble requires a new question, evidence boundary, and gate.
