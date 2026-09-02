//! Generating the evidence-claims document from what the package identifies
//! and what execution produced, instead of authoring its values by hand.
//!
//! Input attestations come from the package's artifact identities; claims for
//! executed steps come from the adapters' extraction over fresh output bytes;
//! claims for steps that were not executed are carried from the committed
//! document as recorded attestations. The result is compared canonically with
//! the committed claims document so any drift is visible.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

use avila_core_compiler::CompiledContract;
use avila_core_evidence::CasePackageManifest;
use avila_core_kernel::{ExactNumber, canonicalize_json, lower_authored_decimal};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const CLAIMS_SCHEMA_VERSION: &str = avila_core_compiler::CLAIMS_SCHEMA_VERSION;
pub const SEMANTIC_PROFILE: &str = avila_core_kernel::SEMANTIC_PROFILE;

/// One claim produced by an executed step, ready to enter the document.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedClaim {
    pub claim_id: String,
    pub step_id: String,
    pub output_slot: String,
    pub artifact_sha256: String,
    pub media_type: String,
    pub producer_package_id: String,
    pub producer_sha256: String,
    pub claim: Value,
    /// The producer's envelope for this run, when the package qualifies it.
    pub qualification: Option<Value>,
    /// Whether the output was reused from a committed receipt rather than
    /// produced by a fresh execution in this run.
    pub reused: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedClaims {
    pub value: Value,
    pub bytes: Vec<u8>,
    pub canonical_sha256: String,
    pub input_attestations: usize,
    pub executed_claims: usize,
    pub reused_claims: usize,
    pub recorded_claims: usize,
    /// Committed claims that were not carried because a supplied input
    /// reaches their step; they described a different candidate.
    pub invalidated_claims: usize,
}

pub fn generate_claims(
    compiled: &CompiledContract,
    manifest: &CasePackageManifest,
    committed: &Value,
    executed: &[GeneratedClaim],
    invalidated_steps: &BTreeSet<String>,
) -> Result<GeneratedClaims, Box<dyn Error>> {
    let artifact_by_evidence: BTreeMap<&str, &avila_core_evidence::PackageArtifact> = manifest
        .artifacts
        .iter()
        .flat_map(|artifact| {
            artifact
                .evidence_ids
                .iter()
                .map(move |evidence_id| (evidence_id.as_str(), artifact))
        })
        .collect();
    let committed_inputs: BTreeMap<&str, &Value> = committed
        .get("inputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|input| {
            input
                .get("input_id")
                .and_then(Value::as_str)
                .map(|id| (id, input))
        })
        .collect();

    let mut inputs = Vec::with_capacity(compiled.inputs.len());
    for input in &compiled.inputs {
        let evidence_id = format!("input:{}", input.input_id);
        if let Some(artifact) = artifact_by_evidence.get(evidence_id.as_str()) {
            inputs.push(json!({
                "input_id": input.input_id,
                "artifact": { "sha256": artifact.sha256, "media_type": input.media_type },
            }));
        } else if let Some(recorded) = committed_inputs.get(input.input_id.as_str()) {
            inputs.push((*recorded).clone());
        }
    }

    let executed_steps: BTreeSet<&str> = executed
        .iter()
        .map(|claim| claim.step_id.as_str())
        .collect();
    let committed_claims: Vec<&Value> = committed
        .get("claims")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    let workflow_steps: BTreeSet<&str> = compiled
        .workflow
        .iter()
        .map(|step| step.step_id.as_str())
        .collect();

    let mut claims = Vec::new();
    let mut executed_count = 0;
    let mut reused_count = 0;
    let mut recorded_count = 0;
    let mut invalidated_count = 0;
    for step in &compiled.workflow {
        if executed_steps.contains(step.step_id.as_str()) {
            for claim in executed
                .iter()
                .filter(|claim| claim.step_id == step.step_id)
            {
                if claim.reused {
                    reused_count += 1;
                } else {
                    executed_count += 1;
                }
                claims.push(json!({
                    "claim_id": claim.claim_id,
                    "step_id": claim.step_id,
                    "output_slot": claim.output_slot,
                    "artifact": { "sha256": claim.artifact_sha256, "media_type": claim.media_type },
                    "producer": { "package_id": claim.producer_package_id, "sha256": claim.producer_sha256 },
                    "claim": claim.claim,
                }));
                if let Some(qualification) = &claim.qualification
                    && let Some(object) = claims.last_mut().and_then(Value::as_object_mut)
                {
                    object.insert("qualification".into(), qualification.clone());
                }
            }
        } else {
            for claim in committed_claims.iter().filter(|claim| {
                claim.get("step_id").and_then(Value::as_str) == Some(step.step_id.as_str())
            }) {
                if invalidated_steps.contains(&step.step_id) {
                    invalidated_count += 1;
                    continue;
                }
                recorded_count += 1;
                claims.push((*claim).clone());
            }
        }
    }
    for claim in committed_claims.iter().filter(|claim| {
        claim
            .get("step_id")
            .and_then(Value::as_str)
            .is_none_or(|step_id| !workflow_steps.contains(step_id))
    }) {
        recorded_count += 1;
        claims.push((*claim).clone());
    }

    let value = json!({
        "schema_version": CLAIMS_SCHEMA_VERSION,
        "semantic_profile": SEMANTIC_PROFILE,
        "compiled_snapshot_sha256": compiled.snapshot_sha256,
        "inputs": inputs,
        "claims": claims,
    });
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    let canonical_sha256 = canonical_identity(&bytes)?;
    Ok(GeneratedClaims {
        input_attestations: value["inputs"].as_array().map_or(0, Vec::len),
        executed_claims: executed_count,
        reused_claims: reused_count,
        recorded_claims: recorded_count,
        invalidated_claims: invalidated_count,
        value,
        bytes,
        canonical_sha256,
    })
}

/// The canonical identity of a JSON document's bytes.
pub fn canonical_identity(bytes: &[u8]) -> Result<String, Box<dyn Error>> {
    let canonical = canonicalize_json(bytes)?;
    Ok(format!("sha256:{:x}", Sha256::digest(&canonical)))
}

/// Render an exact value as a canonical plain decimal, or refuse when it has
/// no finite decimal expansion within the work budget.
pub fn canonical_decimal(value: &ExactNumber) -> Result<String, String> {
    for scale in 0..=38 {
        if let Ok(fixed) = value.to_fixed_decimal(scale) {
            return lower_authored_decimal(&fixed).map_err(|error| error.detail().to_string());
        }
    }
    Err(format!("{value} has no finite decimal expansion"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_render_canonically() {
        let half = ExactNumber::from_canonical("1/2").unwrap();
        assert_eq!(canonical_decimal(&half).unwrap(), "0.5");
        let whole = ExactNumber::from_canonical("3").unwrap();
        assert_eq!(canonical_decimal(&whole).unwrap(), "3");
        let third = ExactNumber::from_canonical("1/3").unwrap();
        assert!(canonical_decimal(&third).is_err());
        let small = ExactNumber::from_canonical("0.0000000000000000000000062816").unwrap();
        assert_eq!(
            canonical_decimal(&small).unwrap(),
            "0.0000000000000000000000062816"
        );
    }
}
