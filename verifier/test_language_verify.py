#!/usr/bin/env python3
"""Tests for the language-evaluation replay port (EL-04).

Run with: python3 -m unittest test_language_verify -v   (from verifier/)

Each section names the spec surface it exercises — §7 [O1] observation
binding, [O2] postcondition replay, §8 premise admissibility, §10
projection/identity, [V-*] bounded verdicts — and pins it against the
committed evaluation corpus under fixtures/language/ (generated from the
shared implementation once, then held constant so the replay stays
independent of the Rust toolchain).
"""

from __future__ import annotations

import json
import sys
import unittest
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import language_verify as lv  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURES = Path(__file__).resolve().parent / "fixtures" / "language"


def load(path: Path):
    return json.loads(path.read_text())


class Args:
    """Namespace stand-in for argparse output."""

    def __init__(self, **kw):
        self.__dict__.update(kw)


def verify(directory: Path, obs: str = "observations",
           evaluation: str = "evaluation", analysis: bool = True):
    args = Args(
        program=str(directory / "program.json"),
        library=str(directory / "library.json"),
        plan=str(directory / "plan.json"),
        observations=str(directory / f"{obs}.json"),
        evaluation=str(directory / f"{evaluation}.json"),
        analysis=str(directory / "analysis.json") if analysis else None,
    )
    return lv.verify_evaluation(args)


def statuses(report) -> dict:
    counts = {"verified": 0, "mismatch": 0, "not_checked": 0, "as_of": 0}
    for c in report.checks:
        counts[c.status] += 1
    return counts


# ---------------------------------------------------------------------------
# §10 identity — semantic projection, canonical bytes, digests
# ---------------------------------------------------------------------------


class ProjectionTests(unittest.TestCase):
    def test_annotation_fields_do_not_touch_semantic_identity(self):
        directory = FIXTURES / "clearance-pass"
        program = load(directory / "program.json")
        original = lv.semantic_sha256(program, "program")
        # `description`/`label`/`note` are annotations — editing them changes
        # the document bytes but must leave the semantic digest alone.
        mutated = json.loads(json.dumps(program))
        mutated["description"] = "edited note"
        mutated["inputs"][0]["note"] = "an annotation"
        mutated["body"][0]["label"] = "renamed"
        self.assertEqual(original, lv.semantic_sha256(mutated, "program"))

    def test_semantic_field_edit_changes_identity(self):
        directory = FIXTURES / "clearance-pass"
        program = load(directory / "program.json")
        original = lv.semantic_sha256(program, "program")
        mutated = json.loads(json.dumps(program))
        mutated["inputs"][0]["binding"]["value"]["lower"] = "9/10"
        self.assertNotEqual(original, lv.semantic_sha256(mutated, "program"))

    def test_declared_sets_sort_by_projected_bytes(self):
        directory = FIXTURES / "clearance-pass"
        program = load(directory / "program.json")
        identity = lv.semantic_sha256(program, "program")
        shuffled = json.loads(json.dumps(program))
        shuffled["inputs"] = list(reversed(shuffled["inputs"]))
        self.assertEqual(identity, lv.semantic_sha256(shuffled, "program"))

    def test_undeclared_field_rejects(self):
        with self.assertRaises(lv.ProjectionError):
            lv.project({"kind": "exact", "value": "1", "bogus": 2}, lv.VALUE, "")
        with self.assertRaises(lv.ProjectionError):
            lv.project("not-an-object", lv.VALUE, "")

    def test_map_keys_are_semantic_identifiers(self):
        slot = {"quantity_kind": "q", "claim": "exact",
                "geometry": "g1", "scenario": "s1"}
        projected = lv.project(slot, lv.SLOT, "")
        self.assertEqual(projected["geometry"], "g1")


class NumberTests(unittest.TestCase):
    def test_canonical_rationals_parse(self):
        self.assertEqual(lv.exact_number("3/10"), Fraction(3, 10))
        self.assertEqual(lv.exact_number("42"), Fraction(42))
        self.assertEqual(lv.exact_number("-2/5"), Fraction(-2, 5))

    def test_non_canonical_rational_rejected(self):
        # `6/20` reduces to 3/10 — the staged-input contract refuses
        # unreduced forms; the port must fail closed, never normalize.
        with self.assertRaises(Exception):
            lv.exact_number("6/20")

    def test_denominator_zero_rejected(self):
        with self.assertRaises(Exception):
            lv.exact_number("1/0")

    def test_rational_render(self):
        self.assertEqual(lv.canonical_rational(Fraction(3, 10)), "3/10")
        self.assertEqual(lv.canonical_rational(Fraction(4)), "4")

    def test_claim_satisfaction_order(self):
        self.assertTrue(lv.claim_satisfies("exact", "enclosure"))
        self.assertTrue(lv.claim_satisfies("enclosure", "enclosure"))
        self.assertTrue(lv.claim_satisfies("nominal", "nominal"))
        self.assertFalse(lv.claim_satisfies("nominal", "enclosure"))
        self.assertFalse(lv.claim_satisfies("enclosure", "exact"))


class ExpressionTests(unittest.TestCase):
    def test_interval_arithmetic(self):
        kinds = {"disp": {"canonical_unit": "mm"}}
        products = {}
        operands = {
            "a": lv.SemVal(lv.QuantityType("disp", "enclosure"), "mm",
                           lv.Numeric(lower=Fraction(1, 10), upper=Fraction(1, 5)), "established"),
            "b": lv.SemVal(lv.QuantityType("disp", "enclosure"), "mm",
                           lv.Numeric(lower=Fraction(1, 4), upper=Fraction(1, 2)), "established"),
        }
        out = lv.eval_expression(lv.parse_expression("a + b"), operands, products)
        self.assertEqual(out.value.lower, Fraction(7, 20))
        self.assertEqual(out.value.upper, Fraction(7, 10))
        sub = lv.eval_expression(lv.parse_expression("b - a"), operands, products)
        self.assertEqual(sub.value.lower, Fraction(1, 20))
        self.assertEqual(sub.value.upper, Fraction(2, 5))

    def test_exact_values_stay_exact(self):
        products = {}
        operands = {
            "a": lv.SemVal(lv.QuantityType("q", "exact"), "u",
                           lv.Numeric(exact=Fraction(3)), "established"),
            "b": lv.SemVal(lv.QuantityType("q", "exact"), "u",
                           lv.Numeric(exact=Fraction(4)), "established"),
        }
        out = lv.eval_expression(lv.parse_expression("a * b"), operands,
                                 {("q", "q"): ("qq", "u2")})
        self.assertEqual(out.value.exact, Fraction(12))
        self.assertEqual(out.ty.claim, "exact")

    def test_mul_with_no_kind_product_fails(self):
        operands = {
            "a": lv.SemVal(lv.QuantityType("x", "exact"), "", lv.Numeric(exact=Fraction(1)), "established"),
            "b": lv.SemVal(lv.QuantityType("y", "exact"), "", lv.Numeric(exact=Fraction(1)), "established"),
        }
        with self.assertRaises(lv.EvalFailure):
            lv.eval_expression(lv.parse_expression("a * b"), operands, {})

    def test_unknown_rule_and_syntax_reject(self):
        for bad in ("frob(a)", "a b", "a +", "(", "a,,b"):
            with self.assertRaises(lv.EvalFailure, msg=bad):
                lv.parse_ensures(f"output = {bad}")
        with self.assertRaises(lv.EvalFailure):
            lv.parse_ensures("sum = a + b")
        with self.assertRaises(lv.EvalFailure):
            lv.parse_ensures("output a + b")


class AdmissionTests(unittest.TestCase):
    def test_admit_value_claim_and_unit(self):
        ty = lv.QuantityType("disp", "enclosure")
        kinds = {"disp": {"canonical_unit": "mm"}}
        ok = lv.admit_value(
            {"kind": "enclosure", "lower": "1/10", "upper": "1/5", "unit": "mm"},
            ty, kinds,
        )
        self.assertIsNotNone(ok)
        self.assertIsNone(lv.admit_value(
            {"kind": "nominal", "value": "1/10", "unit": "mm"}, ty, kinds))
        self.assertIsNone(lv.admit_value(
            {"kind": "enclosure", "lower": "1/5", "upper": "1/10", "unit": "mm"},
            ty, kinds))
        self.assertIsNone(lv.admit_value(
            {"kind": "enclosure", "lower": "1/10", "upper": "1/5", "unit": "cm"},
            ty, kinds))


# ---------------------------------------------------------------------------
# End-to-end: the committed evaluation corpus
# ---------------------------------------------------------------------------


class CorpusTests(unittest.TestCase):
    """Every committed evaluation must replay clean — the verifier's own
    derivation, not the record's claims, produces the checks."""

    PROGRAMS = [
        "clearance-pass",
        "clearance-fail",
        "clearance-inconclusive",
        "clearance-not-evaluated",
        "clearance-heuristic-unusable",
        "positive-assumption-discharge",
        "positive-measurement-pass",
    ]

    EXPECTED = {
        "clearance-pass": ("pass", "bounded.ge"),
        "clearance-fail": ("fail", "bounded.ge"),
        "clearance-inconclusive": ("inconclusive", "bounded.ge"),
        "clearance-not-evaluated": ("not_evaluated",
                                    "not_evaluated.missing_evidence"),
        "clearance-heuristic-unusable": ("not_evaluated",
                                         "not_evaluated.unestablished"),
        "positive-assumption-discharge": ("pass", "bounded.ge"),
        "positive-measurement-pass": ("pass", "bounded.le"),
    }

    def test_all_programs_replay_clean(self):
        for name in self.PROGRAMS:
            with self.subTest(program=name):
                report = verify(FIXTURES / name)
                counts = statuses(report)
                self.assertEqual(
                    0, counts["mismatch"],
                    f"{name}: {[c.detail for c in report.checks if c.status == 'mismatch']}",
                )
                self.assertGreater(counts["verified"], 10)

    def test_verdict_states_match_the_spec_matrix(self):
        for name, (status, rule) in self.EXPECTED.items():
            with self.subTest(program=name):
                evaluation = load(FIXTURES / name / "evaluation.json")
                for rid, verdict in evaluation["requirements"].items():
                    self.assertEqual(status, verdict["status"], f"{name}/{rid}")
                    self.assertEqual(rule, verdict["rule"], f"{name}/{rid}")

    def test_conditional_residuals_in_detail(self):
        evaluation = load(FIXTURES / "positive-assumption-discharge"
                          / "evaluation.json")
        detail = evaluation["requirements"]["EL-R1"]["detail"]
        self.assertIn("conditional on", detail)


class AdversarialTests(unittest.TestCase):
    """Foreign, tampered, transplanted, duplicated, and absent evidence —
    each honestly recorded by the implementation, each replayed clean here.
    The verifier agreeing the record *says rejected* is the check."""

    def test_tampered_output_rejects_and_verifies(self):
        directory = FIXTURES / "clearance-pass"
        report = verify(
            directory,
            obs="observations-tampered-output",
            evaluation="evaluation-tampered-output",
        )
        counts = statuses(report)
        self.assertEqual(0, counts["mismatch"],
                         [c.detail for c in report.checks
                          if c.status == "mismatch"])
        evaluation = load(directory / "evaluation-tampered-output.json")
        self.assertEqual("rejected",
                         evaluation["observations"][0]["state"])
        self.assertEqual("not_evaluated",
                         evaluation["requirements"]["EL-R1"]["status"])

    def test_foreign_plan_rejects_every_record(self):
        directory = FIXTURES / "clearance-pass"
        report = verify(
            directory,
            obs="observations-foreign-plan",
            evaluation="evaluation-foreign-plan",
        )
        self.assertEqual(0, statuses(report)["mismatch"])
        evaluation = load(directory / "evaluation-foreign-plan.json")
        self.assertTrue(all(o["state"] == "rejected"
                            for o in evaluation["observations"]))

    def test_absent_observation_leaves_the_requirement_unevaluated(self):
        directory = FIXTURES / "clearance-pass"
        report = verify(
            directory,
            obs="observations-absent",
            evaluation="evaluation-absent",
        )
        self.assertEqual(0, statuses(report)["mismatch"])
        evaluation = load(directory / "evaluation-absent.json")
        self.assertEqual("absent",
                         evaluation["observations"][0]["state"])
        self.assertEqual("not_evaluated",
                         evaluation["requirements"]["EL-R1"]["status"])

    def test_duplicate_claims_reject_both(self):
        directory = FIXTURES / "clearance-pass"
        report = verify(
            directory,
            obs="observations-duplicate",
            evaluation="evaluation-duplicate",
        )
        self.assertEqual(0, statuses(report)["mismatch"])
        evaluation = load(directory / "evaluation-duplicate.json")
        rejected = [o for o in evaluation["observations"]
                    if o["state"] == "rejected"]
        self.assertTrue(rejected)

    def test_malformed_observations_document_foreign_in_toto(self):
        """A doc carrying a null field fails canonical admission — `json.loads`
        would accept it silently; the replay must treat it as no material at
        all and the record must carry a document finding + empty digest."""
        directory = FIXTURES / "clearance-pass"
        report = verify(
            directory,
            obs="observations-malformed",
            evaluation="evaluation-malformed",
        )
        self.assertEqual(0, statuses(report)["mismatch"],
                         [c.detail for c in report.checks
                          if c.status == "mismatch"])

    def test_transplanted_receipts_reject(self):
        directory = FIXTURES / "clearance-heuristic-unusable"
        report = verify(
            directory,
            obs="observations-transplant",
            evaluation="evaluation-transplant",
        )
        self.assertEqual(0, statuses(report)["mismatch"])
        evaluation = load(directory / "evaluation-transplant.json")
        self.assertTrue(all(o["state"] == "rejected"
                            for o in evaluation["observations"]))
        self.assertEqual("not_evaluated",
                         evaluation["requirements"]["EL-R1"]["status"])

    def test_forged_evaluation_record_is_flagged(self):
        """An eval record claiming `fail` over honest observations — digests
        honestly recomputed — still replays to `pass` and must mismatch."""
        directory = FIXTURES / "clearance-pass"
        report = verify(directory, evaluation="evaluation-forged-verdict")
        self.assertGreater(statuses(report)["mismatch"], 0)


class ExplainTests(unittest.TestCase):
    def test_semantic_edge_diff(self):
        directory = FIXTURES / "clearance-pass"
        args = Args(
            program=str(directory / "program.json"),
            evaluation_old=str(directory / "evaluation-absent.json"),
            evaluation_new=str(directory / "evaluation.json"),
        )
        report = lv.explain(args)
        names = {c.check for c in report.checks}
        self.assertIn("change.requirement[EL-R1]", names)


if __name__ == "__main__":
    unittest.main()
