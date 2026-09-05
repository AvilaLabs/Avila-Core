#!/usr/bin/env python3
"""Tests for the independent verifier.

Run with: python3 -m unittest discover -s verifier -v
(or, from inside verifier/: python3 -m unittest test_verifier -v)

Every numbered section corresponds to a scope item in
avila_core_verify.py's module docstring, and cites the fixture vector file
or committed example case it proves agreement against.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import avila_core_verify as v

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURES = REPO_ROOT / "fixtures" / "semantic-core"
EXAMPLES = REPO_ROOT / "examples" / "cases"


def load(path: Path):
    return json.loads(path.read_text())


# ---------------------------------------------------------------------------
# Section 2: canonical JSON — fixtures/semantic-core/vectors/canon.v1.json
# ---------------------------------------------------------------------------


class TestCanonVectors(unittest.TestCase):
    def setUp(self):
        self.doc = load(FIXTURES / "vectors" / "canon.v1.json")

    def test_every_canon_vector(self):
        for vec in self.doc["vectors"]:
            op = vec["operation"]
            with self.subTest(id=vec["id"], op=op):
                if op == "canonicalize_json":
                    got = v.canonicalize_json(vec["input_bytes"].encode())
                    self.assertEqual(got, vec["expected_bytes"].encode())
                elif op == "read_authoritative_decimal":
                    if "expected_error" in vec:
                        with self.assertRaises(v.CanonError):
                            v.read_authoritative_decimal(vec["input"])
                    else:
                        got = v.read_authoritative_decimal(vec["input"])
                        self.assertEqual(got, Fraction(vec["expected"]))
                elif op == "lower_authored_decimal":
                    got = v.lower_authored_decimal(vec["input"])
                    self.assertEqual(got, vec["expected"])
                elif op == "read_authoritative_rational":
                    if "expected_error" in vec:
                        with self.assertRaises(v.CanonError):
                            v.read_authoritative_rational(vec["input"])
                    else:
                        got = v.read_authoritative_rational(vec["input"])
                        num, den = vec["expected"].split("/")
                        self.assertEqual(got, Fraction(int(num), int(den)))
                elif op == "read_authoritative_json":
                    if "expected_error" in vec:
                        with self.assertRaises(v.CanonError):
                            v.read_authoritative_json(vec["input_bytes"].encode())
                    else:
                        v.read_authoritative_json(vec["input_bytes"].encode())
                else:
                    self.fail(f"unhandled canon vector operation {op!r}")


# ---------------------------------------------------------------------------
# Section 5 (unit half): fixtures/semantic-core/vectors/unit-scaling.v1.json
# ---------------------------------------------------------------------------


class TestUnitScalingVectors(unittest.TestCase):
    def setUp(self):
        self.doc = load(FIXTURES / "vectors" / "unit-scaling.v1.json")
        self.kinds = v.kinds_from_vector_doc(self.doc["kinds"])

    def test_every_unit_scaling_vector(self):
        for vec in self.doc["vectors"]:
            with self.subTest(id=vec["id"]):
                kind_id = vec["input"]["kind"]
                value = v.read_authoritative_exact(vec["input"]["quantity"]["value"])
                unit = vec["input"]["quantity"]["unit"]
                expected = vec["expected"]
                if "error" in expected:
                    if expected["error"] == "CORE-T2001":
                        kind = self.kinds[kind_id]
                        with self.assertRaises(v.UnitError) as ctx:
                            kind.scale(value, unit)
                        self.assertEqual(ctx.exception.code, "CORE-T2001")
                        self.assertEqual(sorted(ctx.exception.candidates), sorted(expected["repair"]["candidates"]))
                    else:
                        # CORE-T2102: cross-kind refusal. This verifier looks
                        # the unit up only within the *declared* kind's table
                        # (it never guesses a different kind), so the unit is
                        # simply absent from that kind's class -> CORE-T2001
                        # from this implementation's point of view. The
                        # cross-kind repair suggestion itself is a compiler
                        # concern outside this profile (a "convert" capability
                        # this file does not implement) — not exercised at
                        # verdict re-derivation time by any committed case.
                        kind = self.kinds[kind_id]
                        with self.assertRaises(v.UnitError):
                            kind.scale(value, unit)
                    continue
                kind = self.kinds[kind_id]
                got = kind.scale(value, unit)
                self.assertEqual(kind.canonical_unit, expected["canonical_unit"])
                self.assertEqual(got, v.read_authoritative_exact(expected["value"]))


# ---------------------------------------------------------------------------
# Section 5 (verdict half): fixtures/semantic-core/vectors/verdict-calculus.v1.json
# ---------------------------------------------------------------------------


class TestVerdictCalculusVectors(unittest.TestCase):
    def setUp(self):
        self.doc = load(FIXTURES / "vectors" / "verdict-calculus.v1.json")
        self.kinds = v.kinds_from_vector_doc(self.doc["kinds"])

    def _evidence_from_vector(self, ev):
        claim = {"model": ev["model"]}
        for key in ("lower", "upper", "nominal", "coverage", "basis"):
            if key in ev and key != "basis":
                claim[key] = ev[key]
        reduced = v.reduce_claim_value(ev["evidence_id"], ev["state"], claim)
        reduced.aggregation_instance = ev.get("aggregation_instance")
        return reduced

    def test_every_requirement_vector(self):
        for vec in self.doc["vectors"]:
            with self.subTest(id=vec["id"]):
                inp = vec["input"]
                kind = self.kinds[inp["kind"]]
                requirement = inp["requirement"]
                comparison = requirement["comparison"]
                limit = v.read_authoritative_exact(requirement["limit"]["value"])
                limit_unit = requirement["limit"]["unit"]
                basis = requirement["basis"]["kind"]
                coverage_required = v.read_authoritative_exact(requirement["basis"]["coverage"]) if "coverage" in requirement["basis"] else None
                tolerance = v.read_authoritative_exact(requirement["tolerance"]["value"]) if "tolerance" in requirement else None
                tolerance_unit = requirement["tolerance"]["unit"] if "tolerance" in requirement else None
                aggregation = requirement.get("aggregation")
                evidence = [self._evidence_from_vector(ev) for ev in inp["evidence"]]

                result = v.evaluate_numeric_requirement(
                    comparison, limit, kind, limit_unit, basis, coverage_required, tolerance, tolerance_unit, evidence, aggregation=aggregation
                )
                expected = vec["expected"]
                self.assertEqual(result.status, expected["status"], vec["id"])
                self.assertEqual(result.rule, expected["rule"], vec["id"])
                if "canonical_unit" in expected:
                    self.assertEqual(result.canonical_unit, expected["canonical_unit"], vec["id"])
                if "limit_canonical" in expected:
                    self.assertEqual(result.limit_canonical, expected["limit_canonical"], vec["id"])
                if "lower_canonical" in expected:
                    self.assertEqual(result.lower_canonical, expected["lower_canonical"], vec["id"])
                if "upper_canonical" in expected:
                    self.assertEqual(result.upper_canonical, expected["upper_canonical"], vec["id"])
                if "nominal_canonical" in expected:
                    self.assertEqual(result.nominal_canonical, expected["nominal_canonical"], vec["id"])
                if "tolerance_canonical" in expected:
                    self.assertEqual(result.tolerance_canonical, expected["tolerance_canonical"], vec["id"])
                if "basis_visible" in expected:
                    self.assertEqual(result.basis_visible, expected["basis_visible"], vec["id"])
                if expected.get("numbers_present") is False:
                    self.assertIsNone(result.lower_canonical)
                    self.assertIsNone(result.upper_canonical)
                    self.assertIsNone(result.nominal_canonical)

    def test_every_aggregation_status_vector(self):
        for vec in self.doc["aggregation_vectors"]:
            with self.subTest(id=vec["id"]):
                got = v.aggregate_statuses(vec["input"]["aggregation"], vec["input"]["verdicts"])
                self.assertEqual(got, vec["expected"]["status"])


# ---------------------------------------------------------------------------
# Section 5 (campaign fixtures): fixtures/semantic-core/campaigns/campaign-cases.v1.json
#
# Three fixtures need admission facts this profile does not build without a
# compiler (a compiled dataflow graph for A3's parent cascade, a registry
# role's permitted-claim-model list for A6, and the compiler's own
# recomputed compiled-snapshot identity) and are explicitly excluded from
# the pass/fail assertion below, each named and reasoned — never silently
# skipped. Every other fixture's `expected.verdicts` must match exactly.
# ---------------------------------------------------------------------------

NOT_RE_DERIVABLE_CAMPAIGN_FIXTURES = {
    "campaign.parent-missing.not_evaluated": "needs the compiled dataflow graph to cascade a missing input attestation to a downstream step's admission (A3); this profile has no compiler",
    "campaign.model-not-permitted.quarantine": "needs a registry role's declared permitted-claim-model list (A6 type check); this profile does not bind capability-type output slots to registry roles",
    "campaign.snapshot-mismatch.rejected": "needs the compiler's own recomputed compiled_snapshot_sha256 to detect a mismatch; this profile checks compiled-snapshot equality but never recomputes it",
}


class TestCampaignFixtures(unittest.TestCase):
    def setUp(self):
        self.doc = load(FIXTURES / "campaigns" / "campaign-cases.v1.json")
        self.base = FIXTURES / "campaigns"

    def test_every_campaign_fixture(self):
        for fx in self.doc["fixtures"]:
            with self.subTest(id=fx["fixture_id"]):
                contract = load((self.base / fx["contract"]).resolve())
                registry = load((self.base / fx["registry"]).resolve())
                claims = load(self.base / fx["claims"])
                expected_report = {"verdicts": [{"requirement_id": vd["requirement_id"], "verdict": vd} for vd in fx["expected"].get("verdicts", [])]}

                report = v.Report(fx["fixture_id"])
                v.verify_case_verdicts(contract, registry, claims, expected_report, report)

                if fx["fixture_id"] in NOT_RE_DERIVABLE_CAMPAIGN_FIXTURES:
                    continue  # not asserted; the reason is documented above and this fixture is not silently dropped from the loop
                mismatches = [c for c in report.checks if c.status == "mismatch"]
                self.assertEqual(mismatches, [], f"{fx['fixture_id']}: {mismatches}")

    def test_not_re_derivable_fixtures_are_named_not_generic(self):
        # Guards against silently deleting the exclusion list above instead
        # of actually confronting what it means: every fixture in it must
        # still be a real fixture id in the corpus.
        fixture_ids = {fx["fixture_id"] for fx in self.doc["fixtures"]}
        for excluded in NOT_RE_DERIVABLE_CAMPAIGN_FIXTURES:
            self.assertIn(excluded, fixture_ids)


# ---------------------------------------------------------------------------
# Section 3: every committed execution receipt's invocation_sha256
# ---------------------------------------------------------------------------


class TestReceiptInvocationIdentity(unittest.TestCase):
    def test_every_committed_receipt_invocation_identity_reproduces(self):
        receipts = sorted((EXAMPLES).glob("*/receipts/*.json"))
        self.assertGreater(len(receipts), 0, "expected at least one committed receipt")
        for path in receipts:
            with self.subTest(path=str(path.relative_to(EXAMPLES.parent.parent))):
                receipt = load(path)
                computed = v.invocation_identity_from_receipt(receipt)
                self.assertEqual(computed, receipt["invocation_sha256"])


# ---------------------------------------------------------------------------
# Section 6: every committed attempt-lineage log
# ---------------------------------------------------------------------------


class TestAttemptLineage(unittest.TestCase):
    def test_lineage_matches_every_committed_attempts_log(self):
        logs = sorted(EXAMPLES.glob("*/search/attempts.jsonl"))
        self.assertGreater(len(logs), 0, "expected at least one committed attempts.jsonl (CASE-008, CASE-009)")
        for log_path in logs:
            with self.subTest(path=str(log_path.relative_to(EXAMPLES.parent.parent))):
                report = v.Report(str(log_path))
                v.verify_attempt_log(log_path, report)
                mismatches = [c for c in report.checks if c.status == "mismatch"]
                self.assertEqual(mismatches, [])
                self.assertGreater(len(report.checks), 0)


# ---------------------------------------------------------------------------
# Section 5 (margin): every committed campaign-log.jsonl / attempts.jsonl
# margin value in the repository
# ---------------------------------------------------------------------------


class TestMarginRendering(unittest.TestCase):
    def test_margin_rendering_matches_every_committed_log_entry(self):
        logs = sorted(EXAMPLES.glob("**/*.jsonl"))
        self.assertGreater(len(logs), 0)
        report = v.Report("all-logs")
        for log_path in logs:
            v.verify_verdict_log_margins(log_path, report)
        mismatches = [c for c in report.checks if c.status == "mismatch"]
        self.assertEqual(mismatches, [])
        self.assertGreater(report.counts()["verified"], 1000, "expected thousands of margin values across the repository's committed logs")


# ---------------------------------------------------------------------------
# Item 7 (positive path): CASE-000, CASE-001, CASE-002, CASE-003, CASE-008,
# CASE-009 with whichever artifact roots exist on this machine.
# ---------------------------------------------------------------------------


IN_REPO_ROOTS = {
    "case": None,  # filled in per case below (the case's own directory)
    "shielding": EXAMPLES.parent / "capabilities" / "shielding",
    "coupled": EXAMPLES.parent / "capabilities" / "shield-coupled",
    "agents": REPO_ROOT / "examples" / "agents",
    "thermal": EXAMPLES.parent / "capabilities" / "thermal",
    "magnetic-compliance": EXAMPLES.parent / "capabilities" / "magnetic-compliance",
}


class TestPositivePathOnRealCases(unittest.TestCase):
    """Runs the full verifier against every case named in the task's
    profile, with whichever artifact roots this machine actually has
    in-repo (nuclear-data, actinv-data, actinv-release and simsopt are
    external checkouts this repository does not vendor; their artifacts are
    expected to come back `not_checked`, never `mismatch`)."""

    CASES = [
        "case-000-actinv-aftermatter",
        "case-001-shield-search",
        "case-002-coupled-shield",
        "case-003-thermal-spreader",
        "case-008-mode-selective-quench",
        "case-009-ncsx-copper-discharge",
    ]

    def test_every_named_case_has_zero_mismatches(self):
        for case_name in self.CASES:
            case_dir = EXAMPLES / case_name
            with self.subTest(case=case_name):
                roots = {name: path for name, path in IN_REPO_ROOTS.items() if path is not None}
                roots["case"] = case_dir
                report = v.verify_case(case_dir, roots)
                mismatches = [c for c in report.checks if c.status == "mismatch"]
                self.assertEqual(mismatches, [], f"{case_name}: {[m.to_dict() for m in mismatches]}")
                self.assertGreater(report.counts()["verified"], 0)


# ---------------------------------------------------------------------------
# Item 7: mutation tests
#
# Each test corrupts exactly one thing in a scratch copy of a real case,
# re-hashing everything ELSE so the corruption is "self-consistent" (the
# forged file matches its own manifest entry) exactly the way S-030's
# adversarial designer operated — proving this verifier catches a lie that
# stays internally consistent, not just a torn/truncated file. CASE-001 and
# CASE-003 carry no ADR-0014 attempt-lineage log (only CASE-008/CASE-009
# do), so the "log line" mutation uses a scratch copy of CASE-009 instead;
# this substitution is deliberate and stated here, not silent.
# ---------------------------------------------------------------------------


def _write_json(path: Path, obj) -> None:
    path.write_text(json.dumps(obj, indent=2) + "\n")


def _rehash_document(package: dict, document_id: str, new_sha256: str) -> None:
    for doc in package["documents"]:
        if doc["document_id"] == document_id:
            doc["sha256"] = new_sha256
            return
    raise KeyError(document_id)


class TestMutations(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="avila-core-verifier-mutation-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)

    def _copy_case(self, name: str) -> Path:
        dst = Path(self.tmp) / name
        shutil.copytree(EXAMPLES / name, dst)
        return dst

    def test_corrupted_claim_value_is_named_by_verdict_re_derivation(self):
        case_dir = self._copy_case("case-003-thermal-spreader")
        claims_path = case_dir / "claims.json"
        claims = json.loads(claims_path.read_text())
        for c in claims["claims"]:
            if c["claim_id"] == "fe-hotspot-temperature":
                self.assertEqual(c["claim"]["upper"]["value"], "322.82127")
                c["claim"]["upper"]["value"] = "999"  # was safely below the 340 K limit; now wildly above it
        _write_json(claims_path, claims)
        new_claims_bytes = claims_path.read_bytes()
        new_claims_file_sha256 = v.sha256_file(claims_path)
        new_claims_canonical_sha256 = v.sha256_bytes(v.canonicalize_json(new_claims_bytes))

        package_path = case_dir / "package.json"
        package = json.loads(package_path.read_text())
        _rehash_document(package, "case-003-claims", new_claims_file_sha256)
        _write_json(package_path, package)

        report_path = case_dir / "campaign-report.json"
        campaign_report = json.loads(report_path.read_text())
        campaign_report["claims_sha256"] = new_claims_canonical_sha256  # keep the OTHER layers self-consistent
        _write_json(report_path, campaign_report)
        _rehash_document(package, "case-003-expected-campaign", v.sha256_file(report_path))
        _write_json(package_path, package)

        report = v.verify_case(case_dir, {"case": case_dir, "thermal": EXAMPLES.parent / "capabilities" / "thermal"})
        by_check = {c.check: c for c in report.checks}
        self.assertEqual(by_check["package.document.case-003-claims"].status, "verified")
        self.assertEqual(by_check["claims.claims_sha256_matches_campaign_report"].status, "verified")
        self.assertEqual(by_check["verdict.THERM-R2-hotspot"].status, "mismatch")

    def test_corrupted_receipt_output_digest_is_named_by_receipt_verification(self):
        case_dir = self._copy_case("case-003-thermal-spreader")
        receipt_path = case_dir / "receipts" / "fe.json"
        receipt = json.loads(receipt_path.read_text())
        real_digest = receipt["outputs"][0]["sha256"]
        forged_digest = "sha256:" + ("0" * 63 + "1" if real_digest[-1] != "1" else "0" * 64)
        self.assertNotEqual(forged_digest, real_digest)
        receipt["outputs"][0]["sha256"] = forged_digest
        _write_json(receipt_path, receipt)

        package_path = case_dir / "package.json"
        package = json.loads(package_path.read_text())
        _rehash_document(package, "case-003-fe-receipt", v.sha256_file(receipt_path))
        _write_json(package_path, package)

        report = v.verify_case(case_dir, {"case": case_dir, "thermal": EXAMPLES.parent / "capabilities" / "thermal"})
        by_check = {c.check: c for c in report.checks}
        self.assertEqual(by_check["package.document.case-003-fe-receipt"].status, "verified")
        self.assertEqual(by_check["receipt.fe.invocation_identity"].status, "verified")
        self.assertEqual(by_check["receipt.fe.output.hotspot-temperature"].status, "mismatch")

    def test_corrupted_manifest_entry_is_named_by_package_identity(self):
        case_dir = self._copy_case("case-001-shield-search")
        package_path = case_dir / "package.json"
        package = json.loads(package_path.read_text())
        _rehash_document(package, "case-001-contract", "sha256:" + "0" * 64)
        _write_json(package_path, package)

        report = v.verify_case(case_dir, {"case": case_dir})
        by_check = {c.check: c for c in report.checks}
        self.assertEqual(by_check["package.document.case-001-contract"].status, "mismatch")

    def test_corrupted_log_line_is_named_by_attempt_lineage(self):
        case_dir = self._copy_case("case-009-ncsx-copper-discharge")  # CASE-001/003 carry no attempt-lineage log; see class docstring
        log_path = case_dir / "search" / "attempts.jsonl"
        lines = log_path.read_bytes().splitlines()
        self.assertEqual(len(lines), 2)
        parent = json.loads(lines[0])
        self.assertEqual(parent["attempt"]["attempt_id"], "copper-ratio2-r0")
        # Edit something outside candidate_state so candidate_state_sha256
        # stays self-consistent and only the parent-line binding is stale.
        parent["verdicts"][0]["margin"] = "-0.000000000001"
        lines[0] = json.dumps(parent).encode()
        log_path.write_bytes(b"\n".join(lines) + b"\n")

        report = v.Report(str(log_path))
        v.verify_attempt_log(log_path, report)
        by_check = {c.check: c for c in report.checks}
        self.assertEqual(by_check["lineage.copper-ratio2-r0.candidate_state_sha256"].status, "verified")
        self.assertEqual(by_check["lineage.copper-ratio4-r1.parent_record_sha256"].status, "mismatch")

    def test_corrupted_verdict_margin_is_named_by_margin_cross_check(self):
        case_dir = self._copy_case("case-001-shield-search")
        log_path = case_dir / "search" / "campaign-log.jsonl"
        lines = log_path.read_bytes().splitlines()
        row0 = json.loads(lines[0])
        found = False
        for vd in row0["verdicts"]:
            if vd.get("requirement_id") == "SHIELD-R4-thickness":
                self.assertEqual(vd["margin"], "30")
                vd["margin"] = "12345"
                found = True
        self.assertTrue(found)
        lines[0] = json.dumps(row0).encode()
        log_path.write_bytes(b"\n".join(lines) + b"\n")

        report = v.Report(str(log_path))
        v.verify_verdict_log_margins(log_path, report)
        matching = [c for c in report.checks if c.check.startswith(f"log.{log_path.name}:0.SHIELD-R4-thickness")]
        self.assertEqual(len(matching), 1)
        self.assertEqual(matching[0].status, "mismatch")


if __name__ == "__main__":
    unittest.main()
