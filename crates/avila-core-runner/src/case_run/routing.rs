//! Presentation-routing records (ADR-0024, SC-11 A9).
//!
//! The routing record a presentation gate produces is the committed
//! `avila.core/staged-review-record` shape — the gate's `decision_role`
//! is literally `core.presentation.routing-record`: an embedded
//! `review_request` carries the exact materialized request the reviewer
//! answered, `disposition` records the routing outcome, and
//! `record_sha256` binds the document's own bytes. The engine does not
//! route — it verifies the recorded routing against the materialized
//! gate. Every failure quarantines the *record* (`CORE-X6501`/`X6502`);
//! the artifact, admission state, and verdict it was attached to are
//! byte-for-byte unchanged — A9's own invariant.

use std::error::Error;

use avila_core_compiler::{ImmutablePolicyRef, ReviewDisposition, ReviewerRole, SourceLocation};
use avila_core_evidence::VerifiedCasePackage;
use avila_core_kernel::canonicalize_json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::case_run::FindingClass;
use crate::diagnostic::{CORE_X6501, CORE_X6502, RunFinding, RunStage};

use super::{PresentationGateReport, RoutingReport, RoutingState};

pub const STAGED_REVIEW_SCHEMA_VERSION: &str = "avila.core/staged-review-record/v0.1-draft";

/// `avila.core/staged-review-record/v0.1-draft`: the embedded request,
/// the disposition, and the record's own digest. Inert members the
/// record family carries (`candidate_id`, `actions`, `attestation`,
/// `limitations`, `rationale`) pass through `extra` verbatim so the
/// record's own digest still recomputes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedReviewRecord {
    pub schema_version: String,
    /// The exact materialized request the reviewer answered — a
    /// re-edited gate or dossier produces a different `request_sha256`.
    pub review_request: serde_json::Value,
    /// The recorded routing outcome; must be a member of the request's
    /// declared `allowed_dispositions`.
    pub disposition: ReviewDisposition,
    /// The reviewer attribution — stated, never authenticated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<ReviewerRef>,
    /// The candidate the routing named, when the record carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    /// The document's digest over its canonical form with this member
    /// absent — the same rule `request_sha256` uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_sha256: Option<String>,
    /// Inert prose and declared limitations — data, never instructions.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewerRef {
    pub role: ReviewerRole,
    #[serde(default)]
    pub identity: Option<String>,
}

/// Verify every package-bound `staged_review_record` against the
/// materialized gates, attach the outcome to each gate's report, and
/// return the findings. A verified record marks its gate `recorded` —
/// the arrival a pending `respond_by` deadline was waiting on. A record
/// answering a step whose gate the contract declares but this campaign
/// did not materialize (a rejected campaign produces none) is
/// unresolvable — skipped without a finding, exactly as the verifier
/// treats an uncommitted request digest: not forged, not checked.
pub(super) fn check_routing_records(
    package: &VerifiedCasePackage,
    compiled: &avila_core_compiler::CompiledContract,
    gates: &mut [PresentationGateReport],
) -> Vec<RunFinding> {
    let mut findings = Vec::new();
    for document in package
        .manifest
        .documents
        .iter()
        .filter(|document| document.role == "staged_review_record")
    {
        let location = || {
            SourceLocation::new(
                "manifest",
                format!("/documents/*/document_id={}", document.document_id),
            )
        };
        let Some(bytes) = package.document_by_id(&document.document_id) else {
            findings.push(routing_finding(
                CORE_X6502,
                location(),
                format!(
                    "routing record `{}` has no bytes in the package",
                    document.document_id
                ),
            ));
            continue;
        };
        let record: StagedReviewRecord = match serde_json::from_slice(bytes) {
            Ok(record) => record,
            Err(error) => {
                findings.push(routing_finding(
                    CORE_X6502,
                    location(),
                    format!(
                        "routing record `{}` is malformed: {error}",
                        document.document_id
                    ),
                ));
                continue;
            }
        };
        if record.schema_version != STAGED_REVIEW_SCHEMA_VERSION {
            findings.push(routing_finding(
                CORE_X6502,
                location(),
                format!(
                    "routing record `{}` carries schema_version `{}`, not `{STAGED_REVIEW_SCHEMA_VERSION}`",
                    document.document_id, record.schema_version
                ),
            ));
            continue;
        }
        let step_id = record
            .review_request
            .get("step_id")
            .and_then(|id| id.as_str())
            .unwrap_or("");
        // A record answering a gate the contract never declares is a
        // forged claim about a nonexistent review — quarantine it.
        let declared = compiled
            .workflow
            .iter()
            .any(|step| step.step_id == step_id && step.presentation_gate.is_some());
        if !declared {
            findings.push(routing_finding(
                CORE_X6501,
                location(),
                format!(
                    "routing record `{}` answers step `{step_id}`, which the contract declares no presentation gate for",
                    document.document_id
                ),
            ));
            continue;
        }
        // The gate is declared but did not materialize this run — the
        // record's binding is unresolvable, not invalid.
        if !gates.iter().any(|gate| gate.step_id == step_id) {
            continue;
        }
        match check_record(&record, gates) {
            Ok(gate_index) => {
                let request = &record.review_request;
                let policy = request
                    .get("reviewer_eligibility_policy")
                    .and_then(|policy| policy.get("policy_id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or("?");
                gates[gate_index].routing = Some(RoutingReport {
                    record_id: document.document_id.clone(),
                    state: RoutingState::Recorded,
                    disposition: Some(record.disposition),
                    detail: format!(
                        "routed under policy `{policy}`; disposition `{}`",
                        disposition_label(record.disposition)
                    ),
                });
            }
            Err(reason) => {
                findings.push(routing_finding(CORE_X6501, location(), reason.clone()));
                if let Some(gate) = gates.iter_mut().find(|gate| gate.step_id == step_id) {
                    gate.routing = Some(RoutingReport {
                        record_id: document.document_id.clone(),
                        state: RoutingState::Quarantined,
                        disposition: None,
                        detail: reason,
                    });
                }
            }
        }
    }
    findings
}

/// The first A9 binding the record violates, or the matching gate's index
/// when every bound digest and the disposition agree with the
/// materialized gate.
pub(super) fn check_record(
    record: &StagedReviewRecord,
    gates: &[PresentationGateReport],
) -> Result<usize, String> {
    let request = &record.review_request;
    let step_id = request
        .get("step_id")
        .and_then(|id| id.as_str())
        .ok_or_else(|| "routing record's embedded request names no `step_id`".to_string())?;
    let Some(gate_index) = gates.iter().position(|gate| gate.step_id == step_id) else {
        return Err(format!(
            "routing record answers step `{step_id}`, which this campaign has no presentation gate for"
        ));
    };
    let gate = &gates[gate_index];
    let bound_request = request
        .get("request_sha256")
        .and_then(|digest| digest.as_str())
        .ok_or_else(|| {
            "routing record's embedded request carries no `request_sha256`".to_string()
        })?;
    let recomputed = canonical_identity(request, "request_sha256").map_err(|error| {
        format!("routing record's request digest cannot be recomputed: {error}")
    })?;
    if recomputed.as_deref() != Some(bound_request) {
        return Err(format!(
            "request_sha256 {bound_request} does not recompute to {recomputed:?} — the embedded request was rewritten"
        ));
    }
    if bound_request != gate.request_sha256 {
        return Err(format!(
            "binds request `{bound_request}`, but the materialized request is `{}` — a routing made against a different dossier does not answer this gate",
            gate.request_sha256
        ));
    }
    let policy = request
        .get("reviewer_eligibility_policy")
        .cloned()
        .and_then(|value| serde_json::from_value::<ImmutablePolicyRef>(value).ok());
    match policy {
        Some(policy) if policy == gate.reviewer_eligibility_policy => {}
        Some(policy) => {
            return Err(format!(
                "names policy `{}` rev {}, but the gate binds `{}` rev {} — a routing performed under a different policy does not answer this gate",
                policy.policy_id,
                policy.revision,
                gate.reviewer_eligibility_policy.policy_id,
                gate.reviewer_eligibility_policy.revision
            ));
        }
        None => {
            return Err(
                "embedded request carries no legible `reviewer_eligibility_policy`".to_string(),
            );
        }
    }
    let allowed = request
        .get("allowed_dispositions")
        .and_then(|value| serde_json::from_value::<Vec<ReviewDisposition>>(value.clone()).ok());
    if let Some(allowed) = allowed
        && !allowed.contains(&record.disposition)
    {
        return Err(format!(
            "records disposition `{}`, which the request's `allowed_dispositions` does not allow",
            disposition_label(record.disposition)
        ));
    }
    if let Some(reviewer) = &record.reviewer {
        let declared = request
            .get("reviewer_role")
            .and_then(|role| serde_json::from_value::<ReviewerRole>(role.clone()).ok());
        if declared.is_some_and(|role| role != reviewer.role) {
            return Err("record's reviewer role differs from the request's `reviewer_role`".into());
        }
    }
    if let Some(recorded) = &record.record_sha256 {
        let mut whole = serde_json::to_value(record).map_err(|error| error.to_string())?;
        whole
            .as_object_mut()
            .expect("a record serializes as an object")
            .remove("record_sha256");
        let bytes = serde_json::to_vec(&whole).map_err(|error| error.to_string())?;
        let canonical = canonicalize_json(&bytes).map_err(|error| error.to_string())?;
        let recomputed = format!("sha256:{:x}", Sha256::digest(&canonical));
        if recomputed != *recorded {
            return Err(format!(
                "record_sha256 {recorded} recomputes to {recomputed} — the record was rewritten after binding"
            ));
        }
    }
    Ok(gate_index)
}

pub(super) fn canonical_identity(
    document: &serde_json::Value,
    field: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let mut body = document.clone();
    if body
        .as_object_mut()
        .and_then(|object| object.remove(field))
        .is_none()
    {
        return Ok(None);
    }
    Ok(Some(format!(
        "sha256:{:x}",
        Sha256::digest(canonicalize_json(&serde_json::to_vec(&body)?)?)
    )))
}

fn routing_finding(code: &'static str, primary: SourceLocation, message: String) -> RunFinding {
    RunFinding::runtime(
        code,
        FindingClass::Notice,
        RunStage::CampaignEvaluation,
        "requester",
        primary,
        message,
    )
}

fn disposition_label(disposition: ReviewDisposition) -> &'static str {
    match disposition {
        ReviewDisposition::PresentToUser => "present_to_user",
        ReviewDisposition::RequestChanges => "request_changes",
        ReviewDisposition::Abstain => "abstain",
    }
}
