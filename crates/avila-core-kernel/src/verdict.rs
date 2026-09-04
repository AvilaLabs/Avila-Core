use std::cmp::Ordering;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    CORE_E7301, CORE_R3301, CORE_S1102, CORE_T2203, ExactNumber, KernelError, KindRegistry,
    Quantity, read_authoritative_decimal,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    Pass,
    Fail,
    Inconclusive,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictComparison {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoricalComparison {
    Equals,
    InSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BasisKind {
    Bounded,
    Enclosure,
    Nominal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregation {
    All,
    Any,
    Max,
    Min,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceState {
    Admitted,
    Quarantined,
    Invalidated,
}

impl EvidenceState {
    const fn label(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Quarantined => "quarantined",
            Self::Invalidated => "invalidated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceModel {
    Exact,
    Interval,
    CoverageInterval,
    WorstCase,
    Unquantified,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementBasis {
    pub kind: BasisKind,
    #[serde(default)]
    pub coverage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementPolicy {
    #[serde(default)]
    pub permit_nominal_basis: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    HalfEven,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredQuantity {
    pub value: String,
    pub unit: String,
}

impl AuthoredQuantity {
    fn exact(&self) -> Result<Quantity, KernelError> {
        Ok(Quantity {
            value: read_authoritative_decimal(&self.value)?,
            unit: self.unit.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayRounding {
    pub quantum: AuthoredQuantity,
    pub mode: RoundingMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KernelRequirement {
    pub comparison: VerdictComparison,
    pub limit: Quantity,
    pub basis: RequirementBasis,
    #[serde(default)]
    pub tolerance: Option<Quantity>,
    #[serde(default)]
    pub policy: RequirementPolicy,
    #[serde(default)]
    pub aggregation: Option<Aggregation>,
    #[serde(default)]
    pub display_rounding: Option<DisplayRounding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceClaim {
    pub evidence_id: String,
    pub state: EvidenceState,
    pub model: EvidenceModel,
    #[serde(default)]
    pub lower: Option<Quantity>,
    #[serde(default)]
    pub upper: Option<Quantity>,
    #[serde(default)]
    pub nominal: Option<Quantity>,
    #[serde(default)]
    pub coverage: Option<String>,
    #[serde(default)]
    pub basis: Option<String>,
    #[serde(default)]
    pub aggregation_instance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictCase {
    pub kind: String,
    pub requirement: KernelRequirement,
    #[serde(default)]
    pub evidence: Vec<EvidenceClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoricalRequirement {
    pub comparison: CategoricalComparison,
    pub accepted_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoricalEvidenceClaim {
    pub evidence_id: String,
    pub state: EvidenceState,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoricalVerdictCase {
    pub requirement: CategoricalRequirement,
    #[serde(default)]
    pub evidence: Vec<CategoricalEvidenceClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum VerdictReason {
    CodeOwner {
        code: String,
        owner: String,
    },
    EvidenceState {
        evidence_id: String,
        state: String,
    },
    DuplicateClaims {
        code: String,
        evidence_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerdictOutput {
    pub status: VerdictStatus,
    pub rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<Aggregation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_canonical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_canonical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_canonical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nominal_canonical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance_canonical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis_visible: Option<BasisKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numbers_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<VerdictReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_upper_text: Option<String>,
}

impl VerdictOutput {
    fn not_evaluated(rule: impl Into<String>, reasons: Vec<VerdictReason>) -> Self {
        Self {
            status: VerdictStatus::NotEvaluated,
            rule: rule.into(),
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
            observed_category: None,
            accepted_categories: None,
            reasons,
            display_upper_text: None,
        }
    }
}

pub struct VerdictEvaluator<'a> {
    kinds: &'a KindRegistry,
}

impl<'a> VerdictEvaluator<'a> {
    #[must_use]
    pub const fn new(kinds: &'a KindRegistry) -> Self {
        Self { kinds }
    }

    pub fn evaluate(&self, case: &VerdictCase) -> Result<VerdictOutput, KernelError> {
        if case.evidence.is_empty() {
            return Ok(VerdictOutput::not_evaluated(
                "not_evaluated.missing",
                vec![VerdictReason::CodeOwner {
                    code: CORE_R3301.into(),
                    owner: "requester".into(),
                }],
            ));
        }

        if let Some(claim) = case
            .evidence
            .iter()
            .find(|claim| claim.state != EvidenceState::Admitted)
        {
            return Ok(VerdictOutput::not_evaluated(
                format!("not_evaluated.{}", claim.state.label()),
                vec![VerdictReason::EvidenceState {
                    evidence_id: claim.evidence_id.clone(),
                    state: claim.state.label().into(),
                }],
            ));
        }

        if case.evidence.len() > 1 && case.requirement.aggregation.is_none() {
            return Ok(VerdictOutput::not_evaluated(
                "not_evaluated.duplicate_claim",
                vec![VerdictReason::DuplicateClaims {
                    code: CORE_E7301.into(),
                    evidence_ids: case
                        .evidence
                        .iter()
                        .map(|claim| claim.evidence_id.clone())
                        .collect(),
                }],
            ));
        }

        if case.evidence.len() > 1
            && case.requirement.basis.kind == BasisKind::Bounded
            && case
                .evidence
                .iter()
                .any(|claim| claim.model == EvidenceModel::CoverageInterval)
        {
            return Ok(VerdictOutput::not_evaluated(
                "not_evaluated.coverage_aggregation_requires_capability",
                vec![VerdictReason::CodeOwner {
                    code: CORE_T2203.into(),
                    owner: "method_owner".into(),
                }],
            ));
        }

        let reduced = self.reduce(case)?;
        let limit = self.kinds.scale(&case.kind, &case.requirement.limit)?;
        let tolerance = case
            .requirement
            .tolerance
            .as_ref()
            .map(|quantity| self.kinds.scale(&case.kind, quantity))
            .transpose()?;
        if tolerance
            .as_ref()
            .is_some_and(|quantity| !quantity.value.is_zero() && !quantity.value.is_positive())
        {
            return Err(invalid_verdict("requirement tolerance cannot be negative"));
        }

        let (status, rule) = match case.requirement.basis.kind {
            BasisKind::Nominal => {
                if !case.requirement.policy.permit_nominal_basis {
                    return Err(invalid_verdict(
                        "nominal evaluation requires explicit policy permission",
                    ));
                }
                let nominal = reduced.nominal.as_ref().ok_or_else(|| {
                    invalid_verdict("nominal evaluation requires an admitted nominal value")
                })?;
                evaluate_nominal(
                    case.requirement.comparison,
                    &nominal.value,
                    &limit.value,
                    tolerance.as_ref().map(|quantity| &quantity.value),
                )?
            }
            BasisKind::Bounded | BasisKind::Enclosure => evaluate_bounds(
                case.requirement.basis.kind,
                case.requirement.comparison,
                reduced.lower.as_ref().map(|quantity| &quantity.value),
                reduced.upper.as_ref().map(|quantity| &quantity.value),
                &limit.value,
                tolerance.as_ref().map(|quantity| &quantity.value),
            )?,
        };

        let mut output = VerdictOutput {
            status,
            rule,
            aggregation: case.requirement.aggregation,
            canonical_unit: Some(limit.canonical_unit),
            limit_canonical: Some(limit.value.canonical_rational()),
            lower_canonical: (case.requirement.basis.kind != BasisKind::Nominal)
                .then(|| {
                    reduced
                        .lower
                        .as_ref()
                        .map(|quantity| quantity.value.canonical_rational())
                })
                .flatten(),
            upper_canonical: (case.requirement.basis.kind != BasisKind::Nominal)
                .then(|| {
                    reduced
                        .upper
                        .as_ref()
                        .map(|quantity| quantity.value.canonical_rational())
                })
                .flatten(),
            nominal_canonical: reduced
                .nominal
                .as_ref()
                .map(|quantity| quantity.value.canonical_rational()),
            tolerance_canonical: tolerance
                .as_ref()
                .map(|quantity| quantity.value.canonical_rational()),
            coverage: reduced.coverage,
            basis_visible: (case.requirement.basis.kind == BasisKind::Nominal)
                .then_some(BasisKind::Nominal),
            numbers_present: None,
            observed_category: None,
            accepted_categories: None,
            reasons: Vec::new(),
            display_upper_text: None,
        };

        if let (Some(rounding), Some(upper)) =
            (&case.requirement.display_rounding, reduced.upper.as_ref())
        {
            output.display_upper_text =
                Some(self.render_rounded_upper(&case.kind, &upper.value, rounding)?);
        }
        Ok(output)
    }

    fn reduce(&self, case: &VerdictCase) -> Result<ReducedClaim, KernelError> {
        match case.requirement.aggregation {
            None => self.reduce_one(
                &case.kind,
                &case.requirement,
                case.evidence
                    .first()
                    .ok_or_else(|| invalid_verdict("verdict evidence is missing"))?,
            ),
            Some(Aggregation::Max | Aggregation::Min) => self.reduce_numeric_aggregation(case),
            Some(Aggregation::All | Aggregation::Any) => Err(invalid_verdict(
                "all/any combine verdicts, not numeric evidence claims",
            )),
        }
    }

    fn reduce_one(
        &self,
        kind: &str,
        requirement: &KernelRequirement,
        claim: &EvidenceClaim,
    ) -> Result<ReducedClaim, KernelError> {
        let lower = claim
            .lower
            .as_ref()
            .map(|quantity| self.kinds.scale(kind, quantity))
            .transpose()?;
        let upper = claim
            .upper
            .as_ref()
            .map(|quantity| self.kinds.scale(kind, quantity))
            .transpose()?;
        let nominal = claim
            .nominal
            .as_ref()
            .map(|quantity| self.kinds.scale(kind, quantity))
            .transpose()?;

        validate_claim_shape(claim, &lower, &upper, &nominal)?;
        // ADR-0006: an `exact` claim is the degenerate interval
        // `lo = hi = nominal = value`, so it satisfies a bounded basis.
        let (lower, upper) = if claim.model == EvidenceModel::Exact {
            (nominal.clone(), nominal.clone())
        } else {
            (lower, upper)
        };
        if let (Some(lower), Some(upper)) = (&lower, &upper)
            && lower.value.checked_cmp(&upper.value)? == Ordering::Greater
        {
            return Err(invalid_verdict(
                "evidence lower bound exceeds its upper bound",
            ));
        }
        if let Some(nominal) = &nominal {
            let below_lower = if let Some(lower) = &lower {
                nominal.value.checked_cmp(&lower.value)? == Ordering::Less
            } else {
                false
            };
            let above_upper = if let Some(upper) = &upper {
                nominal.value.checked_cmp(&upper.value)? == Ordering::Greater
            } else {
                false
            };
            if below_lower || above_upper {
                return Err(invalid_verdict(
                    "evidence nominal value lies outside its declared interval",
                ));
            }
        }

        let coverage = coverage_for_output(requirement, claim)?;
        Ok(ReducedClaim {
            lower,
            upper,
            nominal,
            coverage,
        })
    }

    fn reduce_numeric_aggregation(&self, case: &VerdictCase) -> Result<ReducedClaim, KernelError> {
        let mut instance_ids = BTreeSet::new();
        let mut claims = Vec::with_capacity(case.evidence.len());
        for claim in &case.evidence {
            let instance = claim.aggregation_instance.as_ref().ok_or_else(|| {
                invalid_verdict("numeric aggregation requires an instance id on every claim")
            })?;
            if !instance_ids.insert(instance) {
                return Err(invalid_verdict("duplicate numeric aggregation instance"));
            }
            claims.push(self.reduce_one(&case.kind, &case.requirement, claim)?);
        }

        let aggregation = match case.requirement.aggregation {
            Some(aggregation @ (Aggregation::Max | Aggregation::Min)) => aggregation,
            _ => {
                return Err(invalid_verdict("numeric aggregation requires max or min"));
            }
        };
        let lower = match aggregation {
            Aggregation::Max => extremum(
                claims.iter().filter_map(|claim| claim.lower.as_ref()),
                Ordering::Greater,
            )?,
            Aggregation::Min if claims.iter().all(|claim| claim.lower.is_some()) => extremum(
                claims.iter().filter_map(|claim| claim.lower.as_ref()),
                Ordering::Less,
            )?,
            Aggregation::Min => None,
            Aggregation::All | Aggregation::Any => {
                return Err(invalid_verdict("invalid numeric aggregation mode"));
            }
        };
        let upper = match aggregation {
            Aggregation::Max if claims.iter().all(|claim| claim.upper.is_some()) => extremum(
                claims.iter().filter_map(|claim| claim.upper.as_ref()),
                Ordering::Greater,
            )?,
            Aggregation::Max => None,
            Aggregation::Min => extremum(
                claims.iter().filter_map(|claim| claim.upper.as_ref()),
                Ordering::Less,
            )?,
            Aggregation::All | Aggregation::Any => {
                return Err(invalid_verdict("invalid numeric aggregation mode"));
            }
        };
        Ok(ReducedClaim {
            lower,
            upper,
            nominal: None,
            coverage: None,
        })
    }

    fn render_rounded_upper(
        &self,
        kind: &str,
        canonical_upper: &ExactNumber,
        rounding: &DisplayRounding,
    ) -> Result<String, KernelError> {
        let quantum = rounding.quantum.exact()?;
        let quantum_canonical = self.kinds.scale(kind, &quantum)?;
        if !quantum_canonical.value.is_positive() {
            return Err(invalid_verdict("display quantum must be positive"));
        }
        let value_in_display_unit =
            self.kinds
                .value_in_unit(kind, canonical_upper, &rounding.quantum.unit)?;
        let ratio = value_in_display_unit.checked_div(&quantum.value)?;
        let multiple = match rounding.mode {
            RoundingMode::HalfEven => ratio.round_half_even_integer()?,
        };
        let rounded = quantum.value.checked_mul_integer(multiple)?;
        let scale = decimal_scale(&rounding.quantum.value)?;
        Ok(format!(
            "{} {}",
            rounded.to_fixed_decimal(scale)?,
            rounding.quantum.unit
        ))
    }
}

pub struct CategoricalVerdictEvaluator;

impl CategoricalVerdictEvaluator {
    pub fn evaluate(case: &CategoricalVerdictCase) -> Result<VerdictOutput, KernelError> {
        let accepted = &case.requirement.accepted_values;
        if accepted.is_empty() {
            return Err(invalid_verdict(
                "a categorical requirement must accept at least one value",
            ));
        }
        if case.requirement.comparison == CategoricalComparison::Equals && accepted.len() != 1 {
            return Err(invalid_verdict(
                "categorical equality requires exactly one accepted value",
            ));
        }
        if accepted.iter().any(|value| value.trim().is_empty()) {
            return Err(invalid_verdict(
                "categorical accepted values must not be empty",
            ));
        }
        let unique: BTreeSet<_> = accepted.iter().collect();
        if unique.len() != accepted.len() {
            return Err(invalid_verdict(
                "categorical accepted values must not repeat",
            ));
        }

        if case.evidence.is_empty() {
            let mut output = VerdictOutput::not_evaluated(
                "not_evaluated.missing",
                vec![VerdictReason::CodeOwner {
                    code: CORE_R3301.into(),
                    owner: "requester".into(),
                }],
            );
            output.accepted_categories = Some(accepted.clone());
            return Ok(output);
        }
        if let Some(claim) = case
            .evidence
            .iter()
            .find(|claim| claim.state != EvidenceState::Admitted)
        {
            let mut output = VerdictOutput::not_evaluated(
                format!("not_evaluated.{}", claim.state.label()),
                vec![VerdictReason::EvidenceState {
                    evidence_id: claim.evidence_id.clone(),
                    state: claim.state.label().into(),
                }],
            );
            output.accepted_categories = Some(accepted.clone());
            return Ok(output);
        }
        if case.evidence.len() > 1 {
            let mut output = VerdictOutput::not_evaluated(
                "not_evaluated.duplicate_claim",
                vec![VerdictReason::DuplicateClaims {
                    code: CORE_E7301.into(),
                    evidence_ids: case
                        .evidence
                        .iter()
                        .map(|claim| claim.evidence_id.clone())
                        .collect(),
                }],
            );
            output.accepted_categories = Some(accepted.clone());
            return Ok(output);
        }
        let claim = &case.evidence[0];
        let Some(value) = &claim.value else {
            let mut output = VerdictOutput::not_evaluated(
                "not_evaluated.category_missing",
                vec![VerdictReason::CodeOwner {
                    code: CORE_R3301.into(),
                    owner: "executor".into(),
                }],
            );
            output.accepted_categories = Some(accepted.clone());
            return Ok(output);
        };
        let matches = accepted.contains(value);
        let operator = match case.requirement.comparison {
            CategoricalComparison::Equals => "equals",
            CategoricalComparison::InSet => "in_set",
        };
        Ok(VerdictOutput {
            status: if matches {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            },
            rule: format!(
                "categorical.{operator}.{}",
                if matches { "match" } else { "mismatch" }
            ),
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
            observed_category: Some(value.clone()),
            accepted_categories: Some(accepted.clone()),
            reasons: Vec::new(),
            display_upper_text: None,
        })
    }
}

#[derive(Debug, Clone)]
struct ReducedClaim {
    lower: Option<crate::CanonicalQuantity>,
    upper: Option<crate::CanonicalQuantity>,
    nominal: Option<crate::CanonicalQuantity>,
    coverage: Option<String>,
}

pub fn aggregate_verdicts(
    aggregation: Aggregation,
    verdicts: &[VerdictStatus],
) -> Result<VerdictStatus, KernelError> {
    if verdicts.is_empty() {
        return Err(invalid_verdict(
            "verdict aggregation requires at least one member",
        ));
    }
    match aggregation {
        Aggregation::All => {
            for candidate in [
                VerdictStatus::Fail,
                VerdictStatus::NotEvaluated,
                VerdictStatus::Inconclusive,
                VerdictStatus::Pass,
            ] {
                if verdicts.contains(&candidate) {
                    return Ok(candidate);
                }
            }
            Err(invalid_verdict(
                "all aggregation did not encounter a recognized verdict state",
            ))
        }
        Aggregation::Any => {
            for candidate in [
                VerdictStatus::Pass,
                VerdictStatus::NotEvaluated,
                VerdictStatus::Inconclusive,
                VerdictStatus::Fail,
            ] {
                if verdicts.contains(&candidate) {
                    return Ok(candidate);
                }
            }
            Err(invalid_verdict(
                "any aggregation did not encounter a recognized verdict state",
            ))
        }
        Aggregation::Max | Aggregation::Min => Err(invalid_verdict(
            "max/min aggregate numeric evidence, not derived verdict states",
        )),
    }
}

fn validate_claim_shape(
    claim: &EvidenceClaim,
    lower: &Option<crate::CanonicalQuantity>,
    upper: &Option<crate::CanonicalQuantity>,
    nominal: &Option<crate::CanonicalQuantity>,
) -> Result<(), KernelError> {
    let valid = match claim.model {
        EvidenceModel::Exact => {
            nominal.is_some() && lower.is_none() && upper.is_none() && claim.coverage.is_none()
        }
        EvidenceModel::Interval => lower.is_some() && upper.is_some() && claim.coverage.is_none(),
        EvidenceModel::CoverageInterval => {
            lower.is_some() && upper.is_some() && nominal.is_some() && claim.coverage.is_some()
        }
        EvidenceModel::WorstCase => lower.is_some() ^ upper.is_some(),
        EvidenceModel::Unquantified => {
            nominal.is_some() && lower.is_none() && upper.is_none() && claim.coverage.is_none()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_verdict(format!(
            "evidence `{}` does not satisfy its declared uncertainty model",
            claim.evidence_id
        )))
    }
}

fn coverage_for_output(
    requirement: &KernelRequirement,
    claim: &EvidenceClaim,
) -> Result<Option<String>, KernelError> {
    let Some(actual_text) = &claim.coverage else {
        return Ok(None);
    };
    let actual = valid_coverage(actual_text)?;
    let Some(required_text) = &requirement.basis.coverage else {
        return Ok(None);
    };
    let required = valid_coverage(required_text)?;
    if actual.checked_cmp(&required)? == Ordering::Less {
        return Err(invalid_verdict(
            "admitted evidence coverage is below the requirement basis",
        ));
    }
    Ok((actual != required).then(|| actual_text.clone()))
}

fn valid_coverage(input: &str) -> Result<ExactNumber, KernelError> {
    let value = read_authoritative_decimal(input)?;
    let zero = ExactNumber::from_canonical("0")?;
    let one = ExactNumber::from_canonical("1")?;
    if value.checked_cmp(&zero)? != Ordering::Greater
        || value.checked_cmp(&one)? == Ordering::Greater
    {
        return Err(invalid_verdict("coverage must be in the interval (0, 1]"));
    }
    Ok(value)
}

fn evaluate_nominal(
    comparison: VerdictComparison,
    nominal: &ExactNumber,
    limit: &ExactNumber,
    tolerance: Option<&ExactNumber>,
) -> Result<(VerdictStatus, String), KernelError> {
    if comparison == VerdictComparison::Equal {
        let tolerance = tolerance
            .ok_or_else(|| invalid_verdict("equality requires an exact tolerance quantity"))?;
        let lower = limit.checked_sub(tolerance)?;
        let upper = limit.checked_add(tolerance)?;
        let inside = nominal.checked_cmp(&lower)? != Ordering::Less
            && nominal.checked_cmp(&upper)? != Ordering::Greater;
        return Ok(if inside {
            (VerdictStatus::Pass, "nominal.equal.within".into())
        } else {
            (VerdictStatus::Fail, "nominal.equal.outside".into())
        });
    }

    let ordering = nominal.checked_cmp(limit)?;
    let satisfied = comparison_satisfied(comparison, ordering);
    let (status, outcome) = if satisfied {
        (VerdictStatus::Pass, "within")
    } else {
        let outcome = match comparison {
            VerdictComparison::GreaterThan | VerdictComparison::GreaterThanOrEqual => "below",
            _ => "exceeds",
        };
        (VerdictStatus::Fail, outcome)
    };
    Ok((
        status,
        format!("nominal.{}.{}", comparison.rule_token(), outcome),
    ))
}

fn evaluate_bounds(
    basis: BasisKind,
    comparison: VerdictComparison,
    lower: Option<&ExactNumber>,
    upper: Option<&ExactNumber>,
    limit: &ExactNumber,
    tolerance: Option<&ExactNumber>,
) -> Result<(VerdictStatus, String), KernelError> {
    if comparison == VerdictComparison::Equal {
        return evaluate_equal_bounds(lower, upper, limit, tolerance);
    }
    if lower.is_none() && upper.is_none() {
        return Err(invalid_verdict("bounded evaluation has no admitted bound"));
    }

    let prefix = match basis {
        BasisKind::Bounded => "bounded",
        BasisKind::Enclosure => "enclosure",
        BasisKind::Nominal => {
            return Err(invalid_verdict(
                "nominal basis cannot enter bounded evaluation",
            ));
        }
    };
    let token = comparison.rule_token();
    let upper_ordering = upper.map(|value| value.checked_cmp(limit)).transpose()?;
    let lower_ordering = lower.map(|value| value.checked_cmp(limit)).transpose()?;
    let upper_satisfies = upper_ordering
        .map(|ordering| comparison_satisfied(comparison, ordering))
        .unwrap_or(false);
    let lower_satisfies = lower_ordering
        .map(|ordering| comparison_satisfied(comparison, ordering))
        .unwrap_or(false);

    let (status, outcome) = match comparison {
        VerdictComparison::LessThan | VerdictComparison::LessThanOrEqual => {
            if upper_satisfies {
                (VerdictStatus::Pass, "within")
            } else if lower.is_some() && !lower_satisfies {
                (VerdictStatus::Fail, "exceeds")
            } else if lower.is_some() && upper.is_some() {
                (VerdictStatus::Inconclusive, "crossing")
            } else if upper.is_some() {
                (VerdictStatus::Inconclusive, "upper_only")
            } else {
                (VerdictStatus::Inconclusive, "lower_only")
            }
        }
        VerdictComparison::GreaterThan | VerdictComparison::GreaterThanOrEqual => {
            if lower_satisfies {
                (VerdictStatus::Pass, "within")
            } else if upper.is_some() && !upper_satisfies {
                (VerdictStatus::Fail, "below")
            } else if lower.is_some() && upper.is_some() {
                (VerdictStatus::Inconclusive, "crossing")
            } else if lower.is_some() {
                (VerdictStatus::Inconclusive, "lower_only")
            } else {
                (VerdictStatus::Inconclusive, "upper_only")
            }
        }
        VerdictComparison::Equal => {
            return Err(invalid_verdict(
                "equality comparison did not enter equality evaluation",
            ));
        }
    };
    Ok((status, format!("{prefix}.{token}.{outcome}")))
}

fn evaluate_equal_bounds(
    lower: Option<&ExactNumber>,
    upper: Option<&ExactNumber>,
    limit: &ExactNumber,
    tolerance: Option<&ExactNumber>,
) -> Result<(VerdictStatus, String), KernelError> {
    let tolerance = tolerance
        .ok_or_else(|| invalid_verdict("equality requires an exact tolerance quantity"))?;
    let band_lower = limit.checked_sub(tolerance)?;
    let band_upper = limit.checked_add(tolerance)?;

    let contained = match (lower, upper) {
        (Some(lower), Some(upper)) => {
            lower.checked_cmp(&band_lower)? != Ordering::Less
                && upper.checked_cmp(&band_upper)? != Ordering::Greater
        }
        _ => false,
    };
    if contained {
        return Ok((VerdictStatus::Pass, "equal.within".into()));
    }
    let disjoint = if let Some(lower) = lower {
        lower.checked_cmp(&band_upper)? == Ordering::Greater
    } else {
        false
    } || if let Some(upper) = upper {
        upper.checked_cmp(&band_lower)? == Ordering::Less
    } else {
        false
    };
    if disjoint {
        Ok((VerdictStatus::Fail, "equal.outside".into()))
    } else {
        Ok((VerdictStatus::Inconclusive, "equal.partial".into()))
    }
}

impl VerdictComparison {
    const fn rule_token(self) -> &'static str {
        match self {
            Self::LessThan => "lt",
            Self::LessThanOrEqual => "le",
            Self::GreaterThan => "gt",
            Self::GreaterThanOrEqual => "ge",
            Self::Equal => "equal",
        }
    }
}

const fn comparison_satisfied(comparison: VerdictComparison, ordering: Ordering) -> bool {
    match comparison {
        VerdictComparison::LessThan => ordering.is_lt(),
        VerdictComparison::LessThanOrEqual => ordering.is_le(),
        VerdictComparison::GreaterThan => ordering.is_gt(),
        VerdictComparison::GreaterThanOrEqual => ordering.is_ge(),
        VerdictComparison::Equal => ordering.is_eq(),
    }
}

fn extremum<'a>(
    values: impl Iterator<Item = &'a crate::CanonicalQuantity>,
    desired: Ordering,
) -> Result<Option<crate::CanonicalQuantity>, KernelError> {
    let mut selected: Option<&crate::CanonicalQuantity> = None;
    for value in values {
        let replace = if let Some(current) = selected {
            value.value.checked_cmp(&current.value)? == desired
        } else {
            true
        };
        if replace {
            selected = Some(value);
        }
    }
    Ok(selected.cloned())
}

fn decimal_scale(input: &str) -> Result<u32, KernelError> {
    read_authoritative_decimal(input)?;
    u32::try_from(
        input
            .split_once('.')
            .map_or(0, |(_, fraction)| fraction.len()),
    )
    .map_err(|_| invalid_verdict("display quantum scale exceeds the work budget"))
}

fn invalid_verdict(detail: impl Into<String>) -> KernelError {
    KernelError::new(CORE_S1102, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_and_any_precedence_is_exhaustive_for_pairs() {
        let states = [
            VerdictStatus::Pass,
            VerdictStatus::Fail,
            VerdictStatus::Inconclusive,
            VerdictStatus::NotEvaluated,
        ];
        for left in states {
            for right in states {
                let values = [left, right];
                let all = aggregate_verdicts(Aggregation::All, &values).unwrap();
                let expected_all = if values.contains(&VerdictStatus::Fail) {
                    VerdictStatus::Fail
                } else if values.contains(&VerdictStatus::NotEvaluated) {
                    VerdictStatus::NotEvaluated
                } else if values.contains(&VerdictStatus::Inconclusive) {
                    VerdictStatus::Inconclusive
                } else {
                    VerdictStatus::Pass
                };
                assert_eq!(all, expected_all);

                let any = aggregate_verdicts(Aggregation::Any, &values).unwrap();
                let expected_any = if values.contains(&VerdictStatus::Pass) {
                    VerdictStatus::Pass
                } else if values.contains(&VerdictStatus::NotEvaluated) {
                    VerdictStatus::NotEvaluated
                } else if values.contains(&VerdictStatus::Inconclusive) {
                    VerdictStatus::Inconclusive
                } else {
                    VerdictStatus::Fail
                };
                assert_eq!(any, expected_any);
            }
        }
    }

    #[test]
    fn bounded_inequality_tables_cover_every_small_interval() {
        let limit = ExactNumber::from_canonical("0").unwrap();
        for lower_integer in -3..=3 {
            for upper_integer in lower_integer..=3 {
                let lower = ExactNumber::from_canonical(&lower_integer.to_string()).unwrap();
                let upper = ExactNumber::from_canonical(&upper_integer.to_string()).unwrap();

                let (le, _) = evaluate_bounds(
                    BasisKind::Bounded,
                    VerdictComparison::LessThanOrEqual,
                    Some(&lower),
                    Some(&upper),
                    &limit,
                    None,
                )
                .unwrap();
                let expected_le = if upper_integer <= 0 {
                    VerdictStatus::Pass
                } else if lower_integer > 0 {
                    VerdictStatus::Fail
                } else {
                    VerdictStatus::Inconclusive
                };
                assert_eq!(
                    le, expected_le,
                    "LE interval [{lower_integer}, {upper_integer}]"
                );

                let (ge, _) = evaluate_bounds(
                    BasisKind::Bounded,
                    VerdictComparison::GreaterThanOrEqual,
                    Some(&lower),
                    Some(&upper),
                    &limit,
                    None,
                )
                .unwrap();
                let expected_ge = if lower_integer >= 0 {
                    VerdictStatus::Pass
                } else if upper_integer < 0 {
                    VerdictStatus::Fail
                } else {
                    VerdictStatus::Inconclusive
                };
                assert_eq!(
                    ge, expected_ge,
                    "GE interval [{lower_integer}, {upper_integer}]"
                );
            }
        }
    }

    #[test]
    fn equality_table_covers_every_small_interval() {
        let limit = ExactNumber::from_canonical("0").unwrap();
        let tolerance = ExactNumber::from_canonical("1").unwrap();
        for lower_integer in -3..=3 {
            for upper_integer in lower_integer..=3 {
                let lower = ExactNumber::from_canonical(&lower_integer.to_string()).unwrap();
                let upper = ExactNumber::from_canonical(&upper_integer.to_string()).unwrap();
                let (status, _) =
                    evaluate_equal_bounds(Some(&lower), Some(&upper), &limit, Some(&tolerance))
                        .unwrap();
                let expected = if lower_integer >= -1 && upper_integer <= 1 {
                    VerdictStatus::Pass
                } else if lower_integer > 1 || upper_integer < -1 {
                    VerdictStatus::Fail
                } else {
                    VerdictStatus::Inconclusive
                };
                assert_eq!(
                    status, expected,
                    "equality interval [{lower_integer}, {upper_integer}]"
                );
            }
        }
    }

    #[test]
    fn categorical_equals_and_in_set_are_closed_and_four_state() {
        let evidence = |value: Option<&str>, state| CategoricalEvidenceClaim {
            evidence_id: "category".into(),
            state,
            value: value.map(str::to_owned),
        };
        let equals = |value: Option<&str>, state| CategoricalVerdictCase {
            requirement: CategoricalRequirement {
                comparison: CategoricalComparison::Equals,
                accepted_values: vec!["candidate_unreviewed".into()],
            },
            evidence: vec![evidence(value, state)],
        };

        let pass = CategoricalVerdictEvaluator::evaluate(&equals(
            Some("candidate_unreviewed"),
            EvidenceState::Admitted,
        ))
        .unwrap();
        assert_eq!(pass.status, VerdictStatus::Pass);
        assert_eq!(pass.rule, "categorical.equals.match");
        assert_eq!(
            pass.observed_category.as_deref(),
            Some("candidate_unreviewed")
        );

        let fail = CategoricalVerdictEvaluator::evaluate(&equals(
            Some("rejected"),
            EvidenceState::Admitted,
        ))
        .unwrap();
        assert_eq!(fail.status, VerdictStatus::Fail);
        assert_eq!(fail.rule, "categorical.equals.mismatch");

        let quarantined = CategoricalVerdictEvaluator::evaluate(&equals(
            Some("candidate_unreviewed"),
            EvidenceState::Quarantined,
        ))
        .unwrap();
        assert_eq!(quarantined.status, VerdictStatus::NotEvaluated);

        let in_set = CategoricalVerdictCase {
            requirement: CategoricalRequirement {
                comparison: CategoricalComparison::InSet,
                accepted_values: vec!["clear".into(), "candidate_unreviewed".into()],
            },
            evidence: vec![evidence(Some("clear"), EvidenceState::Admitted)],
        };
        assert_eq!(
            CategoricalVerdictEvaluator::evaluate(&in_set)
                .unwrap()
                .status,
            VerdictStatus::Pass
        );
    }
}
