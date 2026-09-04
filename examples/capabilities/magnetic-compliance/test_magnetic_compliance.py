#!/usr/bin/env python3
"""Numerical sanity checks for the reduced magnetic-compliance capability."""

from __future__ import annotations

import importlib.util
import math
import sys
import unittest
from pathlib import Path

import numpy as np


MODULE_PATH = Path(__file__).with_name("magnetic_compliance.py")
SPEC = importlib.util.spec_from_file_location("magnetic_compliance", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MC = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MC
SPEC.loader.exec_module(MC)


class MagneticComplianceTests(unittest.TestCase):
    def test_polygonal_loop_converges_to_axis_field(self) -> None:
        radius = 1.7
        current = 2.3e5
        height = 0.8
        theta = np.linspace(0.0, 2.0 * np.pi, 4096, endpoint=False)
        loop = np.column_stack((radius * np.cos(theta), radius * np.sin(theta), np.zeros_like(theta)))
        observed = MC.field_from_coil(loop, np.array([[0.0, 0.0, height]]), current)[0, 2]
        expected = 4.0e-7 * np.pi * current * radius**2 / (
            2.0 * (radius**2 + height**2) ** 1.5
        )
        self.assertLess(abs(observed - expected) / expected, 2.0e-6)

    def test_support_is_positive_definite_and_trace_preserving(self) -> None:
        count = 8
        matrix = MC.support_matrix(count, (0.75, 1.0, 1.25), (0.5, 0.25, 0.0))
        self.assertGreater(float(np.linalg.eigvalsh(matrix)[0]), 0.0)
        self.assertAlmostEqual(float(np.trace(matrix)), 3.0 * count, places=12)

    def test_zero_coupling_is_directional_independent_support(self) -> None:
        matrix = MC.support_matrix(6, (0.5, 1.0, 1.5), (0.0, 0.0, 0.0))
        expected = np.tile((0.5, 1.0, 1.5), 6)
        np.testing.assert_allclose(matrix, np.diag(expected), atol=0.0, rtol=0.0)


if __name__ == "__main__":
    unittest.main()
