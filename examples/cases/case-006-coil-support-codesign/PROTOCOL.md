# CASE-006 protocol: NCSX coil/support co-design

**Status:** exploratory iteration run 2026-09-04.

**Registration note:** the thresholds were written into the temporary search
program and stated before candidate evaluation, but the protocol was not
committed before the interactive run. The 1.5 target and passive-support
limits were inherited from CASE-004/005. Treat this as a reproducible
exploration, not a blinded or independently held-out confirmation.

## Question

Can a bounded low-order perturbation of the public NCSX modular-coil geometry
reshape rigid-translation magnetic sensitivity enough that the globally ideal
equal-trace passive stiffness matrix reaches 1.5 times the original NCSX
uncoupled control without materially degrading the nominal target-surface
field?

## Search

- Parameterization: Cartesian constant and sine/cosine coefficients through
  Fourier mode 6 for each coordinate of three unique coils; 117 variables.
- Physical expansion: the original three field periods and stellarator
  symmetry; original currents are fixed.
- Screen: 64 segments per coil and a 24 by 16 surface.
- At each iteration, central differences form the nominal normal-field
  Jacobian. SVD supplies its 18 weakest right-singular directions.
- Central differences estimate the ideal-ceiling gradient in that subspace.
  One descent direction and 20 seeded perturbations are tested at six line
  steps. Seed: 20260904.
- Four iterations were allowed. The fourth found no feasible improvement;
  649 unique candidates, including the baseline, were scored.

The full search trace and raw coefficient vector are content-bound. The Core
checker reconstructs and verifies the recorded candidate but does not spend
four minutes rerunning candidate selection.

## Fixed comparison and gates

The original geometry supplies the uncoupled-control denominator and field
normalization at every resolution. The boundary, currents, symmetry, load
basis, stiffness trace, compliance limit, and condition limit cannot change.

The required reduction is at least 1.5. Other limits are: normal-field error
ratio at most 1.02; surface RMS field ratio in [0.98, 1.02]; RMS coil movement
at most 20 mm; point movement at most 50 mm; coil-length change at most 2%;
curvature ratio at most 1.10; worst compliance at most 2; stiffness condition
number at most 10; reference/fine difference at most 15%; and derivative-step
difference at most 2%.

## Feedback and decision rule

The coarse winner is evaluated at the CASE-005 fine resolution. Its fine-grid
normal-field failure triggers only a smaller amplitude of the same vector;
the comparator, target, and gates do not move. The conservative repair factor
is 0.5. Because that repair uses fine-grid feedback, the fine grid is not
claimed as a holdout.

Continue to a new equilibrium-aware confirmation only if every Core
requirement passes. Otherwise stop work on this declared candidate family.
Failure is not a theorem over all stellarator coils or over active, nonlinear,
or differentially wound mechanisms.
