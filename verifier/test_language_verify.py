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
        "positive-matmul-pass",
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
        "positive-matmul-pass": ("pass", "bounded.le"),
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


# ---------------------------------------------------------------------------
# Audit-parity regressions — branches the committed corpus never exercises
# ---------------------------------------------------------------------------


def _mini():
    """A one-application program + library for `Replay` unit tests."""
    library = {
        "schema_version": "avila.core/method-library/v0.1-draft",
        "profile": "avila.core/language/0.1-draft",
        "library": {"name": "mini", "revision": 1},
        "quantity_kinds": {
            "length": {"canonical_unit": "mm"},
            "coeff": {"canonical_unit": "1/K"},
        },
        "kind_products": [
            {
                "lhs_kind": "coeff",
                "rhs_kind": "length",
                "result_kind": "length",
                "result_unit": "mm",
            }
        ],
        "propositions": {"symmetric": {"params": ["geometry"]}},
        "methods": [
            {
                "id": "scale",
                "variables": {"g": "geometry"},
                "inputs": {
                    "coefficient": {
                        "quantity_kind": "coeff",
                        "claim": "enclosure",
                    },
                    "length": {
                        "quantity_kind": "length",
                        "claim": "enclosure",
                        "geometry": "g",
                    },
                },
                "output": {
                    "quantity_kind": "length",
                    "claim": "enclosure",
                    "geometry": "g",
                },
                "ensures": [
                    {
                        "kind": "relation",
                        "expression": "output = coefficient * length",
                        "check": "interval_arithmetic",
                    }
                ],
                "implementation": {
                    "kind": "primitive",
                    "body": "interval.mul(coefficient, length)",
                },
            }
        ],
    }
    program = {
        "schema_version": "avila.core/language-program/v0.1-draft",
        "profile": "avila.core/language/0.1-draft",
        "id": "mini",
        "library": {"name": "mini", "revision": 1, "semantic_sha256": "sha256:x"},
        "entities": {"geometries": {"bracket@2": {}}},
        "inputs": [
            {
                "id": "length",
                "type": {
                    "quantity_kind": "length",
                    "claim": "enclosure",
                    "geometry": "bracket@2",
                },
                "binding": {
                    "state": "bound",
                    "value": {"kind": "exact", "value": "10", "unit": "mm"},
                    "source": {
                        "kind": "assertion",
                        "party": "drawing@1",
                        "edge": "e-length",
                    },
                },
            },
            {
                "id": "coefficient",
                "type": {"quantity_kind": "coeff", "claim": "exact"},
                "binding": {
                    "state": "bound",
                    "value": {"kind": "exact", "value": "2", "unit": "1/K"},
                    "source": {
                        "kind": "assertion",
                        "party": "cert@1",
                        "edge": "e-coeff",
                    },
                },
            },
        ],
        "body": [
            {
                "bind": "out",
                "apply": "scale",
                "arguments": {
                    "length": {"ref": "length"},
                    "coefficient": {"ref": "coefficient"},
                },
            }
        ],
        "requirements": [
            {
                "id": "R1",
                "subject": {"ref": "out"},
                "comparison": "ge",
                "quantity_kind": "length",
                "limit": {"kind": "exact", "value": "15", "unit": "mm"},
            }
        ],
        "premises": [],
        "assumptions": [],
    }
    return program, library


def _external_method(library):
    method = json.loads(json.dumps(library["methods"][0]))
    method["id"] = "scale-ext"
    method["implementation"] = {
        "kind": "external",
        "executable": "synthetic/scale@1",
        "produces": {"quantity_kind": "length", "unit": "mm"},
    }
    library["methods"].append(method)
    return method


def _honest_record(replay, index: int, method: dict, out_decl: dict) -> dict:
    """Build a fully self-consistent observation record the way the runner
    emits it — every digest honestly recomputed."""
    at = f"body[{index}]"
    step = replay.program["body"][index]
    slot_values = {
        slot: replay.env[arg["ref"]]
        for slot, arg in step.get("arguments", {}).items()
    }
    staged = {
        slot: lv.staged_input_sha256(v)
        for slot, v in slot_values.items()
        if lv.staged_input_sha256(v) is not None
    }
    out_sha = lv.canon_sha256(out_decl)
    receipt = {
        "at": at,
        "executable": method["implementation"]["executable"],
        "executable_sha256": "sha256:" + "aa" * 32,
        "inputs": [
            {"slot": slot, "sha256": sha}
            for slot, sha in sorted(staged.items())
        ],
        "invocation": {},
        "outputs": [
            {"output_id": "output", "state": "collected", "sha256": out_sha}
        ],
        "status": "completed",
        "plan_sha256": replay.expected_plan_sha256,
    }
    receipt["invocation_sha256"] = lv.canon_sha256(
        {
            "executable": receipt["executable"],
            "executable_sha256": receipt["executable_sha256"],
            "inputs": receipt["inputs"],
            "invocation": receipt["invocation"],
        }
    )
    record = {
        "at": at,
        "bind": step["bind"],
        "executable": method["implementation"]["executable"],
        "inputs": staged,
        "output": out_decl,
        "output_sha256": out_sha,
        "receipt": receipt,
    }
    record["receipt_sha256"] = lv.canon_sha256(receipt)
    replay.expected_plan_sha256 = receipt["plan_sha256"]
    replay.site_map = {index: record}
    return record


class ParityTests(unittest.TestCase):
    """EL-04 audit regressions — each is a branch the committed corpus does
    not exercise, checked against the analyzer's semantics."""

    def test_noncanonical_number_spellings_rejected(self):
        # The canonical exact-number grammar, not Python's permissive
        # Fraction parsing, governs authoritative fields.
        for bad in ("+3", "1.50", "1e2", "007", "0.0", "-0", "6/20", " 3",
                    "3 ", "1/0", "1.5e3"):
            with self.assertRaises(Exception, msg=bad):
                lv.exact_number(bad)
        for good in ("0", "-3", "1.5", "5/2", "42"):
            self.assertIsInstance(lv.exact_number(good), Fraction, good)

    def test_eval_state_join(self):
        ty = lv.QuantityType("q", "enclosure")

        def mk(state):
            return lv.SemVal(
                ty, "u",
                lv.Numeric(lower=Fraction(1), upper=Fraction(2)), state,
            )

        operands = {
            "est": mk("established"),
            "dec": mk("declared"),
            "uns": mk("unestablished"),
        }
        cases = {
            "est + est": "established",
            "est + dec": "declared",
            "dec + dec": "declared",
            "dec + uns": "unestablished",
            "est + uns": "unestablished",
        }
        for text, want in cases.items():
            with self.subTest(expr=text):
                out = lv.eval_expression(
                    lv.parse_expression(text), operands, {}
                )
                self.assertEqual(want, out.state)
                self.assertIsNotNone(out.value)

    def test_expression_budgets_and_ascii_whitespace(self):
        with self.assertRaises(lv.EvalFailure):
            lv.parse_expression("(" * 200 + "a" + ")" * 200)
        with self.assertRaises(lv.EvalFailure):
            lv.parse_expression("a" + "+a" * 5000)
        # Non-ASCII whitespace is not whitespace in the grammar — the
        # document cannot sneak a token boundary past the parser.
        with self.assertRaises(lv.EvalFailure):
            lv.parse_expression("a\xa0+ b")
        self.assertIsNotNone(lv.parse_ensures("output = a\t+\nb"))

    def test_step_must_carry_exactly_one_operation(self):
        program, library = _mini()
        step = {
            "bind": "x",
            "apply": "scale",
            "infer": {
                "rule": "interval.add",
                "arguments": [{"ref": "length"}],
            },
            "arguments": {
                "length": {"ref": "length"},
                "coefficient": {"ref": "coefficient"},
            },
        }
        program["body"].append(step)
        replay = lv.Replay(program, library)
        replay.run()
        # A malformed step produces no binding — the first-match dispatch
        # that would have bound `x` is exactly what the audit removed.
        self.assertNotIn("x", replay.env)

    def test_lifecycle_refusal_and_conflict(self):
        program, library = _mini()
        replay = lv.Replay(
            program, library, lifecycle=[("library:mini@1", "withdrawn")]
        )
        replay.run()
        self.assertEqual("unestablished", replay.env["out"].state)
        self.assertEqual(
            "not_evaluated.lifecycle_refused", replay.blocked["out"]
        )

        replay = lv.Replay(
            program,
            library,
            lifecycle=[
                ("library:mini@1", "expired"),
                ("library:mini@1", "withdrawn"),
            ],
        )
        replay.run()
        # A conflict poisons the binding but is not a refusal.
        self.assertEqual("unestablished", replay.env["out"].state)
        self.assertNotIn("out", replay.blocked)

        replay = lv.Replay(
            program, library, lifecycle=[("library:mini@1", "superseded")]
        )
        replay.run()
        self.assertEqual("established", replay.env["out"].state)

    def test_contradicted_assumptions_block_dependents(self):
        program, library = _mini()
        library["methods"][0]["assumes"] = ["symmetric"]
        program["assumptions"] = [
            {"asserts": "symmetric", "at": {"geometry": "bracket@2"}},
            {"denies": "symmetric", "at": {"geometry": "bracket@2"}},
        ]
        replay = lv.Replay(program, library)
        replay.run()
        # The carried assumption is asserted AND denied at this scope — the
        # binding it grounds is blocked for every downstream verdict.
        self.assertEqual(
            "not_evaluated.contradiction", replay.blocked["out"]
        )

    def test_premise_provenance_must_be_admissible(self):
        program, library = _mini()
        library["methods"][0]["assumes"] = ["symmetric"]
        program["assumptions"] = [
            {"asserts": "symmetric", "at": {"geometry": "bracket@2"}}
        ]
        # A certificate naming a check the profile does not support cannot
        # discharge — the assumption stays residual in the verdict detail.
        program["premises"] = [
            {
                "id": "p1",
                "proposition": "symmetric",
                "at": {"geometry": "bracket@2"},
                "established_by": {
                    "kind": "certificate",
                    "digest": "sha256:" + "ab" * 32,
                    "check": "other_engine",
                },
            }
        ]
        replay = lv.Replay(program, library)
        replay.run()
        verdicts = replay.requirement_reports()
        self.assertIn(
            "conditional on", verdicts["R1"]["verdict"]["detail"]
        )
        # An unattributed premise is likewise not a witness.
        program["premises"][0]["established_by"] = {"kind": "assertion"}
        replay = lv.Replay(program, library)
        replay.run()
        verdicts = replay.requirement_reports()
        self.assertIn(
            "conditional on", verdicts["R1"]["verdict"]["detail"]
        )

    def test_operands_must_satisfy_slot_claim_and_kind(self):
        program, library = _mini()
        # Nominal operand into an `enclosure`-claimed slot — Rust's
        # `ClaimModel::satisfies` refuses this; the binding must poison.
        program["inputs"][1]["type"]["claim"] = "nominal"
        program["inputs"][1]["binding"]["value"] = {
            "kind": "nominal",
            "unit": "1/K",
        }
        replay = lv.Replay(program, library)
        replay.run()
        self.assertEqual("unestablished", replay.env["out"].state)

    def test_import_source_provenance_completeness(self):
        program, library = _mini()
        base_import = {
            "type": {"quantity_kind": "coeff", "claim": "exact"},
            "value": {"kind": "exact", "value": "3", "unit": "1/K"},
            "assumptions": [],
        }

        def bind_cert(source):
            step = {
                "bind": "cert",
                "import": dict(base_import, **({"source": source} if source else {})),
            }
            program["body"] = [s for s in program["body"] if s.get("bind") != "cert"]
            program["body"].insert(0, step)
            replay = lv.Replay(program, library)
            replay.run()
            return replay.env["cert"].state

        # Certificate without a supported check → malformed → unestablished.
        self.assertEqual(
            "unestablished",
            bind_cert({"kind": "certificate", "digest": "sha256:" + "ab" * 32}),
        )
        # Assertion without a party → malformed → unestablished.
        self.assertEqual(
            "unestablished", bind_cert({"kind": "assertion"})
        )
        # A malformed digest does not pass format admission either.
        self.assertEqual(
            "unestablished",
            bind_cert(
                {
                    "kind": "certificate",
                    "digest": "sha256:not-hex",
                    "check": "interval_arithmetic",
                }
            ),
        )
        # No `source` field at all → claims nothing → admissible.
        self.assertEqual("established", bind_cert(None))

    def test_primitive_ensures_need_a_supported_check(self):
        program, library = _mini()
        library["methods"][0]["ensures"] = [
            {
                "kind": "relation",
                "expression": "output = coefficient * length",
                "check": "oracle",
            }
        ]
        replay = lv.Replay(program, library)
        replay.run(evaluating=True)
        # An unsupported `check` name never discharges — the obligation
        # stays open and blocks the binding's verdicts.
        self.assertEqual(
            (
                "postcondition",
                "open",
                "`output = coefficient * length` checked by oracle",
            ),
            replay.obligation_states["body[0].ensures[0]"],
        )
        self.assertEqual(
            "not_evaluated.obligation_unmet", replay.blocked["out"]
        )

    def test_unknown_comparison_is_malformed_not_le(self):
        program, library = _mini()
        program["requirements"][0]["comparison"] = "gt"
        replay = lv.Replay(program, library)
        replay.run()
        verdicts = replay.requirement_reports()
        self.assertEqual(
            "not_evaluated.malformed", verdicts["R1"]["verdict"]["rule"]
        )

    def test_observation_binding_checks(self):
        program, library = _mini()
        _external_method(library)
        program["body"] = [
            {
                "bind": "out",
                "apply": "scale-ext",
                "arguments": {
                    "length": {"ref": "length"},
                    "coefficient": {"ref": "coefficient"},
                },
            }
        ]
        out_decl = {
            "kind": "enclosure",
            "lower": "19",
            "upper": "21",
            "unit": "mm",
        }

        plan_sha = "sha256:" + "bb" * 32

        def fresh_replay():
            replay = lv.Replay(program, library)
            replay.run()  # analyze pass — operands staged
            replay.expected_plan_sha256 = plan_sha
            return replay

        # Baseline: an honestly built record binds.
        replay = fresh_replay()
        record = _honest_record(replay, 0, library["methods"][1], out_decl)
        assert record["receipt"]["plan_sha256"] == plan_sha
        observed = replay.bind_observation(
            0, library["methods"][1],
            {s: replay.env[a["ref"]] for s, a in program["body"][0]["arguments"].items()},
            {}, record,
        )
        self.assertIsNotNone(observed)
        self.assertEqual("established", observed.state)

        # The record-level `executable` must match the declaration — a
        # record whose receipt is honest but whose own field lies is
        # foreign.
        forged = json.loads(json.dumps(record))
        forged["executable"] = "synthetic/other@1"
        self.assertIsNone(
            fresh_replay().bind_observation(
                0, library["methods"][1],
                {s: fresh_replay().env[a["ref"]] for s, a in program["body"][0]["arguments"].items()},
                {}, forged,
            )
        )

        # A decoy `outputs` row before the real one must not shadow the
        # `output` slot's collected state.
        moved = json.loads(json.dumps(record))
        moved["receipt"]["outputs"] = [
            {"output_id": "log", "state": "collected", "sha256": "sha256:" + "00" * 32},
            *moved["receipt"]["outputs"],
        ]
        moved["receipt_sha256"] = lv.canon_sha256(moved["receipt"])
        replay = fresh_replay()
        observed = replay.bind_observation(
            0, library["methods"][1],
            {s: replay.env[a["ref"]] for s, a in program["body"][0]["arguments"].items()},
            {}, moved,
        )
        self.assertIsNotNone(observed)

        # The right id but a non-collected state never binds.
        stale = json.loads(json.dumps(record))
        stale["receipt"]["outputs"][0]["state"] = "dropped"
        stale["receipt_sha256"] = lv.canon_sha256(stale["receipt"])
        replay = fresh_replay()
        self.assertIsNone(
            replay.bind_observation(
                0, library["methods"][1],
                {s: replay.env[a["ref"]] for s, a in program["body"][0]["arguments"].items()},
                {}, stale,
            )
        )

        # A duplicate slot row in the receipt must not collapse into the
        # expected digest set.
        dup = json.loads(json.dumps(record))
        dup["receipt"]["inputs"].append(dict(dup["receipt"]["inputs"][0]))
        dup["receipt"]["invocation_sha256"] = lv.canon_sha256(
            {
                "executable": dup["receipt"]["executable"],
                "executable_sha256": dup["receipt"]["executable_sha256"],
                "inputs": dup["receipt"]["inputs"],
                "invocation": dup["receipt"]["invocation"],
            }
        )
        dup["receipt_sha256"] = lv.canon_sha256(dup["receipt"])
        replay = fresh_replay()
        self.assertIsNone(
            replay.bind_observation(
                0, library["methods"][1],
                {s: replay.env[a["ref"]] for s, a in program["body"][0]["arguments"].items()},
                {}, dup,
            )
        )


class CompletenessTests(unittest.TestCase):
    """The record may not *omit* what the replay derives — completeness is
    checked in both directions, not just row-by-row."""

    def _verify_with(self, directory, mutate):
        import tempfile

        evaluation = load(directory / "evaluation.json")
        mutate(evaluation)
        # Recompute the self-digests honestly — the forged record is then
        # internally consistent, so only the semantic checks can catch it.
        context = evaluation["context"]
        evaluation["context_sha256"] = lv.canon_sha256(context)
        body = {k: v for k, v in evaluation.items() if k != "evaluation_sha256"}
        evaluation["evaluation_sha256"] = lv.canon_sha256(body)
        with tempfile.TemporaryDirectory() as tmp:
            forged = Path(tmp) / "evaluation.json"
            forged.write_text(json.dumps(evaluation))
            return self._run(directory, forged)

    def _run(self, directory, forged):
        args = Args(
            program=str(directory / "program.json"),
            library=str(directory / "library.json"),
            plan=str(directory / "plan.json"),
            observations=str(directory / "observations.json"),
            evaluation=str(forged),
            analysis=str(directory / "analysis.json"),
        )
        return lv.verify_evaluation(args)

    def test_omitted_obligation_row_mismatches(self):
        directory = FIXTURES / "clearance-pass"

        def drop(ev):
            ev["obligations"] = ev["obligations"][:-1]

        report = self._verify_with(directory, drop)
        self.assertTrue(
            any(c.status == "mismatch" for c in report.checks),
            "a record dropping a replayed obligation must not verify",
        )

    def test_forged_obligation_row_mismatches(self):
        directory = FIXTURES / "clearance-pass"

        def add(ev):
            ev["obligations"].append(
                {
                    "rule": "postcondition",
                    "subject": "body[7].ensures[0]",
                    "state": "discharged",
                    "detail": "invented",
                }
            )

        report = self._verify_with(directory, add)
        self.assertTrue(
            any(
                c.status == "mismatch" and "body[7]" in c.check
                for c in report.checks
            ),
            "a record inventing an obligation must not verify",
        )

    def test_omitted_requirement_mismatches(self):
        directory = FIXTURES / "clearance-pass"

        def drop(ev):
            ev["requirements"] = {}

        report = self._verify_with(directory, drop)
        self.assertTrue(
            any(
                c.status == "mismatch" and "EL-R1" in c.check
                for c in report.checks
            ),
            "a record omitting the verdict must not verify",
        )

    def test_forged_application_event_mismatches(self):
        directory = FIXTURES / "clearance-pass"

        def add(ev):
            ev["applications"].append(
                {
                    "rule": "discharge",
                    "subject": "body[0] linear-expansion",
                    "state": "discharged",
                    "detail": "fabricated second discharge",
                }
            )

        report = self._verify_with(directory, add)
        self.assertTrue(
            any(
                c.status == "mismatch" and c.check == "application-events"
                for c in report.checks
            ),
            "a record with a fabricated event must not verify",
        )


if __name__ == "__main__":
    unittest.main()


# ---------------------------------------------------------------------------
# Admission parity — inadmissible documents carry no semantic identity
# ---------------------------------------------------------------------------


class AdmissionParityTests(unittest.TestCase):
    """The context identity gate is fail-closed in both directions: honest
    records over refused documents verify `""`, and a forged identity on an
    inadmissible document — self-digests honestly recomputed — still
    mismatches the semantic check."""

    def test_inadmissible_program_verifies_empty_identity(self):
        report = verify(FIXTURES / "inadmissible-program")
        counts = statuses(report)
        self.assertEqual(0, counts["mismatch"], [
            c.detail for c in report.checks if c.status == "mismatch"
        ])

    def test_inadmissible_library_verifies_empty_identity(self):
        report = verify(FIXTURES / "inadmissible-library")
        counts = statuses(report)
        self.assertEqual(0, counts["mismatch"], [
            c.detail for c in report.checks if c.status == "mismatch"
        ])

    def _forged_identity(self, directory, field):
        import tempfile
        evaluation = load(directory / "evaluation.json")
        program = load(directory / "program.json")
        # Claim the projection digest the document would publish if it
        # admitted — internally consistent digests, wrong verdict.
        evaluation["context"][field] = lv.semantic_sha256(
            program if field == "program_sha256" else load(
                directory / "library.json"
            ),
            "program" if field == "program_sha256" else "library",
        )
        evaluation["context_sha256"] = lv.canon_sha256(evaluation["context"])
        body = {k: v for k, v in evaluation.items() if k != "evaluation_sha256"}
        evaluation["evaluation_sha256"] = lv.canon_sha256(body)
        with tempfile.TemporaryDirectory() as tmp:
            forged = Path(tmp) / "evaluation.json"
            forged.write_text(json.dumps(evaluation))
            args = Args(
                program=str(directory / "program.json"),
                library=str(directory / "library.json"),
                plan=str(directory / "plan.json"),
                observations=str(directory / "observations.json"),
                evaluation=str(forged),
                analysis=str(directory / "analysis.json"),
            )
            report = lv.verify_evaluation(args)
        kinds = {c.check for c in report.checks if c.status == "mismatch"}
        self.assertIn(f"{field.split('_')[0]}-semantic-identity", kinds)

    def test_forged_program_identity_on_inadmissible_rejects(self):
        self._forged_identity(
            FIXTURES / "inadmissible-program", "program_sha256"
        )

    def test_forged_library_identity_on_inadmissible_rejects(self):
        self._forged_identity(
            FIXTURES / "inadmissible-library", "library_sha256"
        )

    def _mini_admission(self, mutate=None):
        program, library = _mini()
        if mutate:
            mutate(program, library)
        replay = lv.Replay(program, library, [])
        replay.run(evaluating=False, site_map={})
        return lv.program_admission_ok(
            program, library, replay.finding_kinds
        ), lv.library_admission_ok(library), replay

    def test_undeclared_proposition_unadmits(self):
        ok, lib, _ = self._mini_admission(
            lambda p, l: p["assumptions"].append(
                {"id": "a-bogus", "asserts": "no-such-proposition",
                 "at": {"geometry": "g"}}
            )
        )
        self.assertFalse(ok)
        self.assertTrue(lib)

    def test_scope_key_outside_declared_params_unadmits(self):
        # `independent` is a built-in — its params are the relation names;
        # a `bogus` scope key is malformed.
        ok, lib, _ = self._mini_admission(
            lambda p, l: p["assumptions"].append(
                {"id": "a-scoped", "asserts": "independent",
                 "at": {"bogus": "x"}}
            )
        )
        self.assertFalse(ok)

    def test_forward_reference_unadmits(self):
        ok, _, _ = self._mini_admission(
            lambda p, l: p["body"][0]["arguments"].__setitem__(
                "length", {"ref": "ghost"}
            )
        )
        self.assertFalse(ok)

    def test_shadow_binding_unadmits(self):
        ok, _, _ = self._mini_admission(
            lambda p, l: p["body"][0].__setitem__("bind", "length")
        )
        self.assertFalse(ok)

    def test_unsupported_certificate_check_is_not_admission(self):
        # `unsupported` is not an admission kind — the premise is
        # inadmissible as a witness, but the document still publishes a
        # semantic identity.
        def mut(p, l):
            p["premises"].append({
                "id": "w", "proposition": "independent",
                "arguments": {"over": ["out"]},
                "at": {},
                "established_by": {
                    "kind": "certificate",
                    "digest": "sha256:" + "ab" * 32,
                    "check": "not-a-supported-check",
                },
            })
        ok, lib, replay = self._mini_admission(mut)
        self.assertTrue(ok)
        self.assertIn("unsupported", replay.finding_kinds)
        self.assertNotIn("malformed", replay.finding_kinds & {"malformed"})

    def test_missing_certificate_check_is_admission(self):
        def mut(p, l):
            p["premises"].append({
                "id": "w", "proposition": "independent",
                "arguments": {"over": ["out"]},
                "at": {},
                "established_by": {
                    "kind": "certificate",
                    "digest": "sha256:" + "ab" * 32,
                },
            })
        ok, _, _ = self._mini_admission(mut)
        self.assertFalse(ok)

    def test_bad_slot_kind_unadmits_library(self):
        _, lib, _ = self._mini_admission(
            lambda p, l: l["methods"][0]["inputs"]["length"].__setitem__(
                "quantity_kind", "bogus"
            )
        )
        self.assertFalse(lib)

    def test_unknown_requires_kind_unadmits_library(self):
        _, lib, _ = self._mini_admission(
            lambda p, l: l["methods"][0].setdefault("requires", []).append(
                {"kind": "no-such-kind"}
            )
        )
        self.assertFalse(lib)

    def test_body_names_outside_inputs_unadmit_library(self):
        _, lib, _ = self._mini_admission(
            lambda p, l: l["methods"][0]["implementation"].__setitem__(
                "body", "interval.add(length, ghost)"
            )
        )
        self.assertFalse(lib)

    def test_inverted_entity_interval_unadmits(self):
        ok, _, _ = self._mini_admission(
            lambda p, l: p["entities"].setdefault("scenarios", {}).__setitem__(
                "field",
                {"scope": "steady-state",
                 "operating_domain": {"quantity_kind": "length", "unit": "mm",
                                      "lower": "3", "upper": "1"}},
            )
        )
        self.assertFalse(ok)

    def test_runtime_malformed_finding_unadmits(self):
        # An admission-kind finding emitted during the analyze run — here
        # a `malformed` from an undeclared method id — also unadmits.
        ok, _, _ = self._mini_admission(
            lambda p, l: p["body"][0].__setitem__("apply", "no-such-method")
        )
        self.assertFalse(ok)

    def test_kind_without_canonical_unit_unadmits_library(self):
        # EL-02 review: a unitless kind bypassed every unit check — values
        # of one kind could carry mixed units into the additive rules.
        _, lib, _ = self._mini_admission(
            lambda p, l: l["quantity_kinds"]["coeff"].pop("canonical_unit")
        )
        self.assertFalse(lib)

    def test_mixed_unit_addition_fails(self):
        # The eval gate directly: add of equal kinds with different units
        # is a type_mismatch rule failure, not a silently wrong result.
        from fractions import Fraction
        from language_verify import (
            eval_expression, EvalFailure, Numeric, QuantityType, SemVal)
        ty = QuantityType("length", "enclosure", {})
        operands = {
            "a": SemVal(ty, "mm", Numeric(lower=Fraction(1), upper=Fraction(2)),
                        "established", [], set()),
            "b": SemVal(ty, "cm", Numeric(lower=Fraction(10), upper=Fraction(20)),
                        "established", [], set()),
        }
        with self.assertRaises(EvalFailure) as ctx:
            eval_expression(("call", "add", [("name", "a"), ("name", "b")]),
                            operands, {})
        self.assertEqual(ctx.exception.kind, "type_mismatch")
