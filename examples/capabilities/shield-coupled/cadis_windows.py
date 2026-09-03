#!/usr/bin/env python3
"""Build CADIS (Consistent Adjoint-Driven Importance Sampling) neutron weight
windows for `transport_cadis.py` from `slab_sn.py`'s adjoint solution.

Method
------
`slab_sn.run_case(..., adjoint=True)` gives the forward and adjoint scalar
flux on the S_N solver's own fine mesh (default 0.5 cm) and 175 groups. This
script collapses both onto a coarser `openmc.RegularMesh` along x (cells of
1 to 2 cm, default 2 to match transport.py's own analytic-window mesh) and a
coarse energy grid (about 10 bins spanning the library's full group range,
each a contiguous run of ~n_groups/10 fine groups -- not log-uniform energy
edges, which on this library's actual (non-log-uniform) group spacing can
leave a bin with zero fine groups in it; see build_coarse_energy_bins()),
then sets

    lower_bound(x, E) = C / adjoint_coarse(x, E)
    upper_bound(x, E) = lower_bound(x, E) * upper_bound_ratio

C is fixed by requiring the window at the source face (the first spatial
cell) in the coarse bin containing the source group be (about) 1 -- the
standard CADIS normalization, which starts source particles at unit weight
so they are neither split nor rouletted at birth.

The spatial and energy collapse of the adjoint onto the coarse grid is
forward-flux-weighted (`Sum forward*adjoint / Sum forward` over the fine
cells/groups inside each coarse cell/bin), not a flat average: this is the
standard reaction-rate-preserving way to collapse an importance function,
consistent with how `mgxs_build.py` itself flux-weights cross sections.
Where the forward flux underflows to exactly zero in a coarse block (deep
in a fully opaque region), the collapse falls back to a flat average of the
adjoint values there.

Everything here is a deterministic function of the (deterministic) adjoint
solve and simple weighted averages: no `openmc.WeightWindowGenerator`, no
iteration, no randomness, so the same candidate and library always produce
the same JSON, byte for byte -- `transport_cadis.py` loads it directly
in-process rather than shelling out, and never regenerates it stochastically.
"""

import argparse
import json
from decimal import Decimal, getcontext

import numpy as np

import slab_sn

getcontext().prec = 40
SIGNIFICANT_DIGITS = 8

SCHEMA = "avila.shielding/cadis-weight-windows/v1"
DEFAULT_MESH_WIDTH_CM = 2.0
DEFAULT_N_ENERGY_BINS = 10
DEFAULT_UPPER_RATIO = 5.0
DEFAULT_SURVIVAL_RATIO = 5.0
DEFAULT_MAX_SPLIT = 10
MIN_MESH_BINS = 10
MAX_MESH_BINS = 200


def canonical(value: Decimal) -> str:
    text = format(value.normalize(), "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    if text in ("", "-0"):
        text = "0"
    return text


def round_sig_array(values: np.ndarray, digits: int = SIGNIFICANT_DIGITS) -> np.ndarray:
    arr = np.asarray(values, dtype=np.float64)
    out = np.zeros_like(arr)
    nz = arr != 0
    if np.any(nz):
        exponent = np.floor(np.log10(np.abs(arr[nz])))
        factor = 10.0 ** (digits - 1 - exponent)
        out[nz] = np.round(arr[nz] * factor) / factor
    return out


def build_coarse_energy_bins(boundaries_eV: np.ndarray, n_bins: int):
    """n_bins coarse bins spanning the library's full group range, each a
    contiguous run of roughly n_groups/n_bins fine groups; returns
    (coarse_edges[n_bins+1], fine_group_to_bin[n_groups]).

    Partitioning by fine-group INDEX rather than by n_bins log-uniform
    energy edges is deliberate: VITAMIN-J-175 is not close to log-uniform
    (much finer in some regions, coarser in others), so picking edges
    log-uniformly and assigning each fine group to whichever edge-defined
    bin contains its log-midpoint can leave some coarse bins with ZERO fine
    groups in them -- confirmed directly on this library at n_bins=10.
    collapse_adjoint() then has nothing to average for that bin at any
    spatial cell, so it silently produces exactly 0 everywhere, which
    upstream turns into every lower bound in that whole energy bin sitting
    on the numerical floor rather than tracking the real (if partly
    unconverged) adjoint importance -- and en route to a working weight
    window, a floor that flat made rouletting fire on effectively every
    history that scattered out of the source's own energy bin, killing the
    run. Index-contiguous partitioning guarantees every bin gets at least
    one fine group (as long as n_bins <= n_groups), so this cannot happen.
    """
    n_groups = len(boundaries_eV) - 1
    if n_bins > n_groups:
        raise ValueError(f"n_energy_bins ({n_bins}) must be <= the library's group count ({n_groups})")
    fine_to_bin = np.minimum((np.arange(n_groups) * n_bins) // n_groups, n_bins - 1)
    # coarse edges: the fine-group boundary at the start of each bin, plus the top edge
    bin_starts = np.searchsorted(fine_to_bin, np.arange(n_bins))
    coarse_edges = np.concatenate([boundaries_eV[bin_starts], boundaries_eV[-1:]])
    return coarse_edges, fine_to_bin


def build_coarse_spatial_cells(mesh: "slab_sn.Mesh", mesh_width_cm: float):
    """n_x coarse cells of ~mesh_width_cm spanning [0, shield_thickness);
    returns (edges[n_x+1], fine_cell_to_coarse[n_fine_cells], n_x). Detector
    cells (beyond the shield) are not part of this mesh -- CADIS windows
    bias transport through the shield, matching transport.py's own
    analytic-window mesh, which spans the same range."""
    thickness = mesh.shield_thickness_cm
    n_x = int(np.clip(round(thickness / mesh_width_cm), MIN_MESH_BINS, MAX_MESH_BINS))
    edges = np.linspace(0.0, thickness, n_x + 1)
    fine_mid = mesh.depth_mid
    fine_to_coarse = np.clip(np.searchsorted(edges, fine_mid, side="right") - 1, 0, n_x - 1)
    fine_to_coarse = np.where(mesh.is_detector, -1, fine_to_coarse)  # excluded, marked -1
    return edges, fine_to_coarse, n_x


def collapse_adjoint(forward_phi, adjoint_phi, fine_to_coarse_x, fine_to_bin_e, n_x, n_e):
    """Forward-flux-weighted collapse of the adjoint scalar flux onto the
    (n_x, n_e) coarse grid. forward_phi, adjoint_phi: (n_fine_cells,
    n_groups). Cells with fine_to_coarse_x == -1 (the detector) are excluded."""
    numerator = np.zeros((n_x, n_e))
    denominator = np.zeros((n_x, n_e))
    flat_sum = np.zeros((n_x, n_e))
    flat_count = np.zeros((n_x, n_e))

    n_fine_cells, n_groups = forward_phi.shape
    for fc in range(n_fine_cells):
        xc = fine_to_coarse_x[fc]
        if xc < 0:
            continue
        for fg in range(n_groups):
            eb = fine_to_bin_e[fg]
            w = forward_phi[fc, fg]
            a = adjoint_phi[fc, fg]
            numerator[xc, eb] += w * a
            denominator[xc, eb] += w
            flat_sum[xc, eb] += a
            flat_count[xc, eb] += 1

    with np.errstate(divide="ignore", invalid="ignore"):
        weighted = np.where(denominator > 0, numerator / np.where(denominator > 0, denominator, 1.0), 0.0)
        flat = np.where(flat_count > 0, flat_sum / np.where(flat_count > 0, flat_count, 1.0), 0.0)
    return np.where(denominator > 0, weighted, flat)


DEFAULT_ADJOINT_INNER_ITERATIONS = 6  # see build_cadis_windows()'s docstring note on adjoint convergence


def build_cadis_windows(
    candidate_path, materials_path, source_path, library_path,
    mesh_width_cm=DEFAULT_MESH_WIDTH_CM, n_energy_bins=DEFAULT_N_ENERGY_BINS,
    upper_ratio=DEFAULT_UPPER_RATIO, survival_ratio=DEFAULT_SURVIVAL_RATIO, max_split=DEFAULT_MAX_SPLIT,
    sn_order=slab_sn.DEFAULT_SN_ORDER, mesh_cm=slab_sn.DEFAULT_MESH_CM, convergence=slab_sn.DEFAULT_CONVERGENCE,
    inner_iterations=DEFAULT_ADJOINT_INNER_ITERATIONS,
    library: "slab_sn.Library" = None, lateral_cm: float = None,
):
    """... (see module docstring). `inner_iterations` defaults higher than
    slab_sn.py's own solver default (2): the adjoint's non-thermal groups
    only get a single outer pass (see slab_sn.py's thermal-iteration
    docstring), so their own per-group self-scatter sub-iterations are the
    only mechanism diffusing the detector source back through a thick
    shield for those groups, and 2 is not enough for that to happen well --
    confirmed directly (some coarse adjoint cells/bins numerically
    underflow toward zero at low inner_iterations). 6 is a partial,
    pragmatic mitigation within practical runtime, not full convergence;
    `collapse_adjoint`'s floor (see below) is the other half of coping with
    what's left unconverged. See the case report's runtime findings.
    """
    if not (1.0 <= mesh_width_cm <= 2.0):
        raise SystemExit(f"mesh_width_cm must be in [1, 2] cm, got {mesh_width_cm}")

    (candidate, library, mesh, quad, forward, adjoint_result, psi0, source_group, dose, timing) = slab_sn.run_case(
        candidate_path, materials_path, source_path, library_path,
        sn_order=sn_order, mesh_cm=mesh_cm, convergence=convergence, inner_iterations=inner_iterations,
        adjoint=True, library=library,
    )
    if adjoint_result is None:
        raise RuntimeError("adjoint solve did not run")

    with open(source_path, encoding="utf-8") as handle:
        source = json.load(handle)
    if lateral_cm is None:
        lateral_cm = float(source["lateral_cm"])
    half = lateral_cm / 2.0

    coarse_e_edges, fine_to_bin_e = build_coarse_energy_bins(library.boundaries_eV, n_energy_bins)
    n_e = len(coarse_e_edges) - 1
    source_bin = int(fine_to_bin_e[source_group])

    x_edges, fine_to_coarse_x, n_x = build_coarse_spatial_cells(mesh, mesh_width_cm)

    forward_phi = forward.phi[:, :, 0]
    adjoint_phi = adjoint_result.phi[:, :, 0]
    coarse_adjoint = collapse_adjoint(forward_phi, adjoint_phi, fine_to_coarse_x, fine_to_bin_e, n_x, n_e)

    # Two floors, for two different failure modes of an under-converged adjoint
    # (slab_sn.py's source iteration does not fully diffuse importance back
    # through 100+ cm of a strongly self-scattering shield for groups outside
    # its upscatter/self-scatter re-sweep set -- see that module and the case
    # report's runtime findings):
    #
    # 1. A GLOBAL floor, relative to the whole array's maximum: without it, a
    #    numerically near-zero (cell, energy-bin) value (observed directly:
    #    down to ~1e-108) turns into an absurd lower bound (observed: ~1e116)
    #    once divided into the normalization. This keeps every bound finite.
    #
    # 2. A PER-CELL floor, relative to that same spatial cell's OWN best
    #    (highest-adjoint, most reliable -- closest to the source group,
    #    fewest re-sweeps needed) energy bin. Without it, the source energy's
    #    bin can sit near 1 immediately at the source face while every OTHER
    #    energy bin at that same shallow depth sits far below it (their
    #    importance is still climbing out of under-convergence) -- a cliff a
    #    real particle falls off at its very first collision, since scattering
    #    changes group essentially immediately, while position barely has.
    #    Confirmed directly: without this floor, a real transport_cadis.py run
    #    (5,000 particles x 5 batches) came back with exactly zero neutron
    #    detector hits -- every history rouletted to death within its first
    #    few collisions, near the source face. This floor limits how far any
    #    one energy bin can trail the best-converged bin *at the same depth*,
    #    which is a much smaller, more targeted concession than flattening
    #    the mesh's overall (well-behaved, spatial) dynamic range.
    GLOBAL_RELATIVE_FLOOR = 1e-8
    PER_CELL_RELATIVE_FLOOR = 0.1
    positive = coarse_adjoint[coarse_adjoint > 0]
    global_floor = (positive.max() * GLOBAL_RELATIVE_FLOOR) if positive.size else 1e-300
    per_cell_floor = coarse_adjoint.max(axis=1, keepdims=True) * PER_CELL_RELATIVE_FLOOR
    safe_adjoint = np.maximum(coarse_adjoint, np.maximum(global_floor, per_cell_floor))

    normalization = safe_adjoint[0, source_bin]
    lower = normalization / safe_adjoint
    upper = lower * upper_ratio

    lower_rounded = round_sig_array(lower)
    upper_rounded = round_sig_array(upper)

    # Shape (n_x, 1, 1, n_e), C order -- see openmc.WeightWindows: lower_ww_bounds
    # has shape (ni, nj, nk, num_energy_bins) for a StructuredMesh, and a flat
    # array passed to the constructor is reshaped in that same C order.
    lower_flat = lower_rounded.reshape(n_x, 1, 1, n_e).ravel(order="C")
    upper_flat = upper_rounded.reshape(n_x, 1, 1, n_e).ravel(order="C")

    document = {
        "schema": SCHEMA,
        "candidate_id": candidate.get("candidate_id"),
        "particle_type": "neutron",
        "mesh": {
            "type": "regular",
            "dimension": [n_x, 1, 1],
            "lower_left_cm": [0.0, -half, -half],
            "upper_right_cm": [float(x_edges[-1]), half, half],
        },
        "energy_bounds_eV": [float(x) for x in coarse_e_edges],
        "lower_ww_bounds": {
            "shape": [n_x, 1, 1, n_e],
            "order": "C",
            "values": [canonical(Decimal(repr(float(v)))) for v in lower_flat],
        },
        "upper_ww_bounds": {
            "shape": [n_x, 1, 1, n_e],
            "order": "C",
            "values": [canonical(Decimal(repr(float(v)))) for v in upper_flat],
        },
        "upper_bound_ratio": canonical(Decimal(repr(upper_ratio))),
        "survival_ratio": canonical(Decimal(repr(survival_ratio))),
        "max_split": max_split,
        "construction": {
            "method": "CADIS: lower_bound(x,E) = C / adjoint_scalar_flux_coarse(x,E), C fixed so the "
                      "source-face cell in the source-group's coarse bin is 1; upper_bound = lower_bound * "
                      "upper_bound_ratio.",
            "adjoint_source": slab_sn.run_case.__doc__ and "detector dose response, isotropic, per group (see slab_sn.py)",
            "spatial_collapse": f"S_N mesh (mesh_cm={mesh_cm}) collapsed onto {n_x} cells of "
                                 f"{x_edges[1] - x_edges[0]:.4f} cm spanning the shield only (not the detector), "
                                 "forward-flux-weighted",
            "energy_collapse": f"175 groups collapsed onto {n_e} log-spaced bins spanning the library's full "
                                "range, forward-flux-weighted",
            "sn_order": sn_order,
            "sn_mesh_cm": mesh_cm,
            "sn_convergence": convergence,
            "normalization_cell": [0, source_bin],
            "normalization_value_C": canonical(Decimal(repr(float(normalization)))),
        },
        "limitations": [
            "Deterministic: no WeightWindowGenerator, no iterative refinement against the actual biased "
            "run; built once from the S_N adjoint solution and never touched again in transport_cadis.py.",
            "The adjoint solve shares every approximation slab_sn.py and mgxs_build.py make (multigroup "
            "library, P3 scattering, depth-zone selection, diamond difference): a CADIS window built from a "
            "poor S_N approximation of the true adjoint is still a valid (if less efficient) variance-"
            "reduction scheme -- an importance function only needs to be roughly right to help; the FOM gain "
            "in the case report bounds how well it actually did.",
        ],
    }
    return document


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--materials", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--library", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--mesh-width-cm", type=float, default=DEFAULT_MESH_WIDTH_CM)
    parser.add_argument("--n-energy-bins", type=int, default=DEFAULT_N_ENERGY_BINS)
    parser.add_argument("--upper-ratio", type=float, default=DEFAULT_UPPER_RATIO)
    parser.add_argument("--survival-ratio", type=float, default=DEFAULT_SURVIVAL_RATIO)
    parser.add_argument("--max-split", type=int, default=DEFAULT_MAX_SPLIT)
    parser.add_argument("--sn-order", type=int, default=slab_sn.DEFAULT_SN_ORDER)
    parser.add_argument("--mesh-cm", type=float, default=slab_sn.DEFAULT_MESH_CM)
    parser.add_argument("--convergence", type=float, default=slab_sn.DEFAULT_CONVERGENCE)
    parser.add_argument("--inner-iterations", type=int, default=DEFAULT_ADJOINT_INNER_ITERATIONS)
    args = parser.parse_args()

    document = build_cadis_windows(
        args.candidate, args.materials, args.source, args.library,
        mesh_width_cm=args.mesh_width_cm, n_energy_bins=args.n_energy_bins,
        upper_ratio=args.upper_ratio, survival_ratio=args.survival_ratio, max_split=args.max_split,
        sn_order=args.sn_order, mesh_cm=args.mesh_cm, convergence=args.convergence,
        inner_iterations=args.inner_iterations,
    )
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
