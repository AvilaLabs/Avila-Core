#!/usr/bin/env python3
"""Unit checks for the full-matrix passive-support ceiling."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

import numpy as np


MODULE_PATH = Path(__file__).with_name("ncsx_passive_ceiling.py")
SPEC = importlib.util.spec_from_file_location("ncsx_passive_ceiling", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
CEILING = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CEILING
SPEC.loader.exec_module(CEILING)


class PassiveCeilingTests(unittest.TestCase):
    def test_closed_form_is_stationary_and_trace_preserving(self) -> None:
        hessian = np.diag((1.0, 8.0, 27.0, 64.0))
        matrix, values, residual = CEILING.ideal_full_stiffness(hessian)
        np.testing.assert_allclose(matrix, np.diag(values), atol=1.0e-14, rtol=0.0)
        np.testing.assert_allclose(values / values[0], (1.0, 2.0, 3.0, 4.0))
        self.assertAlmostEqual(float(np.trace(matrix)), 4.0, places=13)
        self.assertLess(residual, 1.0e-14)

    def test_closed_form_beats_random_same_trace_stiffness(self) -> None:
        rng = np.random.default_rng(8102)
        transform = rng.normal(size=(5, 5))
        hessian = transform.T @ transform + 0.25 * np.eye(5)
        ideal, _, _ = CEILING.ideal_full_stiffness(hessian)
        ideal_score = CEILING.response_rms(hessian, ideal)
        for _ in range(100):
            basis, _ = np.linalg.qr(rng.normal(size=(5, 5)))
            values = rng.uniform(0.2, 2.0, size=5)
            values *= 5.0 / np.sum(values)
            candidate = (basis * values) @ basis.T
            self.assertGreaterEqual(
                CEILING.response_rms(hessian, candidate), ideal_score - 1.0e-12
            )

    def test_surface_normals_and_weights_are_well_formed(self) -> None:
        coefficients = np.asarray(
            [
                [0.0, 0.0, 3.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.5, 0.0, 0.0, 0.5],
            ]
        )
        model = type("Model", (), {"nfp": 1})()
        resolution = CEILING.Resolution(64, 12, 8)
        points, normals, weights = CEILING.surface(coefficients, model, resolution)
        self.assertEqual(points.shape, (96, 3))
        np.testing.assert_allclose(np.linalg.norm(normals, axis=1), 1.0)
        self.assertAlmostEqual(float(np.sum(weights)), 1.0, places=14)
        self.assertTrue(np.all(weights > 0))


if __name__ == "__main__":
    unittest.main()
