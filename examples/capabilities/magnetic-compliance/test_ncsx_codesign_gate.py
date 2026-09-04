#!/usr/bin/env python3
"""Unit checks for the NCSX coil/support co-design gate."""

from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from pathlib import Path

import numpy as np


ROOT = Path(__file__).parent
MODULE_PATH = ROOT / "ncsx_codesign_gate.py"
SPEC = importlib.util.spec_from_file_location("ncsx_codesign_gate", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
GATE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = GATE
SPEC.loader.exec_module(GATE)


class CodesignGateTests(unittest.TestCase):
    def test_bound_model_and_search_record_are_closed_and_consistent(self) -> None:
        model = GATE.load_codesign_model(
            (ROOT / "ncsx-codesign-gate-model.json").read_bytes()
        )
        search = GATE.load_search_record(
            (ROOT / "ncsx-codesign-search-record.json").read_bytes()
        )
        self.assertEqual(model.maximum_fourier_mode, 6)
        self.assertEqual(model.repair_scale, 0.5)
        self.assertEqual(search.seed, 20260904)
        self.assertEqual(search.evaluations, 649)
        self.assertEqual(len(search.candidate), 117)
        self.assertEqual(
            model.search_record_sha256,
            GATE._sha256(ROOT / "ncsx-codesign-search-record.json"),
        )

    def test_parameter_order_is_unique_and_candidate_scaling_is_exact(self) -> None:
        base_model = type("Model", (), {"base_coil_count": 3})()
        indices = GATE.parameter_indices(base_model, 6)
        self.assertEqual(len(indices), 117)
        self.assertEqual(len(indices), len(set(indices)))
        self.assertEqual(indices[:3], [(0, 1), (1, 0), (1, 1)])
        self.assertEqual(indices[-1], (6, 17))
        original = np.zeros((7, 18))
        offsets = np.arange(117, dtype=float) / 1000.0
        candidate = GATE.candidate_coefficients(
            original, indices, offsets, scale=0.5
        )
        observed = np.asarray([candidate[row, column] for row, column in indices])
        np.testing.assert_allclose(observed, 0.5 * offsets, atol=0.0, rtol=0.0)
        self.assertEqual(int(np.count_nonzero(candidate)), 116)

    def test_unknown_model_key_is_rejected(self) -> None:
        data = json.loads((ROOT / "ncsx-codesign-gate-model.json").read_text())
        data["undeclared"] = True
        with self.assertRaisesRegex(GATE.CodesignError, "keys differ"):
            GATE.load_codesign_model(json.dumps(data).encode())

    def test_truncated_candidate_is_rejected_before_geometry(self) -> None:
        original = np.zeros((7, 18))
        base_model = type("Model", (), {"base_coil_count": 3})()
        indices = GATE.parameter_indices(base_model, 6)
        with self.assertRaisesRegex(GATE.CodesignError, "expected 117"):
            GATE.candidate_coefficients(
                original, indices, np.zeros(116), scale=0.5
            )


if __name__ == "__main__":
    unittest.main()
