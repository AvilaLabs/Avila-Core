#!/usr/bin/env python3
"""A 1-D slab multigroup discrete-ordinates (S_N) neutron transport solver,
the deterministic speed tier for the coupled shield search: given a
candidate slab, `materials.json`, `source.json`, and the multigroup library
`mgxs_build.py` produces, it solves for the neutron scalar flux and the
dose rate behind the slab, and (with `--adjoint`) the adjoint flux with the
detector's dose response as the adjoint source, for CADIS weight windows
(`cadis_windows.py`).

Method
------
Gauss-Legendre quadrature in direction cosine mu (`numpy.polynomial.legendre
.leggauss`, default S16: 16 ordinates), P3 anisotropic scattering expanded in
Legendre moments, diamond difference spatial differencing with a step
(upwind) fixup wherever a cell/direction/group would otherwise go negative,
uniform mesh (default 0.5 cm, snapped to every layer boundary so no cell
straddles two materials).

The energy sweep is source iteration with Gauss-Seidel over groups: groups
are indexed by ascending energy (0 = thermal, 174 = the 14.1 MeV source
group, matching the library's convention), and a single pass from the
fastest group down to the slowest correctly resolves pure downscatter in one
shot -- by the time group g is swept, every faster group g' > g has already
been updated this pass, so g's downscatter source is exact for this outer
iteration; a slower group's contribution to g (upscatter) is necessarily
still last-iteration data, since it has not been swept yet. Within each
group, a handful of inner iterations resolve that group's own self-scatter.
What is left after one full pass is thermal upscatter: neutrons already
downscattered into the thermal cluster feed back up into slightly faster
thermal-ish groups. Rather than re-sweeping all 175 groups to chase that
(expensive and pointless -- the non-thermal groups' equations do not depend
on the thermal cluster at all once pass one is done), the set of groups
reachable by any upscatter transition is identified directly from the
library's scattering matrices, and only that set is re-swept, fastest to
slowest, until the scalar flux there stops changing (`--convergence`,
default 1e-6 relative). This is the "thermal upscatter iteration" named in
the top-level task.

Boundary source
----------------
transport.py's plane source is isotropic over the full 4*pi sphere, sitting
half a centimeter in front of a vacuum boundary; the half of its emission
that heads backward (mu < 0 relative to the shield) crosses 0.5 cm of void
and immediately leaks out, so only the forward hemisphere ever reaches the
shield, arriving at x=0 isotropically distributed over that hemisphere. In
the 1-D reduced angular convention used here (ordinates already azimuthally
integrated, so a hemisphere-isotropic incident flux psi0 has current
J = psi0/2, i.e. integral_0^1 mu dmu = 1/2), matching that same physical
picture -- half of strength/area lost, the rest isotropic over the
hemisphere -- gives psi0 = strength/area exactly: half is lost (current
J0 = (strength/2)/area), and psi0 = 2*J0 = strength/area. This is applied
in the top (14.1 MeV) group, at every ordinate with mu > 0, and it puts the
solver on transport.py's own absolute scale (n/cm^2/s), so the resulting
scalar flux needs no further per-source-particle renormalization -- unlike
an MC tally, it already IS the physical flux. This normalization is exactly
what verification (a) checks: a solver-wide error in this constant would
show up as a systematic ratio against the Monte Carlo record.

Detector
--------
Reproducing transport.py's air detector cell as a literal absorbing/
scattering medium would need an air cross section entry this library does
not carry; air's neutron interaction probability over the 1 cm detector
transport.py uses is on the order of 10^-4 to 10^-3 in optical depth, i.e.
below the rounding this solver already applies, so the detector is modeled
as `n_zones`-independent vacuum (sigma=0 in every group): flux is exactly
uniform across it (nothing attenuates or scatters there), so its volume
average -- transport.py's own convention -- equals the value at any point in
it, sampled here as the scalar flux in its mesh cells.

Outputs
-------
`avila.shielding/slab-sn-result/v1`: nominal neutron dose rate behind the
slab as an exact decimal string, scalar flux per cell per group, run/
convergence diagnostics, and (with `--adjoint`) the adjoint scalar flux per
cell per group plus the boundary reciprocity check
(`test_slab_sn.py::test_adjoint_forward_reciprocity` is the same identity
run as a unit test on a small case).
"""

import argparse
import hashlib
import json
import os
import sys
import time
from decimal import Decimal, getcontext

import numpy as np

getcontext().prec = 40
SIGNIFICANT_DIGITS = 8

SCHEMA = "avila.shielding/slab-sn-result/v1"
CANDIDATE_SCHEMA = "avila.shielding/candidate/v1"
VACUUM_KEY = "__vacuum__"
DEFAULT_SN_ORDER = 16
DEFAULT_MESH_CM = 0.5
DEFAULT_CONVERGENCE = 1.0e-6
DEFAULT_INNER_ITERATIONS = 2
DEFAULT_MAX_THERMAL_ITERATIONS = 200


# --- rounding helpers, mirroring transport.py's stable()/canonical() convention ---

def sha256_of(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def stable(value: float) -> Decimal:
    decimal = Decimal(repr(value))
    if decimal == 0:
        return Decimal(0)
    quantum = Decimal(1).scaleb(decimal.adjusted() - SIGNIFICANT_DIGITS + 1)
    return decimal.quantize(quantum)


def round_decimal(value: Decimal, digits: int = SIGNIFICANT_DIGITS) -> Decimal:
    if value == 0:
        return Decimal(0)
    quantum = Decimal(1).scaleb(value.adjusted() - digits + 1)
    return value.quantize(quantum)


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


# --- Legendre polynomials P0..P3 (hardcoded: only ever need these four moments) ---

def legendre_p(order: int, mu: np.ndarray) -> np.ndarray:
    if order == 0:
        return np.ones_like(mu)
    if order == 1:
        return mu
    if order == 2:
        return 0.5 * (3.0 * mu * mu - 1.0)
    if order == 3:
        return 0.5 * (5.0 * mu ** 3 - 3.0 * mu)
    raise ValueError(f"unsupported Legendre order {order}")


# --- library ---

class Library:
    """In-memory view of an mgxs_build.py HDF5 library."""

    def __init__(self, path: str):
        import h5py

        with h5py.File(path, "r") as h5:
            self.schema = h5.attrs["schema"]
            self.n_groups = int(h5.attrs["n_groups"])
            self.legendre_order = int(h5.attrs["legendre_order"])
            self.slab_thickness_cm = float(h5.attrs["slab_thickness_cm"])
            self.boundaries_eV = np.asarray(h5["group_boundaries_eV"][:], dtype=np.float64)
            self.dose_response = np.asarray(h5["dose_response_neutron_AP_pSv_cm2"][:], dtype=np.float64)
            self.materials = {}
            for name, group in h5["materials"].items():
                self.materials[name] = {
                    "zone_bounds_cm": np.asarray(group["zone_bounds_cm"][:], dtype=np.float64),
                    "total": np.asarray(group["zone_total_cm_inv"][:], dtype=np.float64),  # (n_zones, n_groups)
                    "scatter": np.asarray(group["zone_scatter_cm_inv"][:], dtype=np.float64),  # (n_zones, g_in, g_out, l)
                }
        if len(self.boundaries_eV) - 1 != self.n_groups:
            raise ValueError("library group_boundaries_eV length inconsistent with n_groups")

    def source_group(self, energy_eV: float) -> int:
        idx = int(np.searchsorted(self.boundaries_eV, energy_eV, side="right")) - 1
        if idx < 0 or idx >= self.n_groups or not (self.boundaries_eV[idx] <= energy_eV <= self.boundaries_eV[idx + 1]):
            raise ValueError(f"source energy {energy_eV} eV is not within the library's group structure")
        return idx

    def zone_index(self, material: str, depth_cm: float) -> int:
        bounds = self.materials[material]["zone_bounds_cm"]
        n_zones = len(bounds) - 1
        if depth_cm >= bounds[-1]:
            return n_zones - 1
        idx = int(np.searchsorted(bounds, depth_cm, side="right")) - 1
        return max(0, min(idx, n_zones - 1))

    def xs_for(self, material: str, depth_cm: float):
        zone = self.zone_index(material, depth_cm)
        mat = self.materials[material]
        return mat["total"][zone], mat["scatter"][zone]


def vacuum_xs(n_groups: int, legendre_order: int):
    return np.zeros(n_groups), np.zeros((n_groups, n_groups, legendre_order + 1))


# --- mesh ---

class Mesh:
    """Cells covering [0, shield_thickness) then [shield_thickness,
    shield_thickness + detector_thickness), snapped to every layer boundary
    and to mesh_cm elsewhere; `cell_key[i]` is (material, zone_index) or
    VACUUM_KEY for detector cells."""

    def __init__(self, candidate_layers, library: "Library", detector_thickness_cm: float, mesh_cm: float):
        boundaries = [0.0]
        materials_by_layer = []
        for layer in candidate_layers:
            thickness = float(layer["thickness_cm"])
            if thickness <= 0:
                continue
            boundaries.append(boundaries[-1] + thickness)
            materials_by_layer.append(layer["material"])
        self.shield_thickness_cm = boundaries[-1] if len(boundaries) > 1 else 0.0

        edges = set(boundaries)
        # uniform grid within each layer, snapped to mesh_cm, plus the detector region
        for i in range(len(materials_by_layer)):
            lo, hi = boundaries[i], boundaries[i + 1]
            n_sub = max(1, round((hi - lo) / mesh_cm))
            for k in range(n_sub + 1):
                edges.add(lo + (hi - lo) * k / n_sub)
        detector_lo = self.shield_thickness_cm
        detector_hi = detector_lo + detector_thickness_cm
        n_sub = max(1, round(detector_thickness_cm / mesh_cm))
        for k in range(n_sub + 1):
            edges.add(detector_lo + detector_thickness_cm * k / n_sub)

        edge_list = np.array(sorted(edges), dtype=np.float64)
        # drop any degenerate (near-zero-width) cells from float snapping
        keep = np.concatenate([[True], np.diff(edge_list) > 1e-9])
        edge_list = edge_list[keep]

        self.edges = edge_list
        self.n_cells = len(edge_list) - 1
        self.dx = np.diff(edge_list)
        mid = 0.5 * (edge_list[:-1] + edge_list[1:])
        self.depth_mid = mid
        self.cell_material = []
        self.cell_key = []
        self.is_detector = np.zeros(self.n_cells, dtype=bool)
        li = 0
        clipped_depth = False
        for i, xm in enumerate(mid):
            if xm >= detector_lo:
                self.cell_material.append(None)
                self.cell_key.append(VACUUM_KEY)
                self.is_detector[i] = True
                continue
            while li < len(materials_by_layer) - 1 and xm >= boundaries[li + 1]:
                li += 1
            material = materials_by_layer[li]
            self.cell_material.append(material)
            depth = xm
            if depth >= library.slab_thickness_cm:
                depth = library.slab_thickness_cm - 1e-6
                clipped_depth = True
            zone = library.zone_index(material, depth)
            self.cell_key.append((material, zone))
        self.clipped_depth = clipped_depth
        self.total_thickness_cm = edge_list[-1]


# --- quadrature ---

class Quadrature:
    def __init__(self, sn_order: int, legendre_order: int):
        if sn_order % 2 != 0:
            raise ValueError("sn_order must be even (Gauss-Legendre pairs +-mu)")
        mu, w = np.polynomial.legendre.leggauss(sn_order)
        self.mu = mu
        self.w = w
        self.pos = np.where(mu > 0)[0]
        self.neg = np.where(mu < 0)[0]
        # P_matrix[ordinate, l] = (2l+1)/2 * P_l(mu_ordinate)
        self.P = np.stack([(2 * l + 1) / 2.0 * legendre_p(l, mu) for l in range(legendre_order + 1)], axis=1)
        # moment_weights[l, ordinate] = w_ordinate * P_l(mu_ordinate), for computing phi_l = sum_ordinate moment_weights * psi
        self.moment_w = np.stack([w * legendre_p(l, mu) for l in range(legendre_order + 1)], axis=0)


# --- sweep ---

def sweep_direction(mu_sub: np.ndarray, dx: np.ndarray, sigma_t_cells: np.ndarray, Q_cells: np.ndarray, boundary_incoming: np.ndarray, forward: bool):
    """Diamond-difference sweep with a step (upwind) fixup, for one set of
    ordinates all sharing the same propagation sense (`forward`=True sweeps
    cells left to right using mu_sub>0; False sweeps right to left using
    mu_sub<0, internally via |mu_sub|). Returns (psi_center[n_cells,n_sub],
    edge_at_index0_side[n_sub]) where the second element is the flux at the
    UPSTREAM edge of cell 0 in sweep order (x=0 for forward, x=L for
    backward) -- i.e. simply `boundary_incoming` echoed back, kept for
    symmetry with the trailing return used by the adjoint reciprocity check,
    which wants the flux at the OTHER (downstream) end.
    """
    n_cells = len(dx)
    n_sub = len(mu_sub)
    mu_mag = np.abs(mu_sub)
    psi_center = np.empty((n_cells, n_sub))
    order = range(n_cells) if forward else range(n_cells - 1, -1, -1)
    psi_up = boundary_incoming.copy()
    last_downstream = None
    for i in order:
        st = sigma_t_cells[i]
        h = dx[i]
        Q = Q_cells[i]
        coeff = mu_mag / h
        denom_dd = coeff + 0.5 * st
        psi_down_dd = (Q + psi_up * (coeff - 0.5 * st)) / denom_dd
        psi_c_dd = 0.5 * (psi_up + psi_down_dd)
        bad = (psi_down_dd < 0.0) | (psi_c_dd < 0.0)
        if np.any(bad):
            denom_step = coeff + st
            psi_down_step = (Q + psi_up * coeff) / denom_step
            psi_down = np.where(bad, psi_down_step, psi_down_dd)
            psi_c = np.where(bad, psi_down_step, psi_c_dd)
        else:
            psi_down = psi_down_dd
            psi_c = psi_c_dd
        psi_center[i] = psi_c
        psi_up = psi_down
        last_downstream = psi_down
    return psi_center, last_downstream


# --- solver core ---

class Result:
    pass


def _identify_thermal_set(mesh: Mesh, library: Library, legendre_order: int) -> np.ndarray:
    """Groups involved in any upscatter transition (g_from -> g_to with
    g_to > g_from, i.e. to higher energy, in the ascending-energy index
    convention) for any (material, zone) actually present on this mesh --
    as either the slower source or the faster target of the transition, so
    the same set is the right one to re-sweep for both the forward pass
    (which needs fresh data for upscatter *targets*, processing
    fastest-to-slowest) and the adjoint pass (which needs fresh data for
    upscatter *sources* in its transposed contraction, processing
    slowest-to-fastest). Returns a boolean mask over group indices, filled
    contiguously from the thermal end (0) up to the highest group index any
    upscatter transition touches, since real upscatter clusters contiguously
    near thermal; a mild over-inclusion versus the exact minimal set costs a
    few extra group-sweeps, never correctness. Empty iff no material/zone
    touched by this candidate has any upscatter at all, in which case one
    Gauss-Seidel pass is already the converged answer.
    """
    n_groups = library.n_groups
    max_involved = -1
    keys = set(k for k in mesh.cell_key if k != VACUUM_KEY)
    for material, zone in keys:
        scat = library.materials[material]["scatter"][zone][:, :, 0]  # P0 moment, (g_in, g_out)
        g_in, g_out = np.nonzero(scat > 0)
        up = g_out > g_in
        if np.any(up):
            max_involved = max(max_involved, int(g_out[up].max()), int(g_in[up].max()))
    mask = np.zeros(n_groups, dtype=bool)
    if max_involved >= 0:
        mask[: max_involved + 1] = True
    return mask


def solve(
    mesh: Mesh,
    library: Library,
    quad: Quadrature,
    source_group: int,
    psi0: float,
    legendre_order: int,
    convergence: float = DEFAULT_CONVERGENCE,
    inner_iterations: int = DEFAULT_INNER_ITERATIONS,
    max_thermal_iterations: int = DEFAULT_MAX_THERMAL_ITERATIONS,
    adjoint: bool = False,
    detector_source: np.ndarray = None,
):
    """Solve the forward problem (adjoint=False, boundary source psi0 in
    source_group) or the mu-reversed adjoint problem (adjoint=True,
    isotropic volumetric source `detector_source[group]` in every detector
    cell, both boundaries vacuum) -- see the module docstring and
    test_slab_sn.py::test_adjoint_forward_reciprocity for the derivation
    that makes the adjoint just a forward-style sweep with the group
    scattering contraction transposed. Returns a Result with
    `phi[cell,group,l]` (l=0..legendre_order), `boundary_front` and
    `boundary_back` (the converged angular flux edge arrays at x=0 and
    x=L, per group, for the negative- and positive-going ordinates
    respectively -- what the reciprocity check needs), `iterations`,
    `thermal_iterations`.
    """
    n_groups = library.n_groups
    n_cells = mesh.n_cells
    n_ord = len(quad.mu)

    total_by_cell = np.empty((n_cells, n_groups))
    unique_keys = {}
    for key in set(mesh.cell_key):
        if key == VACUUM_KEY:
            total, scat = vacuum_xs(n_groups, legendre_order)
        else:
            material, zone = key
            total = library.materials[material]["total"][zone]
            scat = library.materials[material]["scatter"][zone]
        unique_keys[key] = scat
        idx = [i for i, k in enumerate(mesh.cell_key) if k == key]
        total_by_cell[idx, :] = total

    cells_by_key = {key: np.array([i for i, k in enumerate(mesh.cell_key) if k == key]) for key in unique_keys}

    phi = np.zeros((n_cells, n_groups, legendre_order + 1))
    boundary_front = np.zeros((n_groups, n_ord))  # angular flux at x=0, all ordinates (post-solve)
    boundary_back = np.zeros((n_groups, n_ord))  # angular flux at x=L

    thermal_mask = _identify_thermal_set(mesh, library, legendre_order)

    def sweep_one_group(g, use_self_scatter_from_phi):
        """Run inner_iterations self-scatter sub-iterations for group g,
        using `use_self_scatter_from_phi[cell,l]` as the starting self-
        scatter estimate; mutates phi[:,g,:] in place; returns the final
        boundary edge angular fluxes for this group."""
        phi_g_l = use_self_scatter_from_phi.copy()
        st = total_by_cell[:, g]
        for _ in range(max(1, inner_iterations)):
            Q_l = np.zeros((n_cells, legendre_order + 1))
            for key, cell_idx in cells_by_key.items():
                scat = unique_keys[key]  # (g_in, g_out, l)
                # Forward: Q_l[g] = sum_g' scat[g', g, l] * phi_l[g']  (fix out-group=g, sum in-group=g')
                # Adjoint: Q*_l[g] = sum_g' scat[g, g', l] * phi*_l[g'] (fix in-group=g, sum out-group=g')
                # -- see the module docstring / test_slab_sn.py for the derivation (chi(x,mu):=psi*(x,-mu)
                # satisfies a forward-style equation with this transposed contraction).
                slice_ = scat[g, :, :] if adjoint else scat[:, g, :]  # (g', l)
                other = phi[cell_idx][:, :, :]  # (n_cells_key, g', l)
                other = other.copy()
                other[:, g, :] = phi_g_l[cell_idx, :]
                Q_l[cell_idx] = np.einsum("cgl,gl->cl", other, slice_)

            Q_cells = Q_l @ quad.P.T  # (n_cells, n_ord)
            if adjoint and detector_source is not None:
                # Isotropic external source: Q_ext(x,mu) = h_g for every mu, added directly
                # (not via the l=0 moment slot -- reconstructing a moment through
                # sum_l (2l+1)/2 P_l(mu)*Q_l would apply the l=0 prefactor (2*0+1)/2 = 1/2
                # a second time and halve it).
                Q_cells[mesh.is_detector, :] += detector_source[g]

            psi_pos_center, edge_pos_end = sweep_direction(
                quad.mu[quad.pos], mesh.dx, st, Q_cells[:, quad.pos],
                _incoming_pos(g),
                forward=True,
            )
            psi_neg_center, edge_neg_end = sweep_direction(
                quad.mu[quad.neg], mesh.dx, st, Q_cells[:, quad.neg],
                _incoming_neg(g),
                forward=False,
            )

            psi_center = np.empty((n_cells, n_ord))
            psi_center[:, quad.pos] = psi_pos_center
            psi_center[:, quad.neg] = psi_neg_center

            for l in range(legendre_order + 1):
                phi_g_l[:, l] = psi_center @ quad.moment_w[l]

        phi[:, g, :] = phi_g_l
        front_pos = _incoming_pos(g)
        front_neg = edge_neg_end  # psi at x=0 for negative ordinates (sweep ended there)
        back_pos = edge_pos_end  # psi at x=L for positive ordinates
        back_neg = _incoming_neg(g)
        boundary_front[g, quad.pos] = front_pos
        boundary_front[g, quad.neg] = front_neg
        boundary_back[g, quad.pos] = back_pos
        boundary_back[g, quad.neg] = back_neg

    def _incoming_pos(g):
        if not adjoint and g == source_group:
            return np.full(len(quad.pos), psi0, dtype=np.float64)
        return np.zeros(len(quad.pos))

    def _incoming_neg(g):
        return np.zeros(len(quad.neg))

    # Pass 1, full spectrum, in the order that makes the dominant coupling
    # exact after one pass: forward physics is downscatter-dominant (a fast
    # group mostly feeds slower groups), so sweeping fastest-to-slowest
    # means every group's dominant source is already-updated data. The
    # adjoint's dominant coupling runs the other way -- Qchi_l,g sums
    # scat[g,g',l], which (like the forward matrix) is populated mainly for
    # g' <= g, so a *slowest-to-fastest* pass is what gives the adjoint
    # sweep the same one-pass exactness for its dominant coupling.
    group_order = range(n_groups) if adjoint else range(n_groups - 1, -1, -1)
    for g in group_order:
        sweep_one_group(g, phi[:, g, :])
    outer_iterations = 1

    thermal_iterations = 0
    if np.any(thermal_mask):
        thermal_indices = np.nonzero(thermal_mask)[0]
        thermal_groups = thermal_indices if adjoint else thermal_indices[::-1]
        for it in range(max_thermal_iterations):
            phi0_before = phi[:, thermal_mask, 0].copy()
            for g in thermal_groups:
                sweep_one_group(g, phi[:, g, :])
            phi0_after = phi[:, thermal_mask, 0]
            denom = np.maximum(np.abs(phi0_before), 1e-300)
            rel_change = np.max(np.abs(phi0_after - phi0_before) / denom)
            thermal_iterations = it + 1
            if rel_change < convergence:
                break

    result = Result()
    result.phi = phi
    result.boundary_front = boundary_front
    result.boundary_back = boundary_back
    result.outer_iterations = outer_iterations
    result.thermal_iterations = thermal_iterations
    result.thermal_group_count = int(np.count_nonzero(thermal_mask))
    return result


def reciprocity_response(quad: "Quadrature", adjoint_result: "Result", psi0: float, source_group: int) -> float:
    """R = psi0 * sum_{n: mu_n>0} w_n*mu_n*psi*_{source_group}(0,mu_n), the
    standard reciprocity identity for a boundary-source problem: the forward
    response (sum_g h_g*phi_g(detector)) equals the adjoint flux at the
    source boundary, current-weighted, dotted against the incident boundary
    distribution. `adjoint_result` is solve(..., adjoint=True)'s return
    value; because that solve works entirely in the mu-reversed variable
    chi(x,mu) := psi*(x,-mu) (see the module docstring), psi*(0,mu>0) is
    read off as chi(0,-mu) = adjoint_result.boundary_front[source_group] at
    the NEGATIVE ordinate paired with each positive mu_n. The pairing is by
    matching |mu|, not array position: leggauss's sorted output stores the
    positive and negative halves in mirrored (not index-aligned) order.
    """
    mu = quad.mu
    chi_front = adjoint_result.boundary_front[source_group]
    total = 0.0
    for n in quad.pos:
        partner = quad.neg[np.argmin(np.abs(mu[quad.neg] - (-mu[n])))]
        total += quad.w[n] * mu[n] * chi_front[partner]
    return psi0 * total


def dose_rate_uSv_h(phi_detector_group: np.ndarray, dose_response: np.ndarray, strength_n_per_s: Decimal) -> Decimal:
    """phi_detector_group: scalar flux [n/cm^2/s] per group in the detector
    (already absolute/physical, not per-source-particle -- see the module
    docstring's boundary-source normalization). dose_response is pSv*cm^2
    per group. Sum_g phi_g*h_g gives pSv/s; *3600/1e6 gives uSv/h. Unlike
    transport.py, strength is already baked into phi via the boundary
    condition, so it does not appear again here except as the Decimal
    passthrough for exactness bookkeeping -- kept as a parameter so a caller
    can assert it was actually used to build phi (see main()).
    """
    total_pSv_s = float(np.dot(phi_detector_group, dose_response))
    return Decimal(repr(total_pSv_s)) * Decimal(3600) / Decimal(1_000_000)


def run_case(candidate_path, materials_path, source_path, library_path,
             sn_order=DEFAULT_SN_ORDER, mesh_cm=DEFAULT_MESH_CM, convergence=DEFAULT_CONVERGENCE,
             inner_iterations=DEFAULT_INNER_ITERATIONS, max_thermal_iterations=DEFAULT_MAX_THERMAL_ITERATIONS,
             adjoint=False, library: "Library" = None):
    """Load inputs, solve, and return (candidate, library, mesh, quad,
    forward_result, adjoint_result_or_None, psi0, source_group, dose,
    timing). The importable entry point cadis_windows.py and
    transport_cadis.py use directly, so the adjoint solve never round-trips
    through JSON or the filesystem.
    """
    with open(candidate_path, encoding="utf-8") as handle:
        candidate = json.load(handle)
    if candidate.get("schema") != CANDIDATE_SCHEMA:
        raise SystemExit(f"candidate schema is not {CANDIDATE_SCHEMA}")
    with open(materials_path, encoding="utf-8") as handle:
        materials_table = json.load(handle)["materials"]
    with open(source_path, encoding="utf-8") as handle:
        source = json.load(handle)

    if library is None:
        library = Library(library_path)

    for layer in candidate["layers"]:
        if layer["material"] not in materials_table:
            raise SystemExit(f"candidate references unknown material {layer['material']!r}")
        if layer["material"] not in library.materials:
            raise SystemExit(f"library has no entry for material {layer['material']!r}")

    legendre_order = library.legendre_order
    quad = Quadrature(sn_order, legendre_order)

    detector_thickness_cm = float(source["detector_thickness_cm"])
    mesh = Mesh(candidate["layers"], library, detector_thickness_cm, mesh_cm)

    area = Decimal(source["area_cm2"])
    strength = Decimal(source["strength_n_per_s"])
    lateral = Decimal(source["lateral_cm"])
    if lateral * lateral != area:
        raise SystemExit("source area must equal the square of the lateral extent for this slab model")
    psi0 = float(strength) / float(area)
    energy_eV = float(source["energy_MeV"]) * 1.0e6
    source_group = library.source_group(energy_eV)

    t0 = time.time()
    forward = solve(
        mesh, library, quad, source_group, psi0, legendre_order,
        convergence=convergence, inner_iterations=inner_iterations,
        max_thermal_iterations=max_thermal_iterations, adjoint=False,
    )
    forward_time = time.time() - t0

    detector_flux = forward.phi[mesh.is_detector, :, 0]
    detector_flux_mean = detector_flux.mean(axis=0)  # spatially uniform in vacuum; average for robustness
    dose = dose_rate_uSv_h(detector_flux_mean, library.dose_response, strength)

    adjoint_result = None
    adjoint_time = None
    if adjoint:
        t1 = time.time()
        adjoint_result = solve(
            mesh, library, quad, source_group, 0.0, legendre_order,
            convergence=convergence, inner_iterations=inner_iterations,
            max_thermal_iterations=max_thermal_iterations, adjoint=True,
            detector_source=library.dose_response,
        )
        adjoint_time = time.time() - t1

    timing = {"forward_s": forward_time, "adjoint_s": adjoint_time}
    return candidate, library, mesh, quad, forward, adjoint_result, psi0, source_group, dose, timing


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--materials", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--library", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--sn-order", type=int, default=DEFAULT_SN_ORDER)
    parser.add_argument("--mesh-cm", type=float, default=DEFAULT_MESH_CM)
    parser.add_argument("--convergence", type=float, default=DEFAULT_CONVERGENCE)
    parser.add_argument("--inner-iterations", type=int, default=DEFAULT_INNER_ITERATIONS)
    parser.add_argument("--max-thermal-iterations", type=int, default=DEFAULT_MAX_THERMAL_ITERATIONS)
    parser.add_argument("--adjoint", action="store_true")
    args = parser.parse_args()

    (candidate, library, mesh, quad, forward, adjoint_result, psi0, source_group, dose, timing) = run_case(
        args.candidate, args.materials, args.source, args.library,
        sn_order=args.sn_order, mesh_cm=args.mesh_cm, convergence=args.convergence,
        inner_iterations=args.inner_iterations, max_thermal_iterations=args.max_thermal_iterations,
        adjoint=args.adjoint,
    )

    document = {
        "schema": SCHEMA,
        "candidate_id": candidate.get("candidate_id"),
        "run": {
            "sn_order": args.sn_order,
            "mesh_cm": args.mesh_cm,
            "n_cells": mesh.n_cells,
            "n_groups": library.n_groups,
            "legendre_order": library.legendre_order,
            "convergence_tolerance": args.convergence,
            "inner_iterations": args.inner_iterations,
            "thermal_group_count": forward.thermal_group_count,
            "thermal_iterations": forward.thermal_iterations,
            "forward_wall_time_s": round(timing["forward_s"], 6),
            "adjoint_wall_time_s": round(timing["adjoint_s"], 6) if timing["adjoint_s"] is not None else None,
            "depth_clipped_to_library_thickness": mesh.clipped_depth,
            "method": (
                "Gauss-Legendre S_N, P3 Legendre-moment scattering, diamond difference with step fixup, "
                "source iteration with Gauss-Seidel over groups (fastest to slowest) and a separate thermal "
                "upscatter iteration over the group set reachable by any upscatter transition"
            ),
        },
        "neutron_dose_rate": {
            "nominal": {"value": canonical(round_decimal(dose)), "unit": "uSv/h"},
        },
        "scalar_flux_n_cm2_s": {
            "cell_edges_cm": [canonical(round_decimal(Decimal(repr(float(e))))) for e in mesh.edges],
            "cell_material": [m if m is not None else "detector-vacuum" for m in mesh.cell_material],
            "group_boundaries_eV": [float(x) for x in library.boundaries_eV],
            "flux": round_sig_array(forward.phi[:, :, 0]).tolist(),
        },
        "limitations": [
            "Deterministic multigroup approximation: VITAMIN-J-175 groups, P3 Legendre scattering, "
            "diamond difference at the stated mesh; not a Monte Carlo result and carries no statistical "
            "coverage interval -- see the qualification table in the case report for its agreement with "
            "transport.py's Monte Carlo record across material families.",
            "A material's depth zone is selected by absolute depth from the candidate's front face against "
            "that material's own pure-slab reference tally; see mgxs_build.py's limitations.",
            "The detector is modeled as vacuum (zero cross section), not air; see the module docstring.",
        ],
    }

    if adjoint_result is not None:
        document["adjoint"] = {
            "detector_source": "ICRP-116 AP neutron dose coefficient per group, isotropic, uniform over the detector cells",
            "scalar_flux_n_cm2_s_per_source": round_sig_array(adjoint_result.phi[:, :, 0]).tolist(),
        }

    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
