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
# Item 8: ADR-0015 Ed25519 signatures
#
# RFC8032_ED25519_VECTORS is embedded verbatim from RFC 8032 section 7.1
# ("Test Vectors for Ed25519"): TEST 1, TEST 2, TEST 3, TEST 1024, and
# TEST SHA(abc). Each field is exactly the hex string the RFC states
# (secret key, public key, message, signature), transcribed with no
# reformatting of the digits themselves.
# ---------------------------------------------------------------------------

RFC8032_ED25519_VECTORS = [
    {
        "id": "TEST 1",
        "secret_key_hex": "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "public_key_hex": "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
        "message_hex": "",
        "signature_hex": "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    },
    {
        "id": "TEST 2",
        "secret_key_hex": "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
        "public_key_hex": "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
        "message_hex": "72",
        "signature_hex": "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
    },
    {
        "id": "TEST 3",
        "secret_key_hex": "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
        "public_key_hex": "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
        "message_hex": "af82",
        "signature_hex": "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
    },
    {
        "id": "TEST 1024",
        "secret_key_hex": "f5e5767cf153319517630f226876b86c8160cc583bc013744c6bf255f5cc0ee5",
        "public_key_hex": "278117fc144c72340f67d0f2316e8386ceffbf2b2428c9c51fef7c597f1d426e",
        "message_hex": (
            "08b8b2b733424243760fe426a4b54908632110a66c2f6591eabd3345e3e4eb98fa6e264bf09efe12ee50f8f54e9f77b1"
            "e355f6c50544e23fb1433ddf73be84d879de7c0046dc4996d9e773f4bc9efe5738829adb26c81b37c93a1b270b20329d"
            "658675fc6ea534e0810a4432826bf58c941efb65d57a338bbd2e26640f89ffbc1a858efcb8550ee3a5e1998bd177e93a"
            "7363c344fe6b199ee5d02e82d522c4feba15452f80288a821a579116ec6dad2b3b310da903401aa62100ab5d1a36553e"
            "06203b33890cc9b832f79ef80560ccb9a39ce767967ed628c6ad573cb116dbefefd75499da96bd68a8a97b928a8bbc10"
            "3b6621fcde2beca1231d206be6cd9ec7aff6f6c94fcd7204ed3455c68c83f4a41da4af2b74ef5c53f1d8ac70bdcb7ed1"
            "85ce81bd84359d44254d95629e9855a94a7c1958d1f8ada5d0532ed8a5aa3fb2d17ba70eb6248e594e1a2297acbbb39d"
            "502f1a8c6eb6f1ce22b3de1a1f40cc24554119a831a9aad6079cad88425de6bde1a9187ebb6092cf67bf2b13fd65f270"
            "88d78b7e883c8759d2c4f5c65adb7553878ad575f9fad878e80a0c9ba63bcbcc2732e69485bbc9c90bfbd62481d9089b"
            "eccf80cfe2df16a2cf65bd92dd597b0707e0917af48bbb75fed413d238f5555a7a569d80c3414a8d0859dc65a46128ba"
            "b27af87a71314f318c782b23ebfe808b82b0ce26401d2e22f04d83d1255dc51addd3b75a2b1ae0784504df543af8969b"
            "e3ea7082ff7fc9888c144da2af58429ec96031dbcad3dad9af0dcbaaaf268cb8fcffead94f3c7ca495e056a9b47acdb7"
            "51fb73e666c6c655ade8297297d07ad1ba5e43f1bca32301651339e22904cc8c42f58c30c04aafdb038dda0847dd988d"
            "cda6f3bfd15c4b4c4525004aa06eeff8ca61783aacec57fb3d1f92b0fe2fd1a85f6724517b65e614ad6808d6f6ee34df"
            "f7310fdc82aebfd904b01e1dc54b2927094b2db68d6f903b68401adebf5a7e08d78ff4ef5d63653a65040cf9bfd4aca7"
            "984a74d37145986780fc0b16ac451649de6188a7dbdf191f64b5fc5e2ab47b57f7f7276cd419c17a3ca8e1b939ae49e4"
            "88acba6b965610b5480109c8b17b80e1b7b750dfc7598d5d5011fd2dcc5600a32ef5b52a1ecc820e308aa342721aac09"
            "43bf6686b64b2579376504ccc493d97e6aed3fb0f9cd71a43dd497f01f17c0e2cb3797aa2a2f256656168e6c496afc5f"
            "b93246f6b1116398a346f1a641f3b041e989f7914f90cc2c7fff357876e506b50d334ba77c225bc307ba537152f3f161"
            "0e4eafe595f6d9d90d11faa933a15ef1369546868a7f3a45a96768d40fd9d03412c091c6315cf4fde7cb68606937380d"
            "b2eaaa707b4c4185c32eddcdd306705e4dc1ffc872eeee475a64dfac86aba41c0618983f8741c5ef68d3a101e8a3b8ca"
            "c60c905c15fc910840b94c00a0b9d0"
        ),
        "signature_hex": "0aab4c900501b3e24d7cdf4663326a3a87df5e4843b2cbdb67cbf6e460fec350aa5371b1508f9f4528ecea23c436d94b5e8fcd4f681e30a6ac00a9704a188a03",
    },
    {
        "id": "TEST SHA(abc)",
        "secret_key_hex": "833fe62409237b9d62ec77587520911e9a759cec1d19755b7da901b96dca3d42",
        "public_key_hex": "ec172b93ad5e563bf4932c70e1245034c35467ef2efd4d64ebf819683467e2bf",
        "message_hex": "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        "signature_hex": "dc2a4459e7369633a52b1bf277839a00201009a3efbf3ecb69bea2186c26b58909351fc9ac90b3ecfdfbc7c66431e0303dca179c138ac17ad9bef1177331a704",
    },
]

# RFC 8032 Table 1's own literal decimal constants for d's numerator (in
# ``-121665/121666 = <this>``), the base point B = (X(P), Y(P)), and L's
# ``2^252 + <this>`` term — cross-checked once against this file's
# constants computed from the defining relations, so a mistake in either
# the from-scratch derivation or a hand-transcribed literal would show up
# as a self-contradiction, not agree with itself.
_RFC8032_TABLE_1_D_NUMERATOR = 37095705934669439343138083508754565189542113879843219016388785533085940283555
_RFC8032_TABLE_1_GX = 15112221349535400772501151409588531511454012693041857206046113283949847762202
_RFC8032_TABLE_1_GY = 46316835694926478169428394003475163141307993866256225615783033603165251855960
_RFC8032_TABLE_1_L_EXTRA = 27742317777372353535851937790883648493


class TestEd25519RFC8032Vectors(unittest.TestCase):
    def test_constants_match_rfc8032_table_1(self):
        p = v._ED25519_P
        self.assertEqual(v._ED25519_D, (-121665 * pow(121666, p - 2, p)) % p)
        self.assertEqual(v._ED25519_D % p, _RFC8032_TABLE_1_D_NUMERATOR % p)
        self.assertEqual(v._ED25519_GX, _RFC8032_TABLE_1_GX)
        self.assertEqual(v._ED25519_GY, _RFC8032_TABLE_1_GY)
        self.assertEqual(v._ED25519_L, 2**252 + _RFC8032_TABLE_1_L_EXTRA)

    def test_every_vector_reproduces_public_key_and_signature(self):
        self.assertEqual(len(RFC8032_ED25519_VECTORS), 5)
        for vec in RFC8032_ED25519_VECTORS:
            with self.subTest(id=vec["id"]):
                seed = bytes.fromhex(vec["secret_key_hex"])
                expected_public_key = bytes.fromhex(vec["public_key_hex"])
                message = bytes.fromhex(vec["message_hex"])
                expected_signature = bytes.fromhex(vec["signature_hex"])

                self.assertEqual(v.ed25519_public_key_from_seed(seed), expected_public_key)
                self.assertEqual(v.ed25519_sign(seed, message), expected_signature)
                self.assertTrue(v.ed25519_verify(expected_public_key, message, expected_signature))

    def test_tampering_the_signature_or_message_fails_verification(self):
        vec = RFC8032_ED25519_VECTORS[0]
        public_key = bytes.fromhex(vec["public_key_hex"])
        message = bytes.fromhex(vec["message_hex"])
        signature = bytearray(bytes.fromhex(vec["signature_hex"]))
        self.assertTrue(v.ed25519_verify(public_key, message, bytes(signature)))

        flipped = bytearray(signature)
        flipped[0] ^= 0x01
        self.assertFalse(v.ed25519_verify(public_key, message, bytes(flipped)))
        self.assertFalse(v.ed25519_verify(public_key, b"a different message", bytes(signature)))
        self.assertFalse(v.ed25519_verify(bytes(32), message, bytes(signature)))


# ---------------------------------------------------------------------------
# Item 8: every ADR-0015 signature document actually committed under
# examples/cases/*/signatures/, verified against examples/keys/trust-root.json
# ---------------------------------------------------------------------------


class TestSignaturesAgainstCommittedDocuments(unittest.TestCase):
    def setUp(self):
        self.trust_root = v.load_trust_root(REPO_ROOT / "examples" / "keys" / "trust-root.json")

    def test_every_committed_signature_document_verifies(self):
        signature_paths = sorted(EXAMPLES.glob("*/signatures/*.sig.json"))
        self.assertGreaterEqual(len(signature_paths), 9, "expected at least the 9 ADR-0015 signatures across CASE-001/002/003")
        for path in signature_paths:
            with self.subTest(path=str(path.relative_to(EXAMPLES.parent.parent))):
                document = load(path)
                role = "requester" if document["signed_document"]["role"] == "manifest" else "runner"
                digest = v.digest_from_prefixed(document["signed_document"]["sha256"])
                status = v.signature_status(document, digest, self.trust_root, role)
                self.assertEqual(status, {"state": "verified", "signed_by": document["key_id"]})

    def test_key_ids_in_trust_root_match_examples_keys_readme(self):
        # examples/keys/README.md states these two key ids literally.
        self.assertIsNotNone(self.trust_root.find("191c470d8848ffedf3bb54e33798e98423a5659db297877d6482a06c96149cd7", "requester"))
        self.assertIsNotNone(self.trust_root.find("49104f3eb2b3489e8b26bc1ebb0934637e43aacf515719b1698758532c6daf62", "runner"))


class TestManifestSigningDigest(unittest.TestCase):
    def test_reproduces_every_committed_manifest_signature_target(self):
        for case_name in ("case-001-shield-search", "case-002-coupled-shield", "case-003-thermal-spreader"):
            with self.subTest(case=case_name):
                case_dir = EXAMPLES / case_name
                package = load(case_dir / "package.json")
                manifest_bytes = (case_dir / "package.json").read_bytes()
                found = v.find_signature_for(case_dir, package, "manifest", package["case_id"])
                self.assertIsNotNone(found)
                signature_document_id, document = found
                computed = v.manifest_signing_digest(manifest_bytes, signature_document_id)
                self.assertEqual(computed, v.digest_from_prefixed(document["signed_document"]["sha256"]))

    def test_removing_an_absent_entry_is_a_no_op(self):
        # ADR-0015's own symmetry claim: signing (before the signature
        # entry exists) and verifying (after) must compute the identical
        # digest, because removing an absent id is a no-op.
        before = json.dumps({"schema_version": "x", "documents": [{"document_id": "contract", "role": "contract"}]}).encode()
        after = json.dumps(
            {
                "schema_version": "x",
                "documents": [
                    {"document_id": "contract", "role": "contract"},
                    {"document_id": "signature-manifest", "role": "signature"},
                ],
            }
        ).encode()
        self.assertEqual(
            v.manifest_signing_digest(before, "signature-manifest"),
            v.manifest_signing_digest(after, "signature-manifest"),
        )


# ---------------------------------------------------------------------------
# Item 8: campaign log-line signatures. No committed campaign-log.jsonl or
# attempts.jsonl in this repository carries one (log-line signing needs a
# live `run --runner-key`; every committed log was produced without one —
# TestPositivePathOnRealCases' `not_checked` count is the honest evidence
# for that), so this exercises the code path directly: a synthetic log line
# signed with the real, committed example runner seed
# (examples/keys/runner.seed), matching exactly what
# case_run/log.rs::sign_log_line produces (the canonical form of the line
# with `signature` absent, signed, then spliced back in).
# ---------------------------------------------------------------------------


class TestLogLineSignatureVerification(unittest.TestCase):
    def setUp(self):
        self.trust_root = v.load_trust_root(REPO_ROOT / "examples" / "keys" / "trust-root.json")
        self.runner_seed = (REPO_ROOT / "examples" / "keys" / "runner.seed").read_bytes()
        self.assertEqual(len(self.runner_seed), 32)

    def _sign_line(self, unsigned: dict, seed: bytes | None = None) -> dict:
        canonical = v.canonicalize_json(json.dumps(unsigned).encode("utf-8"))
        digest = v.sha256_bytes(canonical)  # "sha256:<hex>"
        raw_seed = seed if seed is not None else self.runner_seed
        signature = v.ed25519_sign(raw_seed, v.digest_from_prefixed(digest))
        key_id = v.sha256_bytes(v.ed25519_public_key_from_seed(raw_seed)).split(":", 1)[1]
        document = {
            "schema_version": v.SIGNATURE_SCHEMA_VERSION,
            "signed_document": {"role": "log_line", "document_id": unsigned.get("case_id", "log-line"), "sha256": digest},
            "key_id": key_id,
            "algorithm": v.ALGORITHM_ED25519,
            "signature_hex": signature.hex(),
            "notice": "test fixture signature; proves nothing about real approval",
        }
        signed = dict(unsigned)
        signed["signature"] = document
        return signed

    def test_a_genuinely_signed_log_line_verifies(self):
        line = self._sign_line({"case_id": "CASE-XYZ", "status": "campaign_evaluated"})
        status = v.verify_log_line_signature(line, self.trust_root)
        self.assertEqual(status["state"], "verified")
        # The key id matches the committed runner key id from examples/keys/README.md.
        self.assertEqual(status["signed_by"], "49104f3eb2b3489e8b26bc1ebb0934637e43aacf515719b1698758532c6daf62")

    def test_a_log_line_edited_after_signing_is_invalid(self):
        line = self._sign_line({"case_id": "CASE-XYZ", "status": "campaign_evaluated"})
        line["status"] = "compile_rejected"  # edited after signing, signature left as-is
        status = v.verify_log_line_signature(line, self.trust_root)
        self.assertEqual(status["state"], "invalid")

    def test_an_unsigned_line_is_reported_unsigned(self):
        status = v.verify_log_line_signature({"case_id": "CASE-XYZ"}, self.trust_root)
        self.assertEqual(status, {"state": "unsigned"})

    def test_verify_log_signatures_reports_a_mixed_log_file(self):
        tmp = Path(tempfile.mkdtemp(prefix="avila-core-verifier-log-"))
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        log_path = tmp / "campaign-log.jsonl"
        good = self._sign_line({"case_id": "CASE-A"})
        bad = self._sign_line({"case_id": "CASE-B"})
        bad["case_id"] = "CASE-TAMPERED"
        unsigned = {"case_id": "CASE-C"}
        log_path.write_text("\n".join(json.dumps(row) for row in (good, bad, unsigned)) + "\n")

        report = v.Report(str(log_path))
        v.verify_log_signatures(log_path, self.trust_root, report)
        self.assertEqual(len(report.checks), 1)
        self.assertEqual(report.checks[0].status, "mismatch")  # one invalid line dominates the single summary row


# ---------------------------------------------------------------------------
# Item 9: the generic Strong-Kleene predicate evaluator, proved against
# fixtures/semantic-core/vectors/scope-predicates.v1.json (ADR-0006 SC-7) —
# the same corpus crates/avila-core-kernel/tests/scope_predicate_vectors.rs
# proves the real evaluator against, including the same locally-defined
# time_registry() that vector file's cooling_time/time_step vectors need
# (the vectors carry no kind table of their own).
# ---------------------------------------------------------------------------


def _scope_predicate_time_kinds() -> dict:
    unit_class = {
        "s": Fraction(1),
        "min": Fraction(60),
        "h": Fraction(3600),
        "d": Fraction(86400),
    }
    return {"core.time.duration@1": v.Kind("core.time.duration@1", "s", unit_class)}


class TestScopePredicateVectors(unittest.TestCase):
    def setUp(self):
        self.doc = load(FIXTURES / "vectors" / "scope-predicates.v1.json")
        self.kinds = _scope_predicate_time_kinds()
        self.fact_kinds = {"time_step": "core.time.duration@1"}
        self.param_kinds = {"cooling_time": "core.time.duration@1"}

    def test_vector_set_shape(self):
        self.assertEqual(self.doc["semantic_profile"], v.SEMANTIC_PROFILE)
        self.assertEqual(len(self.doc["vectors"]), 19, "update the corpus count intentionally")

    def test_every_scope_predicate_vector(self):
        for vec in self.doc["vectors"]:
            with self.subTest(id=vec["id"]):
                result = v.evaluate_predicate(
                    vec["input"]["predicate"], vec["input"]["context"], self.kinds, self.fact_kinds, self.param_kinds
                )
                self.assertEqual(result, vec["expected"]["result"])

    def test_fact_string_bool_and_integer_comparison_paths(self):
        # scope-predicates.v1.json's 19 vectors never reach a fact whose
        # *value* is a string, bool, or integer with a matching context
        # entry present (its two "fact" vectors are quantity-valued, and
        # its two context-less "eq" vectors on a string fact stop at
        # "unknown" before comparing anything) — every branch below is
        # otherwise real: CASE-001/002's committed
        # qualification-transport.json scopes a "source.geometry" string
        # fact with op "eq" exactly this shape, and slab.layer_count is an
        # integer fact with op "le". Hand-written, not vector-sourced.
        def fact(name, op, value, cls="validated_input", validator="test/adapter@1"):
            return {"fact": {"name": name, "op": op, "value": value, "source_requirement": {"class": cls, "validator": validator}}}

        def ctx(name, value, cls="validated_input", validator="test/adapter@1"):
            return {"facts": {name: {"value": value, "source": {"class": cls, "identity": "x", "validator": validator, "receipt": "sha256:x"}}}}

        cases = [
            (fact("source.geometry", "eq", "plane"), ctx("source.geometry", "plane"), "true"),
            (fact("source.geometry", "eq", "plane"), ctx("source.geometry", "cylinder"), "false"),
            (fact("source.geometry", "ne", "plane"), ctx("source.geometry", "cylinder"), "true"),
            (fact("flag", "eq", True), ctx("flag", True), "true"),
            (fact("flag", "eq", True), ctx("flag", False), "false"),
            (fact("slab.layer_count", "le", 3), ctx("slab.layer_count", 3), "true"),
            (fact("slab.layer_count", "le", 3), ctx("slab.layer_count", 4), "false"),
            (fact("slab.layer_count", "gt", 1), ctx("slab.layer_count", 4), "true"),
            # Mismatched value types (a quantity fact answered by a string, say) fall through to unknown.
            (fact("source.geometry", "eq", "plane"), ctx("source.geometry", 1), "unknown"),
        ]
        for predicate, context, expected in cases:
            with self.subTest(predicate=predicate, context=context):
                self.assertEqual(v.evaluate_predicate(predicate, context, {}, {}, {}), expected)

    def test_ordered_comparison_of_a_string_or_bool_fact_is_a_predicate_error(self):
        # predicate.rs's compare_scalar refuses lt/le/gt/ge on a non-numeric
        # value (CORE's own invalid_predicate); evaluate_envelope_terms
        # catches this as an issue and reports that one term "unknown"
        # rather than raising, but evaluate_predicate itself must still
        # raise so a direct caller cannot mistake the refusal for a real
        # "unknown" fact.
        predicate = {
            "fact": {
                "name": "source.geometry",
                "op": "lt",
                "value": "plane",
                "source_requirement": {"class": "validated_input", "validator": "t"},
            }
        }
        context = {"facts": {"source.geometry": {"value": "plane", "source": {"class": "validated_input", "identity": "x", "validator": "t", "receipt": "sha256:x"}}}}
        with self.assertRaises(v.PredicateError):
            v.evaluate_predicate(predicate, context, {}, {}, {})

    def test_structural_predicate_errors(self):
        for bad_predicate in (
            {"all": []},
            {"any": []},
            {"param_in_range": {"param": "x"}},  # neither min nor max
            {"not_a_real_variant": True},
            {"too": "many", "keys": "here"},
        ):
            with self.subTest(predicate=bad_predicate):
                with self.assertRaises(v.PredicateError):
                    v.evaluate_predicate(bad_predicate, {}, {}, {}, {})

    def test_evaluate_envelope_terms_turns_a_predicate_error_into_an_unknown_term_and_an_issue(self):
        record = {"scope": {"all": [{"param_in_range": {"param": "x"}}]}, "fact_kinds": {}}
        state, terms, issues = v.evaluate_envelope_terms(record, {}, {})
        self.assertEqual(state, "unknown")
        self.assertEqual([t["result"] for t in terms], ["unknown"])
        self.assertEqual(len(issues), 1)


# ---------------------------------------------------------------------------
# Item 9: qualification-envelope structural consistency for every
# qualification-carrying claim in CASE-001, 002, and 003's committed
# claims.json — see verify_case_qualification_envelopes's own boundary
# comment for exactly what this does and does not independently re-derive.
# ---------------------------------------------------------------------------


class TestQualificationEnvelopeConsistency(unittest.TestCase):
    CASES = ["case-001-shield-search", "case-002-coupled-shield", "case-003-thermal-spreader"]

    def test_every_qualifying_claim_is_self_consistent_with_its_bound_record(self):
        total_checked = 0
        for case_name in self.CASES:
            case_dir = EXAMPLES / case_name
            package = load(case_dir / "package.json")
            docs_by_role: dict = {}
            for d in package.get("documents", []):
                docs_by_role.setdefault(d["role"], []).append(d)
            claims = load(case_dir / "claims.json")

            report = v.Report(case_name)
            v.verify_case_qualification_envelopes(case_dir, package, docs_by_role, claims, v.kinds_from_registry_doc(load(case_dir / "registry.json")), report)

            qualification_checks = [c for c in report.checks if c.check.startswith("qualification.") and c.check != "qualification.envelope_predicate_over_facts"]
            self.assertGreater(len(qualification_checks), 0, case_name)
            for check in qualification_checks:
                self.assertEqual(check.status, "verified", f"{case_name}: {check.to_dict()}")
            total_checked += len(qualification_checks)

            disclosure = [c for c in report.checks if c.check == "qualification.envelope_predicate_over_facts"]
            self.assertEqual(len(disclosure), 1)
            self.assertEqual(disclosure[0].status, "not_checked")
            self.assertIn("never persisted", disclosure[0].reason)

        self.assertGreaterEqual(total_checked, 9, "screen mass/thickness + transport/activation/fe claims across the three cases")

    def test_evaluate_envelope_terms_splits_case_001_screen_scope_exactly_as_committed(self):
        # A term-splitting cross-check independent of verify_case_qualification_envelopes
        # itself: feed the real committed qualification-screen.json record through
        # evaluate_envelope_terms with an EMPTY context (so every result is "unknown" —
        # this checks the *term text*, not a claimed truth value) and compare the
        # predicate strings against claims.json's own recorded ones.
        case_dir = EXAMPLES / "case-001-shield-search"
        record = load(case_dir / "qualification-screen.json")
        claims = load(case_dir / "claims.json")
        screen_mass = next(c for c in claims["claims"] if c["claim_id"] == "screen-mass")

        _, terms, _issues = v.evaluate_envelope_terms(record, {}, {})
        predicates = [t["predicate"] for t in terms]
        recorded_predicates = [t["predicate"] for t in screen_mass["qualification"]["terms"]]
        self.assertEqual(predicates, recorded_predicates)
        self.assertEqual(len(predicates), 5)


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
        trust_root = v.load_trust_root(REPO_ROOT / "examples" / "keys" / "trust-root.json")
        for case_name in self.CASES:
            case_dir = EXAMPLES / case_name
            with self.subTest(case=case_name):
                roots = {name: path for name, path in IN_REPO_ROOTS.items() if path is not None}
                roots["case"] = case_dir
                report = v.verify_case(case_dir, roots, trust_root=trust_root)
                mismatches = [c for c in report.checks if c.status == "mismatch"]
                self.assertEqual(mismatches, [], f"{case_name}: {[m.to_dict() for m in mismatches]}")
                self.assertGreater(report.counts()["verified"], 0)

    def test_signed_cases_verify_every_signature_with_the_example_trust_root(self):
        # CASE-001, 002, 003 are the three ADR-0015-signed cases (examples/keys/README.md).
        trust_root = v.load_trust_root(REPO_ROOT / "examples" / "keys" / "trust-root.json")
        for case_name in ("case-001-shield-search", "case-002-coupled-shield", "case-003-thermal-spreader"):
            case_dir = EXAMPLES / case_name
            with self.subTest(case=case_name):
                roots = {name: path for name, path in IN_REPO_ROOTS.items() if path is not None}
                roots["case"] = case_dir
                report = v.verify_case(case_dir, roots, trust_root=trust_root)
                signature_checks = [c for c in report.checks if c.check.startswith("signatures.manifest") or c.check.startswith("signatures.receipt.")]
                self.assertGreater(len(signature_checks), 0, case_name)
                for check in signature_checks:
                    self.assertEqual(check.status, "verified", f"{case_name}: {check.to_dict()}")


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

    # -----------------------------------------------------------------
    # Item 8-9's own mutations: a flipped signature byte, a swapped key
    # id, a signature over a re-blessed manifest, and a qualification
    # envelope term edited to lie.
    # -----------------------------------------------------------------

    def _trust_root(self):
        return v.load_trust_root(EXAMPLES.parent / "keys" / "trust-root.json")

    def test_flipped_signature_byte_is_named_by_signature_verification(self):
        case_dir = self._copy_case("case-003-thermal-spreader")
        sig_path = case_dir / "signatures" / "screen-receipt.sig.json"
        document = json.loads(sig_path.read_text())
        real_hex = document["signature_hex"]
        flipped_char = "0" if real_hex[-1] != "0" else "1"
        document["signature_hex"] = real_hex[:-1] + flipped_char
        self.assertNotEqual(document["signature_hex"], real_hex)
        _write_json(sig_path, document)

        report = v.verify_case(
            case_dir,
            {"case": case_dir, "thermal": EXAMPLES.parent / "capabilities" / "thermal"},
            trust_root=self._trust_root(),
        )
        by_check = {c.check: c for c in report.checks}
        self.assertEqual(by_check["signatures.receipt.screen"].status, "mismatch")
        self.assertIn("does not verify", by_check["signatures.receipt.screen"].detail)

    def test_swapped_key_id_is_named_by_signature_verification(self):
        case_dir = self._copy_case("case-003-thermal-spreader")
        manifest_sig_path = case_dir / "signatures" / "manifest.sig.json"
        document = json.loads(manifest_sig_path.read_text())
        trust_root = self._trust_root()
        requester_key_id = document["key_id"]
        runner_key_id = next(key_id for (key_id, role) in trust_root.keys if role == "runner")
        self.assertNotEqual(requester_key_id, runner_key_id)
        document["key_id"] = runner_key_id  # signed by the runner key instead of the requester key (ADR-0015 acceptance test)
        _write_json(manifest_sig_path, document)

        report = v.verify_case(case_dir, {"case": case_dir, "thermal": EXAMPLES.parent / "capabilities" / "thermal"}, trust_root=trust_root)
        by_check = {c.check: c for c in report.checks}
        self.assertEqual(by_check["signatures.manifest"].status, "mismatch")
        self.assertIn("not listed under role `requester`", by_check["signatures.manifest"].detail)

    def test_signature_over_a_re_blessed_manifest_is_named_by_signature_verification(self):
        case_dir = self._copy_case("case-003-thermal-spreader")
        package_path = case_dir / "package.json"
        package = json.loads(package_path.read_text())
        # Simulate a document swapped in after the manifest was signed: the
        # manifest is "re-blessed" (a new, self-consistent hash for one
        # entry) but signatures/manifest.sig.json is never re-run.
        _rehash_document(package, "case-003-contract", "sha256:" + "7" * 64)
        _write_json(package_path, package)

        report = v.verify_case(case_dir, {"case": case_dir, "thermal": EXAMPLES.parent / "capabilities" / "thermal"}, trust_root=self._trust_root())
        by_check = {c.check: c for c in report.checks}
        self.assertEqual(by_check["signatures.manifest"].status, "mismatch")
        self.assertIn("does not match the digest", by_check["signatures.manifest"].detail)

    def test_envelope_term_edited_to_lie_is_named_by_qualification_consistency(self):
        case_dir = self._copy_case("case-001-shield-search")
        claims_path = case_dir / "claims.json"
        claims = json.loads(claims_path.read_text())
        edited = False
        for claim in claims["claims"]:
            if claim["claim_id"] == "screen-mass":
                for term in claim["qualification"]["terms"]:
                    if "slab.total_thickness" in term["predicate"]:
                        self.assertIn('"value":"120"', term["predicate"])
                        term["predicate"] = term["predicate"].replace('"value":"120"', '"value":"999"')
                        edited = True
        self.assertTrue(edited)
        _write_json(claims_path, claims)

        package_path = case_dir / "package.json"
        package = json.loads(package_path.read_text())
        _rehash_document(package, "case-001-claims", v.sha256_file(claims_path))
        _write_json(package_path, package)

        report = v.Report(str(case_dir))
        docs_by_role: dict = {}
        for d in package.get("documents", []):
            docs_by_role.setdefault(d["role"], []).append(d)
        v.verify_case_qualification_envelopes(
            case_dir, package, docs_by_role, claims, v.kinds_from_registry_doc(json.loads((case_dir / "registry.json").read_text())), report
        )
        by_check = {c.check: c for c in report.checks}
        self.assertEqual(by_check["qualification.screen-mass"].status, "mismatch")
        self.assertIn("do not match the bound record's own scope", by_check["qualification.screen-mass"].detail)


if __name__ == "__main__":
    unittest.main()
