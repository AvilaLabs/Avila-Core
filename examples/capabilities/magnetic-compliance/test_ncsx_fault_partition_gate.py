#!/usr/bin/env python3
"""Unit checks for the NCSX fault-balanced partition screen."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("ncsx_fault_partition_gate.py")
SPEC = importlib.util.spec_from_file_location("ncsx_fault_partition_gate", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
GATE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = GATE
SPEC.loader.exec_module(GATE)


class FaultPartitionTests(unittest.TestCase):
    def test_six_positions_have_fifteen_pair_partitions(self) -> None:
        partitions = GATE.pair_partitions()
        self.assertEqual(len(partitions), 15)
        self.assertEqual(len(set(partitions)), 15)
        for partition in partitions:
            self.assertEqual(sorted(value for pair in partition for value in pair), list(range(6)))

    def test_balanced_architecture_is_accepted(self) -> None:
        groups = (
            (0, 4, 7, 11, 14, 15),
            (1, 3, 8, 10, 12, 17),
            (2, 5, 6, 9, 13, 16),
        )
        self.assertEqual(GATE.validate_architecture(groups), groups)

    def test_type_segregated_control_is_not_a_balanced_candidate(self) -> None:
        groups = tuple(tuple(3 * copy + coil_type for copy in range(6)) for coil_type in range(3))
        with self.assertRaises(GATE.PartitionError):
            GATE.validate_architecture(groups)

    def test_group_from_pairs_contains_two_of_each_type(self) -> None:
        group = GATE.group_from_pairs(((0, 5), (1, 2), (3, 4)))
        self.assertEqual(group, (0, 4, 7, 11, 14, 15))
        self.assertEqual([sum(index % 3 == kind for index in group) for kind in range(3)], [2, 2, 2])


if __name__ == "__main__":
    unittest.main()
