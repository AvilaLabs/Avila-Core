#!/usr/bin/env python3
"""Unit tests for the log-reading helpers in shield_llm_tools.py."""

import unittest

import shield_llm_tools as tools


class TransportedTests(unittest.TestCase):
    def test_v03_object_steps(self):
        row = {"steps": [{"step_id": "screen", "state": "executed"}, {"step_id": "fe", "state": "reused"}]}
        self.assertTrue(tools.transported(row))

    def test_v03_not_run_step_is_not_transported(self):
        row = {"steps": [{"step_id": "screen", "state": "executed"}, {"step_id": "fe", "state": "not_run"}]}
        self.assertFalse(tools.transported(row))

    def test_v02_pair_steps(self):
        self.assertTrue(tools.transported({"steps": [["screen", "executed"], ["fe", "reused"]]}))
        self.assertFalse(tools.transported({"steps": [["fe", "not_run"], ["screen", "executed"]]}))

    def test_empty_or_malformed_steps(self):
        self.assertFalse(tools.transported({"steps": []}))
        self.assertFalse(tools.transported({}))
        self.assertFalse(tools.transported({"steps": [{"step_id": "fe"}]}))
        self.assertFalse(tools.transported({"steps": ["fe"]}))


if __name__ == "__main__":
    unittest.main()
