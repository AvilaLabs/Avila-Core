#!/usr/bin/env python3
"""Unit tests for slab_sn.py, and for mgxs_build.py's rounding.

Most of this runs under the plain system interpreter -- slab_sn.py only
imports h5py inside Library.__init__ and mgxs_build.py only imports
openmc/h5py inside main(), so tests that build a synthetic Library via
Library.__new__ (bypassing file I/O) or call mgxs_build's rounding
functions directly need neither, only numpy:

    /usr/bin/python3 -m unittest test_slab_sn -v

TestOneGroupVsOpenMC and TestLibraryBuildByteStability need OpenMC (the
former as an independent Monte Carlo cross-check on the exact one-group
problem being verified, the latter to actually run mgxs_build.py); both are
skipped automatically when openmc is not importable, so a full run needs the
venv python:

    ulimit -v 12000000
    "$OPENMC_PYTHON" -m unittest test_slab_sn -v
"""

import math
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import slab_sn  # noqa: E402

try:
    import openmc  # noqa: F401
    HAVE_OPENMC = True
except ImportError:
    HAVE_OPENMC = False

HERE = os.path.dirname(os.path.abspath(__file__))


def make_library(n_groups, legendre_order, thickness, total, scatter, dose_response=None):
    """A single-material, single-zone synthetic Library, built directly
    (Library.__new__ bypasses __init__'s HDF5 load, so no file and no h5py
    import is needed) -- total: (n_groups,), scatter: (n_groups, n_groups,
    legendre_order+1) [in_group, out_group, moment], both already in the
    ascending-energy convention slab_sn.py and mgxs_build.py use."""
    lib = slab_sn.Library.__new__(slab_sn.Library)
    lib.n_groups = n_groups
    lib.legendre_order = legendre_order
    lib.slab_thickness_cm = thickness
    lib.boundaries_eV = np.exp(np.linspace(np.log(1e-5), np.log(2.0e7), n_groups + 1))
    lib.dose_response = np.ones(n_groups) if dose_response is None else dose_response
    lib.materials = {
        "test-material": {
            "zone_bounds_cm": np.array([0.0, thickness]),
            "total": total.reshape(1, n_groups),
            "scatter": scatter.reshape(1, n_groups, n_groups, legendre_order + 1),
        }
    }
    return lib


def solve_slab(lib, thickness, mesh_cm, sn_order, source_group, psi0, **kwargs):
    candidate_layers = [{"material": "test-material", "thickness_cm": str(thickness)}]
    mesh = slab_sn.Mesh(candidate_layers, lib, detector_thickness_cm=1.0, mesh_cm=mesh_cm)
    quad = slab_sn.Quadrature(sn_order, lib.legendre_order)
    result = slab_sn.solve(mesh, lib, quad, source_group, psi0, lib.legendre_order, **kwargs)
    return mesh, quad, result


class TestPureAbsorber(unittest.TestCase):
    """A pure-absorber slab (no scattering at all) against the analytic
    exponential: each ordinate mu_n > 0 attenuates independently and
    exactly as psi0 * exp(-sigma_t * L / mu_n) (mu * dpsi/dx = -sigma_t*psi
    has no other solution with no scattering source), so the analytic
    detector scalar flux is the same S16 quadrature sum the solver itself
    uses to reconstruct phi_0, just with the exact exponential in place of
    the diamond-difference approximation to it."""

    def _run(self, mesh_cm, sn_order=16):
        n_groups, legendre_order = 1, 3
        sigma_t = 0.8
        L = 6.0
        psi0 = 3.0
        total = np.array([sigma_t])
        scatter = np.zeros((n_groups, n_groups, legendre_order + 1))
        lib = make_library(n_groups, legendre_order, L, total, scatter)
        mesh, quad, result = solve_slab(
            lib, L, mesh_cm, sn_order, source_group=0, psi0=psi0,
            convergence=1e-12, inner_iterations=1, max_thermal_iterations=1,
        )
        detector_flux = result.phi[mesh.is_detector, 0, 0].mean()
        analytic = psi0 * sum(w * math.exp(-sigma_t * L / mu) for mu, w in zip(quad.mu, quad.w) if mu > 0)
        return detector_flux, analytic

    def test_exponential_attenuation(self):
        sn_flux, analytic = self._run(mesh_cm=0.02)
        rel_err = abs(sn_flux - analytic) / analytic
        self.assertLess(rel_err, 2e-3, f"Sn={sn_flux} analytic={analytic} rel_err={rel_err}")

    def test_discretization_error_shrinks_with_finer_mesh(self):
        _, analytic = self._run(mesh_cm=0.5)
        coarse_flux, _ = self._run(mesh_cm=0.5)
        fine_flux, _ = self._run(mesh_cm=0.05)
        coarse_err = abs(coarse_flux - analytic) / analytic
        fine_err = abs(fine_flux - analytic) / analytic
        self.assertLess(fine_err, coarse_err, f"coarse_err={coarse_err} fine_err={fine_err}")


class TestAdjointForwardReciprocity(unittest.TestCase):
    """R = sum_g h_g * phi_g(detector) [forward] must equal
    psi0 * sum_{mu_n>0} w_n*mu_n*psi*_source_group(0,mu_n) [adjoint], the
    standard boundary-source reciprocity identity for a problem with no
    interior forward source (see slab_sn.py's module docstring for the
    derivation of psi*(0,mu>0) = chi(0,-mu) that makes the right side
    computable directly from solve(adjoint=True)'s boundary_front). Uses 3
    groups with both up- and down-scatter and nonzero P1 so the transposed
    scattering contraction slab_sn.solve uses for adjoint=True is actually
    exercised, not just a degenerate 1-group case.
    """

    def test_reciprocity(self):
        n_groups, legendre_order = 3, 3
        L = 4.0
        psi0 = 2.0
        source_group = n_groups - 1  # fastest group, ascending-energy convention
        total = np.array([0.9, 0.7, 0.5])
        scatter = np.zeros((n_groups, n_groups, legendre_order + 1))
        # P0: down/self/up-scatter among all three groups (ascending-energy: g=0 slowest, g=2 fastest)
        scatter[2, 2, 0], scatter[2, 1, 0], scatter[2, 0, 0] = 0.10, 0.25, 0.05
        scatter[1, 1, 0], scatter[1, 0, 0], scatter[1, 2, 0] = 0.15, 0.20, 0.02
        scatter[0, 0, 0], scatter[0, 1, 0] = 0.30, 0.05
        # a little P1 anisotropy so it's exercised too
        scatter[2, 2, 1], scatter[1, 1, 1], scatter[0, 0, 1] = 0.02, 0.03, 0.01
        dose_response = np.array([1.3, 0.7, 2.1])  # distinct per-group weights, not all-ones
        lib = make_library(n_groups, legendre_order, L, total, scatter, dose_response=dose_response)

        mesh, quad, forward = solve_slab(
            lib, L, mesh_cm=0.1, sn_order=16, source_group=source_group, psi0=psi0,
            convergence=1e-10, inner_iterations=6, max_thermal_iterations=100,
        )
        detector_flux = forward.phi[mesh.is_detector, :, 0].mean(axis=0)
        R_forward = float(np.dot(detector_flux, lib.dose_response))

        candidate_layers = [{"material": "test-material", "thickness_cm": str(L)}]
        adjoint = slab_sn.solve(
            mesh, lib, quad, source_group, 0.0, legendre_order,
            convergence=1e-10, inner_iterations=6, max_thermal_iterations=100,
            adjoint=True, detector_source=lib.dose_response,
        )
        R_adjoint = slab_sn.reciprocity_response(quad, adjoint, psi0, source_group)

        self.assertGreater(abs(R_forward), 0.0)
        rel_diff = abs(R_forward - R_adjoint) / abs(R_forward)
        self.assertLess(rel_diff, 1e-3, f"R_forward={R_forward} R_adjoint={R_adjoint} rel_diff={rel_diff}")


@unittest.skipUnless(HAVE_OPENMC, "needs the OpenMC venv python (independent Monte Carlo cross-check)")
class TestOneGroupVsOpenMC(unittest.TestCase):
    """A one-group isotropic-scattering slab against an independently
    computed benchmark: OpenMC's own multi-group Monte Carlo mode
    (`energy_mode='multi-group'`), given the exact same one-group cross
    sections and the exact same "isotropic in the incoming hemisphere"
    boundary condition slab_sn.py implements (psi(0,mu) = psi0, constant,
    for mu in (0,1]), solving the identical problem by a completely
    different method (Monte Carlo vs. deterministic S_N).

    Getting the MC source to actually reproduce psi(0,mu) = const took two
    corrections worth recording, since both are easy to get backwards:

    1. OpenMC's multi-group mode segfaults on a truly void (unfilled) cell
       (confirmed directly: the same geometry with void gap/detector cells
       crashes mid-batch-1); an explicit all-zero-cross-section material
       avoids it, used here for the detector region.
    2. Sampling the incident direction cosine mu UNIFORMLY on [0,1] does
       NOT reproduce an isotropic angular flux boundary condition -- it
       reproduces a source that is isotropic in particle *count*, which
       corresponds to psi(mu) ~ 1/mu (verified directly: it gives a pure-
       absorber detector flux about 42% below the analytic exponential,
       consistently, not noise). An isotropic angular *flux* needs the
       incident current in dmu, mu*psi0*dmu, to be what's uniform, i.e.
       mu needs to be sampled from the density p(mu) = 2*mu on [0,1]
       (`openmc.stats.PowerLaw(0, 1, 1)`) -- the same mu-weighting that
       makes a Lambertian surface's *emission* look isotropic to a distant
       observer. With that fix the pure-absorber case matches the
       validated analytic exponential to 0.1%, and this test's full
       scattering case matches OpenMC to 0.2%.

    The source sits directly at the slab's front face (x=epsilon, not a
    separate vacuum gap): psi0 is the boundary condition itself here, not
    something derived from a further upstream physical source, so there is
    nothing upstream to model. Source rate: current J0 = integral_0^1
    mu*psi0 dmu = psi0/2, and since every sampled direction has mu>0 (no
    loss), the MC particle rate that reproduces current J0 over area A is
    just strength_mc = J0*A = psi0*A/2.
    """

    def test_against_openmc_multigroup_mc(self):
        sigma_t = 1.0
        c = 0.7
        sigma_s = c * sigma_t
        sigma_a = sigma_t - sigma_s
        L = 5.0
        lateral = 100.0
        half = lateral / 2.0
        detector_t = 1.0
        area = lateral * lateral
        psi0 = 5.0e-4
        strength_mc = (psi0 / 2.0) * area
        eps = 1.0e-4  # off the x=0 boundary; sampling exactly on it starves OpenMC's source-rejection check

        workdir = tempfile.mkdtemp(prefix="test_onegroup_")
        cwd = os.getcwd()
        try:
            os.chdir(workdir)
            eg = openmc.mgxs.EnergyGroups(group_edges=[1e-5, 2.0e7])
            xs = openmc.XSdata("absorber-scatterer", eg, representation="isotropic")
            xs.order = 0
            xs.set_total(np.array([sigma_t]))
            xs.set_absorption(np.array([sigma_a]))
            xs.set_scatter_matrix(np.array([[[sigma_s]]]))
            xs_void = openmc.XSdata("void", eg, representation="isotropic")
            xs_void.order = 0
            xs_void.set_total(np.array([0.0]))
            xs_void.set_absorption(np.array([0.0]))
            xs_void.set_scatter_matrix(np.array([[[0.0]]]))
            mg_lib = openmc.MGXSLibrary(eg)
            mg_lib.add_xsdatas([xs, xs_void])
            mg_lib.export_to_hdf5("mgxs.h5")

            mat = openmc.Material(name="absorber-scatterer")
            mat.set_density("macro", 1.0)
            mat.add_macroscopic("absorber-scatterer")
            mat_void = openmc.Material(name="void")
            mat_void.set_density("macro", 1.0)
            mat_void.add_macroscopic("void")
            materials = openmc.Materials([mat, mat_void])
            materials.cross_sections = "mgxs.h5"
            materials.export_to_xml()

            ymin = openmc.YPlane(-half, boundary_type="reflective")
            ymax = openmc.YPlane(half, boundary_type="reflective")
            zmin = openmc.ZPlane(-half, boundary_type="reflective")
            zmax = openmc.ZPlane(half, boundary_type="reflective")
            lateral_region = +ymin & -ymax & +zmin & -zmax
            p0 = openmc.XPlane(0.0, boundary_type="vacuum")
            pL = openmc.XPlane(L)
            pDet = openmc.XPlane(L + detector_t, boundary_type="vacuum")
            slab = openmc.Cell(fill=mat, region=+p0 & -pL & lateral_region)
            detector = openmc.Cell(fill=mat_void, region=+pL & -pDet & lateral_region)
            openmc.Geometry([slab, detector]).export_to_xml()

            src = openmc.IndependentSource()
            src.space = openmc.stats.Box((eps, -half, -half), (eps, half, half))
            src.angle = openmc.stats.PolarAzimuthal(
                mu=openmc.stats.PowerLaw(0.0, 1.0, 1.0),
                phi=openmc.stats.Uniform(0.0, 2.0 * math.pi),
                reference_uvw=(1.0, 0.0, 0.0),
                reference_vwu=(0.0, 1.0, 0.0),
            )
            src.energy = openmc.stats.Discrete([1.0e6], [1.0])
            settings = openmc.Settings()
            settings.energy_mode = "multi-group"
            settings.run_mode = "fixed source"
            settings.source = src
            settings.particles = 4_000_000
            settings.batches = 10
            settings.seed = 1
            settings.output = {"summary": False, "tallies": False}
            settings.export_to_xml()

            tally = openmc.Tally(name="detector-flux")
            tally.filters = [openmc.CellFilter(detector)]
            tally.scores = ["flux"]
            tally.estimator = "tracklength"
            openmc.Tallies([tally]).export_to_xml()

            executable = os.path.join(os.path.dirname(sys.executable), "openmc")
            threads = int(os.environ.get("OMP_NUM_THREADS", "0")) or None
            openmc.run(openmc_exec=executable, output=False, threads=threads)

            sp = openmc.StatePoint(f"statepoint.{settings.batches}.h5")
            t = sp.get_tally(name="detector-flux")
            mean = float(t.mean.ravel()[0])
            std = float(t.std_dev.ravel()[0])
            volume = lateral * lateral * detector_t
            mc_flux = (mean / volume) * strength_mc
            mc_flux_std = (std / volume) * strength_mc
        finally:
            os.chdir(cwd)
            shutil.rmtree(workdir, ignore_errors=True)

        legendre_order = 3
        total = np.array([sigma_t])
        scatter = np.zeros((1, 1, legendre_order + 1))
        scatter[0, 0, 0] = sigma_s
        lib = make_library(1, legendre_order, L, total, scatter)
        # A single-group problem forces ALL scattering to be "self-scatter within
        # the one group", so inner_iterations alone (not cross-group Gauss-Seidel,
        # which has nothing to do with 1 group) has to converge it -- verified
        # separately that this c=0.7, 5-mean-free-path case needs ~20 iterations
        # to stabilize; realistic 175-group candidates converge far faster per
        # group since most scattering moves a neutron to a different group
        # entirely, not back into the same narrow one (see the case report's
        # discretization/convergence notes).
        mesh, quad, result = solve_slab(
            lib, L, mesh_cm=0.05, sn_order=16, source_group=0, psi0=psi0,
            convergence=1e-10, inner_iterations=60, max_thermal_iterations=5,
        )
        sn_flux = result.phi[mesh.is_detector, 0, 0].mean()

        rel_diff = abs(sn_flux - mc_flux) / mc_flux
        # 3-sigma MC band plus a small deterministic-discretization allowance
        tolerance = 3.0 * (mc_flux_std / mc_flux) + 0.01
        self.assertLess(
            rel_diff, tolerance,
            f"Sn={sn_flux:.6e} MC={mc_flux:.6e}+-{mc_flux_std:.2e} rel_diff={rel_diff:.4f} tolerance={tolerance:.4f}",
        )


class TestLibraryRounding(unittest.TestCase):
    """mgxs_build.py's rounding functions -- the mechanism that makes the
    library byte-stable across two runs at the same seed (verified as a
    full-pipeline sha256 comparison in the case report; this is the fast
    unit-level check of the rounding itself, needing neither OpenMC nor
    h5py since mgxs_build.py only imports either inside main())."""

    @classmethod
    def setUpClass(cls):
        import importlib

        cls.mgxs_build = importlib.import_module("mgxs_build")

    def test_stable_collapses_thread_order_noise(self):
        base = 1.234567891234
        noisy = base + 3e-13  # perturbation far below the 8th significant digit
        self.assertEqual(self.mgxs_build.stable(base), self.mgxs_build.stable(noisy))

    def test_stable_eight_significant_digits(self):
        value = float(self.mgxs_build.stable(123456789.123))
        self.assertEqual(value, 123456790.0)

    def test_round_sig_array_matches_scalar_stable(self):
        rng = np.random.default_rng(0)
        values = rng.uniform(-1000.0, 1000.0, size=500) * 10.0 ** rng.integers(-8, 8, size=500)
        array_rounded = self.mgxs_build.round_sig_array(values)
        for v, r in zip(values, array_rounded):
            expected = float(self.mgxs_build.stable(float(v)))
            self.assertAlmostEqual(r, expected, delta=abs(expected) * 1e-9 + 1e-300)

    def test_round_sig_array_deterministic(self):
        rng = np.random.default_rng(1)
        values = rng.uniform(-1.0, 1.0, size=5000)
        a = self.mgxs_build.round_sig_array(values)
        b = self.mgxs_build.round_sig_array(values.copy())
        self.assertTrue(np.array_equal(a, b))

    def test_round_sig_array_thread_noise_collapses(self):
        rng = np.random.default_rng(2)
        values = rng.uniform(1.0, 1000.0, size=2000)
        noisy = values * (1.0 + rng.uniform(-1e-13, 1e-13, size=2000))
        a = self.mgxs_build.round_sig_array(values)
        b = self.mgxs_build.round_sig_array(noisy)
        self.assertTrue(np.array_equal(a, b))


@unittest.skipUnless(HAVE_OPENMC, "needs the OpenMC venv python to actually run mgxs_build.py")
class TestLibraryBuildByteStability(unittest.TestCase):
    """Two small, fast mgxs_build.py runs (one material, few particles) at
    the same seed must produce byte-identical HDF5 files -- the same
    property verified at full production scale (all 6 materials, 5,000,000
    histories each) in the case report via sha256, reproduced here quickly
    enough to run as part of the routine test suite."""

    def test_same_seed_byte_identical(self):
        materials_path = os.path.join(HERE, "materials.json")
        source_path = os.path.join(HERE, "source.json")
        cross_sections_index = os.environ.get("OPENMC_CROSS_SECTIONS")
        if not cross_sections_index or not os.path.isfile(cross_sections_index):
            self.skipTest("OPENMC_CROSS_SECTIONS not set to a readable index")

        workdir = tempfile.mkdtemp(prefix="test_libbuild_")
        try:
            outputs = []
            for label in ("a", "b"):
                run_dir = os.path.join(workdir, label)
                os.makedirs(run_dir, exist_ok=True)
                output = os.path.join(run_dir, "lib.h5")
                index = os.path.join(run_dir, "lib-index.json")
                cmd = [
                    sys.executable, os.path.join(HERE, "mgxs_build.py"),
                    "--materials", materials_path, "--source", source_path,
                    "--cross-sections-index", cross_sections_index,
                    "--particles", "3000", "--batches", "3", "--seed", "7",
                    "--only-materials", "iron",
                    "--output", output, "--output-index", index,
                ]
                env = dict(os.environ)
                env["HOME"] = run_dir
                subprocess.run(cmd, cwd=run_dir, env=env, check=True, capture_output=True, timeout=300)
                outputs.append(output)

            import hashlib

            def sha256_of(path):
                digest = hashlib.sha256()
                with open(path, "rb") as handle:
                    for chunk in iter(lambda: handle.read(1 << 20), b""):
                        digest.update(chunk)
                return digest.hexdigest()

            self.assertEqual(sha256_of(outputs[0]), sha256_of(outputs[1]))
        finally:
            shutil.rmtree(workdir, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
