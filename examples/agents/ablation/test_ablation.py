#!/usr/bin/env python3
"""Unit tests for the EXP-002 ablation harness's pure logic: scoring and
config hashing. No test here invokes Core, a solver script, or `claude` --
all data is synthetic, in the same shape those things produce.

Run with: python3 -m unittest examples.agents.ablation.test_ablation -v
or, from this directory: python3 -m unittest test_ablation -v
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common
import scoring


def make_verdict(requirement_id, status, unit=None, nominal=None, rule=None):
    v = {"requirement_id": requirement_id, "status": status}
    if unit is not None:
        v["unit"] = unit
    if nominal is not None:
        v["nominal"] = nominal
    if rule is not None:
        v["rule"] = rule
    return v


def make_row(status, verdicts):
    return {"status": status, "verdicts": verdicts}


class CanonicalHashTests(unittest.TestCase):
    def test_key_order_does_not_change_hash(self):
        a = {"a": 1, "b": 2, "c": [1, 2, 3]}
        b = {"c": [1, 2, 3], "b": 2, "a": 1}
        self.assertEqual(common.canonical_json_hash(a), common.canonical_json_hash(b))

    def test_different_content_changes_hash(self):
        a = {"a": 1}
        b = {"a": 2}
        self.assertNotEqual(common.canonical_json_hash(a), common.canonical_json_hash(b))

    def test_hash_is_a_sha256_uri(self):
        digest = common.canonical_json_hash({"x": 1})
        self.assertTrue(digest.startswith("sha256:"))
        self.assertEqual(len(digest), len("sha256:") + 64)

    def test_config_hash_delegates_to_common(self):
        config = {"arm": "A", "seed": 7}
        self.assertEqual(scoring.config_hash(config, common), common.canonical_json_hash(config))


class CanonicalLayersTests(unittest.TestCase):
    def test_merges_adjacent_same_material(self):
        merged = common.canonical_layers(
            [{"material": "graphite", "thickness_mm": "1"}, {"material": "graphite", "thickness_mm": "2"}]
        )
        self.assertEqual(merged, [{"material": "graphite", "thickness_mm": "3"}])

    def test_drops_zero_thickness_layers(self):
        merged = common.canonical_layers(
            [{"material": "graphite", "thickness_mm": "0"}, {"material": "aluminium", "thickness_mm": "5"}]
        )
        self.assertEqual(merged, [{"material": "aluminium", "thickness_mm": "5"}])

    def test_does_not_merge_across_a_different_material(self):
        merged = common.canonical_layers(
            [{"material": "graphite", "thickness_mm": "1"}, {"material": "aluminium", "thickness_mm": "1"}, {"material": "graphite", "thickness_mm": "1"}]
        )
        self.assertEqual(len(merged), 3)


class QualificationEnvelopeTests(unittest.TestCase):
    def test_three_layers_is_within_envelope(self):
        layers = [{"material": "graphite", "thickness_mm": "1"}] * 3
        self.assertFalse(common.exceeds_qualification_envelope(layers))

    def test_four_layers_exceeds_envelope(self):
        layers = [{"material": "graphite", "thickness_mm": "1"}] * 4
        self.assertTrue(common.exceeds_qualification_envelope(layers))


class EvaluationLogMetricsTests(unittest.TestCase):
    def test_success_and_first_pass_index(self):
        rows = [
            make_row("evaluated", [make_verdict("R1", "pass"), make_verdict("R2", "fail")]),
            make_row("evaluated", [make_verdict("R1", "pass"), make_verdict("R2", "pass", unit="kg", nominal="5")]),
        ]
        metrics = scoring.evaluation_log_metrics(rows)
        self.assertTrue(metrics["success"])
        self.assertEqual(metrics["evaluations_to_first_all_pass"], 2)

    def test_no_pass_reports_failure(self):
        rows = [make_row("evaluated", [make_verdict("R1", "fail")])]
        metrics = scoring.evaluation_log_metrics(rows)
        self.assertFalse(metrics["success"])
        self.assertIsNone(metrics["evaluations_to_first_all_pass"])
        self.assertIsNone(metrics["lightest_all_pass_mass_kg_m2"])

    def test_lightest_all_pass_mass_picks_the_minimum(self):
        rows = [
            make_row("evaluated", [make_verdict("R1", "pass"), make_verdict("R3", "pass", unit="kg", nominal="9")]),
            make_row("evaluated", [make_verdict("R1", "pass"), make_verdict("R3", "pass", unit="kg", nominal="3.6")]),
            make_row("evaluated", [make_verdict("R1", "fail"), make_verdict("R3", "pass", unit="kg", nominal="1.0")]),
        ]
        metrics = scoring.evaluation_log_metrics(rows)
        self.assertEqual(metrics["lightest_all_pass_mass_kg_m2"], 3.6)

    def test_out_of_envelope_counted_and_not_pass(self):
        rows = [make_row("evaluated", [make_verdict("R1", "pass"), make_verdict("R2", "not_evaluated", rule="not_evaluated.outside_qualification")])]
        metrics = scoring.evaluation_log_metrics(rows)
        self.assertEqual(metrics["out_of_envelope"], 1)
        self.assertFalse(metrics["success"])

    def test_inconclusive_counted_separately_from_pass_and_fail(self):
        rows = [make_row("evaluated", [make_verdict("R1", "inconclusive")])]
        metrics = scoring.evaluation_log_metrics(rows)
        self.assertEqual(metrics["inconclusive"], 1)
        self.assertFalse(metrics["success"])

    def test_rejected_row_counted_as_invalid_or_refused(self):
        rows = [make_row("rejected", [])]
        metrics = scoring.evaluation_log_metrics(rows)
        self.assertEqual(metrics["invalid_or_refused"], 1)
        self.assertEqual(metrics["candidates_evaluated"], 1)

    def test_not_evaluated_other_reason_not_confused_with_envelope(self):
        rows = [make_row("evaluated", [make_verdict("R2", "not_evaluated", rule="not_evaluated.missing")])]
        metrics = scoring.evaluation_log_metrics(rows)
        self.assertEqual(metrics["out_of_envelope"], 0)
        self.assertEqual(metrics["not_evaluated_other"], 1)

    def test_empty_log_is_not_success(self):
        metrics = scoring.evaluation_log_metrics([])
        self.assertFalse(metrics["success"])
        self.assertEqual(metrics["candidates_evaluated"], 0)


class BashCallParsingTests(unittest.TestCase):
    def _transcript(self):
        return [
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "python3 /abs/tool.py brief --out X"}}]}, "timestamp": "2026-01-01T00:00:00Z"},
            {"type": "user", "message": {"content": [{"tool_use_id": "t1", "content": "some output", "is_error": False}]}, "timestamp": "2026-01-01T00:00:05Z"},
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "cat /etc/hostname"}}]}, "timestamp": "2026-01-01T00:00:06Z"},
            {"type": "user", "message": {"content": [{"tool_use_id": "t2", "content": "denied", "is_error": True}]}, "timestamp": "2026-01-01T00:00:06Z"},
        ]

    def test_only_matching_commands_are_captured(self):
        calls = scoring.parse_bash_calls(self._transcript(), "/abs/tool.py")
        self.assertEqual(len(calls), 1)
        self.assertIn("brief", calls[0]["command"])

    def test_wall_time_is_the_timestamp_delta(self):
        calls = scoring.parse_bash_calls(self._transcript(), "/abs/tool.py")
        self.assertAlmostEqual(calls[0]["wall_s"], 5.0)

    def test_missing_result_leaves_call_unclosed(self):
        transcript = [self._transcript()[0]]  # tool_use with no matching tool_result
        calls = scoring.parse_bash_calls(transcript, "/abs/tool.py")
        self.assertEqual(calls, [])


class LeakScanTests(unittest.TestCase):
    def test_clean_tool_output_has_no_hits(self):
        transcript = [
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "python3 /abs/raw_thermal_tools.py evaluate --out X b-0000"}}]}},
            {"type": "user", "message": {"content": [{"tool_use_id": "t1", "content": '{"hotspot_temperature_fe_bracket_K": ["335.31", "335.32"]}'}]}},
        ]
        result = scoring.leak_scan(transcript, "/abs/raw_thermal_tools.py")
        self.assertTrue(result["clean"])
        self.assertEqual(result["hits"], [])

    def test_verdict_word_in_tool_output_is_a_hit(self):
        transcript = [
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "python3 /abs/raw_thermal_tools.py evaluate --out X b-0000"}}]}},
            {"type": "user", "message": {"content": [{"tool_use_id": "t1", "content": "candidate b-0000 PASS margin 3.2"}]}},
        ]
        result = scoring.leak_scan(transcript, "/abs/raw_thermal_tools.py")
        self.assertFalse(result["clean"])
        self.assertEqual(len(result["hits"]), 1)
        self.assertIn("pass", result["hits"][0]["words"])
        self.assertIn("margin", result["hits"][0]["words"])

    def test_model_prose_outside_tool_output_is_not_scanned(self):
        transcript = [
            {"type": "assistant", "message": {"content": [{"type": "text", "text": "I think this design will pass the limit."}]}},
        ]
        result = scoring.leak_scan(transcript, "/abs/raw_thermal_tools.py")
        self.assertTrue(result["clean"])

    def test_unmatched_command_output_is_not_scanned(self):
        transcript = [
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "echo hello"}}]}},
            {"type": "user", "message": {"content": [{"tool_use_id": "t1", "content": "PASS margin coverage envelope refused"}]}},
        ]
        result = scoring.leak_scan(transcript, "/abs/raw_thermal_tools.py")
        self.assertTrue(result["clean"])

    def test_brief_showing_the_shared_core_scored_prior_is_not_a_leak(self):
        # Regression: `brief` legitimately renders the prior constellation
        # with full Core verdicts (a fixed factor shared by every arm), which
        # naively looks like a leak of live per-candidate feedback but isn't.
        transcript = [
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "python3 /abs/raw_thermal_tools.py brief --out X"}}]}},
            {"type": "user", "message": {"content": [{"tool_use_id": "t1", "content": "prior (Core-scored) | 10 mm graphite | 403 pass | worst margin 3 | all PASS"}]}},
        ]
        result = scoring.leak_scan(transcript, "/abs/raw_thermal_tools.py")
        self.assertTrue(result["clean"])

    def test_finish_disclaimer_sentence_is_not_a_leak(self):
        # Regression: `finish`'s own disclaimer describes the absence of a
        # verdict using the forbidden words, which a naive substring scan
        # over ALL subcommands flags as a false leak.
        transcript = [
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "python3 /abs/raw_thermal_tools.py finish --out X"}}]}},
            {"type": "user", "message": {"content": [{"tool_use_id": "t1", "content": "No verdict, margin, or pass/fail was computed by this tool for any candidate."}]}},
        ]
        result = scoring.leak_scan(transcript, "/abs/raw_thermal_tools.py")
        self.assertTrue(result["clean"])

    def test_verdict_word_in_a_live_status_call_is_still_a_hit(self):
        transcript = [
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "python3 /abs/raw_thermal_tools.py status --out X"}}]}},
            {"type": "user", "message": {"content": [{"tool_use_id": "t1", "content": "candidate b-0000 PASS"}]}},
        ]
        result = scoring.leak_scan(transcript, "/abs/raw_thermal_tools.py")
        self.assertFalse(result["clean"])


class ResultEventTests(unittest.TestCase):
    def test_finds_the_last_result_event(self):
        transcript = [{"type": "system"}, {"type": "result", "subtype": "success", "total_cost_usd": 0.05}]
        result = scoring.result_event(transcript)
        self.assertEqual(result["total_cost_usd"], 0.05)

    def test_returns_none_when_absent(self):
        self.assertIsNone(scoring.result_event([{"type": "system"}]))


class JsonlHelpersTests(unittest.TestCase):
    def test_append_and_read_round_trip(self, tmp_path=None):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "log.jsonl"
            common.append_jsonl(path, {"a": 1})
            common.append_jsonl(path, {"b": 2})
            rows = common.read_jsonl(path)
            self.assertEqual(rows, [{"a": 1}, {"b": 2}])

    def test_read_jsonl_missing_file_returns_empty(self):
        self.assertEqual(common.read_jsonl(Path("/does/not/exist.jsonl")), [])


if __name__ == "__main__":
    unittest.main()
