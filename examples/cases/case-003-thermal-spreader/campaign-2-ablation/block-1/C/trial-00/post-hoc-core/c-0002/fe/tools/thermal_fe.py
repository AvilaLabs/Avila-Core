#!/usr/bin/env python3
"""Two-dimensional steady conduction in a layered heat spreader, by
finite elements (scikit-fem, P2 triangles).

Geometry: a vertical cross-section, x along the plate (0 at the strip's
symmetry center, half_width at the plate's cut edge) and y through the
thickness (0 at the heated face, total_thickness at the cooled face). Layers
are stacked in y, from the heated face outward, each spanning the full x
extent; conductivity is piecewise-constant in y. Boundary conditions:

  * y = 0 (heated face), 0 <= x <= strip_half_width: Neumann, a prescribed
    heat flux entering the domain (the candidate's heat_flux_W_m2).
  * y = 0, strip_half_width < x <= half_width: insulated (natural, no term).
  * x = 0 (the strip's symmetry plane) and x = half_width (the plate's cut
    edge): insulated (natural, no term).
  * y = total_thickness (cooled face): Robin, convection to ambient.

Solving the half-width by symmetry and leaving both x edges natural
reproduces, respectively, the mirror condition at the strip's center and the
plate's own insulated edge -- both are zero-flux and both are the same
"do nothing" boundary in the weak form.

The two solver primitives below -- `build_tensor_mesh` (a layered or plain
tensor mesh from explicit boundary coordinates, refined uniformly) and
`solve_conduction` (assembly and solve for an arbitrary conductivity field,
Neumann sources, Robin faces, and an optional Dirichlet condition) -- carry
no knowledge of this problem's specific geometry or boundary layout. They
are the same functions `validation/nafems_t4.py` imports and composes with
NAFEMS T4's own (different) geometry and boundary conditions, so the two
solves share one physics code path and nothing is duplicated between them.

Discretisation bracket: each candidate is solved at two mesh levels, h and
h/2 (one uniform refinement apart). The reported hotspot is
[min, max] of {the fine value, the Richardson extrapolation of the two},
each widened outward by |fine - coarse|. The extrapolation assumes
second-order convergence (p = 2); this is a stated, unverified assumption
(two levels cannot estimate their own order), but the outward widening by
the full fine-coarse gap dominates the final width in practice, so the
bracket is not sensitive to that assumption -- see `limitations` in the
output document. This is a discretisation bracket, not a proof: it says
nothing about modeling error (the conductivity values, the convection
coefficient, or the 2-D idealisation itself).

Determinism: no randomness anywhere, and the direct sparse solve scipy uses
is deterministic for a fixed matrix and right-hand side, so two runs on the
same machine and library versions produce bit-identical results. Every
reported number is nonetheless rounded to 8 significant digits (the same
convention `shield-coupled/transport.py`'s `stable()` uses for tally
statistics), which additionally absorbs the kind of last-bit float
nondeterminism a different BLAS backend or thread count could otherwise
introduce, so the same candidate produces the same output bytes across
machines, not just across repeated runs on one machine.
"""

import argparse
import json
import sys
from decimal import Decimal, getcontext

import numpy as np
import skfem
from skfem import Basis, BilinearForm, ElementTriP2, FacetBasis, LinearForm, MeshTri, asm, condense, solve
from skfem.helpers import dot, grad

getcontext().prec = 40
SIGNIFICANT_DIGITS = 8
RICHARDSON_ORDER = 2

SCHEMA = "avila.thermal/fe-result/v1"

# Mesh resolution: elements per layer-thickness segment and per x segment in
# the coarse ("h") tensor mesh, before uniform refinement. Fixed constants,
# not fitted or searched -- the same candidate always produces the same
# mesh. See the module docstring and this script's own unit tests for the
# accuracy this buys: already within a few thousandths of a kelvin of the
# next refinement level at these settings, for the candidates this case
# considers, in well under a second per solve.
BASE_ELEMENTS_PER_SEGMENT = 2
COARSE_REFINEMENTS = 1
FINE_REFINEMENTS = 2


def canonical(value: Decimal) -> str:
    text = format(value.normalize(), "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    if text in ("", "-0"):
        text = "0"
    return text


def round_significant(value: float, digits: int = SIGNIFICANT_DIGITS) -> Decimal:
    """Round a raw solver float to `digits` significant digits. Mirrors
    `shield-coupled/transport.py`'s `stable()`: same quantum-based rounding,
    applied to a finite-element nodal value instead of a Monte Carlo tally
    statistic."""
    decimal = Decimal(repr(value))
    if decimal == 0:
        return Decimal(0)
    quantum = Decimal(1).scaleb(decimal.adjusted() - digits + 1)
    return decimal.quantize(quantum)


def round_decimal(value: Decimal, digits: int = SIGNIFICANT_DIGITS) -> Decimal:
    """Round an already-exact Decimal (not a raw solver float) to `digits`
    significant digits, with the same quantum logic as `round_significant`.
    Mirrors `transport.py`'s `round_decimal`: used for a quantity derived
    from already-rounded values by exact Decimal arithmetic (the Richardson
    correction divides by 3, which reintroduces far more digits than the
    inputs carried), so the *reported* bracket stays at a bounded, stable
    number of digits without a second helping of float noise."""
    if value == 0:
        return Decimal(0)
    quantum = Decimal(1).scaleb(value.adjusted() - digits + 1)
    return value.quantize(quantum)


def subdivided(boundaries, per_segment):
    """A sorted 1-D node array covering [boundaries[0], boundaries[-1]],
    with every element of the strictly increasing `boundaries` sequence
    preserved exactly as a node, and `per_segment` equal-width elements
    across each consecutive pair. Used for both mesh axes so that material
    interfaces (y) and the strip edge (x) always fall exactly on element
    boundaries -- no element ever straddles two materials or a boundary
    condition change."""
    nodes = [float(boundaries[0])]
    for lo, hi in zip(boundaries[:-1], boundaries[1:]):
        segment = np.linspace(float(lo), float(hi), per_segment + 1)[1:]
        nodes.extend(segment.tolist())
    return np.array(nodes)


def build_tensor_mesh(x_boundaries, y_boundaries, per_segment, refine):
    """A `MeshTri` tensor mesh whose nodes include every coordinate in
    `x_boundaries` and `y_boundaries` exactly (via `subdivided`), uniformly
    refined `refine` times. Uniform (red) refinement bisects every triangle
    edge at its midpoint; a horizontal or vertical edge's midpoint stays on
    the same horizontal or vertical line, so every material interface or
    boundary-condition line present in the coarse mesh is exactly preserved,
    at every finer level, with no interpolation smearing across it. This
    function knows nothing about what the boundaries mean (material
    interfaces, a strip edge, a plate edge): it is shared, geometry-agnostic
    machinery, reused by `validation/nafems_t4.py` for a single-region mesh
    with no interior boundaries at all (call it with only the two endpoints
    on each axis)."""
    x_nodes = subdivided(x_boundaries, per_segment)
    y_nodes = subdivided(y_boundaries, per_segment)
    return MeshTri.init_tensor(x_nodes, y_nodes).refined(refine)


def solve_conduction(mesh, conductivity, neumann=(), robin=(), dirichlet=None):
    """Assemble and solve steady 2-D conduction with P2 triangles.

    conductivity: a callable `k(y_array) -> k_array` for a field that varies
        with the global y-coordinate (piecewise-constant layers use this),
        or a plain scalar for a uniform material.
    neumann: an iterable of (facets, flux) pairs. `flux` is the heat flux
        entering the domain across `facets` (a prescribed source term).
    robin: an iterable of (facets, h, ambient) pairs: convection to
        `ambient` at coefficient `h` across `facets`.
    dirichlet: an optional (predicate, value) pair fixing the degrees of
        freedom `basis.get_dofs(predicate)` selects to `value` before
        condensing and solving. A predicate, not precomputed dofs, because
        the `Basis` those dofs are drawn from is built inside this
        function -- the caller never sees it otherwise.

    Returns (basis, T): the P2 `Basis` and the nodal temperature vector.
    This function has no knowledge of what problem it is solving -- the
    spreader script below and `validation/nafems_t4.py` both call it with
    their own, different geometry and boundary conditions; the assembly and
    solve are the one physics code path shared between them.
    """
    basis = Basis(mesh, ElementTriP2())

    @BilinearForm
    def conduction(u, v, w):
        k = conductivity(w.x[1]) if callable(conductivity) else conductivity
        return k * dot(grad(u), grad(v))

    A = asm(conduction, basis)
    b = basis.zeros()

    for facets, h, ambient in robin:
        fb = FacetBasis(mesh, ElementTriP2(), facets=facets)

        @BilinearForm
        def robin_bilinear(u, v, w, h=h):
            return h * u * v

        @LinearForm
        def robin_linear(v, w, h=h, ambient=ambient):
            return h * ambient * v

        A = A + asm(robin_bilinear, fb)
        b = b + asm(robin_linear, fb)

    for facets, flux in neumann:
        fb = FacetBasis(mesh, ElementTriP2(), facets=facets)

        @LinearForm
        def neumann_linear(v, w, flux=flux):
            return flux * v

        b = b + asm(neumann_linear, fb)

    if dirichlet is not None:
        predicate, value = dirichlet
        dofs = basis.get_dofs(predicate)
        T = basis.zeros()
        T[dofs] = value
        T = solve(*condense(A, b, x=T, D=dofs))
    else:
        T = solve(A, b)

    return basis, T


def probe_point(basis, T, x, y):
    """The nodal solution `T` interpolated at a single point `(x, y)`."""
    probe = basis.probes(np.array([[x], [y]]))
    return float((probe @ T)[0])


def conductivity_lookup(interior_boundaries, k_values):
    """A callable `k(y_array) -> k_array` for layers stacked in y: the i-th
    value of `k_values` applies between `interior_boundaries[i-1]` and
    `interior_boundaries[i]` (the outer boundaries are implicit). Quadrature
    points never land exactly on a layer interface (they are strictly
    interior to their element, and no element straddles an interface -- see
    `build_tensor_mesh`), so a plain `np.digitize` against the interior
    boundaries alone is exact."""
    interior = np.array(interior_boundaries, dtype=float)
    values = np.array(k_values, dtype=float)

    def lookup(y):
        idx = np.digitize(y, interior) if interior.size else np.zeros_like(y, dtype=int)
        return values[idx]

    return lookup


def solve_candidate(layers_m, half_width_m, strip_half_width_m, heat_flux, convection, ambient, refine):
    """Solve the spreader problem once, at one refinement level of the
    shared coarse tensor mesh, and return the hotspot temperature (K) at the
    strip's center on the heated face, `(0, 0)`, together with the basis
    (for diagnostics such as dof counts)."""
    thicknesses = [t for _, t in layers_m]
    y_boundaries = np.concatenate([[0.0], np.cumsum(thicknesses)])
    total_thickness = float(y_boundaries[-1])
    x_boundaries = sorted({0.0, strip_half_width_m, half_width_m})

    mesh = build_tensor_mesh(x_boundaries, y_boundaries, BASE_ELEMENTS_PER_SEGMENT, refine)
    conductivity = conductivity_lookup(y_boundaries[1:-1], [k for k, _ in layers_m])

    far_facets = mesh.facets_satisfying(lambda p: p[1] > total_thickness - 1e-9)
    strip_facets = mesh.facets_satisfying(
        lambda p: (p[1] < 1e-9) & (p[0] <= strip_half_width_m + 1e-9)
    )

    basis, T = solve_conduction(
        mesh,
        conductivity,
        neumann=[(strip_facets, heat_flux)],
        robin=[(far_facets, convection, ambient)],
    )
    hotspot = probe_point(basis, T, 0.0, 0.0)
    return hotspot, int(basis.N), int(mesh.t.shape[1])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--materials", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    with open(args.candidate, encoding="utf-8") as handle:
        candidate = json.load(handle)
    with open(args.materials, encoding="utf-8") as handle:
        materials = json.load(handle)["materials"]
    with open(args.source, encoding="utf-8") as handle:
        source = json.load(handle)

    if candidate.get("schema") != "avila.thermal/candidate/v1":
        raise SystemExit("candidate schema is not avila.thermal/candidate/v1")
    layers = candidate["layers"]
    if not layers:
        raise SystemExit("candidate has no layers")

    width_mm = Decimal(source["width_mm"])
    strip_width_mm = Decimal(source["strip_width_mm"])
    if strip_width_mm <= 0 or strip_width_mm > width_mm:
        raise SystemExit("strip_width_mm must be positive and at most width_mm")
    heat_flux = float(Decimal(source["heat_flux_W_m2"]))
    convection = float(Decimal(source["convection_W_m2K"]))
    if convection <= 0:
        raise SystemExit("convection_W_m2K must be positive")
    ambient = float(Decimal(source["ambient_K"]))

    layers_m = []
    for layer in layers:
        material = materials[layer["material"]]
        t_mm = Decimal(layer["thickness_mm"])
        if t_mm <= 0:
            raise SystemExit("every layer needs a positive thickness_mm for the finite-element mesh")
        conductivity = float(Decimal(material["conductivity_W_mK"]))
        if conductivity <= 0:
            raise SystemExit(f"material `{layer['material']}` has non-positive conductivity")
        layers_m.append((conductivity, float(t_mm) / 1000.0))

    half_width_m = float(width_mm) / 1000.0 / 2.0
    strip_half_width_m = float(strip_width_mm) / 1000.0 / 2.0

    coarse, coarse_dofs, coarse_elements = solve_candidate(
        layers_m, half_width_m, strip_half_width_m, heat_flux, convection, ambient, COARSE_REFINEMENTS
    )
    fine, fine_dofs, fine_elements = solve_candidate(
        layers_m, half_width_m, strip_half_width_m, heat_flux, convection, ambient, FINE_REFINEMENTS
    )

    coarse_r = round_significant(coarse)
    fine_r = round_significant(fine)
    richardson = round_decimal(fine_r + (fine_r - coarse_r) / Decimal(2**RICHARDSON_ORDER - 1))
    gap = abs(fine_r - coarse_r)
    lower = round_decimal(min(fine_r, richardson) - gap)
    upper = round_decimal(max(fine_r, richardson) + gap)

    result = {
        "schema": SCHEMA,
        "candidate_id": candidate.get("candidate_id"),
        "layers": layers,
        "hotspot_temperature": {
            "lower": canonical(lower),
            "upper": canonical(upper),
            "unit": "K",
            "interpretation": (
                "a discretisation bracket from two mesh levels (h and h/2) and their "
                "Richardson extrapolation (assuming second-order convergence), widened "
                "outward by the fine-minus-coarse gap; not a statistical or metrological "
                "coverage claim -- see limitations"
            ),
        },
        "mesh": {
            "element": "P2 (quadratic) triangles",
            "coarse": {"level": "h", "refinements": COARSE_REFINEMENTS, "elements": coarse_elements, "dofs": coarse_dofs},
            "fine": {"level": "h/2", "refinements": FINE_REFINEMENTS, "elements": fine_elements, "dofs": fine_dofs},
        },
        "engine": {
            "name": "scikit-fem",
            "version": skfem.__version__,
            "python": sys.version.split()[0],
        },
        "limitations": [
            "This is a discretisation bracket between two mesh levels, not a proof of mesh "
            "convergence and not a bound on modeling error: the material conductivities and "
            "densities are unqualified nominal values (see materials.json), the convection "
            "coefficient is a single fixed number with no uncertainty, and the problem is "
            "two-dimensional (uniform in the third direction) with no contact resistance "
            "between layers and no radiation.",
            "The Richardson extrapolation assumes second-order convergence (p=2) of the P2 "
            "solution at the probe point; with only two mesh levels this order is assumed, "
            "not verified against a third level. In practice the outward widening by the "
            "full fine-minus-coarse gap is larger than the extrapolation correction itself "
            "for every candidate this case considers, so the reported bracket is not "
            "sensitive to this assumption.",
            "The hotspot is probed at (0, 0): the strip's symmetry center on the heated "
            "face. With no internal heat generation, heat flows away from the source "
            "everywhere, so the maximum temperature lies on the heated boundary within the "
            "flux region; by the problem's mirror symmetry about x=0, any hotter off-center "
            "point would have an equally hot mirror image, which is inconsistent with a "
            "single interior (on-boundary) maximum for this elliptic problem, so the center "
            "is that maximum. This is a physical argument, not a proof, and this script does "
            "not itself search the mesh for a hotter point.",
            "Not qualified for any decision.",
        ],
    }
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
