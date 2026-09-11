#!/usr/bin/env python3
"""Tests for `adversarial_check.py`'s ADR-0015 log-line signature check.

Run with: python3 -m unittest examples/cases/case-002-coupled-shield/test_adversarial_check.py -v
(from the repository root; both this file and the module it imports resolve
their own paths from `__file__`, not from the current working directory.)

`TestLogLineSignature` needs a real `--runner-key`-signed campaign log to
check against, which needs a built `avila-core` binary. It produces that log
itself, once, from CASE-003 (the one reference case with no external root)
into a scratch directory, by invoking the binary directly — never `cargo`.
If no built binary can be found (checked via `AVILA_CORE_BIN`,
`CARGO_TARGET_DIR`, or the repository's own `target/`), the whole class is
skipped rather than failing: this file never builds anything itself.
"""

from __future__ import annotations

import copy
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import adversarial_check as ac  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[3]
CASE_003 = REPO_ROOT / "examples" / "cases" / "case-003-thermal-spreader"
THERMAL = REPO_ROOT / "examples" / "capabilities" / "thermal"
TRUST_ROOT = REPO_ROOT / "examples" / "keys" / "trust-root.json"
RUNNER_SEED = REPO_ROOT / "examples" / "keys" / "runner.seed"


def find_avila_core_binary() -> Path | None:
    """A pre-built `avila-core` binary, or `None`. Never builds one: this
    module runs as a plain Python test, outside the Rust build's own
    `cargo test`, and this repository's rule that every `cargo` invocation
    goes through its serializing wrapper applies to a person or agent
    driving the build, not to a test that only ever shells out to whatever
    binary already exists.
    """
    candidates = []
    if explicit := os.environ.get("AVILA_CORE_BIN"):
        candidates.append(Path(explicit))
    if target_dir := os.environ.get("CARGO_TARGET_DIR"):
        candidates += [
            Path(target_dir) / "debug" / "avila-core",
            Path(target_dir) / "release" / "avila-core",
        ]
    candidates += [
        REPO_ROOT / "target" / "debug" / "avila-core",
        REPO_ROOT / "target" / "release" / "avila-core",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return None


@unittest.skipUnless(find_avila_core_binary(), "no built avila-core binary found (set AVILA_CORE_BIN)")
class TestLogLineSignature(unittest.TestCase):
    """`verify_log_line_signature` and `classify_row` against a real,
    `--runner-key`-signed CASE-003 log, produced fresh into a scratch
    directory so nothing here depends on a committed fixture drifting out
    of sync with the signing format.
    """

    @classmethod
    def setUpClass(cls):
        cls.scratch = Path(tempfile.mkdtemp(prefix="avila-core-adversarial-check-test-"))
        cls.log_path = cls.scratch / "campaign-log.jsonl"
        binary = find_avila_core_binary()
        result = subprocess.run(
            [
                str(binary),
                "run",
                str(CASE_003),
                "--source-root",
                f"case={CASE_003}",
                "--source-root",
                f"thermal={THERMAL}",
                "--trust-root",
                str(TRUST_ROOT),
                "--runner-key",
                str(RUNNER_SEED),
                "--log",
                str(cls.log_path),
            ],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, (
            f"producing the fixture log failed (exit {result.returncode}):\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )
        lines = [line for line in cls.log_path.read_text().splitlines() if line.strip()]
        assert len(lines) == 1, f"expected exactly one log row, found {len(lines)}"
        cls.row = json.loads(lines[0])
        cls.trust_root = ac.load_trust_root(TRUST_ROOT)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.scratch, ignore_errors=True)

    def test_the_fixture_row_actually_carries_a_log_line_signature(self):
        # A precondition for every other test here, not the thing under
        # test: if this ever stops holding, `--runner-key` stopped signing
        # log lines and every test below would otherwise pass vacuously.
        signature = self.row.get("signature")
        self.assertIsNotNone(signature, self.row)
        self.assertEqual(signature["signed_document"]["role"], "log_line")
        self.assertEqual(signature["algorithm"], "ed25519")

    def test_an_unedited_signed_row_verifies_against_the_trust_root(self):
        state, key_id = ac.verify_log_line_signature(self.row, self.trust_root)
        self.assertEqual(state, "verified", key_id)
        self.assertEqual(key_id, self.row["signature"]["key_id"])

    def test_a_row_edited_after_signing_is_invalid_however_small_the_edit(self):
        # The row's own verdicts are still `pass`/`fail` at this schema
        # version; flipping one status is exactly the shape of edit a
        # forger would make, and it touches nothing about the signature
        # document itself.
        edited = copy.deepcopy(self.row)
        self.assertTrue(edited["verdicts"], "CASE-003's reference row must carry verdicts to edit")
        first_status = edited["verdicts"][0]["status"]
        edited["verdicts"][0]["status"] = "pass" if first_status != "pass" else "fail"
        state, detail = ac.verify_log_line_signature(edited, self.trust_root)
        self.assertEqual(state, "invalid", detail)
        self.assertIn("changed after it was signed", detail)

    def test_classification_of_an_edited_row_is_refused_not_undetected_or_honest(self):
        edited = copy.deepcopy(self.row)
        edited["verdicts"][0]["status"] = "fail"
        # `classify_row` reads `manifest_sha256` first (falling back to
        # `compiled_snapshot_sha256`, then `snapshot_sha256`); matching it
        # here is what makes this row classify as `honest`, ignoring the
        # signature — the exact case the pre-S-043 script could not tell
        # apart from a forged row.
        reference = edited.get("manifest_sha256", "")
        cls, note = ac.classify_row(edited, reference, case_changes=[], trust_root=self.trust_root)
        self.assertEqual(cls, "refused", note)
        self.assertIn("signature invalid", note)

    def test_without_a_trust_root_a_signed_row_is_not_checked_and_classifies_as_before(self):
        state, detail = ac.verify_log_line_signature(self.row, None)
        self.assertEqual(state, "not_checked", detail)
        reference = self.row.get("manifest_sha256", "")
        cls, _note = ac.classify_row(self.row, reference, case_changes=[], trust_root=None)
        self.assertEqual(cls, "honest")

    def test_a_row_with_no_signature_member_is_unsigned_and_unaffected(self):
        unsigned = copy.deepcopy(self.row)
        del unsigned["signature"]
        state, detail = ac.verify_log_line_signature(unsigned, self.trust_root)
        self.assertEqual(state, "unsigned")
        self.assertIsNone(detail)
        reference = unsigned.get("manifest_sha256", "")
        cls, _note = ac.classify_row(unsigned, reference, case_changes=[], trust_root=self.trust_root)
        self.assertEqual(cls, "honest")

    def test_a_key_id_not_listed_under_role_runner_is_invalid(self):
        edited_trust_root = {"runner": {}}  # the signing key is listed nowhere
        state, detail = ac.verify_log_line_signature(self.row, edited_trust_root)
        self.assertEqual(state, "invalid", detail)
        self.assertIn("not listed", detail)


class TestLoadTrustRoot(unittest.TestCase):
    """No live run needed: this only reshapes the committed example trust
    root, so it always runs.
    """

    def test_the_committed_example_trust_root_has_a_runner_and_requester_key(self):
        by_role = ac.load_trust_root(TRUST_ROOT)
        self.assertIn("runner", by_role)
        self.assertIn("requester", by_role)
        for role, keys in by_role.items():
            for key_id, public_key_hex in keys.items():
                self.assertEqual(len(key_id), 64, key_id)
                self.assertEqual(len(public_key_hex), 64, public_key_hex)


if __name__ == "__main__":
    unittest.main()
