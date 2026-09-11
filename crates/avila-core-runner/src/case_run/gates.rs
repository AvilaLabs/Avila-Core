//! Presentation-gate materialization.
//!
//! After campaign evaluation, each workflow step that declares a
//! `presentation_gate` is realized into a `PresentationGateReport`: the
//! gate's declared evidence bindings are resolved against the generated
//! claims document, a readiness state is derived, and a canonical request
//! identity (`request_sha256`) is bound over the realized report so a
//! reviewer's decision can name exactly what was presented.

use std::error::Error;

use avila_core_compiler::{
    CampaignReport, ClaimsDocument, CompiledContract, ResolvedBinding, SourceRef,
};
use sha2::{Digest, Sha256};

use super::{PresentationGateReadiness, PresentationGateReport, PresentedEvidence};

pub(super) fn build_presentation_gates(
    compiled: &CompiledContract,
    claims: &ClaimsDocument,
    campaign: &CampaignReport,
) -> Result<Vec<PresentationGateReport>, Box<dyn Error>> {
    let Some(campaign_sha256) = campaign.campaign_sha256.as_ref() else {
        return Ok(Vec::new());
    };
    let mut stages = Vec::new();
    for step in &compiled.workflow {
        let Some(gate) = &step.presentation_gate else {
            continue;
        };
        let mut presented_evidence = Vec::new();
        let mut missing_evidence = Vec::new();
        for binding in &gate.presented_evidence {
            match realize_presented_evidence(binding, claims) {
                Some(evidence) => presented_evidence.push(evidence),
                None => missing_evidence.push(binding.source.clone()),
            }
        }
        let readiness = if missing_evidence.is_empty() {
            PresentationGateReadiness::ReadyForAgent
        } else {
            PresentationGateReadiness::AwaitingEvidence
        };
        let mut gate = PresentationGateReport {
            request_sha256: String::new(),
            compiled_snapshot_sha256: compiled.snapshot_sha256.clone(),
            campaign_sha256: campaign_sha256.clone(),
            step_id: step.step_id.clone(),
            gate_state: gate.state,
            reviewer_role: gate.reviewer_role,
            readiness,
            presented_evidence,
            missing_evidence,
            decision_role: gate.decision_role.clone(),
            decision_media_type: gate.decision_media_type.clone(),
            allowed_dispositions: gate.allowed_dispositions.clone(),
            reviewer_eligibility_policy: gate.reviewer_eligibility_policy.clone(),
            independence: gate.independence.clone(),
            instructions: gate.instructions.clone(),
        };
        gate.request_sha256 = presentation_request_identity(&gate)?;
        stages.push(gate);
    }
    Ok(stages)
}

fn realize_presented_evidence(
    binding: &ResolvedBinding,
    claims: &ClaimsDocument,
) -> Option<PresentedEvidence> {
    match &binding.source {
        SourceRef::ContractInput { input_id } => {
            let mut matches = claims
                .inputs
                .iter()
                .filter(|input| input.input_id == *input_id);
            let input = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            Some(PresentedEvidence {
                input_slot: binding.input_slot.clone(),
                source: binding.source.clone(),
                evidence_id: format!("input:{input_id}"),
                sha256: input.artifact.sha256.clone(),
                media_type: input.artifact.media_type.clone(),
            })
        }
        SourceRef::StepOutput {
            step_id,
            output_slot,
        } => {
            let mut matches = claims
                .claims
                .iter()
                .filter(|claim| claim.step_id == *step_id && claim.output_slot == *output_slot);
            let output = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            Some(PresentedEvidence {
                input_slot: binding.input_slot.clone(),
                source: binding.source.clone(),
                evidence_id: output.claim_id.clone(),
                sha256: output.artifact.sha256.clone(),
                media_type: output.artifact.media_type.clone(),
            })
        }
    }
}

fn presentation_request_identity(gate: &PresentationGateReport) -> Result<String, Box<dyn Error>> {
    let mut value = serde_json::to_value(gate)?;
    value
        .as_object_mut()
        .expect("a presentation gate serializes as an object")
        .remove("request_sha256");
    let bytes = serde_json::to_vec(&value)?;
    let canonical = avila_core_kernel::canonicalize_json(&bytes)?;
    Ok(format!("sha256:{:x}", Sha256::digest(&canonical)))
}
