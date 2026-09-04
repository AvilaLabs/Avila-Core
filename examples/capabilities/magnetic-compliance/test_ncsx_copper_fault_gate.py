#!/usr/bin/env python3
"""Unit checks for the NCSX-referenced cryoresistive fault gate."""

from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from dataclasses import replace
from pathlib import Path

import numpy as np


MODULE_PATH = Path(__file__).with_name("ncsx_copper_fault_gate.py")
SPEC = importlib.util.spec_from_file_location("ncsx_copper_fault_gate", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
GATE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = GATE
SPEC.loader.exec_module(GATE)


class CopperFaultTests(unittest.TestCase):
    def setUp(self) -> None:
        self.model = GATE.load_model(
            MODULE_PATH.with_name("ncsx-copper-fault-model.json").read_bytes()
        )

    def test_coherent_dimensional_model_loads(self) -> None:
        self.assertEqual(self.model.model_id, "ncsx-m45r00-asynchronous-discharge-r0")
        self.assertEqual(self.model.names, ("M1", "M2", "M3"))
        self.assertTrue(
            np.allclose(
                self.model.copper_mass,
                [2162.36769, 2097.358404, 1922.921723],
            )
        )

    def test_nist_material_curve_is_anchored_and_increases(self) -> None:
        self.assertAlmostEqual(GATE.copper_resistivity(85.0, self.model), 2.36e-9, places=20)
        self.assertGreater(
            GATE.copper_resistivity(125.0, self.model),
            2.0 * GATE.copper_resistivity(85.0, self.model),
        )
        self.assertGreater(GATE.heat_capacity(125.0, self.model.material), 0)

    def test_thermal_scale_reproduces_published_energy_temperature_mapping(self) -> None:
        validation = GATE.validate_thermal_scale(self.model)
        self.assertLess(validation.maximum_energy_relative_error, 0.02)
        self.assertLess(validation.maximum_temperature_absolute_error, 0.3)

    def test_branch_incidence_reproduces_mesh_power(self) -> None:
        incidence, matrix, shared = GATE.resistor_network(0.05, 4.0)
        self.assertAlmostEqual(shared, 0.05)
        self.assertTrue(np.allclose(matrix, 0.05 * np.eye(3) + 0.05 * incidence.T @ incidence))
        self.assertTrue(GATE.verify_network(0.05, 4.0))

    def test_candidate_box_rejects_unbounded_damping(self) -> None:
        candidate = {
            "schema": GATE.CANDIDATE_SCHEMA,
            "candidate_id": "bad",
            "network": {"differential_to_common_mode_ratio": "4.1"},
        }
        with self.assertRaises(GATE.CopperFaultError):
            GATE.load_candidate(json.dumps(candidate).encode())

    def test_shape_departure_is_zero_at_the_nominal_state(self) -> None:
        shape = GATE.ShapeModel(np.eye(3), np.eye(9), np.ones(3) / 3.0)
        magnetic, force = GATE.shape_departure(
            self.model.initial_current, self.model.initial_current, shape
        )
        self.assertAlmostEqual(magnetic, 0.0)
        self.assertAlmostEqual(force, 0.0)

    def test_batched_scenario_order_matches_individual_simulation(self) -> None:
        model = replace(
            self.model,
            detection_delays=(0.005,),
            nominal_detection_delay=0.005,
            end_time=0.01,
        )
        inductance = np.diag(model.self_inductance)
        shape = GATE.ShapeModel(np.eye(3), np.eye(9), np.ones(3) / 3.0)
        evaluation = GATE.evaluate_sweep(
            inductance,
            shape,
            model,
            2.0,
            0.001,
            resistance_scales=(1.0,),
            heat_capacity_scales=(1.0,),
            delays=(0.005,),
        )
        for fault in range(3):
            individual = GATE.simulate(
                inductance,
                shape,
                model,
                2.0,
                fault,
                0.005,
                1.0,
                0.001,
            )
            batched = evaluation.candidate[fault]
            self.assertAlmostEqual(
                individual.peak_magnetic_departure,
                batched.peak_magnetic_departure,
            )
            self.assertTrue(np.allclose(individual.total_i2t, batched.total_i2t))
            self.assertGreater(float(np.sum(batched.coil_energy)), 0.0)
            self.assertLess(batched.energy_balance_relative_error, 1.0e-6)

    def test_prehistory_i2t_exactly_integrates_linear_current_segments(self) -> None:
        model = replace(
            self.model,
            prehistory_times=np.asarray((0.0, 1.0)),
            prehistory_currents=np.asarray(((0.0, 0.0, 0.0), (3.0, 6.0, 9.0))),
        )
        self.assertTrue(np.allclose(GATE._prehistory_i2t(model), (3.0, 12.0, 27.0)))

    def test_mutual_inductance_difference_excludes_fixed_self_terms(self) -> None:
        first = np.asarray(((10.0, 1.0), (1.0, 20.0)))
        second = np.asarray(((10.0, 2.0), (2.0, 20.0)))
        self.assertAlmostEqual(GATE._mutual_inductance_difference(first, second), 0.5)

    def test_activation_timing_does_not_call_first_post_switch_sample_pre_switch(self) -> None:
        self.assertTrue(GATE._at_or_before_activation(0.05, 0.05))
        self.assertFalse(GATE._at_or_before_activation(0.051, 0.05))


if __name__ == "__main__":
    unittest.main()
