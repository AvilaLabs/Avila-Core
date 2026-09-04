#!/usr/bin/env python3
"""Unit checks for the reduced NCSX circuit/quench feasibility gate."""

from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from pathlib import Path

import numpy as np


MODULE_PATH = Path(__file__).with_name("ncsx_circuit_quench_gate.py")
SPEC = importlib.util.spec_from_file_location("ncsx_circuit_quench_gate", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
GATE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = GATE
SPEC.loader.exec_module(GATE)


class CircuitQuenchTests(unittest.TestCase):
    def test_bound_model_loads(self) -> None:
        path = MODULE_PATH.with_name("ncsx-circuit-quench-model.json")
        model = GATE.load_model(path.read_bytes())
        self.assertEqual(model.model_id, "ncsx-three-loop-mode-selective-dump-r0")
        self.assertEqual(len(model.groups), 3)

    def test_network_has_declared_common_and_differential_modes(self) -> None:
        matrix, shared = GATE.resistor_network(1.45, 2.0)
        self.assertAlmostEqual(shared, 1.45 / 3.0)
        self.assertTrue(np.allclose(matrix @ np.ones(3), 1.45 * np.ones(3)))
        eigenvalues = np.linalg.eigvalsh(matrix)
        self.assertTrue(np.allclose(eigenvalues, [1.45, 2.9, 2.9]))

    def test_candidate_rejects_active_or_negative_shared_network(self) -> None:
        candidate = {
            "schema": GATE.CANDIDATE_SCHEMA,
            "candidate_id": "bad",
            "network": {
                "private_resistance_scale": "1",
                "differential_to_common_mode_ratio": "0.9",
            },
        }
        with self.assertRaises(GATE.CircuitQuenchError):
            GATE.load_candidate(json.dumps(candidate).encode())

    def test_event_split_simulation_is_repeatable_and_dissipative(self) -> None:
        model_path = MODULE_PATH.with_name("ncsx-circuit-quench-model.json")
        model = GATE.load_model(model_path.read_bytes())
        inductance = np.array(
            [[1.0, 0.2, 0.2], [0.2, 1.0, 0.2], [0.2, 0.2, 1.0]]
        )
        first = GATE.simulate(inductance, 1.45, 2.0, 0, 0.0025, model)
        second = GATE.simulate(inductance, 1.45, 2.0, 0, 0.0025, model)
        self.assertTrue(np.array_equal(first.currents, second.currents))
        self.assertGreater(first.i2t, 0)
        self.assertLess(first.final_energy_fraction, 1.0e-6)
        self.assertGreaterEqual(first.minimum_current, -1.0e-12)

    def test_common_current_has_zero_shape_departure(self) -> None:
        times = np.array([0.0, 1.0, 2.0])
        currents = np.array([[1.0, 1.0, 1.0], [0.5, 0.5, 0.5], [0.0, 0.0, 0.0]])
        transient = GATE.Transient(times, currents, 1.0, 1.0, 0.0, 0.0)
        metric = GATE.fault_metric(transient, np.eye(3), np.eye(9), 0)
        self.assertEqual(metric.peak_magnetic_departure, 0.0)
        self.assertEqual(metric.peak_force_departure, 0.0)


if __name__ == "__main__":
    unittest.main()
