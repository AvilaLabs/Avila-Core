//! Admission of attested inputs and output claims.
//!
//! Each admitted record gets an explicit state with reasons. Conditions
//! checked: the record names something the compiled snapshot has, its
//! artifact identity is well formed, every parent it used is admitted (A3), a
//! claim exists (A5), the claim's model is permitted by the output, its shape
//! satisfies that model, its quantities scale in the role's kind, and its media
//! type matches (A6 at type level), and one claim exists per output slot.
//!
//! Every check performed is also emitted as a `RuleApplication` premise, so
//! the derivation replays the same checks rather than trusting the recorded
//! state (ADR-0026).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use avila_core_kernel::{ExactNumber, read_authoritative_decimal};

use super::context::EvaluationContext;
use super::derivation::{RuleApplication, RulePremise};
use super::document::{ClaimValue, OutputClaim};
use super::{AdmissionReason, AdmissionRecord, AdmissionState, ArtifactCheck, ArtifactCheckState};
use crate::compile::CompiledStep;
use crate::compile::registry::RegistryIndex;
use crate::compile::review::is_sha256_identity;
use crate::diagnostic::{
    CORE_E7002, CORE_E7101, CORE_E7103, CORE_E7201, CORE_E7301, CORE_S1102, CoreDiagnostic,
    FindingClass, SourceLocation,
};
use crate::document::{
    BoundSide, ClaimModelDeclaration, OutputSlotDefinition, QuantityValue, SourceRef,
};

pub(super) fn admit(
    context: &EvaluationContext,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> (Vec<AdmissionRecord>, Vec<RuleApplication>) {
    let compiled = context.compiled();
    let claims = context.claims();
    let observations = context.observations();
    let mut records: Vec<AdmissionRecord> = Vec::new();
    let mut applications: Vec<RuleApplication> = Vec::new();
    let mut admitted: BTreeSet<SourceRef> = BTreeSet::new();

    // Contract inputs: attested by artifact identity.
    let known_inputs: BTreeSet<&str> = compiled
        .inputs()
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
            applications.push(refused_input(
                &attestation.input_id,
                "undeclared",
                CORE_E7002,
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
            applications.push(refused_input(
                &attestation.input_id,
                "duplicated",
                CORE_E7301,
            ));
            continue;
        }
    }
    for input in compiled.inputs() {
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
        let mut application = RuleApplication::new("admission.input", &evidence_id, "missing");
        application.premises.push(RulePremise {
            kind: "contract_input".into(),
            id: input.input_id.clone(),
            state: "declared".into(),
        });
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
            application.premises.push(RulePremise {
                kind: "input_attestation".into(),
                id: input.input_id.clone(),
                state: "missing".into(),
            });
            application.reasons.push(CORE_E7101.into());
            records.push(AdmissionRecord {
                evidence_id,
                source,
                state: AdmissionState::Missing,
                artifact: None,
                reasons: vec![reason(CORE_E7101, "no artifact attested for this input")],
            });
            applications.push(application);
            continue;
        };
        application.premises.push(RulePremise {
            kind: "input_attestation".into(),
            id: attestation.artifact.sha256.clone(),
            state: if attestations.len() > 1 {
                "duplicated"
            } else {
                "attested"
            }
            .into(),
        });
        let mut reasons = Vec::new();
        if attestations.len() > 1 {
            reasons.push(reason(CORE_E7301, "attested more than once"));
        }
        application.premises.push(RulePremise {
            kind: "artifact_identity".into(),
            id: attestation.artifact.sha256.clone(),
            state: if is_sha256_identity(&attestation.artifact.sha256) {
                "well_formed"
            } else {
                "malformed"
            }
            .into(),
        });
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
        if !observations.is_empty() {
            application.premises.push(RulePremise {
                kind: "artifact_observation".into(),
                id: attestation.artifact.sha256.clone(),
                state: if observations.contains(&attestation.artifact.sha256) {
                    "verified"
                } else {
                    "not_checked"
                }
                .into(),
            });
        }
        application.premises.push(RulePremise {
            kind: "media_type".into(),
            id: attestation.artifact.media_type.clone(),
            state: if attestation.artifact.media_type == input.media_type {
                "match"
            } else {
                "mismatch"
            }
            .into(),
        });
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
        let artifact = (!observations.is_empty()).then(|| ArtifactCheck {
            sha256: attestation.artifact.sha256.clone(),
            check: if observations.contains(&attestation.artifact.sha256) {
                ArtifactCheckState::Verified
            } else {
                ArtifactCheckState::NotChecked
            },
        });
        let state = if reasons.is_empty() {
            admitted.insert(source.clone());
            application.conclusion = "admitted".into();
            AdmissionState::Admitted
        } else {
            application.conclusion = "quarantined".into();
            AdmissionState::Quarantined
        };
        application
            .reasons
            .extend(reasons.iter().map(|reason| reason.code.clone()));
        records.push(AdmissionRecord {
            evidence_id,
            source,
            state,
            reasons,
            artifact,
        });
        applications.push(application);
    }

    // Output claims, in the compiled (topological) step order so parents come first.
    let mut claim_ids = BTreeSet::new();
    let mut duplicated_claim_ids = BTreeSet::new();
    for (index, claim) in claims.claims.iter().enumerate() {
        if !claim_ids.insert(claim.claim_id.as_str()) {
            duplicated_claim_ids.insert(claim.claim_id.as_str());
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
        .workflow()
        .iter()
        .map(|step| (step.step_id.as_str(), step))
        .collect();
    for (index, claim) in claims.claims.iter().enumerate() {
        let mut application = RuleApplication::new("admission.claim", &claim.claim_id, "refused");
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
            application.premises.push(RulePremise {
                kind: "workflow_step".into(),
                id: claim.step_id.clone(),
                state: "undeclared".into(),
            });
            application.reasons.push(CORE_E7002.into());
            applications.push(application);
            continue;
        };
        application.premises.push(RulePremise {
            kind: "workflow_step".into(),
            id: claim.step_id.clone(),
            state: "declared".into(),
        });
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
            application.premises.push(RulePremise {
                kind: "output_slot".into(),
                id: format!("{}.{}", claim.step_id, claim.output_slot),
                state: "undeclared".into(),
            });
            application.reasons.push(CORE_E7002.into());
            applications.push(application);
            continue;
        };
        application.premises.push(RulePremise {
            kind: "output_slot".into(),
            id: format!("{}.{}", claim.step_id, claim.output_slot),
            state: "declared".into(),
        });
        application.premises.push(RulePremise {
            kind: "claim_id".into(),
            id: claim.claim_id.clone(),
            state: if duplicated_claim_ids.contains(claim.claim_id.as_str()) {
                "duplicated"
            } else {
                "unique"
            }
            .into(),
        });
        let source = SourceRef::StepOutput {
            step_id: claim.step_id.clone(),
            output_slot: claim.output_slot.clone(),
        };
        let mut reasons = Vec::new();
        let siblings = &per_slot[&(claim.step_id.clone(), claim.output_slot.clone())];
        application.premises.push(RulePremise {
            kind: "slot_cardinality".into(),
            id: format!("{}.{}", claim.step_id, claim.output_slot),
            state: if siblings.len() > 1 {
                "duplicated"
            } else {
                "unique"
            }
            .into(),
        });
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
        application.premises.push(RulePremise {
            kind: "artifact_identity".into(),
            id: claim.artifact.sha256.clone(),
            state: if is_sha256_identity(&claim.artifact.sha256) {
                "well_formed"
            } else {
                "malformed"
            }
            .into(),
        });
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
        if !observations.is_empty() {
            application.premises.push(RulePremise {
                kind: "artifact_observation".into(),
                id: claim.artifact.sha256.clone(),
                state: if observations.contains(&claim.artifact.sha256) {
                    "verified"
                } else {
                    "not_checked"
                }
                .into(),
            });
        }
        application.premises.push(RulePremise {
            kind: "media_type".into(),
            id: claim.artifact.media_type.clone(),
            state: if claim.artifact.media_type == output.media_type {
                "match"
            } else {
                "mismatch"
            }
            .into(),
        });
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
        application.premises.push(RulePremise {
            kind: "partial_result".into(),
            id: format!("{}.{}", claim.step_id, claim.output_slot),
            state: if !claim.partial {
                "none"
            } else if output.permits_partial {
                "permitted"
            } else {
                "not_permitted"
            }
            .into(),
        });
        if claim.partial && !output.permits_partial {
            let detail = format!(
                "claim records a partial result, but output slot `{}` of `{}@{}` does not declare `permits_partial`",
                claim.output_slot, step.capability_type.id, step.capability_type.major
            );
            findings.push(CoreDiagnostic::new(
                CORE_E7201,
                FindingClass::Inadmissible,
                "executor",
                claims_location(format!("/claims/{index}/partial")),
                detail.clone(),
            ));
            reasons.push(reason(CORE_E7201, detail));
        }
        for parent in &step.bindings {
            let parent_admitted = admitted.contains(&parent.source);
            application.premises.push(RulePremise {
                kind: "parent_admission".into(),
                id: parent.source.label(),
                state: if parent_admitted {
                    "admitted"
                } else {
                    "not_admitted"
                }
                .into(),
            });
            if !parent_admitted {
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
        let problems = validate_claim(registry, output, claim);
        application.premises.push(RulePremise {
            kind: "claim_model".into(),
            id: model_label(&claim.claim).into(),
            state: if problems.iter().any(|(kind, _)| *kind == "claim_model") {
                "not_permitted"
            } else {
                "permitted"
            }
            .into(),
        });
        let mut seen_problem_kinds = BTreeSet::new();
        for (kind, detail) in &problems {
            if seen_problem_kinds.insert(*kind) {
                application.premises.push(RulePremise {
                    kind: (*kind).into(),
                    id: claim.claim_id.clone(),
                    state: "invalid".into(),
                });
            }
            findings.push(CoreDiagnostic::new(
                CORE_E7201,
                FindingClass::Inadmissible,
                "executor",
                claims_location(format!("/claims/{index}/claim")),
                detail.clone(),
            ));
            reasons.push(reason(CORE_E7201, detail.clone()));
        }
        if problems.is_empty() {
            application.premises.push(RulePremise {
                kind: "claim_shape".into(),
                id: claim.claim_id.clone(),
                state: "valid".into(),
            });
        }
        let artifact = (!observations.is_empty()).then(|| ArtifactCheck {
            sha256: claim.artifact.sha256.clone(),
            check: if observations.contains(&claim.artifact.sha256) {
                ArtifactCheckState::Verified
            } else {
                ArtifactCheckState::NotChecked
            },
        });
        let state = if reasons.is_empty() {
            admitted.insert(source.clone());
            application.conclusion = "admitted".into();
            AdmissionState::Admitted
        } else {
            application.conclusion = "quarantined".into();
            AdmissionState::Quarantined
        };
        application
            .reasons
            .extend(reasons.iter().map(|reason| reason.code.clone()));
        records.push(AdmissionRecord {
            evidence_id: claim.claim_id.clone(),
            source,
            state,
            reasons,
            artifact,
        });
        applications.push(application);
    }

    (records, applications)
}

/// An `admission.input` application for an attestation that produced no
/// record — the input is undeclared or attested more than once.
fn refused_input(input_id: &str, state: &str, code: &str) -> RuleApplication {
    let mut application =
        RuleApplication::new("admission.input", format!("input:{input_id}"), "refused");
    application.premises.push(RulePremise {
        kind: "contract_input".into(),
        id: input_id.into(),
        state: state.into(),
    });
    application.reasons.push(code.into());
    application
}

/// Type-level validation of one claim against its output: permitted model,
/// shape, and unit scaling in the role's kind. Each problem names the
/// premise kind it defeats, so the derivation records which check failed —
/// not just that it did.
fn validate_claim(
    registry: &RegistryIndex<'_>,
    output: &OutputSlotDefinition,
    claim: &OutputClaim,
) -> Vec<(&'static str, String)> {
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
        problems.push((
            "claim_model",
            format!(
                "claim model `{}` is not permitted by output `{}`",
                model_label(&claim.claim),
                output.slot_id
            ),
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
                        problems.push((
                            "claim_shape",
                            "coverage must lie in the interval (0, 1]".into(),
                        ));
                    }
                }
                Err(error) => problems.push((
                    "claim_shape",
                    format!(
                        "coverage must be a canonical decimal in (0, 1]: {}",
                        error.detail()
                    ),
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
                problems.push((
                    "claim_shape",
                    "a worst-case claim carries exactly one of `lower` or `upper`".into(),
                ));
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
        problems.push((
            "claim_shape",
            "an unquantified categorical value must not be empty".into(),
        ));
    }
    if let Some(role) = role
        && !role.categorical_values.is_empty()
    {
        match &claim.claim {
            ClaimValue::Unquantified {
                value: Some(value),
                nominal: None,
            } if role.categorical_values.contains(value) => {}
            ClaimValue::Unquantified {
                value: Some(value),
                nominal: None,
            } => problems.push((
                "categorical",
                format!(
                    "categorical value `{value}` is outside role `{}@{}` vocabulary {:?}",
                    role.role.id, role.role.major, role.categorical_values
                ),
            )),
            ClaimValue::Unquantified { value: None, .. } => problems.push((
                "categorical",
                format!(
                    "categorical role `{}@{}` requires a categorical value",
                    role.role.id, role.role.major
                ),
            )),
            _ => {}
        }
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
                    Err(error) => {
                        problems.push(("unit_scaling", format!("`{field}`: {}", error.detail())))
                    }
                }
            }
            if matches!(claim.claim, ClaimValue::Unquantified { nominal: None, .. }) {
                problems.push((
                    "quantity_required",
                    "a quantity role requires a nominal value even when unquantified".into(),
                ));
            }
            if matches!(claim.claim, ClaimValue::Unquantified { value: Some(_), .. }) {
                problems.push((
                    "quantity_required",
                    "a quantity role cannot carry an unquantified categorical value".into(),
                ));
            }
            if let (Some(lower), Some(upper)) = (canonical.get("lower"), canonical.get("upper"))
                && lower
                    .checked_cmp(upper)
                    .is_ok_and(|order| order == Ordering::Greater)
            {
                problems.push(("claim_shape", "`lower` exceeds `upper`".into()));
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
                    problems.push((
                        "claim_shape",
                        "`nominal` lies outside the declared interval".into(),
                    ));
                }
            }
        }
        None => {
            if !quantities.is_empty() {
                problems.push((
                    "quantity_required",
                    format!(
                        "role `{}@{}` is not a quantity role and admits no quantity values",
                        output.role.id, output.role.major
                    ),
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
