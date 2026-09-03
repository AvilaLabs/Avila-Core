#!/usr/bin/env python3
"""Reproduces NAFEMS Test 4M ("T4"), a published 2-D steady-conduction
benchmark, using this case's own finite-element solver code path --
`build_tensor_mesh`, `solve_conduction`, and `probe_point` imported directly
from `thermal_fe.py` one directory up -- rather than a second, separately
written implementation of the same physics. This is the validation evidence
a qualification record for the finite-element capability can bind by
digest: it shows the exact solver the spreader screens candidates with
reproduces a published reference value, on the same code path, not on a
lookalike solver kept only for testing.

Problem (NAFEMS Selected Benchmarks for Thermal Analysis, Test 4M -- a
rectangular plate with mixed boundary conditions, commonly cited as "T4"):
a 0.6 m x 1.0 m plate, thermal conductivity k = 52 W/m-K, no internal heat
generation.

  * y = 0 (bottom edge): Dirichlet, fixed at 100 C.
  * x = 0 (left edge): insulated (natural, no term).
  * x = 0.6 m (right edge) and y = 1.0 m (top edge): Robin, convection at
    h = 750 W/m2-K to an ambient of 0 C.

Reference value: T(0.6, 0.2) = 18.25 C, at the bottom-right corner region.
`thermal_fe.py`'s own conduction and Robin forms are agnostic to which axis
carries a Dirichlet condition or which faces are Robin versus Neumann versus
natural, so this reuses them unmodified: y = 0 is fixed here where the
spreader problem instead applies a Neumann strip, and there is no Neumann
term here at all (`solve_conduction`'s `neumann` argument is simply
omitted).

Solved at three uniformly-refined levels of one coarse tensor mesh so the
reported values show convergence toward the reference, not a single
result that could be a coincidence.
"""

import json
import sys
from decimal import Decimal, getcontext
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from thermal_fe import (  # noqa: E402
    build_tensor_mesh,
    canonical,
    probe_point,
    round_significant,
    solve_conduction,
)

getcontext().prec = 40

SCHEMA = "avila.thermal/validation/v1"
BENCHMARK = "NAFEMS Test 4M (2-D steady conduction, mixed Dirichlet/Robin/insulated boundaries)"
SOURCE = "NAFEMS Selected Benchmarks for Thermal Analysis"
REFERENCE_C = Decimal("18.25")
PROBE_M = (0.6, 0.2)
CONDUCTIVITY_W_MK = 52.0
CONVECTION_W_M2K = 750.0
AMBIENT_C = 0.0
FIXED_EDGE_C = 100.0
WIDTH_M = 0.6
HEIGHT_M = 1.0
BASE_ELEMENTS_PER_SEGMENT = 6
LEVELS = (0, 1, 2)


def solve_level(refine: int):
    mesh = build_tensor_mesh([0.0, WIDTH_M], [0.0, HEIGHT_M], BASE_ELEMENTS_PER_SEGMENT, refine)

    right_top = mesh.facets_satisfying(
        lambda p: (p[0] > WIDTH_M - 1e-9) | (p[1] > HEIGHT_M - 1e-9)
    )

    basis, T = solve_conduction(
        mesh,
        CONDUCTIVITY_W_MK,
        robin=[(right_top, CONVECTION_W_M2K, AMBIENT_C)],
        dirichlet=(lambda p: p[1] < 1e-9, FIXED_EDGE_C),
    )
    value = probe_point(basis, T, *PROBE_M)
    return value, int(basis.N)


def main() -> int:
    output = Path(__file__).resolve().parent / "nafems-t4.json"

    levels = []
    for refine in LEVELS:
        value_c, dofs = solve_level(refine)
        value_r = round_significant(value_c)
        deviation = round_significant(float((value_r - REFERENCE_C) / REFERENCE_C))
        levels.append(
            {
                "refinements": refine,
                "dofs": dofs,
                "computed_C": canonical(value_r),
                "relative_deviation": canonical(deviation),
            }
        )

    finest = levels[-1]
    finest_deviation = abs(Decimal(finest["relative_deviation"]))
    within_tolerance = finest_deviation <= Decimal("0.001")

    document = {
        "schema": SCHEMA,
        "benchmark": BENCHMARK,
        "source": SOURCE,
        "problem": {
            "width_m": canonical(Decimal(repr(WIDTH_M))),
            "height_m": canonical(Decimal(repr(HEIGHT_M))),
            "conductivity_W_mK": canonical(Decimal(repr(CONDUCTIVITY_W_MK))),
            "convection_W_m2K": canonical(Decimal(repr(CONVECTION_W_M2K))),
            "ambient_C": canonical(Decimal(repr(AMBIENT_C))),
            "fixed_edge_C": canonical(Decimal(repr(FIXED_EDGE_C))),
            "boundary_conditions": (
                "y=0 Dirichlet (fixed_edge_C); x=0 insulated; x=width and y=height Robin "
                "(convection_W_m2K to ambient_C)"
            ),
        },
        "probe_point_m": [canonical(Decimal(repr(PROBE_M[0]))), canonical(Decimal(repr(PROBE_M[1])))],
        "reference_C": canonical(REFERENCE_C),
        "levels": levels,
        "within_tolerance": {"threshold": "0.001", "finest_level": within_tolerance},
        "engine": {
            "name": "scikit-fem",
            "version": __import__("skfem").__version__,
            "python": sys.version.split()[0],
            "solver_module": "thermal_fe.py (build_tensor_mesh, solve_conduction, probe_point -- imported, not duplicated)",
        },
        "limitations": [
            "This reproduces one published benchmark point on this case's own solver code "
            "path; it does not itself qualify the spreader problem's different geometry, "
            "layered conductivity, or boundary layout -- it establishes that the shared "
            "assembly-and-solve routine those problems both call is correct on a problem "
            "with an independently published reference answer.",
            "The relative deviation is computed from an already-rounded computed value "
            "against an exact reference; it is a deviation from one published benchmark "
            "point, not a general error bound.",
        ],
    }
    with open(output, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)
        handle.write("\n")

    for level in levels:
        print(
            f"refinements={level['refinements']} dofs={level['dofs']:6d} "
            f"T(0.6,0.2)={level['computed_C']} C (reference {REFERENCE_C})  "
            f"relative deviation={level['relative_deviation']}"
        )
    print(f"within 0.1% at the finest level: {within_tolerance}")
    return 0 if within_tolerance else 1


if __name__ == "__main__":
    raise SystemExit(main())
