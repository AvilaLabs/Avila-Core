import copy
import unittest

import shield_review


POLICY_SHA256 = "sha256:" + "a" * 64
REVIEWER_SHA256 = "sha256:" + "b" * 64
CANDIDATE_SHA256 = "sha256:" + "c" * 64
INSTRUCTIONS = [
    "Use Core's recorded requirement statuses and rules; never derive, edit, or override a technical verdict.",
    "Request changes unless every compiled requirement is PASS.",
    "After technical PASS, request changes for adjacent identical layers, a layer thinner than 10 cm, or liquid water without explicit containment evidence.",
    "A positive result may only recommend the candidate for accountable human review; never approve or reject it for use.",
]


def specimen():
    campaign = {
        "schema_version": "avila.core/campaign-report/v0.2-draft",
        "semantic_profile": "avila.core/semantic/0.2-draft",
        "status": "evaluated",
        "compiled_snapshot_sha256": "sha256:" + "1" * 64,
        "claims_sha256": "sha256:" + "2" * 64,
        "findings": [],
        "admissions": [],
        "verdicts": [
            {
                "requirement_id": "R1",
                "verdict": {"status": "pass", "rule": "bounded.le.within"},
            },
            {
                "requirement_id": "R2",
                "verdict": {"status": "pass", "rule": "bounded.le.within"},
            },
        ],
    }
    campaign["campaign_sha256"] = shield_review.identity(campaign)
    campaign["notice"] = "Specimen campaign notice."
    request = {
        "compiled_snapshot_sha256": "sha256:" + "1" * 64,
        "campaign_sha256": campaign["campaign_sha256"],
        "step_id": "practical-review",
        "fulfillment": "pending_agent_review",
        "reviewer_role": "agent",
        "state": "ready_for_review",
        "presented_evidence": [
            {
                "input_slot": "reviewer",
                "source": {"source": "contract_input", "input_id": "reviewer-script"},
                "evidence_id": "input:reviewer-script",
                "sha256": REVIEWER_SHA256,
                "media_type": "text/x-python",
            },
            {
                "input_slot": "candidate",
                "source": {"source": "contract_input", "input_id": "candidate"},
                "evidence_id": "input:candidate",
                "sha256": CANDIDATE_SHA256,
                "media_type": "application/vnd.avila.shield-candidate+json",
            }
        ],
        "decision_role": {"id": "core.review.routing-decision", "major": 1},
        "decision_media_type": "application/vnd.avila-core.staged-review+json",
        "allowed_dispositions": [
            "recommend_for_accountable_review",
            "request_changes",
            "abstain",
        ],
        "reviewer_eligibility_policy": {
            "policy_id": "avila-labs.shielding/practical-agent-review",
            "revision": 1,
            "sha256": POLICY_SHA256,
        },
        "independence": {"mode": "none"},
        "instructions": INSTRUCTIONS,
    }
    request["request_sha256"] = shield_review.identity(request)
    report = {"campaign": campaign, "review_stages": [request]}
    policy = {
        "policy_id": "avila-labs.shielding/practical-agent-review",
        "revision": 1,
        "reviewer_role": "agent",
        "eligible_reviewer": {"identity": REVIEWER_SHA256},
        "instructions": INSTRUCTIONS,
        "allowed_dispositions": request["allowed_dispositions"],
    }
    candidate = {
        "candidate_id": "c-test",
        "layers": [{"material": "polyethylene", "thickness_cm": "100"}],
    }
    return report, candidate, policy


def rebind_campaign(report):
    campaign = report["campaign"]
    body = {
        key: value
        for key, value in campaign.items()
        if key not in {"campaign_sha256", "notice"}
    }
    campaign["campaign_sha256"] = shield_review.identity(body)
    request = report["review_stages"][0]
    request["campaign_sha256"] = campaign["campaign_sha256"]
    request_body = {
        key: value for key, value in request.items() if key != "request_sha256"
    }
    request["request_sha256"] = shield_review.identity(request_body)


class ShieldReviewTests(unittest.TestCase):
    def review(self, report, candidate, policy):
        return shield_review.review_candidate(
            report,
            candidate,
            CANDIDATE_SHA256,
            policy,
            POLICY_SHA256,
            REVIEWER_SHA256,
        )

    def test_clean_candidate_is_only_recommended_to_a_person(self):
        report, candidate, policy = specimen()
        record = self.review(report, candidate, policy)
        self.assertEqual(record["disposition"], "recommend_for_accountable_review")
        self.assertEqual(record["actions"], [])
        self.assertNotIn(record["disposition"], shield_review.FORBIDDEN_AGENT_DISPOSITIONS)

    def test_core_inconclusive_returns_candidate_without_rederiving_verdict(self):
        report, candidate, policy = specimen()
        report["campaign"]["verdicts"][1]["verdict"].update(
            status="inconclusive", rule="bounded.le.crossing"
        )
        rebind_campaign(report)
        record = self.review(report, candidate, policy)
        self.assertEqual(record["disposition"], "request_changes")
        self.assertIn("Core reports INCONCLUSIVE", record["actions"][0])

    def test_practical_instruction_returns_adjacent_and_thin_layers(self):
        report, candidate, policy = specimen()
        candidate["layers"] = [
            {"material": "polyethylene", "thickness_cm": "95"},
            {"material": "polyethylene", "thickness_cm": "5"},
        ]
        record = self.review(report, candidate, policy)
        self.assertEqual(record["disposition"], "request_changes")
        self.assertEqual(len(record["actions"]), 2)

    def test_agent_authority_in_request_is_refused(self):
        report, candidate, policy = specimen()
        report = copy.deepcopy(report)
        request = report["review_stages"][0]
        request["allowed_dispositions"].append("approve_for_use")
        body = {key: value for key, value in request.items() if key != "request_sha256"}
        request["request_sha256"] = shield_review.identity(body)
        with self.assertRaisesRegex(ValueError, "authority"):
            self.review(report, candidate, policy)

    def test_different_candidate_bytes_are_refused(self):
        report, candidate, policy = specimen()
        with self.assertRaisesRegex(ValueError, "candidate bytes"):
            shield_review.review_candidate(
                report,
                candidate,
                "sha256:" + "d" * 64,
                policy,
                POLICY_SHA256,
                REVIEWER_SHA256,
            )

    def test_unbound_campaign_summary_tampering_is_refused(self):
        report, candidate, policy = specimen()
        report["campaign"]["verdicts"][1]["verdict"]["status"] = "fail"
        with self.assertRaisesRegex(ValueError, "campaign identity"):
            self.review(report, candidate, policy)


if __name__ == "__main__":
    unittest.main()
