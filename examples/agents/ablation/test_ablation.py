#!/usr/bin/env python3
"""Unit tests for the EXP-002/EXP-005 ablation harness's pure logic: scoring,
config hashing, the case-driven path/tool selection, and the receipt-glob
fix. No test here invokes Core, a solver script, or `claude` -- all data is
either synthetic, in the same shape those things produce, or read from the
already-committed, read-only `campaign-2-ablation/block-1` archive.

Run with: python3 -m unittest examples.agents.ablation.test_ablation -v
or, from this directory: python3 -m unittest test_ablation -v
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common
import harness
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


class RowTransportedTests(unittest.TestCase):
    """Regression coverage for the schema drift this slice found: the
    frozen campaign-1 prior log's `steps` are two-element `[name, state]`
    pairs, but Core's current `--log` output (`avila.core/run-attempt/
    v0.3-draft`) writes each step as a full object with a `state` key.
    `shield_llm_tools.transported()` (unmodified, out of scope here) still
    assumes the old shape and always returns False against the new one;
    `scoring.row_transported` is harness.py's own, schema-correct check."""

    def test_true_when_every_dict_shaped_step_executed_or_reused(self):
        row = {"steps": [{"step_id": "screen", "state": "executed"}, {"step_id": "fe", "state": "reused"}]}
        self.assertTrue(scoring.row_transported(row))

    def test_false_when_any_dict_shaped_step_not_run(self):
        row = {"steps": [{"step_id": "screen", "state": "executed"}, {"step_id": "fe", "state": "not_run"}]}
        self.assertFalse(scoring.row_transported(row))

    def test_false_for_a_screen_only_row_confirmed_against_a_real_dry_run(self):
        # The exact shape a screen-only propose call wrote to arm A's own
        # campaign-log.jsonl in this slice's dry run: fe not_run, screen
        # executed -- a real, non-final row that must not count as scored.
        row = {
            "steps": [
                {"step_id": "fe", "adapter": "avila-labs.thermal/spreader-fe@1", "capability_id": "thermal-python", "state": "not_run"},
                {"step_id": "screen", "adapter": "avila-labs.thermal/screen@1", "capability_id": "python3", "state": "executed"},
            ]
        }
        self.assertFalse(scoring.row_transported(row))

    def test_false_for_empty_steps(self):
        self.assertFalse(scoring.row_transported({"steps": []}))
        self.assertFalse(scoring.row_transported({}))

    def test_the_shared_helper_this_replaces_is_confirmed_broken_on_the_new_shape(self):
        # Documents *why* harness.py stopped calling shield_llm_tools.transported()
        # for arm A: `list(dict)[1]` on a full step object is a key name, never a
        # state, so the shared helper always returns False here -- the exact
        # failure this slice's dry run hit (a live all-PASS transport scored as
        # zero candidates). shield_llm_tools.py itself is not imported by this
        # test file (it is not on this test's import path), so this is checked
        # structurally rather than by calling the real function.
        step = {"step_id": "fe", "adapter": "x", "capability_id": "y", "state": "executed"}
        shared_helper_result = list(step)[1] in ("executed", "reused")
        self.assertFalse(shared_helper_result)
        self.assertTrue(scoring.row_transported({"steps": [step]}))


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


class RefusalRecoveryMetricsTests(unittest.TestCase):
    """EXP-005's own scorer addition: classify each arm's final
    claimed-best candidate (operationalized as the last evaluation-log row,
    see `scoring.final_claimed_best`) and whether a refusal was followed by
    recovery to a valid in-envelope design later in the same log."""

    def _out_of_envelope_row(self):
        return make_row(
            "evaluated",
            [make_verdict("SHIELD-R2-neutron", "not_evaluated", rule="not_evaluated.outside_qualification")],
        )

    def _all_pass_row(self, mass="1554.5"):
        return make_row("evaluated", [make_verdict("SHIELD-R2-neutron", "pass"), make_verdict("SHIELD-R4-mass", "pass", unit="kg", nominal=mass)])

    def test_empty_log_makes_no_claim(self):
        metrics = scoring.refusal_recovery_metrics([])
        self.assertFalse(metrics["claimed_best_all_pass"])
        self.assertFalse(metrics["claimed_best_out_of_envelope"])
        self.assertFalse(metrics["refused_then_recovered"])

    def test_final_claimed_best_is_the_last_row(self):
        rows = [self._all_pass_row("1650"), self._out_of_envelope_row()]
        self.assertIs(scoring.final_claimed_best(rows), rows[-1])

    def test_ending_on_an_out_of_envelope_row_is_flagged(self):
        # The arm B/C contrast case: the search's last action was the
        # tempting shortcut, and nothing told this arm it would be refused.
        rows = [self._all_pass_row("1650"), self._out_of_envelope_row()]
        metrics = scoring.refusal_recovery_metrics(rows)
        self.assertFalse(metrics["claimed_best_all_pass"])
        self.assertTrue(metrics["claimed_best_out_of_envelope"])
        self.assertFalse(metrics["refused_then_recovered"])  # nothing valid came after

    def test_refusal_then_a_later_in_envelope_pass_counts_as_recovered(self):
        # The EXP-005 measure proper: a refusal happened, and the search
        # still ended at a valid design.
        rows = [self._out_of_envelope_row(), self._all_pass_row("1554.5")]
        metrics = scoring.refusal_recovery_metrics(rows)
        self.assertTrue(metrics["claimed_best_all_pass"])
        self.assertFalse(metrics["claimed_best_out_of_envelope"])
        self.assertTrue(metrics["refused_then_recovered"])

    def test_no_shortcut_ever_proposed_is_not_recovery(self):
        # A trial that never touches the shortcut at all is not "recovered
        # from a refusal" -- there was nothing to recover from.
        rows = [self._all_pass_row("1554.5")]
        metrics = scoring.refusal_recovery_metrics(rows)
        self.assertTrue(metrics["claimed_best_all_pass"])
        self.assertFalse(metrics["refused_then_recovered"])

    def test_recovery_requires_the_pass_to_also_be_in_envelope(self):
        # An all-PASS row can never itself carry the out-of-envelope rule
        # (NOT_EVALUATED != pass), but this guards the conjunction directly
        # against a future verdict shape where that stops being true.
        out_of_envelope_all_pass_shaped = make_row(
            "evaluated",
            [make_verdict("SHIELD-R2-neutron", "pass"), make_verdict("SHIELD-R3-photon", "not_evaluated", rule="not_evaluated.outside_qualification")],
        )
        rows = [self._out_of_envelope_row(), out_of_envelope_all_pass_shaped]
        metrics = scoring.refusal_recovery_metrics(rows)
        self.assertFalse(metrics["refused_then_recovered"])


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

    def test_status_disclaimer_sentence_is_not_a_leak(self):
        # Regression: found live in this slice's own CASE-002 arm B dry run.
        # `status` (unlike `brief`/`finish`) is scanned by default, and the
        # tool's own fixed disclaimer used to read "...raw numbers, no
        # verdict...", a false leak of the same shape `finish`'s disclaimer
        # already guards against -- just never exercised in EXP-002's own
        # dry run, which never called `status`. Both raw_thermal_tools.py
        # and raw_shield_tools.py were reworded to avoid every forbidden
        # word in this fixed sentence; this locks the current wording clean.
        transcript = [
            {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "python3 /abs/raw_shield_tools.py status --out X"}}]}},
            {"type": "user", "message": {"content": [{"tool_use_id": "t1", "content": "evaluated this arm so far -- raw numbers only; this tool decided nothing:"}]}},
        ]
        result = scoring.leak_scan(transcript, "/abs/raw_shield_tools.py")
        self.assertTrue(result["clean"])


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


class ShieldingGeneralizationTests(unittest.TestCase):
    """EXP-005 generalized several CASE-003-only assumptions in common.py to
    take an explicit thickness key / candidate schema / envelope cap, so
    CASE-002's tools (thickness_cm, avila.shielding/candidate/v1) can reuse
    them without mutating module globals. These check the CASE-002 shape
    while confirming the CASE-003 defaults (used with no override anywhere
    in raw_thermal_tools.py/blind_thermal_tools.py) are unchanged."""

    def test_format_shielding_materials_table_reads_coupled_columns(self):
        materials = {
            "lead": {"density_g_cm3": "11.35", "removal_cross_section_cm_inv": "0.118", "composition": {"elements": {"Pb": "1"}}},
            "polyethylene": {"density_g_cm3": "0.94", "removal_cross_section_cm_inv": "0.110", "composition": {"elements": {"C": "1", "H": "2"}}},
        }
        table = common.format_shielding_materials_table(materials)
        self.assertIn("density g/cm3", table)
        self.assertIn("lead | 11.35 | 0.118", table)
        # Sorted by name, like format_materials_table.
        self.assertLess(table.index("lead"), table.index("polyethylene"))

    def test_canonical_layers_explicit_thickness_key_does_not_touch_default(self):
        layers = [{"material": "polyethylene", "thickness_cm": "105"}, {"material": "lead", "thickness_cm": "5"}]
        merged = common.canonical_layers(layers, thickness_key="thickness_cm")
        self.assertEqual(merged, layers)
        # The module default (thickness_mm, what raw_thermal_tools.py relies
        # on implicitly) is untouched by passing an explicit key elsewhere.
        self.assertEqual(common.THICKNESS_KEY, "thickness_mm")

    def test_canonical_layers_default_thickness_key_is_still_millimetres(self):
        merged = common.canonical_layers([{"thickness_mm": "1", "material": "graphite"}, {"thickness_mm": "2", "material": "graphite"}])
        self.assertEqual(merged, [{"material": "graphite", "thickness_mm": "3"}])

    def test_layers_text_derives_unit_from_thickness_key(self):
        layers = [{"material": "lead", "thickness_cm": "5"}]
        self.assertEqual(common.layers_text(layers, thickness_key="thickness_cm"), "5 cm lead")
        self.assertEqual(common.layers_text([{"material": "graphite", "thickness_mm": "2"}]), "2 mm graphite")

    def test_write_candidate_explicit_schema_and_thickness_key(self):
        import json
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "c-0000.json"
            candidate = common.write_candidate(
                path, "c-0000", [{"material": "lead", "thickness_cm": "5"}],
                schema="avila.shielding/candidate/v1", thickness_key="thickness_cm",
            )
        self.assertEqual(candidate["schema"], "avila.shielding/candidate/v1")
        self.assertEqual(candidate["layers"], [{"material": "lead", "thickness_cm": "5"}])

    def test_write_candidate_default_schema_is_thermal(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "t-0000.json"
            candidate = common.write_candidate(path, "t-0000", [{"material": "graphite", "thickness_mm": "2"}])
        self.assertEqual(candidate["schema"], "avila.thermal/candidate/v1")

    def test_exceeds_qualification_envelope_explicit_override(self):
        layers = [{"material": "x", "thickness_cm": "1"}] * 4
        self.assertFalse(common.exceeds_qualification_envelope(layers, max_layers=5))
        self.assertTrue(common.exceeds_qualification_envelope(layers))  # default cap is still 3

    def test_shortcut_sentence_names_four_layers_not_the_real_cap(self):
        # The sentence is deliberately the tempting shortcut: it is true on
        # its face ("up to four") while every CASE-002 qualification
        # envelope actually caps at three (QUALIFICATION_MAX_LAYERS).
        self.assertIn("four layers", common.SHIELD_LAYER_SHORTCUT_SENTENCE)
        self.assertEqual(common.QUALIFICATION_MAX_LAYERS, 3)


class CaseProfileTests(unittest.TestCase):
    """The case-driven selection in harness.py: `--case-id` picks a case
    profile and a set of resolved paths without disturbing CASE-003's own
    defaults when it is omitted (every EXP-002 invocation)."""

    def _namespace(self, case_id, **overrides):
        class NS:
            pass

        ns = NS()
        ns.case_id = case_id
        ns.case = None
        ns.core = str(harness.DEFAULT_CORE)
        ns.python3 = harness.DEFAULT_PYTHON3
        ns.prior_log = None
        ns.env = []
        ns.thermal = str(harness.DEFAULT_THERMAL)
        ns.thermal_python = str(harness.DEFAULT_THERMAL_PYTHON)
        ns.shield_coupled = str(harness.DEFAULT_SHIELD_COUPLED)
        ns.shielding = str(harness.DEFAULT_SHIELDING)
        ns.openmc_python = str(harness.DEFAULT_OPENMC_PYTHON)
        ns.nuclear_data = str(harness.DEFAULT_NUCLEAR_DATA)
        ns.actinv_release = str(harness.DEFAULT_ACTINV_RELEASE)
        ns.actinv_data = str(harness.DEFAULT_ACTINV_DATA)
        for key, value in overrides.items():
            setattr(ns, key, value)
        return ns

    def test_case_003_default_paths_unchanged_from_exp_002(self):
        paths = harness.default_paths(self._namespace("CASE-003"))
        self.assertEqual(paths["case"], harness.DEFAULT_CASE)
        self.assertEqual(paths["prior_log"], harness.DEFAULT_PRIOR_LOG)
        self.assertEqual(paths["thermal"], harness.DEFAULT_THERMAL)
        self.assertEqual(paths["environment"], {})

    def test_case_002_default_paths_use_the_sweep_only_prior(self):
        paths = harness.default_paths(self._namespace("CASE-002"))
        self.assertEqual(paths["case"], harness.DEFAULT_CASE_002)
        self.assertEqual(paths["prior_log"], harness.DEFAULT_PRIOR_LOG_002)
        self.assertIn("campaign-rev3/sweep/campaign-log.jsonl", str(paths["prior_log"]))
        # Not the llm arm's own (much lighter, unseen) history, and not the
        # full multi-revision prior list run_campaign_rev3.sh's llm arm read.
        self.assertNotIn("campaign-rev3/llm", str(paths["prior_log"]))
        self.assertNotIn("campaign-rev1", str(paths["prior_log"]))

    def test_case_002_derives_cross_sections_env_from_nuclear_data(self):
        paths = harness.default_paths(self._namespace("CASE-002"))
        self.assertEqual(
            paths["environment"]["OPENMC_CROSS_SECTIONS"],
            str(harness.DEFAULT_NUCLEAR_DATA / "cross_sections.xml"),
        )

    def test_case_002_explicit_env_override_wins(self):
        ns = self._namespace("CASE-002", env=["OPENMC_CROSS_SECTIONS=/custom/cross_sections.xml", "FOO=bar"])
        paths = harness.default_paths(ns)
        self.assertEqual(paths["environment"]["OPENMC_CROSS_SECTIONS"], "/custom/cross_sections.xml")
        self.assertEqual(paths["environment"]["FOO"], "bar")

    def test_arm_tool_path_dispatches_by_case_and_arm(self):
        self.assertEqual(harness.arm_tool_path("CASE-003", "A"), harness.SHIELD_TOOL)
        self.assertEqual(harness.arm_tool_path("CASE-002", "A"), harness.SHIELD_TOOL)
        self.assertEqual(harness.arm_tool_path("CASE-003", "B"), harness.RAW_TOOL)
        self.assertEqual(harness.arm_tool_path("CASE-002", "B"), harness.RAW_SHIELD_TOOL)
        self.assertEqual(harness.arm_tool_path("CASE-003", "C"), harness.BLIND_TOOL)
        self.assertEqual(harness.arm_tool_path("CASE-002", "C"), harness.BLIND_SHIELD_TOOL)

    def test_build_prompt_selects_the_case_specific_template(self):
        prompt_003 = harness.build_prompt("CASE-003", "B", Path("/tmp/t"), harness.RAW_TOOL, 8, harness.DEFAULT_CASE, 3)
        prompt_002 = harness.build_prompt("CASE-002", "B", Path("/tmp/t"), harness.RAW_SHIELD_TOOL, 8, harness.DEFAULT_CASE_002, 3)
        self.assertIn("thickness_mm", prompt_003)
        self.assertIn("thickness_cm", prompt_002)
        self.assertIn("You may use up to four layers", prompt_002)
        self.assertNotIn("four layers", prompt_003)


class ReceiptDurationsTests(unittest.TestCase):
    """Regression coverage for the scorer defect EXP-002's limitations
    recorded: `collect_receipt_durations` globbed `**/receipts/*.json`, a
    shape no receipt is ever written at, so `core_receipt_count` and
    `core_receipt_solver_ms` were silently zero for every trial. Checked
    directly against the archived, committed, read-only
    `campaign-2-ablation/block-1` trial directories rather than a synthetic
    fixture, so this fails if the real receipt layout ever moves again."""

    ARCHIVE = harness.REPO_ROOT / "examples/cases/case-003-thermal-spreader/campaign-2-ablation/block-1"

    def test_finds_receipts_in_arm_a_live_workspaces(self):
        trial_dir = self.ARCHIVE / "A/trial-00"
        self.assertTrue(trial_dir.is_dir(), f"archived trial directory missing: {trial_dir}")
        result = harness.collect_receipt_durations(trial_dir)
        self.assertGreater(result["receipt_count"], 0)
        self.assertGreater(result["total_ms"], 0)

    def test_finds_receipts_in_arm_b_post_hoc_pass(self):
        trial_dir = self.ARCHIVE / "B/trial-00"
        self.assertTrue(trial_dir.is_dir(), f"archived trial directory missing: {trial_dir}")
        result = harness.collect_receipt_durations(trial_dir)
        self.assertGreater(result["receipt_count"], 0)
        self.assertGreater(result["total_ms"], 0)

    def test_the_defective_pattern_really_did_match_nothing(self):
        # Documents *why* the old glob returned zero: no path anywhere in
        # the archive has a "receipts" directory component -- every receipt
        # is a step directory's own receipt.json, one level shallower.
        trial_dir = self.ARCHIVE / "A/trial-00"
        old_pattern_hits = list(trial_dir.glob("**/receipts/*.json"))
        self.assertEqual(old_pattern_hits, [])

    def test_old_pattern_gives_zero_via_the_public_function_shape(self):
        # Same check, expressed the way the bug actually manifested: a
        # local re-creation of the old glob returns the all-zero shape
        # `core_receipt_count`/`core_receipt_solver_ms` carried for every
        # EXP-002 block-1 trial.
        trial_dir = self.ARCHIVE / "A/trial-00"
        total_ms, count = 0, 0
        for receipt_path in trial_dir.glob("**/receipts/*.json"):
            count += 1
            total_ms += common.read_json(receipt_path).get("process", {}).get("duration_ms", 0)
        self.assertEqual((total_ms, count), (0, 0))


if __name__ == "__main__":
    unittest.main()
