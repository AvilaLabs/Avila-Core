#!/usr/bin/env python3
"""Build a multigroup neutron cross-section library for the slab S_N solver
(`slab_sn.py`) from continuous-energy OpenMC fixed-source runs, one per
material in `materials.json`.

For each material: a 120 cm slab of that material alone, struck by the same
plane isotropic 14.1 MeV source and reflective-lateral / vacuum-front-and-
back geometry convention `transport.py` uses (a void gap from x=-1 to x=0
with the source at x=-0.5, so half the isotropically emitted source escapes
backward through the vacuum boundary and the surviving half enters the slab
isotropically over the incoming hemisphere -- see `slab_sn.py` for the
boundary-condition algebra this produces). `openmc.mgxs.Library` tallies
flux-weighted total, absorption, and P3 Legendre scattering-matrix cross
sections in the VITAMIN-J-175 group structure.

Whole-slab-averaged constants are not what gets shipped: a 100+ cm
hydrogenous shield hardens (thermalizes) enormously with depth -- the flux
entering a layer 90 cm into polyethylene is nothing like the flux entering
the front face -- so this script tallies four 30 cm depth zones per material
and `slab_sn.py` selects the zone that covers a given cell's depth from the
candidate's front face. `--zones 1` reproduces the whole-slab-averaged
alternative (single 0-120 cm domain) for the comparison this choice owes;
the default and the shipped `mgxs-vitamin-j-175.h5` use 4.

Output: an HDF5 file (default `mgxs-vitamin-j-175.h5`) with, per material,
the group structure, a group-wise ICRP-116 AP neutron dose-response
constant, and per-zone total/absorption/scatter-matrix arrays; and a sidecar
JSON recording OpenMC version, cross-section index digest, seed, particles,
batches, group boundaries, and per-material provenance. Every constant is
rounded to 8 significant digits before it is written, exactly as
`transport.py` rounds its tallies: the discarded digits carry thread-
reduction-order noise, not information, and rounding them away is what
makes the file byte-identical across two runs at the same seed (see
`test_slab_sn.py`'s `test_library_byte_stability`).
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

SCHEMA = "avila.shielding/mgxs-library/v1"
GROUP_STRUCTURE_NAME = "VITAMIN-J-175"
N_GROUPS = 175
SLAB_THICKNESS_CM = 120.0
LEGENDRE_ORDER = 3
DEFAULT_ZONES = 4
DOSE_TABLE_SUBSAMPLE = 400  # log-spaced sub-samples per group for the 1/E-weighted dose-response collapse


# --- rounding helpers, mirroring transport.py's stable()/canonical() convention ---

def sha256_of(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def stable(value: float) -> Decimal:
    """Round a raw MC tally statistic to SIGNIFICANT_DIGITS significant
    digits via its shortest round-tripping decimal representation. See
    transport.py's `stable()`: this removes thread-reduction-order noise
    (far below the statistical uncertainty) so the same seed reproduces the
    same bytes. Used for the small scalar arrays (total, absorption, dose
    response): a handful of Decimal ops per material is cheap.
    """
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


def stable_array(values: np.ndarray) -> np.ndarray:
    """Elementwise `stable()` for a small array, returned as float64."""
    flat = values.ravel()
    out = np.empty(flat.shape, dtype=np.float64)
    for i, v in enumerate(flat):
        out[i] = float(stable(float(v)))
    return out.reshape(values.shape)


def round_sig_array(values: np.ndarray, digits: int = SIGNIFICANT_DIGITS) -> np.ndarray:
    """Vectorized round-to-`digits`-significant-figures for the large
    scattering-matrix arrays (up to 175*175*4 = 122500 elements per zone per
    material -- a few million elements total across the library). Same
    purpose as `stable()` above (discard thread-noise digits so the same
    seed gives the same bytes) implemented with numpy log10/round instead of
    per-element Decimal, because a Python Decimal loop over that many
    elements is needlessly slow; the two differ only in the rare case where
    a value sits almost exactly on a rounding-boundary tie, which is no
    likelier to matter here than it is for transport.py's own Decimal
    rounding at a boundary tie.
    """
    arr = np.asarray(values, dtype=np.float64)
    out = np.zeros_like(arr)
    nz = arr != 0
    if np.any(nz):
        exponent = np.floor(np.log10(np.abs(arr[nz])))
        factor = 10.0 ** (digits - 1 - exponent)
        out[nz] = np.round(arr[nz] * factor) / factor
    return out


def build_zone_geometry(openmc, material, half, n_zones, zone_thickness):
    """The reference slab: void gap (vacuum front boundary at x=-1, source
    at x=-0.5) then `n_zones` depth zones of `material`, each
    `zone_thickness` cm, reflective in y/z at +-half, vacuum behind."""
    x_gap = openmc.XPlane(-1.0, boundary_type="vacuum")
    ymin = openmc.YPlane(-half, boundary_type="reflective")
    ymax = openmc.YPlane(half, boundary_type="reflective")
    zmin = openmc.ZPlane(-half, boundary_type="reflective")
    zmax = openmc.ZPlane(half, boundary_type="reflective")
    lateral = +ymin & -ymax & +zmin & -zmax
    p0 = openmc.XPlane(0.0)
    gap = openmc.Cell(name="gap", region=+x_gap & -p0 & lateral)

    bounds = [0.0] + [zone_thickness * (i + 1) for i in range(n_zones)]
    planes = [p0] + [openmc.XPlane(b) for b in bounds[1:]]
    zones = []
    for i in range(n_zones):
        cell = openmc.Cell(
            name=f"zone{i}", fill=material, region=+planes[i] & -planes[i + 1] & lateral
        )
        zones.append(cell)
    outer = openmc.XPlane(bounds[-1] + 1.0, boundary_type="vacuum")
    tail = openmc.Cell(name="tail", region=+planes[-1] & -outer & lateral)

    geometry = openmc.Geometry([gap] + zones + [tail])
    return geometry, zones, bounds


def collapse_dose_response(energy_eV, dose_pSv_cm2, boundaries_eV):
    """Group-wise ICRP-116 AP neutron dose-response constant: a flat-in-
    lethargy (1/E) weighted average of the tabulated dose coefficient over
    each group,

        h_g = [ integral_{Elo}^{Ehi} h(E)/E dE ] / [ integral_{Elo}^{Ehi} dE/E ]

    i.e. every decade of energy within the group contributes equally,
    computed by trapezoidal integration over a log-spaced sub-grid with h(E)
    interpolated log-log against the tabulated (energy_eV, dose_pSv_cm2)
    points from `openmc.data.dose_coefficients`. 1/E is the standard default
    weighting for collapsing a response or cross section onto a group
    structure when no local spectrum is available at library-build time (the
    slowing-down flux in a moderator is itself close to 1/E, so this is also
    a physically reasonable proxy). The table starts at 0.001 eV, above
    VITAMIN-J-175's lower edge (1e-5 eV); h(E) is held flat (constant, equal
    to h(0.001 eV)) below the table's minimum, since ICRP-116's AP neutron
    coefficient is essentially flat approaching thermal energies from below.
    Returns an array of N_GROUPS values in ascending-energy order (group 0 =
    lowest energy), matching `boundaries_eV`.
    """
    log_e_tab = np.log(energy_eV)
    log_h_tab = np.log(dose_pSv_cm2)
    e_min_tab = energy_eV[0]

    def h_of(e):
        e_clamped = np.maximum(e, e_min_tab)
        return np.exp(np.interp(np.log(e_clamped), log_e_tab, log_h_tab))

    n_groups = len(boundaries_eV) - 1
    out = np.empty(n_groups, dtype=np.float64)
    for g in range(n_groups):
        e_lo, e_hi = boundaries_eV[g], boundaries_eV[g + 1]
        log_grid = np.linspace(np.log(e_lo), np.log(e_hi), DOSE_TABLE_SUBSAMPLE)
        e_grid = np.exp(log_grid)
        h_grid = h_of(e_grid)
        # integral of h(E)/E dE over log E is integral of h d(logE); denominator (integral of dE/E) is just the log-width
        numerator = np.trapezoid(h_grid, log_grid)
        denominator = log_grid[-1] - log_grid[0]
        out[g] = numerator / denominator
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--materials", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--cross-sections-index", required=True)
    parser.add_argument("--particles", type=int, default=250_000)
    parser.add_argument("--batches", type=int, default=20)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--zones", type=int, default=DEFAULT_ZONES, help="depth zones per material (default 4); 1 reproduces the whole-slab-averaged alternative")
    parser.add_argument("--output", required=True)
    parser.add_argument("--output-index", required=True)
    parser.add_argument("--only-materials", default=None, help="comma-separated subset of material names, for development iteration only")
    parser.add_argument("--threads", type=int, default=None)
    args = parser.parse_args()

    library_env = os.environ.get("OPENMC_CROSS_SECTIONS")
    if not library_env:
        raise SystemExit("OPENMC_CROSS_SECTIONS is not set")
    staged_digest = sha256_of(args.cross_sections_index)
    library_digest = sha256_of(library_env)
    if staged_digest != library_digest:
        raise SystemExit(
            f"the data library in use ({library_digest}) is not the attested index ({staged_digest})"
        )

    import openmc  # noqa: E402
    import openmc.mgxs as mgxs  # noqa: E402
    import openmc.data  # noqa: E402

    with open(args.materials, encoding="utf-8") as handle:
        table = json.load(handle)["materials"]
    with open(args.source, encoding="utf-8") as handle:
        source = json.load(handle)

    lateral = float(source["lateral_cm"])
    half = lateral / 2.0
    energy_eV = float(source["energy_MeV"]) * 1.0e6

    n_zones = args.zones
    zone_thickness = SLAB_THICKNESS_CM / n_zones

    groups = mgxs.EnergyGroups(group_edges=mgxs.GROUP_STRUCTURES[GROUP_STRUCTURE_NAME])
    boundaries_eV = np.asarray(groups.group_edges, dtype=np.float64)
    if len(boundaries_eV) - 1 != N_GROUPS:
        raise SystemExit(f"expected {N_GROUPS} groups from {GROUP_STRUCTURE_NAME}, got {len(boundaries_eV) - 1}")

    n_energies, n_coefficients = openmc.data.dose_coefficients("neutron", geometry="AP")
    dose_response = collapse_dose_response(np.asarray(n_energies), np.asarray(n_coefficients), boundaries_eV)
    dose_response_rounded = stable_array(dose_response)

    material_names = list(table.keys())
    if args.only_materials:
        wanted = set(args.only_materials.split(","))
        material_names = [m for m in material_names if m in wanted]

    executable = os.path.join(os.path.dirname(sys.executable), "openmc")
    threads = args.threads or (int(os.environ.get("OMP_NUM_THREADS", "0")) or None)

    materials_out = {}
    provenance = {}
    run_dir = os.getcwd()

    for name in material_names:
        spec = table[name]
        work = f"mgxs-{name}"
        os.makedirs(work, exist_ok=True)
        os.chdir(work)
        try:
            material = openmc.Material(name=name)
            material.set_density("g/cm3", float(spec["density_g_cm3"]))
            basis = spec["composition"]["basis"]
            for element, fraction in spec["composition"]["elements"].items():
                material.add_element(element, float(fraction), basis)

            geometry, zones, zone_bounds = build_zone_geometry(openmc, material, half, n_zones, zone_thickness)
            geometry.export_to_xml()
            openmc.Materials([material]).export_to_xml()

            src = openmc.IndependentSource()
            src.space = openmc.stats.Box((-0.5, -half, -half), (-0.5, half, half))
            src.angle = openmc.stats.Isotropic()
            src.energy = openmc.stats.Discrete([energy_eV], [1.0])

            settings = openmc.Settings()
            settings.run_mode = "fixed source"
            settings.source = src
            settings.particles = args.particles
            settings.batches = args.batches
            settings.seed = args.seed
            settings.output = {"summary": True, "tallies": False}
            settings.photon_transport = False
            settings.export_to_xml()

            library = mgxs.Library(geometry)
            library.energy_groups = groups
            library.mgxs_types = ["total", "absorption", "scatter matrix"]
            library.domain_type = "cell"
            library.domains = zones
            library.legendre_order = LEGENDRE_ORDER
            library.correction = None
            library.scatter_format = "legendre"
            library.by_nuclide = False
            library.build_library()

            tallies = openmc.Tallies()
            library.add_to_tallies(tallies, merge=True)
            tallies.export_to_xml()

            t0 = time.time()
            openmc.run(openmc_exec=executable, output=False, threads=threads)
            wall_time_s = time.time() - t0

            statepoint = openmc.StatePoint(f"statepoint.{args.batches}.h5")
            library.load_from_statepoint(statepoint)

            zone_total = []
            zone_absorption = []
            zone_scatter = []
            zone_stats = []
            flux_sum = np.zeros(N_GROUPS)
            total_rate_sum = np.zeros(N_GROUPS)
            absorption_rate_sum = np.zeros(N_GROUPS)
            for zone in zones:
                total_mgxs = library.get_mgxs(zone, "total")
                abs_mgxs = library.get_mgxs(zone, "absorption")
                scat_mgxs = library.get_mgxs(zone, "scatter matrix")

                total_xs = np.nan_to_num(total_mgxs.get_xs(order_groups="decreasing"), nan=0.0)
                abs_xs = np.nan_to_num(abs_mgxs.get_xs(order_groups="decreasing"), nan=0.0)
                scat_xs = np.nan_to_num(
                    scat_mgxs.get_xs(order_groups="decreasing", row_column="inout", moment="all"), nan=0.0
                )
                zone_total.append(round_sig_array(total_xs))
                zone_absorption.append(round_sig_array(abs_xs))
                zone_scatter.append(round_sig_array(scat_xs))

                flux_vals = total_mgxs.tallies["flux"].get_values(scores=["flux"]).ravel()
                total_rate_vals = total_mgxs.tallies["total"].get_values(scores=["total"]).ravel()
                abs_rate_vals = abs_mgxs.tallies["absorption"].get_values(scores=["absorption"]).ravel()
                flux_sum += flux_vals
                total_rate_sum += total_rate_vals
                absorption_rate_sum += abs_rate_vals

                nonzero = total_xs > 0
                rel_err = np.nan_to_num(total_mgxs.get_xs(order_groups="decreasing", value="rel_err"), nan=0.0)
                zone_stats.append(
                    {
                        "nonzero_groups": int(np.count_nonzero(nonzero)),
                        "median_rel_err_nonzero": float(stable(float(np.median(rel_err[nonzero])))) if np.any(nonzero) else None,
                    }
                )

            with np.errstate(divide="ignore", invalid="ignore"):
                whole_total = np.where(flux_sum > 0, total_rate_sum / flux_sum, 0.0)
                whole_absorption = np.where(flux_sum > 0, absorption_rate_sum / flux_sum, 0.0)
            whole_total = round_sig_array(np.nan_to_num(whole_total, nan=0.0))
            whole_absorption = round_sig_array(np.nan_to_num(whole_absorption, nan=0.0))

            materials_out[name] = {
                "zone_bounds_cm": [float(Decimal(repr(b))) for b in zone_bounds],
                "zone_total": np.stack(zone_total),
                "zone_absorption": np.stack(zone_absorption),
                "zone_scatter": np.stack(zone_scatter),
                "whole_slab_total": whole_total,
                "whole_slab_absorption": whole_absorption,
            }
            provenance[name] = {
                "density_g_cm3": spec["density_g_cm3"],
                "composition": spec["composition"],
                "wall_time_s": round(wall_time_s, 3),
                "n_zones": n_zones,
                "zone_bounds_cm": zone_bounds,
                "zone_stats": zone_stats,
            }
            print(f"{name}: {wall_time_s:.1f}s", file=sys.stderr)
        finally:
            os.chdir(run_dir)

    import h5py

    with h5py.File(args.output, "w") as h5:
        h5.attrs["schema"] = SCHEMA
        h5.attrs["group_structure_name"] = GROUP_STRUCTURE_NAME
        h5.attrs["n_groups"] = N_GROUPS
        h5.attrs["group_order"] = "ascending_energy"
        h5.attrs["legendre_order"] = LEGENDRE_ORDER
        h5.attrs["scatter_convention"] = (
            "scatter[in_group, out_group, moment] is the raw Legendre-moment macroscopic "
            "scattering cross section sigma_s,l(g'->g) in cm^-1, NOT multiplied by the "
            "(2l+1)/2 prefactor (openmc.mgxs.ScatterMatrixXS.get_xs's own convention); "
            "the (2l+1)/2 * P_l(mu) factor is applied by the S_N sweep, not baked in here."
        )
        h5.attrs["slab_thickness_cm"] = SLAB_THICKNESS_CM
        h5.attrs["n_zones"] = n_zones
        h5.create_dataset("group_boundaries_eV", data=boundaries_eV)
        h5.create_dataset("dose_response_neutron_AP_pSv_cm2", data=dose_response_rounded)
        h5["dose_response_neutron_AP_pSv_cm2"].attrs["averaging"] = (
            "flat-in-lethargy (1/E) weighted average of openmc.data.dose_coefficients('neutron', "
            "geometry='AP') over each group, log-log interpolated, flat below 0.001 eV; see "
            "collapse_dose_response() docstring"
        )
        materials_group = h5.create_group("materials")
        for name, data in materials_out.items():
            g = materials_group.create_group(name)
            g.create_dataset("zone_bounds_cm", data=np.asarray(data["zone_bounds_cm"]))
            g.create_dataset("zone_total_cm_inv", data=data["zone_total"])
            g.create_dataset("zone_absorption_cm_inv", data=data["zone_absorption"])
            g.create_dataset("zone_scatter_cm_inv", data=data["zone_scatter"])
            diag = g.create_group("diagnostics")
            diag.create_dataset("whole_slab_total_cm_inv", data=data["whole_slab_total"])
            diag.create_dataset("whole_slab_absorption_cm_inv", data=data["whole_slab_absorption"])
            diag.attrs["note"] = (
                "flux-weighted condensation of the same 4 zone tallies to a single 0-120 cm domain "
                "(summed numerator/denominator tally means, not an average of ratios); not read by "
                "slab_sn.py, kept for the whole-slab-vs-4-zone comparison in the build report"
            )

    index_doc = {
        "schema": "avila.shielding/mgxs-library-index/v1",
        "library_file": os.path.basename(args.output),
        "openmc_version": openmc.__version__,
        "cross_sections_index_sha256": library_digest,
        "seed": args.seed,
        "particles_per_material": args.particles,
        "batches_per_material": args.batches,
        "total_histories_per_material": args.particles * args.batches,
        "group_structure": GROUP_STRUCTURE_NAME,
        "n_groups": N_GROUPS,
        "group_order": "ascending_energy",
        "legendre_order": LEGENDRE_ORDER,
        "n_zones": n_zones,
        "slab_thickness_cm": SLAB_THICKNESS_CM,
        "significant_digits": SIGNIFICANT_DIGITS,
        "rounding": (
            "every stored constant is rounded to 8 significant digits before it is written: "
            "total/absorption/dose-response scalars via Decimal(repr(value)) quantization "
            "(transport.py's stable()); the scattering-matrix arrays via an equivalent vectorized "
            "numpy log10/round (see round_sig_array()). This removes MC thread-reduction-order "
            "noise, not physical precision -- the statistical uncertainty is far larger than the "
            "8th significant digit -- so the file is byte-identical across two runs at the same seed."
        ),
        "materials": provenance,
        "limitations": [
            "Each material's zone constants come from a homogeneous 120 cm slab of that material "
            "alone, source-side out; a material's depth zone is selected by the candidate's own "
            "depth from its front face, not by that material's depth within its own contiguous run "
            "of layers, so a layer preceded by a different material picks up the spectral history "
            "of a pure block of its own material at that absolute depth, not the true upstream "
            "history. This is a documented approximation, not a certified spectral-zoning scheme.",
            "Whole-slab-condensed constants (materials/<name>/diagnostics) are derived from the same "
            "four zone tallies by summing raw reaction-rate and flux tally means before dividing, "
            "not from a separate run; they are not used by slab_sn.py.",
            "Statistical uncertainty on each constant is set by particles_per_material x "
            "batches_per_material and is not itself reported per-value in this file (see the "
            "zone_stats provenance for aggregate nonzero-group counts and median relative error).",
            "No fission, delayed data, or upscatter iteration is implied by this library beyond the "
            "physical thermal upscatter already present in the flux-weighted group constants; "
            "slab_sn.py's thermal upscatter iteration operates on these constants as given.",
        ],
    }
    with open(args.output_index, "w", encoding="utf-8") as handle:
        json.dump(index_doc, handle, indent=2, sort_keys=True)
        handle.write("\n")

    print(f"wrote {args.output} and {args.output_index}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
