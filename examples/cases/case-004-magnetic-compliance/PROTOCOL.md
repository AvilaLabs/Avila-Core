# CASE-004 protocol: reduced field-orthogonal support feasibility

**Status:** pre-registered 2026-09-04, before the reference model was run.

**Owner:** Connor Avila, case author.

**Purpose:** decide whether the magnetic-compliance mechanism merits a second
gate on a published stellarator geometry. It is not an experiment about whether
Avila Core improves optimization performance.

## Question

In the declared analytic eight-coil surrogate, can a passive, cyclic network of
positive ground and nearest-neighbor springs reduce magnetic error from rigid
coil translations relative to the best uncoupled directional support, while
using the same stiffness-trace budget and respecting declared compliance and
numerical-consistency limits?

## Model and comparison

The reference model is `examples/capabilities/magnetic-compliance/model.json`.
It fixes an analytic rotated family of eight non-planar filamentary coils, a
toroidal sampling surface, one million ampere coil current, local radial,
toroidal, and vertical rigid-translation degrees of freedom, a 5 mm
linearization check, and the complete support-search grid.

The magnetic sensitivity matrix is the central finite difference of normal
field on the sampling surface. The load ensemble is the complete orthogonal
basis of the 24 local coil-translation generalized-force directions. This is a
direction-neutral mathematical probe, not a claimed distribution of reactor
loads. The separately calculated mutual-coil Lorentz-force vector checks that
the selected support does not materially degrade the surrogate's nominal,
symmetry-preserving response.

Both arms are selected by exhaustive enumeration of the same directional
stiffness allocations:

| Arm | Allowed structure | Purpose |
| --- | --- | --- |
| Uncoupled control | independent local ground springs; radial, toroidal, and vertical stiffness may differ | strongest simple control, rather than an isotropic straw man |
| Coupled candidate | the same ground springs plus positive nearest-neighbor cyclic coupling in any direction | tests whether spatially selective compliance adds value |

Every support has a stiffness-matrix trace of 24. Search rejects matrices with
worst compliance above 2 times the unit isotropic support or condition number
above 10. The exact grids and every other numeric input are frozen in
`model.json`.

## Numerical checks

- Fine calculation: 128 segments per coil and a 24 by 16 surface grid.
- Coarse calculation: half each fine resolution.
- Sensitivity derivative: compare 0.2 mm and 0.1 mm central differences.
- Nonlinear check: physically translate every filament by the selected support
  response at a 5 mm unit-support displacement scale, recompute Biot--Savart
  fields, and compare against the linear sensitivity prediction.
- The implementation's polygonal-loop field is checked against the analytic
  on-axis field of a circular current loop.

## Frozen decision rule

The reduced mechanism **passes Gate 0** only if all of the following hold:

1. exact displaced-geometry RMS magnetic-error reduction is at least 1.5 times
   relative to the best uncoupled control;
2. exact magnetic error under the calculated nominal mutual-coil load is no
   more than 1.25 times the control;
3. worst compliance is no more than 2 times the unit isotropic support;
4. worst retained linear-versus-exact error is no more than 10%;
5. coarse-versus-fine reduction differs by no more than 15%;
6. finite-difference sensitivity changes by no more than 2%; and
7. at least 10% of the selected stiffness trace is expressed through neighbor
   coupling.

No threshold may be changed after the reference output is viewed. A corrected
implementation may be rerun only with the defect and both outputs recorded.

## Interpretation

- **All PASS:** the reduced-order mechanism exists strongly enough to justify a
  second gate using published coil geometry, beam/shell mechanics, and declared
  operational load cases.
- **Any FAIL:** do not begin topology optimization or a dedicated project. The
  failure identifies whether the obstacle is physical opportunity, load
  conflict, excessive compliance, or numerical instability.

Passing does not establish a practical stellarator support, lower mass or cost,
HTS safety, structural integrity, novelty, patentability, or performance on a
real equilibrium. It only rejects the cheapest version of the hypothesis that
local positive coupling cannot usefully reshape magnetic compliance.
