#!/usr/bin/env python3
"""Non-accountable practical review for CASE-001 shielding finalists.

The reviewer consumes the exact review request materialized by Avila Core and
the candidate a designer proposed. It reads Core's verdicts; it never derives
or edits one. Its only positive disposition is a recommendation that a person
review the candidate. It can request changes or abstain, but can never approve
or reject a design for use.
"""

import argparse
import hashlib
import json
import unicodedata
from pathlib import Path


SCHEMA_VERSION = "avila.core/staged-review-record/v0.1-draft"
REVIEW_STEP_ID = "practical-review"
FORBIDDEN_AGENT_DISPOSITIONS = {"approve_for_use", "reject_for_use"}
ROUTING_DISPOSITIONS = {
    "recommend_for_accountable_review",
    "request_changes",
    "abstain",
}
LIMITATIONS = [
    "This is an unsigned, non-accountable agent routing record; it is not an approval for use.",
    "The reviewer reads Core's verdicts and applies the bound practical instructions; it does not construct, alter, or validate a technical verdict.",
    "A recommendation only places a candidate before an eligible accountable person under a separate review obligation.",
]


def _normalized(value):
    """NFC-normalize JSON and reject values outside this record's profile."""
    if value is None or isinstance(value, (bool, int)):
        return value
    if isinstance(value, float):
        raise ValueError("staged review records cannot contain binary floating-point values")
    if isinstance(value, str):
        return unicodedata.normalize("NFC", value)
    if isinstance(value, list):
        return [_normalized(item) for item in value]
    if isinstance(value, dict):
        normalized = {}
        for key, item in value.items():
            if not isinstance(key, str):
                raise ValueError("staged review record keys must be strings")
            key = unicodedata.normalize("NFC", key)
            if key in normalized:
                raise ValueError(f"normalization creates duplicate key {key!r}")
            normalized[key] = _normalized(item)
        return normalized
    raise ValueError(f"unsupported staged review value {type(value).__name__}")


def canonical_bytes(value):
    return json.dumps(
        _normalized(value), ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")


def identity(value):
    return "sha256:" + hashlib.sha256(canonical_bytes(value)).hexdigest()


def file_identity(path):
    return "sha256:" + hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_policy(path):
    path = Path(path)
    return json.loads(path.read_text()), file_identity(path)


def _review_request(report):
    matches = [
        stage
        for stage in report.get("review_stages", [])
        if stage.get("step_id") == REVIEW_STEP_ID
    ]
    if len(matches) != 1:
        raise ValueError(
            f"Core report must contain exactly one {REVIEW_STEP_ID!r} review stage"
        )
    request = matches[0]
    if request.get("reviewer_role") != "agent":
        raise ValueError("practical review is not compiled for the agent reviewer role")
    if request.get("fulfillment") != "pending_agent_review":
        raise ValueError("practical review does not carry a pending agent obligation")
    if request.get("state") != "ready_for_review":
        missing = request.get("missing_evidence", [])
        raise ValueError(f"practical review dossier is incomplete: {missing}")
    if not request.get("instructions"):
        raise ValueError("practical review has no compiled instructions")
    allowed = set(request.get("allowed_dispositions", []))
    if allowed & FORBIDDEN_AGENT_DISPOSITIONS:
        raise ValueError("agent review was given authority to approve or reject for use")
    if not ROUTING_DISPOSITIONS.issuperset(allowed):
        raise ValueError(f"agent review contains an unknown routing disposition: {allowed}")
    body = {key: value for key, value in request.items() if key != "request_sha256"}
    if identity(body) != request.get("request_sha256"):
        raise ValueError("Core review request identity does not match its body")
    return request


def _presented_sha256(request, input_slot):
    matches = [
        evidence
        for evidence in request["presented_evidence"]
        if evidence.get("input_slot") == input_slot
    ]
    if len(matches) != 1:
        raise ValueError(
            f"review request must contain exactly one {input_slot!r} dossier artifact"
        )
    return matches[0]["sha256"]


def _verify_policy(request, policy, policy_sha256, reviewer_sha256):
    expected = request["reviewer_eligibility_policy"]
    if policy_sha256 != expected["sha256"]:
        raise ValueError(
            f"review policy bytes are {policy_sha256}, expected {expected['sha256']}"
        )
    if policy.get("policy_id") != expected["policy_id"]:
        raise ValueError("review policy id does not match the compiled obligation")
    if policy.get("revision") != expected["revision"]:
        raise ValueError("review policy revision does not match the compiled obligation")
    if policy.get("reviewer_role") != "agent":
        raise ValueError("review policy is not for an agent")
    if policy.get("eligible_reviewer", {}).get("identity") != reviewer_sha256:
        raise ValueError("this reviewer implementation is not the policy's bound agent")
    if _presented_sha256(request, "reviewer") != reviewer_sha256:
        raise ValueError("this reviewer implementation is not the request's dossier agent")
    if policy.get("instructions") != request["instructions"]:
        raise ValueError("policy and compiled review instructions differ")
    if policy.get("allowed_dispositions") != request["allowed_dispositions"]:
        raise ValueError("policy and compiled review dispositions differ")


def _verified_campaign(report, request):
    campaign = report.get("campaign")
    if not isinstance(campaign, dict) or campaign.get("status") != "evaluated":
        raise ValueError("Core report does not contain an evaluated campaign")
    if campaign.get("campaign_sha256") != request["campaign_sha256"]:
        raise ValueError("campaign identity does not match the review request")
    if campaign.get("compiled_snapshot_sha256") != request["compiled_snapshot_sha256"]:
        raise ValueError("campaign snapshot does not match the review request")
    identity_fields = (
        "schema_version",
        "semantic_profile",
        "status",
        "compiled_snapshot_sha256",
        "claims_sha256",
    )
    try:
        body = {field: campaign[field] for field in identity_fields}
    except KeyError as error:
        raise ValueError(f"campaign identity field is missing: {error.args[0]}") from error
    # Core omits empty report arrays, but its identity body always includes
    # them. Reconstruct those semantic defaults before checking the digest.
    body.update(
        findings=campaign.get("findings", []),
        admissions=campaign.get("admissions", []),
        verdicts=campaign.get("verdicts", []),
    )
    if identity(body) != request["campaign_sha256"]:
        raise ValueError("campaign identity does not match its body")
    return campaign


def _technical_actions(campaign):
    actions = []
    seen = set()
    for record in campaign["verdicts"]:
        requirement_id = record.get("requirement_id")
        if not requirement_id or requirement_id in seen:
            raise ValueError("campaign verdicts must name each requirement exactly once")
        seen.add(requirement_id)
        verdict = record.get("verdict", {})
        status = verdict.get("status")
        if status == "pass":
            continue
        rule = verdict.get("rule", "unknown rule")
        if status == "inconclusive":
            actions.append(
                f"Resolve {requirement_id}: Core reports INCONCLUSIVE under {rule}; narrow the admitted uncertainty or change the design."
            )
        elif status == "fail":
            actions.append(
                f"Revise {requirement_id}: Core reports FAIL under {rule}."
            )
        elif status == "not_evaluated":
            actions.append(
                f"Resolve {requirement_id}: Core reports NOT_EVALUATED under {rule}; supply admissible evidence inside its qualification before review."
            )
        else:
            actions.append(
                f"Return {requirement_id}: Core reports unsupported status {status!r} under {rule}."
            )
    return actions


def _practical_actions(candidate):
    layers = candidate.get("layers")
    if not isinstance(layers, list) or not layers:
        return ["Return the candidate: it has no reviewable layer stack."]
    actions = []
    for index, layer in enumerate(layers):
        material = layer.get("material")
        try:
            thickness = int(layer.get("thickness_cm"))
        except (TypeError, ValueError):
            actions.append(f"Return layer {index + 1}: thickness is not a whole number of cm.")
            continue
        if thickness < 10:
            actions.append(
                f"Thicken or remove layer {index + 1}: {thickness} cm is below the policy's 10 cm handling minimum."
            )
        if material == "water":
            actions.append(
                f"Replace or explicitly engineer containment for layer {index + 1}: liquid-water containment, leakage, and maintenance are outside this case."
            )
        if index and material == layers[index - 1].get("material"):
            actions.append(
                f"Merge layers {index} and {index + 1}: adjacent {material} layers add an unnecessary construction interface."
            )
    return actions


def review_candidate(
    report,
    candidate,
    candidate_sha256,
    policy,
    policy_sha256,
    reviewer_sha256,
):
    """Return one content-identified routing record for a Core run report."""
    request = _review_request(report)
    _verify_policy(request, policy, policy_sha256, reviewer_sha256)
    if _presented_sha256(request, "candidate") != candidate_sha256:
        raise ValueError("candidate bytes do not match the request's dossier candidate")
    campaign = _verified_campaign(report, request)

    actions = _technical_actions(campaign)
    if not actions:
        actions.extend(_practical_actions(candidate))
    disposition = (
        "request_changes" if actions else "recommend_for_accountable_review"
    )
    if disposition not in request["allowed_dispositions"]:
        raise ValueError(f"compiled review does not allow {disposition}")
    if disposition in FORBIDDEN_AGENT_DISPOSITIONS:
        raise AssertionError("an agent reviewer cannot govern use")

    candidate_id = candidate.get("candidate_id", "unknown")
    rationale = (
        f"Return {candidate_id} to the designer with {len(actions)} recorded action(s)."
        if actions
        else f"No bound technical or practical routing issue was found for {candidate_id}; send it to an accountable person without approving it."
    )
    record = {
        "schema_version": SCHEMA_VERSION,
        "review_request": request,
        "reviewer": {"role": "agent", "identity": reviewer_sha256},
        "candidate_id": candidate_id,
        "disposition": disposition,
        "rationale": rationale,
        "actions": actions,
        "attestation": "unverified",
        "limitations": LIMITATIONS,
    }
    record["record_sha256"] = identity(record)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, help="Core run-report.json")
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--policy", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    report = json.loads(Path(args.report).read_text())
    candidate = json.loads(Path(args.candidate).read_text())
    policy, policy_sha256 = load_policy(args.policy)
    reviewer_sha256 = file_identity(__file__)
    record = review_candidate(
        report,
        candidate,
        file_identity(args.candidate),
        policy,
        policy_sha256,
        reviewer_sha256,
    )
    output = Path(args.out)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps(record, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
