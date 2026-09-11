//! Post-execution identity binding and expected-report replay.
//!
//! `changes_since` classifies every difference between a committed receipt and
//! the invocation planned now by SC-12 change class — the reuse gate. After a
//! run, `verify_bindings` checks the generated claims document against the
//! package's declared identities, and `replay_expected` compares the evaluated
//! campaign report against the committed `expected_campaign_report` document.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

use avila_core_compiler::{CampaignReport, ClaimsDocument, CompiledContract};
use avila_core_evidence::{
    CapabilityIdentity, CasePackageManifest, ExecutionReceipt, ReceiptInput, ReceiptStatus,
    VerifiedCasePackage,
};
use serde_json::Value;

use crate::execute::PlannedInvocation;

use super::{BindingReport, BindingStatus, ChangeClass, ChangeRecord, ReplayReport};

/// What differs between a committed receipt and the invocation planned now,
/// by SC-12 change class. Empty means the receipt describes exactly this
/// request.
pub(super) fn changes_since(
    committed: &ExecutionReceipt,
    plan: &PlannedInvocation,
    capability: &CapabilityIdentity,
    parameters: &BTreeMap<String, Value>,
    case_id: &str,
) -> Vec<ChangeRecord> {
    let mut changes = Vec::new();
    // A receipt whose case_id differs was produced for a different case,
    // even if every other field of the plan happens to coincide (a donor
    // receipt copied from another package with the same capability,
    // parameters, and input identities). Checked before anything else so
    // such a receipt is never mistaken for a merely-unchanged one.
    //
    // `compiled_snapshot_sha256` is deliberately not compared: SC-12
    // execution memoization is about what a capability ran over, not about
    // the requirement or policy logic later applied to its outputs, so a
    // requirement, registry, or review edit that changes the compiled
    // snapshot identity must not by itself invalidate a step's receipt.
    if committed.case_id != case_id {
        changes.push(ChangeRecord {
            class: ChangeClass::DifferentCase,
            detail: format!(
                "receipt was produced for case `{}`, not `{case_id}`",
                committed.case_id
            ),
        });
    }
    if committed.capability != *capability {
        changes.push(ChangeRecord {
            class: ChangeClass::Capability,
            detail: format!(
                "executable {} → {}",
                committed.capability.executable_sha256, capability.executable_sha256
            ),
        });
    }
    let keys: BTreeSet<&String> = committed
        .parameters
        .keys()
        .chain(parameters.keys())
        .collect();
    for key in keys {
        if committed.parameters.get(key) != parameters.get(key) {
            changes.push(ChangeRecord {
                class: ChangeClass::Parameters,
                detail: format!(
                    "parameter `{key}` {} → {}",
                    committed
                        .parameters
                        .get(key)
                        .map_or("absent".to_string(), Value::to_string),
                    parameters
                        .get(key)
                        .map_or("absent".to_string(), Value::to_string)
                ),
            });
        }
    }
    let before: BTreeMap<&str, &ReceiptInput> = committed
        .inputs
        .iter()
        .map(|input| (input.input_slot.as_str(), input))
        .collect();
    let after: BTreeMap<&str, &ReceiptInput> = plan
        .inputs
        .iter()
        .map(|input| (input.input_slot.as_str(), input))
        .collect();
    let slots: BTreeSet<&str> = before.keys().chain(after.keys()).copied().collect();
    for slot in slots {
        match (before.get(slot), after.get(slot)) {
            (Some(old), Some(new)) => {
                if old.evidence_id != new.evidence_id {
                    changes.push(ChangeRecord {
                        class: ChangeClass::InputBinding,
                        detail: format!(
                            "slot `{slot}` bound `{}` → `{}`",
                            old.evidence_id, new.evidence_id
                        ),
                    });
                }
                if old.sha256 != new.sha256 || old.bytes != new.bytes {
                    changes.push(ChangeRecord {
                        class: ChangeClass::InputBytes,
                        detail: format!("slot `{slot}` bytes {} → {}", old.sha256, new.sha256),
                    });
                } else if old.workspace_path != new.workspace_path
                    || old.media_type != new.media_type
                {
                    changes.push(ChangeRecord {
                        class: ChangeClass::Invocation,
                        detail: format!("slot `{slot}` staging path or media type differs"),
                    });
                }
            }
            (Some(_), None) => changes.push(ChangeRecord {
                class: ChangeClass::InputBinding,
                detail: format!("slot `{slot}` is no longer bound"),
            }),
            (None, Some(_)) => changes.push(ChangeRecord {
                class: ChangeClass::InputBinding,
                detail: format!("slot `{slot}` is newly bound"),
            }),
            (None, None) => {}
        }
    }
    let old = &committed.invocation;
    let new = &plan.invocation;
    if old.arguments != new.arguments
        || old.working_directory != new.working_directory
        || old.environment != new.environment
        || old.required_environment != new.required_environment
        || old.timeout_ms != new.timeout_ms
    {
        changes.push(ChangeRecord {
            class: ChangeClass::Invocation,
            detail: "the adapter's arguments, environment, working directory, or timeout differ"
                .into(),
        });
    }
    // The descriptor digest is invocation identity: extraction or mapping
    // edits that leave argv untouched must still invalidate the receipt.
    if old.adapter_sha256 != new.adapter_sha256 {
        changes.push(ChangeRecord {
            class: ChangeClass::Invocation,
            detail: format!(
                "adapter descriptor {} → {}",
                old.adapter_sha256.as_deref().unwrap_or("none"),
                new.adapter_sha256.as_deref().unwrap_or("none")
            ),
        });
    }
    if committed.status != ReceiptStatus::Completed || committed.process.exit_status != Some(0) {
        changes.push(ChangeRecord {
            class: ChangeClass::ReceiptNotCompleted,
            detail: "the committed receipt did not complete with exit status 0".into(),
        });
    }
    changes
}

pub(super) fn verify_bindings(
    manifest: &CasePackageManifest,
    claims: &ClaimsDocument,
    compiled: &CompiledContract,
    invalidated_steps: &BTreeSet<String>,
) -> BindingReport {
    let mut issues = Vec::new();
    let mut expected = BTreeMap::<String, String>::new();
    for input in &claims.inputs {
        insert_evidence(
            &mut expected,
            format!("input:{}", input.input_id),
            input.artifact.sha256.clone(),
            &mut issues,
        );
    }
    // Claims for steps a supplied input reaches carry the identity their
    // receipt recorded; the package's declared identity describes the
    // reference candidate and is not compared.
    let mut receipted: BTreeSet<String> = BTreeSet::new();
    let withheld: BTreeSet<&str> = manifest
        .executions
        .iter()
        .filter(|execution| invalidated_steps.contains(&execution.step_id))
        .flat_map(|execution| {
            execution
                .outputs
                .iter()
                .map(|output| output.claim_id.as_str())
        })
        .collect();
    for claim in &claims.claims {
        if invalidated_steps.contains(&claim.step_id) {
            receipted.insert(claim.claim_id.clone());
        }
        insert_evidence(
            &mut expected,
            claim.claim_id.clone(),
            claim.artifact.sha256.clone(),
            &mut issues,
        );
    }

    let mut seen = BTreeSet::new();
    let mut bound_evidence_records = 0;
    let mut receipted_evidence_records = 0;
    let mut withheld_evidence_records = 0;
    for artifact in &manifest.artifacts {
        for evidence_id in &artifact.evidence_ids {
            let Some(expected_sha256) = expected.get(evidence_id) else {
                if withheld.contains(evidence_id.as_str()) {
                    withheld_evidence_records += 1;
                    continue;
                }
                issues.push(format!(
                    "artifact `{}` binds unknown evidence record `{evidence_id}`",
                    artifact.artifact_id
                ));
                continue;
            };
            seen.insert(evidence_id.clone());
            if receipted.contains(evidence_id) {
                receipted_evidence_records += 1;
            } else if expected_sha256 == &artifact.sha256 {
                bound_evidence_records += 1;
            } else {
                issues.push(format!(
                    "artifact `{}` digest does not match evidence record `{evidence_id}`",
                    artifact.artifact_id
                ));
            }
        }
    }
    for evidence_id in expected.keys() {
        if !seen.contains(evidence_id) {
            issues.push(format!(
                "evidence record `{evidence_id}` has no package artifact binding"
            ));
        }
    }

    let required_policies: BTreeSet<String> = compiled
        .workflow
        .iter()
        .filter_map(|step| step.presentation_gate.as_ref())
        .map(|review| review.reviewer_eligibility_policy.sha256.clone())
        .collect();
    let package_policies: BTreeSet<String> = manifest
        .documents
        .iter()
        .filter(|document| document.role == "review_policy")
        .map(|document| document.sha256.clone())
        .collect();
    let bound_presentation_policies = required_policies.intersection(&package_policies).count();
    for digest in required_policies.difference(&package_policies) {
        issues.push(format!(
            "compiled presentation gate requires policy `{digest}`, but the package does not contain it"
        ));
    }
    for digest in package_policies.difference(&required_policies) {
        issues.push(format!(
            "package presentation policy `{digest}` is not referenced by the compiled contract"
        ));
    }

    BindingReport {
        status: if issues.is_empty() {
            BindingStatus::Verified
        } else {
            BindingStatus::Failed
        },
        evidence_records: expected.len(),
        bound_evidence_records,
        receipted_evidence_records,
        withheld_evidence_records,
        required_presentation_policies: required_policies.len(),
        bound_presentation_policies,
        issues,
    }
}

fn insert_evidence(
    expected: &mut BTreeMap<String, String>,
    evidence_id: String,
    sha256: String,
    issues: &mut Vec<String>,
) {
    if expected.insert(evidence_id.clone(), sha256).is_some() {
        issues.push(format!(
            "claims document repeats evidence record `{evidence_id}`"
        ));
    }
}

pub(super) fn replay_expected(
    package: &VerifiedCasePackage,
    campaign: &CampaignReport,
) -> Result<Option<ReplayReport>, Box<dyn Error>> {
    let Some(document) = package
        .manifest
        .documents
        .iter()
        .find(|document| document.role == "expected_campaign_report")
    else {
        return Ok(None);
    };
    let bytes = package
        .document_by_id(&document.document_id)
        .ok_or("expected campaign report was not readable after package verification")?;
    let expected: Value = serde_json::from_slice(bytes)?;
    let actual = serde_json::to_value(campaign)?;
    Ok(Some(ReplayReport {
        document_id: document.document_id.clone(),
        matches: expected == actual,
    }))
}
