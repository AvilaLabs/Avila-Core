//! Admission of attested inputs and output claims.
//!
//! Each admitted record gets an explicit state with reasons. Conditions
//! checked: the record names something the compiled snapshot has, its
//! artifact identity is well formed, every parent it used is admitted (A3), a
//! claim exists (A5), the claim's model is permitted by the output, its shape
//! satisfies that model, its quantities scale in the role's kind, and its media
//! type matches (A6 at type level), and one claim exists per output slot.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use avila_core_kernel::{ExactNumber, read_authoritative_decimal};

use super::document::{ClaimValue, ClaimsDocument, OutputClaim};
use super::{AdmissionReason, AdmissionRecord, AdmissionState};
use crate::compile::registry::RegistryIndex;
use crate::compile::review::is_sha256_identity;
use crate::compile::{CompiledContract, CompiledStep};
use crate::diagnostic::{
    CORE_E7002, CORE_E7101, CORE_E7103, CORE_E7201, CORE_E7301, CORE_S1102, CoreDiagnostic,
    FindingClass, SourceLocation,
};
use crate::document::{
    BoundSide, ClaimModelDeclaration, OutputSlotDefinition, QuantityValue, SourceRef,
};

pub(super) fn admit(
    compiled: &CompiledContract,
    registry: &RegistryIndex<'_>,
    claims: &ClaimsDocument,
    findings: &mut Vec<CoreDiagnostic>,
) -> Vec<AdmissionRecord> {
    let mut records: Vec<AdmissionRecord> = Vec::new();
    let mut admitted: BTreeSet<SourceRef> = BTreeSet::new();

    // Contract inputs: attested by artifact identity.
    let known_inputs: BTreeSet<&str> = compiled
        .inputs
        .iter()
        .map(|input| input.input_id.as_str())
        .collect();
    let mut seen_inputs = BTreeSet::new();
    for (index, attestation) in claims.inputs.iter().enumerate() {
        let location = claims_location(format!("/inputs/{index}"));
        if !known_inputs.contains(attestation.input_id.as_str()) {
            findings.push(CoreDiagnostic::new(
                CORE_E7002,
                FindingClass::Invalid,
                "executor",
                claims_location(format!("/inputs/{index}/input_id")),
                format!(
                    "attestation names contract input `{}`, which the compiled snapshot does not declare",
                    attestation.input_id
                ),
            ));
            continue;
        }
        if !seen_inputs.insert(attestation.input_id.as_str()) {
            findings.push(CoreDiagnostic::new(
                CORE_E7301,
                FindingClass::Invalid,
                "executor",
                location,
                format!(
                    "contract input `{}` is attested more than once",
                    attestation.input_id
                ),
            ));
            continue;
        }
    }
    for input in &compiled.inputs {
        let source = SourceRef::ContractInput {
            input_id: input.input_id.clone(),
        };
        let evidence_id = format!("input:{}", input.input_id);
        let attestations: Vec<(usize, _)> = claims
            .inputs
            .iter()
            .enumerate()
            .filter(|(_, attestation)| attestation.input_id == input.input_id)
            .collect();
        let Some((index, attestation)) = attestations.first() else {
            findings.push(CoreDiagnostic::new(
                CORE_E7101,
                FindingClass::Missing,
                "executor",
                claims_location("/inputs"),
                format!(
                    "contract input `{}` has no attested artifact",
                    input.input_id
                ),
            ));
            records.push(AdmissionRecord {
                evidence_id,
                source,
                state: AdmissionState::Missing,
                reasons: vec![reason(CORE_E7101, "no artifact attested for this input")],
            });
            continue;
        };
        let mut reasons = Vec::new();
        if attestations.len() > 1 {
            reasons.push(reason(CORE_E7301, "attested more than once"));
        }
        if !is_sha256_identity(&attestation.artifact.sha256) {
            let detail = "artifact identity must be a lowercase `sha256:` digest of 64 hex digits";
            findings.push(CoreDiagnostic::new(
                CORE_E7101,
                FindingClass::Invalid,
                "executor",
                claims_location(format!("/inputs/{index}/artifact/sha256")),
                detail,
            ));
            reasons.push(reason(CORE_E7101, detail));
        }
        if attestation.artifact.media_type != input.media_type {
            let detail = format!(
                "artifact media type `{}` differs from the declared input media type `{}`",
                attestation.artifact.media_type, input.media_type
            );
            findings.push(CoreDiagnostic::new(
                CORE_E7201,
                FindingClass::Inadmissible,
                "executor",
                claims_location(format!("/inputs/{index}/artifact/media_type")),
                detail.clone(),
            ));
            reasons.push(reason(CORE_E7201, detail));
        }
        let state = if reasons.is_empty() {
            admitted.insert(source.clone());
            AdmissionState::Admitted
        } else {
            AdmissionState::Quarantined
        };
        records.push(AdmissionRecord {
            evidence_id,
            source,
            state,
            reasons,
        });
    }

    // Output claims, in the compiled (topological) step order so parents come first.
    let mut claim_ids = BTreeSet::new();
    for (index, claim) in claims.claims.iter().enumerate() {
        if !claim_ids.insert(claim.claim_id.as_str()) {
            findings.push(CoreDiagnostic::new(
                CORE_S1102,
                FindingClass::Invalid,
                "executor",
                claims_location(format!("/claims/{index}/claim_id")),
                format!("claim identifier `{}` is repeated", claim.claim_id),
            ));
        }
    }
    let mut per_slot: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (index, claim) in claims.claims.iter().enumerate() {
        per_slot
            .entry((claim.step_id.clone(), claim.output_slot.clone()))
            .or_default()
            .push(index);
    }
    let steps: BTreeMap<&str, &CompiledStep> = compiled
        .workflow
        .iter()
        .map(|step| (step.step_id.as_str(), step))
        .collect();
    for (index, claim) in claims.claims.iter().enumerate() {
        let Some(step) = steps.get(claim.step_id.as_str()) else {
            findings.push(CoreDiagnostic::new(
                CORE_E7002,
                FindingClass::Invalid,
                "executor",
                claims_location(format!("/claims/{index}/step_id")),
                format!(
                    "claim names step `{}`, which the compiled snapshot does not have",
                    claim.step_id
                ),
            ));
            continue;
        };
        let output = registry
            .capability_types
            .get(&step.capability_type)
            .and_then(|capability| {
                capability
                    .outputs
                    .iter()
                    .find(|output| output.slot_id == claim.output_slot)
            });
        let Some(output) = output else {
            findings.push(CoreDiagnostic::new(
                CORE_E7002,
                FindingClass::Invalid,
                "executor",
                claims_location(format!("/claims/{index}/output_slot")),
                format!(
                    "step `{}` of type `{}@{}` declares no output slot `{}`",
                    claim.step_id,
                    step.capability_type.id,
                    step.capability_type.major,
                    claim.output_slot
                ),
            ));
            continue;
        };
        let source = SourceRef::StepOutput {
            step_id: claim.step_id.clone(),
            output_slot: claim.output_slot.clone(),
        };
        let mut reasons = Vec::new();
        let siblings = &per_slot[&(claim.step_id.clone(), claim.output_slot.clone())];
        if siblings.len() > 1 {
            let detail = format!(
                "output `{}` of step `{}` carries {} claims; a slot admits exactly one",
                claim.output_slot,
                claim.step_id,
                siblings.len()
            );
            findings.push(CoreDiagnostic::new(
                CORE_E7301,
                FindingClass::Inadmissible,
                "executor",
                claims_location(format!("/claims/{index}")),
                detail.clone(),
            ));
            reasons.push(reason(CORE_E7301, detail));
        }
        if !is_sha256_identity(&claim.artifact.sha256) {
            let detail = "artifact identity must be a lowercase `sha256:` digest of 64 hex digits";
            findings.push(CoreDiagnostic::new(
                CORE_E7101,
                FindingClass::Invalid,
                "executor",
                claims_location(format!("/claims/{index}/artifact/sha256")),
                detail,
            ));
            reasons.push(reason(CORE_E7101, detail));
        }
        if claim.artifact.media_type != output.media_type {
            let detail = format!(
                "artifact media type `{}` differs from the output's declared `{}`",
                claim.artifact.media_type, output.media_type
            );
            findings.push(CoreDiagnostic::new(
                CORE_E7201,
                FindingClass::Inadmissible,
                "executor",
                claims_location(format!("/claims/{index}/artifact/media_type")),
                detail.clone(),
            ));
            reasons.push(reason(CORE_E7201, detail));
        }
        for parent in &step.bindings {
            if !admitted.contains(&parent.source) {
                let detail = format!(
                    "parent `{}` bound to input slot `{}` is not admitted",
                    parent.source.label(),
                    parent.input_slot
                );
                findings.push(CoreDiagnostic::new(
                    CORE_E7103,
                    FindingClass::Inadmissible,
                    "executor",
                    claims_location(format!("/claims/{index}")),
                    detail.clone(),
                ));
                reasons.push(reason(CORE_E7103, detail));
            }
        }
        for detail in validate_claim(registry, output, claim) {
            findings.push(CoreDiagnostic::new(
                CORE_E7201,
                FindingClass::Inadmissible,
                "executor",
                claims_location(format!("/claims/{index}/claim")),
                detail.clone(),
            ));
            reasons.push(reason(CORE_E7201, detail));
        }
        let state = if reasons.is_empty() {
            admitted.insert(source.clone());
            AdmissionState::Admitted
        } else {
            AdmissionState::Quarantined
        };
        records.push(AdmissionRecord {
            evidence_id: claim.claim_id.clone(),
            source,
            state,
            reasons,
        });
    }

    records
}

/// Type-level validation of one claim against its output: permitted model,
/// shape, and unit scaling in the role's kind.
fn validate_claim(
    registry: &RegistryIndex<'_>,
    output: &OutputSlotDefinition,
    claim: &OutputClaim,
) -> Vec<String> {
    let mut problems = Vec::new();
    let permitted = output
        .permitted_claim_models
        .iter()
        .any(|model| match (model, &claim.claim) {
            (ClaimModelDeclaration::Exact, ClaimValue::Exact { .. })
            | (ClaimModelDeclaration::Interval { .. }, ClaimValue::Interval { .. })
            | (ClaimModelDeclaration::CoverageInterval, ClaimValue::CoverageInterval { .. })
            | (ClaimModelDeclaration::Unquantified, ClaimValue::Unquantified { .. }) => true,
            (
                ClaimModelDeclaration::WorstCase { side, .. },
                ClaimValue::WorstCase { lower, upper, .. },
            ) => match side {
                BoundSide::Lower => lower.is_some() && upper.is_none(),
                BoundSide::Upper => upper.is_some() && lower.is_none(),
            },
            _ => false,
        });
    if !permitted {
        problems.push(format!(
            "claim model `{}` is not permitted by output `{}`",
            model_label(&claim.claim),
            output.slot_id
        ));
    }
    let role = registry.roles.get(&output.role);
    let kind = role.and_then(|role| role.quantity_kind.as_deref());
    let quantities: Vec<(&str, &QuantityValue)> = match &claim.claim {
        ClaimValue::Exact { nominal } | ClaimValue::CoverageInterval { nominal, .. }
            if matches!(claim.claim, ClaimValue::Exact { .. }) =>
        {
            vec![("nominal", nominal)]
        }
        ClaimValue::Exact { nominal } => vec![("nominal", nominal)],
        ClaimValue::Interval {
            lower,
            upper,
            nominal,
        } => {
            let mut all = vec![("lower", lower), ("upper", upper)];
            all.extend(nominal.iter().map(|nominal| ("nominal", nominal)));
            all
        }
        ClaimValue::CoverageInterval {
            lower,
            upper,
            nominal,
            coverage,
        } => {
            match read_authoritative_decimal(coverage) {
                Ok(value) => {
                    let one = ExactNumber::from_canonical("1").expect("`1` is canonical");
                    if !value.is_positive()
                        || value
                            .checked_cmp(&one)
                            .is_ok_and(|order| order == Ordering::Greater)
                    {
                        problems.push("coverage must lie in the interval (0, 1]".into());
                    }
                }
                Err(error) => problems.push(format!(
                    "coverage must be a canonical decimal in (0, 1]: {}",
                    error.detail()
                )),
            }
            vec![("lower", lower), ("upper", upper), ("nominal", nominal)]
        }
        ClaimValue::WorstCase {
            lower,
            upper,
            nominal,
        } => {
            if lower.is_some() == upper.is_some() {
                problems
                    .push("a worst-case claim carries exactly one of `lower` or `upper`".into());
            }
            let mut all = Vec::new();
            all.extend(lower.iter().map(|lower| ("lower", lower)));
            all.extend(upper.iter().map(|upper| ("upper", upper)));
            all.extend(nominal.iter().map(|nominal| ("nominal", nominal)));
            all
        }
        ClaimValue::Unquantified { nominal, .. } => {
            nominal.iter().map(|nominal| ("nominal", nominal)).collect()
        }
    };
    if let ClaimValue::Unquantified {
        value: Some(value), ..
    } = &claim.claim
        && value.trim().is_empty()
    {
        problems.push("an unquantified categorical value must not be empty".into());
    }
    match kind {
        Some(kind) => {
            let mut canonical = BTreeMap::new();
            for (field, quantity) in &quantities {
                match registry
                    .kinds
                    .scale_quantity(kind, &quantity.value, &quantity.unit)
                {
                    Ok(scaled) => {
                        canonical.insert(*field, scaled.value);
                    }
                    Err(error) => problems.push(format!("`{field}`: {}", error.detail())),
                }
            }
            if matches!(claim.claim, ClaimValue::Unquantified { nominal: None, .. }) {
                problems
                    .push("a quantity role requires a nominal value even when unquantified".into());
            }
            if matches!(claim.claim, ClaimValue::Unquantified { value: Some(_), .. }) {
                problems
                    .push("a quantity role cannot carry an unquantified categorical value".into());
            }
            if let (Some(lower), Some(upper)) = (canonical.get("lower"), canonical.get("upper"))
                && lower
                    .checked_cmp(upper)
                    .is_ok_and(|order| order == Ordering::Greater)
            {
                problems.push("`lower` exceeds `upper`".into());
            }
            if let Some(nominal) = canonical.get("nominal") {
                let below = canonical.get("lower").is_some_and(|lower| {
                    nominal
                        .checked_cmp(lower)
                        .is_ok_and(|order| order == Ordering::Less)
                });
                let above = canonical.get("upper").is_some_and(|upper| {
                    nominal
                        .checked_cmp(upper)
                        .is_ok_and(|order| order == Ordering::Greater)
                });
                if below || above {
                    problems.push("`nominal` lies outside the declared interval".into());
                }
            }
        }
        None => {
            if !quantities.is_empty() {
                problems.push(format!(
                    "role `{}@{}` is not a quantity role and admits no quantity values",
                    output.role.id, output.role.major
                ));
            }
        }
    }
    problems
}

const fn model_label(claim: &ClaimValue) -> &'static str {
    match claim {
        ClaimValue::Exact { .. } => "exact",
        ClaimValue::Interval { .. } => "interval",
        ClaimValue::CoverageInterval { .. } => "coverage_interval",
        ClaimValue::WorstCase { .. } => "worst_case",
        ClaimValue::Unquantified { .. } => "unquantified",
    }
}

fn reason(code: &str, detail: impl Into<String>) -> AdmissionReason {
    AdmissionReason {
        code: code.into(),
        detail: detail.into(),
    }
}

fn claims_location(pointer: impl Into<String>) -> SourceLocation {
    SourceLocation::new("claims", pointer)
}
