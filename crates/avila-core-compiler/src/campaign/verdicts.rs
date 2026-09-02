//! One verdict per compiled requirement, derived by the kernel from the
//! admission states and claims for the requirement's metric source.

use avila_core_kernel::{
    BasisKind as KernelBasis, EvidenceClaim, EvidenceModel, EvidenceState, ExactNumber,
    KernelRequirement, Quantity, RequirementBasis as KernelRequirementBasis, RequirementPolicy,
    VerdictCase, VerdictComparison, VerdictEvaluator, VerdictOutput, VerdictReason, VerdictStatus,
};

use super::document::{ClaimValue, ClaimsDocument};
use super::{AdmissionRecord, AdmissionState, VerdictBoundary, VerdictRecord};
use crate::compile::registry::RegistryIndex;
use crate::compile::{CanonicalTypedQuantity, CompiledContract};
use crate::diagnostic::CORE_A4401;
use crate::document::{BasisKind, Comparison, QuantityValue, SourceRef};
use crate::qualification::{ClaimQualification, EnvelopeState};

pub(super) fn evaluate(
    compiled: &CompiledContract,
    registry: &RegistryIndex<'_>,
    claims: &ClaimsDocument,
    admissions: &[AdmissionRecord],
    boundary: &VerdictBoundary,
) -> Vec<VerdictRecord> {
    let evaluator = VerdictEvaluator::new(&registry.kinds);

    compiled
        .requirements
        .iter()
        .map(|requirement| {
            let kind = registry
                .roles
                .get(&requirement.metric_role)
                .and_then(|role| role.quantity_kind.clone())
                .unwrap_or_default();
            let mut evidence = Vec::new();
            for record in admissions
                .iter()
                .filter(|record| record.source == requirement.metric)
            {
                let state = match record.state {
                    AdmissionState::Admitted => EvidenceState::Admitted,
                    AdmissionState::Quarantined => EvidenceState::Quarantined,
                    AdmissionState::Missing => continue,
                };
                let claim = claims
                    .claims
                    .iter()
                    .find(|claim| claim.claim_id == record.evidence_id);
                let Some(claim) = claim else {
                    continue;
                };
                evidence.push(evidence_claim(&record.evidence_id, state, &claim.claim));
            }
            let evidence_ids = evidence
                .iter()
                .map(|claim| claim.evidence_id.clone())
                .collect();
            let case = VerdictCase {
                kind,
                requirement: KernelRequirement {
                    comparison: comparison(requirement.comparison),
                    limit: quantity(&requirement.limit),
                    basis: KernelRequirementBasis {
                        kind: match requirement.basis.kind {
                            BasisKind::Bounded => KernelBasis::Bounded,
                            BasisKind::Enclosure => KernelBasis::Enclosure,
                            BasisKind::Nominal => KernelBasis::Nominal,
                        },
                        coverage: requirement.basis.coverage.clone(),
                    },
                    tolerance: requirement.tolerance.as_ref().map(quantity),
                    policy: RequirementPolicy {
                        permit_nominal_basis: compiled.execution_policy.permit_nominal_basis,
                    },
                    aggregation: None,
                    display_rounding: None,
                },
                evidence,
            };
            // A claim from outside its producer's qualification envelope, or
            // of unknown position, cannot establish a bounded requirement.
            let quarantined = quarantined_by_qualification(requirement, claims, admissions);
            let verdict = if !quarantined.is_empty() && requirement.basis.kind != BasisKind::Nominal
            {
                qualification_verdict(&quarantined)
            } else {
                let mut output = evaluator
                    .evaluate(&case)
                    .unwrap_or_else(|error| VerdictOutput {
                        status: VerdictStatus::NotEvaluated,
                        rule: "not_evaluated.kernel_refusal".into(),
                        aggregation: None,
                        canonical_unit: None,
                        limit_canonical: None,
                        lower_canonical: None,
                        upper_canonical: None,
                        nominal_canonical: None,
                        tolerance_canonical: None,
                        coverage: None,
                        basis_visible: None,
                        numbers_present: Some(false),
                        reasons: vec![VerdictReason::CodeOwner {
                            code: error.code().into(),
                            owner: "executor".into(),
                        }],
                        display_upper_text: None,
                    });
                if !quarantined.is_empty() && requirement.basis.kind != BasisKind::Nominal {
                    output.reasons.extend(qualification_reasons(&quarantined));
                }
                output
            };
            VerdictRecord {
                requirement_id: requirement.requirement_id.clone(),
                statement: requirement.statement.clone(),
                metric: requirement.metric.clone(),
                evidence_ids,
                verdict,
                boundary: boundary.clone(),
            }
        })
        .collect()
}

/// Admitted claims for the requirement's metric whose producer's envelope
/// did not contain this run.
fn quarantined_by_qualification(
    requirement: &crate::compile::CompiledRequirement,
    claims: &ClaimsDocument,
    admissions: &[AdmissionRecord],
) -> Vec<(String, ClaimQualification)> {
    admissions
        .iter()
        .filter(|record| record.source == requirement.metric)
        .filter(|record| record.state == AdmissionState::Admitted)
        .filter_map(|record| {
            claims
                .claims
                .iter()
                .find(|claim| claim.claim_id == record.evidence_id)
                .and_then(|claim| claim.qualification.clone())
                .filter(|qualification| qualification.state != EnvelopeState::Inside)
                .map(|qualification| (record.evidence_id.clone(), qualification))
        })
        .collect()
}

fn qualification_reasons(quarantined: &[(String, ClaimQualification)]) -> Vec<VerdictReason> {
    let mut reasons = vec![VerdictReason::CodeOwner {
        code: CORE_A4401.into(),
        owner: "method_owner".into(),
    }];
    for (evidence_id, qualification) in quarantined {
        let state = match qualification.state {
            EnvelopeState::Outside => "outside_qualification",
            EnvelopeState::Unknown => "qualification_unknown",
            EnvelopeState::Inside => "inside_qualification",
        };
        let failed = qualification.failed_terms();
        reasons.push(VerdictReason::EvidenceState {
            evidence_id: evidence_id.clone(),
            state: if failed.is_empty() {
                format!(
                    "{state} ({} rev {})",
                    qualification.qualification_id, qualification.revision
                )
            } else {
                format!(
                    "{state} ({} rev {}): {}",
                    qualification.qualification_id,
                    qualification.revision,
                    failed.join("; ")
                )
            },
        });
    }
    reasons
}

fn qualification_verdict(quarantined: &[(String, ClaimQualification)]) -> VerdictOutput {
    let outside = quarantined
        .iter()
        .any(|(_, qualification)| qualification.state == EnvelopeState::Outside);
    VerdictOutput {
        status: VerdictStatus::NotEvaluated,
        rule: if outside {
            "not_evaluated.outside_qualification".into()
        } else {
            "not_evaluated.qualification_unknown".into()
        },
        aggregation: None,
        canonical_unit: None,
        limit_canonical: None,
        lower_canonical: None,
        upper_canonical: None,
        nominal_canonical: None,
        tolerance_canonical: None,
        coverage: None,
        basis_visible: None,
        numbers_present: Some(false),
        reasons: qualification_reasons(quarantined),
        display_upper_text: None,
    }
}

fn evidence_claim(evidence_id: &str, state: EvidenceState, claim: &ClaimValue) -> EvidenceClaim {
    let (model, lower, upper, nominal, coverage) = match claim {
        ClaimValue::Exact { nominal } => (EvidenceModel::Exact, None, None, Some(nominal), None),
        ClaimValue::Interval {
            lower,
            upper,
            nominal,
        } => (
            EvidenceModel::Interval,
            Some(lower),
            Some(upper),
            nominal.as_ref(),
            None,
        ),
        ClaimValue::CoverageInterval {
            lower,
            upper,
            nominal,
            coverage,
        } => (
            EvidenceModel::CoverageInterval,
            Some(lower),
            Some(upper),
            Some(nominal),
            Some(coverage.clone()),
        ),
        ClaimValue::WorstCase {
            lower,
            upper,
            nominal,
        } => (
            EvidenceModel::WorstCase,
            lower.as_ref(),
            upper.as_ref(),
            nominal.as_ref(),
            None,
        ),
        ClaimValue::Unquantified { nominal } => (
            EvidenceModel::Unquantified,
            None,
            None,
            nominal.as_ref(),
            None,
        ),
    };
    EvidenceClaim {
        evidence_id: evidence_id.into(),
        state,
        model,
        lower: lower.map(authored),
        upper: upper.map(authored),
        nominal: nominal.map(authored),
        coverage,
        basis: None,
        aggregation_instance: None,
    }
}

fn authored(value: &QuantityValue) -> Quantity {
    Quantity {
        value: value.value.clone(),
        unit: value.unit.clone(),
    }
}

fn quantity(value: &CanonicalTypedQuantity) -> Quantity {
    Quantity {
        value: ExactNumber::from_canonical(&value.value)
            .expect("compiled quantities are canonical"),
        unit: value.unit.clone(),
    }
}

const fn comparison(value: Comparison) -> VerdictComparison {
    match value {
        Comparison::LessThan => VerdictComparison::LessThan,
        Comparison::LessThanOrEqual => VerdictComparison::LessThanOrEqual,
        Comparison::GreaterThan => VerdictComparison::GreaterThan,
        Comparison::GreaterThanOrEqual => VerdictComparison::GreaterThanOrEqual,
        Comparison::Equal => VerdictComparison::Equal,
    }
}

impl SourceRef {
    /// Whether this source names an output claim rather than an input.
    #[allow(dead_code)]
    pub(super) const fn is_step_output(&self) -> bool {
        matches!(self, Self::StepOutput { .. })
    }
}
