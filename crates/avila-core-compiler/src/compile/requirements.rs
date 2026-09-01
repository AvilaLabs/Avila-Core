//! Requirement metric binding, claim-model sufficiency, and exact lowering.

use super::findings::{contract_location, invalid_value};
use super::ir::{CanonicalTypedQuantity, CompiledRequirement};
use super::registry::RegistryIndex;
use super::resolve::{
    Candidate, find_exact_candidate, missing_source_finding, produced_by_unknown_type,
};
use super::values::compiler_repair;
use crate::diagnostic::{
    CORE_R3301, CORE_S1102, CORE_T2001, CORE_T2102, CORE_T2103, CORE_T2104, CORE_T2201, CORE_T2203,
    CORE_T2601, CoreDiagnostic, DiagnosticRepair, FindingClass, RepairApplicability,
};
use crate::document::{
    BasisKind, BoundSide, ClaimModelDeclaration, Comparison, ContractSource, RequirementBasis,
    RequirementSource, SourceRef, TypedQuantity,
};
use avila_core_kernel::{ExactNumber, read_authoritative_decimal};
use std::cmp::Ordering;
use std::collections::BTreeSet;

pub(super) fn compile_requirements(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    candidates: &[Candidate],
    invalid_sources: &BTreeSet<SourceRef>,
    unknown_type_steps: &BTreeSet<&str>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Vec<CompiledRequirement> {
    let mut compiled = Vec::new();
    for (index, requirement) in contract.requirements.iter().enumerate() {
        let coverage_valid = validate_basis_coverage(index, requirement, findings);
        let tolerance_present_correctly = validate_tolerance_presence(index, requirement, findings);
        let Some(metric) = &requirement.metric else {
            findings.push(CoreDiagnostic::new(
                CORE_R3301,
                FindingClass::Missing,
                "requester",
                contract_location(format!("/requirements/{index}/metric")),
                format!(
                    "requirement `{}` does not name a metric source",
                    requirement.requirement_id
                ),
            ));
            continue;
        };
        let Some(candidate) = find_exact_candidate(candidates, metric) else {
            if produced_by_unknown_type(metric, unknown_type_steps) {
                continue;
            }
            let (_, message) = missing_source_finding(contract, registry, metric);
            findings.push(CoreDiagnostic::new(
                CORE_R3301,
                FindingClass::Missing,
                "requester",
                contract_location(format!("/requirements/{index}/metric")),
                format!(
                    "metric source `{}` cannot be resolved: {message}",
                    metric.label()
                ),
            ));
            continue;
        };
        if invalid_sources.contains(&candidate.source) {
            continue;
        }
        if registry.purposes.contains_key(&requirement.purpose)
            && candidate.excluded_purposes.contains(&requirement.purpose)
        {
            findings.push(CoreDiagnostic::new(
                CORE_T2601,
                FindingClass::Unsatisfied,
                "requester",
                contract_location(format!("/requirements/{index}/purpose")),
                format!(
                    "metric source `{}` explicitly excludes governed purpose `{}@{}`",
                    candidate.source.label(),
                    requirement.purpose.id,
                    requirement.purpose.major
                ),
            ));
        }
        if !candidate
            .claim_models
            .iter()
            .any(|model| claim_model_satisfies(model, &requirement.basis, requirement.comparison))
        {
            let irreducible = !candidate.claim_models.is_empty()
                && candidate
                    .claim_models
                    .iter()
                    .all(ClaimModelDeclaration::is_kernel_irreducible);
            let mut diagnostic = CoreDiagnostic::new(
                if irreducible { CORE_T2203 } else { CORE_T2201 },
                FindingClass::Unsatisfied,
                "requester",
                contract_location(format!("/requirements/{index}/basis")),
                if irreducible {
                    "metric source permits only claim models that the semantic kernel cannot reduce"
                } else {
                    "metric source cannot emit a claim model sufficient for this comparison basis"
                },
            );
            if irreducible {
                diagnostic = diagnostic.with_repair(DiagnosticRepair::labels(
                    RepairApplicability::MethodOwnerJudgment,
                    vec!["core.uncertainty.expand@1".into()],
                ));
            }
            findings.push(diagnostic);
        }
        let Some(role) = registry.roles.get(&candidate.role) else {
            continue;
        };
        let Some(metric_kind) = role.quantity_kind.as_deref() else {
            findings.push(CoreDiagnostic::new(
                CORE_T2102,
                FindingClass::Invalid,
                "requester",
                contract_location(format!("/requirements/{index}/metric")),
                format!(
                    "metric role `{}@{}` is not a quantity role",
                    role.role.id, role.role.major
                ),
            ));
            continue;
        };
        if metric_kind != requirement.limit.kind {
            findings.push(CoreDiagnostic::new(
                CORE_T2102,
                FindingClass::Invalid,
                "requester",
                contract_location(format!("/requirements/{index}/limit/kind")),
                format!(
                    "limit kind `{}` does not match metric kind `{metric_kind}`",
                    requirement.limit.kind
                ),
            ));
            continue;
        }
        let limit = lower_requirement_quantity(
            index,
            "limit",
            &requirement.limit,
            metric_kind,
            registry,
            findings,
        );
        let tolerance = match (&requirement.tolerance, requirement.comparison) {
            (Some(quantity), Comparison::Equal) => lower_requirement_quantity(
                index,
                "tolerance",
                quantity,
                metric_kind,
                registry,
                findings,
            )
            .filter(|canonical| {
                tolerance_is_nonnegative(index, canonical, &quantity.value, findings)
            }),
            _ => None,
        };
        let (Some(limit), true, true) = (limit, coverage_valid, tolerance_present_correctly) else {
            continue;
        };
        if requirement.comparison == Comparison::Equal && tolerance.is_none() {
            continue;
        }
        compiled.push(CompiledRequirement {
            requirement_id: requirement.requirement_id.clone(),
            statement: requirement.statement.clone(),
            purpose: requirement.purpose.clone(),
            metric: metric.clone(),
            metric_role: candidate.role.clone(),
            comparison: requirement.comparison,
            limit,
            tolerance,
            basis: requirement.basis.clone(),
        });
    }
    compiled
}

/// Checks that a `coverage` basis is a canonical decimal in `(0, 1]` and is
/// declared only on a `bounded` basis. The kernel repeats this check at verdict
/// time; catching it here keeps an unevaluable requirement from compiling.
pub(super) fn validate_basis_coverage(
    index: usize,
    requirement: &RequirementSource,
    findings: &mut Vec<CoreDiagnostic>,
) -> bool {
    let Some(coverage) = requirement.basis.coverage.as_deref() else {
        return true;
    };
    let location = contract_location(format!("/requirements/{index}/basis/coverage"));
    if requirement.basis.kind != BasisKind::Bounded {
        invalid_value(
            location,
            format!(
                "coverage is only meaningful for a `bounded` basis, not `{}`",
                basis_label(requirement.basis.kind)
            ),
            "requester",
            findings,
        );
        return false;
    }
    match read_authoritative_decimal(coverage) {
        Ok(value) => {
            let one = ExactNumber::from_canonical("1").expect("`1` is canonical");
            let in_range = value.is_positive()
                && value
                    .checked_cmp(&one)
                    .is_ok_and(|ordering| ordering != Ordering::Greater);
            if in_range {
                true
            } else {
                invalid_value(
                    location,
                    "coverage must lie in the interval (0, 1]",
                    "requester",
                    findings,
                );
                false
            }
        }
        Err(error) => {
            let mut diagnostic = CoreDiagnostic::new(
                CORE_S1102,
                FindingClass::Invalid,
                "requester",
                location,
                format!(
                    "coverage must be a canonical decimal in the interval (0, 1]: {}",
                    error.detail()
                ),
            );
            if let Some(repair) = error.repair() {
                let path = diagnostic.primary.pointer.clone();
                diagnostic = diagnostic.with_repair(compiler_repair(repair, &path));
            }
            findings.push(diagnostic);
            false
        }
    }
}

/// An equality comparison needs a tolerance to be evaluable at all; any other
/// comparison must not carry one, because it would silently mean nothing.
pub(super) fn validate_tolerance_presence(
    index: usize,
    requirement: &RequirementSource,
    findings: &mut Vec<CoreDiagnostic>,
) -> bool {
    let location = contract_location(format!("/requirements/{index}/tolerance"));
    match (requirement.comparison, &requirement.tolerance) {
        (Comparison::Equal, None) => {
            findings.push(CoreDiagnostic::new(
                CORE_T2104,
                FindingClass::Missing,
                "requester",
                location,
                "an `equal` comparison requires an exact tolerance quantity of the metric kind",
            ));
            false
        }
        (Comparison::Equal, Some(_)) | (_, None) => true,
        (comparison, Some(_)) => {
            let path = location.pointer.clone();
            findings.push(
                CoreDiagnostic::new(
                    CORE_T2104,
                    FindingClass::Invalid,
                    "requester",
                    location,
                    format!(
                        "a tolerance is only meaningful for an `equal` comparison, not `{}`",
                        comparison_label(comparison)
                    ),
                )
                .with_repair(DiagnosticRepair::removal(
                    RepairApplicability::MechanicallySafe,
                    "remove the tolerance",
                    &path,
                )),
            );
            false
        }
    }
}

/// Lowers a requirement-level quantity (`limit` or `tolerance`) into the
/// metric kind's canonical unit, reporting kind and unit mismatches at the
/// exact field.
pub(super) fn lower_requirement_quantity(
    index: usize,
    field: &str,
    quantity: &TypedQuantity,
    metric_kind: &str,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<CanonicalTypedQuantity> {
    if quantity.kind != metric_kind {
        findings.push(CoreDiagnostic::new(
            CORE_T2102,
            FindingClass::Invalid,
            "requester",
            contract_location(format!("/requirements/{index}/{field}/kind")),
            format!(
                "{field} kind `{}` does not match metric kind `{metric_kind}`",
                quantity.kind
            ),
        ));
        return None;
    }
    if !registry.kind_classes.contains_key(metric_kind) {
        return None;
    }
    match registry
        .kinds
        .scale_quantity(metric_kind, &quantity.value, &quantity.unit)
    {
        Ok(canonical) => Some(CanonicalTypedQuantity {
            kind: metric_kind.into(),
            value: canonical.value.canonical_rational(),
            unit: canonical.canonical_unit,
        }),
        Err(error) => {
            let code = match error.code() {
                avila_core_kernel::CORE_T2001 => CORE_T2001,
                avila_core_kernel::CORE_T2102 => CORE_T2103,
                other => other,
            };
            let unit_path = format!("/requirements/{index}/{field}/unit");
            let repair = error
                .repair()
                .map(|repair| compiler_repair(repair, &unit_path));
            let mut diagnostic = CoreDiagnostic::new(
                code,
                FindingClass::Invalid,
                "requester",
                contract_location(unit_path),
                error.detail(),
            );
            if let Some(repair) = repair {
                diagnostic = diagnostic.with_repair(repair);
            }
            findings.push(diagnostic);
            None
        }
    }
}

pub(super) fn tolerance_is_nonnegative(
    index: usize,
    canonical: &CanonicalTypedQuantity,
    authored: &ExactNumber,
    findings: &mut Vec<CoreDiagnostic>,
) -> bool {
    // Unit factors are positive, so the sign of the authored value decides.
    if authored.is_zero() || authored.is_positive() {
        return true;
    }
    invalid_value(
        contract_location(format!("/requirements/{index}/tolerance/value")),
        format!(
            "tolerance cannot be negative; canonical value is `{}` {}",
            canonical.value, canonical.unit
        ),
        "requester",
        findings,
    );
    false
}

pub(super) const fn basis_label(kind: BasisKind) -> &'static str {
    match kind {
        BasisKind::Bounded => "bounded",
        BasisKind::Enclosure => "enclosure",
        BasisKind::Nominal => "nominal",
    }
}

pub(super) const fn comparison_label(comparison: Comparison) -> &'static str {
    match comparison {
        Comparison::LessThan => "less_than",
        Comparison::LessThanOrEqual => "less_than_or_equal",
        Comparison::GreaterThan => "greater_than",
        Comparison::GreaterThanOrEqual => "greater_than_or_equal",
        Comparison::Equal => "equal",
    }
}

pub(super) fn claim_model_satisfies(
    model: &ClaimModelDeclaration,
    basis: &RequirementBasis,
    comparison: Comparison,
) -> bool {
    match basis.kind {
        BasisKind::Bounded => match model {
            ClaimModelDeclaration::Exact
            | ClaimModelDeclaration::Interval { .. }
            | ClaimModelDeclaration::CoverageInterval => true,
            ClaimModelDeclaration::WorstCase { side, .. } => match comparison {
                Comparison::LessThan | Comparison::LessThanOrEqual => *side == BoundSide::Upper,
                Comparison::GreaterThan | Comparison::GreaterThanOrEqual => {
                    *side == BoundSide::Lower
                }
                Comparison::Equal => false,
            },
            ClaimModelDeclaration::StandardUncertainty
            | ClaimModelDeclaration::Samples
            | ClaimModelDeclaration::Distribution
            | ClaimModelDeclaration::Unquantified => false,
        },
        BasisKind::Enclosure => matches!(
            model,
            ClaimModelDeclaration::Exact | ClaimModelDeclaration::Interval { .. }
        ),
        BasisKind::Nominal => match model {
            ClaimModelDeclaration::Exact
            | ClaimModelDeclaration::CoverageInterval
            | ClaimModelDeclaration::Unquantified => true,
            ClaimModelDeclaration::Interval { nominal }
            | ClaimModelDeclaration::WorstCase { nominal, .. } => *nominal,
            ClaimModelDeclaration::StandardUncertainty
            | ClaimModelDeclaration::Samples
            | ClaimModelDeclaration::Distribution => false,
        },
    }
}
