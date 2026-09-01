//! One verdict per compiled requirement, derived by the kernel from the
//! admission states and claims for the requirement's metric source.

use avila_core_kernel::{
    BasisKind as KernelBasis, EvidenceClaim, EvidenceModel, EvidenceState, ExactNumber,
    KernelRequirement, Quantity, RequirementBasis as KernelRequirementBasis, RequirementPolicy,
    ReviewRequirements, VerdictCase, VerdictComparison, VerdictEvaluator, VerdictOutput,
    VerdictReason, VerdictStatus,
};

use super::admission::Decisions;
use super::document::{ClaimValue, ClaimsDocument};
use super::{AdmissionRecord, AdmissionState, VerdictBoundary, VerdictRecord};
use crate::compile::registry::RegistryIndex;
use crate::compile::{CanonicalTypedQuantity, CompiledContract};
use crate::document::{BasisKind, Comparison, QuantityValue, ReviewDisposition, SourceRef};

pub(super) fn evaluate(
    compiled: &CompiledContract,
    registry: &RegistryIndex<'_>,
    claims: &ClaimsDocument,
    admissions: &[AdmissionRecord],
    decisions: &Decisions,
    boundary: &VerdictBoundary,
) -> Vec<VerdictRecord> {
    let evaluator = VerdictEvaluator::new(&registry.kinds);
    let required_reviews: Vec<String> = compiled
        .workflow
        .iter()
        .filter(|step| step.review_obligation.is_some())
        .map(|step| step.step_id.clone())
        .collect();
    let present_reviews: Vec<String> = required_reviews
        .iter()
        .filter(|step_id| decisions.get(*step_id) == Some(&ReviewDisposition::ApproveForUse))
        .cloned()
        .collect();

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
                reviews: ReviewRequirements {
                    required: required_reviews.clone(),
                    present: present_reviews.clone(),
                },
            };
            let verdict = evaluator
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
                    reviews_outstanding: Vec::new(),
                    display_upper_text: None,
                });
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
