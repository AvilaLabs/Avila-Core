//! Typed parameter and execution-factor lowering with domain checks.

use super::findings::parameter_location;
use super::ir::CompiledParameterValue;
use super::registry::RegistryIndex;
use crate::diagnostic::{
    CORE_S1101, CORE_S1301, CORE_T2401, CORE_T2402, CoreDiagnostic, DiagnosticRepair, FindingClass,
    RepairApplicability, SourceLocation,
};
use crate::document::{
    ContractSource, ContractStatus, ExactBound, IntegerBound, ParameterDefinition, ParameterType,
    QuantityBound, QuantityValue,
};
use avila_core_kernel::{ExactNumber, KindRegistry};
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub(super) const NOT_DEFINED_PLACEHOLDER: &str = "not_defined";

pub(super) fn compile_parameters(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> BTreeMap<String, BTreeMap<String, CompiledParameterValue>> {
    let mut compiled_steps = BTreeMap::new();
    for (step_index, step) in contract.workflow.iter().enumerate() {
        let Some(capability) = registry.capability_types.get(&step.capability_type) else {
            continue;
        };
        let definitions: BTreeMap<_, _> = capability
            .parameters
            .iter()
            .map(|definition| (definition.parameter_id.as_str(), definition))
            .collect();
        let mut compiled = BTreeMap::new();

        for parameter_id in step.parameters.keys() {
            if !definitions.contains_key(parameter_id.as_str()) {
                findings.push(CoreDiagnostic::new(
                    CORE_S1101,
                    FindingClass::Invalid,
                    "requester",
                    parameter_location(step_index, parameter_id),
                    format!(
                        "capability type `{}@{}` does not declare parameter `{parameter_id}`",
                        step.capability_type.id, step.capability_type.major
                    ),
                ));
            }
        }

        for definition in &capability.parameters {
            let Some(value) = step.parameters.get(&definition.parameter_id) else {
                if definition.required {
                    add_missing_parameter_finding(
                        contract.status,
                        step_index,
                        &definition.parameter_id,
                        findings,
                    );
                }
                continue;
            };
            if value.as_str() == Some(NOT_DEFINED_PLACEHOLDER) {
                add_placeholder_parameter_finding(
                    contract.status,
                    step_index,
                    &definition.parameter_id,
                    findings,
                );
                continue;
            }
            if let Some(value) =
                lower_parameter_value(definition, value, step_index, &registry.kinds, findings)
            {
                compiled.insert(definition.parameter_id.clone(), value);
            }
        }
        compiled_steps.insert(step.step_id.clone(), compiled);
    }
    compiled_steps
}

pub(super) fn add_missing_parameter_finding(
    status: ContractStatus,
    step_index: usize,
    parameter_id: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_S1301,
        placeholder_finding_class(status),
        "requester",
        parameter_location(step_index, parameter_id),
        format!("required parameter `{parameter_id}` is not defined"),
    ));
}

pub(super) fn add_placeholder_parameter_finding(
    status: ContractStatus,
    step_index: usize,
    parameter_id: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_S1301,
        placeholder_finding_class(status),
        "requester",
        parameter_location(step_index, parameter_id),
        if status == ContractStatus::Draft {
            format!("parameter `{parameter_id}` remains explicitly not defined in this draft")
        } else {
            format!(
                "parameter `{parameter_id}` cannot remain `not_defined` when contract status is `{}`",
                contract_status_label(status)
            )
        },
    ));
}

pub(super) const fn placeholder_finding_class(status: ContractStatus) -> FindingClass {
    if matches!(status, ContractStatus::Draft) {
        FindingClass::Missing
    } else {
        FindingClass::Invalid
    }
}

pub(super) const fn contract_status_label(status: ContractStatus) -> &'static str {
    match status {
        ContractStatus::Draft => "draft",
        ContractStatus::InReview => "in_review",
        ContractStatus::Approved => "approved",
        ContractStatus::Retired => "retired",
    }
}

pub(super) fn lower_parameter_value(
    definition: &ParameterDefinition,
    authored: &serde_json::Value,
    step_index: usize,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<CompiledParameterValue> {
    let location = parameter_location(step_index, &definition.parameter_id);
    lower_typed_value("parameter", definition, authored, location, kinds, findings)
}

pub(super) fn lower_typed_value(
    value_kind: &str,
    definition: &ParameterDefinition,
    authored: &serde_json::Value,
    location: SourceLocation,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<CompiledParameterValue> {
    match &definition.value_type {
        ParameterType::Boolean => authored.as_bool().map_or_else(
            || {
                parameter_type_finding(
                    value_kind,
                    &definition.parameter_id,
                    "boolean",
                    location,
                    None,
                    findings,
                );
                None
            },
            |value| Some(CompiledParameterValue::Boolean { value }),
        ),
        ParameterType::Integer { min, max } => {
            let Some(value) = authored.as_i64() else {
                parameter_type_finding(
                    value_kind,
                    &definition.parameter_id,
                    "integer",
                    location,
                    None,
                    findings,
                );
                return None;
            };
            if integer_outside_domain(value, min, max) {
                parameter_domain_finding(
                    value_kind,
                    &definition.parameter_id,
                    location,
                    "integer value is outside its declared domain",
                    None,
                    findings,
                );
                return None;
            }
            Some(CompiledParameterValue::Integer { value })
        }
        ParameterType::ExactNumber { min, max } => {
            let Some(text) = authored.as_str() else {
                parameter_type_finding(
                    value_kind,
                    &definition.parameter_id,
                    "exact-number string",
                    location,
                    None,
                    findings,
                );
                return None;
            };
            let value = match ExactNumber::from_canonical(text) {
                Ok(value) => value,
                Err(error) => {
                    parameter_type_finding(
                        value_kind,
                        &definition.parameter_id,
                        "exact-number string",
                        location,
                        Some(error.detail()),
                        findings,
                    );
                    return None;
                }
            };
            match exact_outside_domain(&value, min, max) {
                Ok(true) => {
                    parameter_domain_finding(
                        value_kind,
                        &definition.parameter_id,
                        location,
                        "exact number is outside its declared domain",
                        None,
                        findings,
                    );
                    None
                }
                Ok(false) => Some(CompiledParameterValue::ExactNumber {
                    value: value.canonical_rational(),
                }),
                Err(error) => {
                    parameter_type_finding(
                        value_kind,
                        &definition.parameter_id,
                        "comparable exact-number value",
                        location,
                        Some(error.detail()),
                        findings,
                    );
                    None
                }
            }
        }
        ParameterType::Text { allowed_values } => {
            let Some(value) = authored.as_str() else {
                parameter_type_finding(
                    value_kind,
                    &definition.parameter_id,
                    "text",
                    location,
                    None,
                    findings,
                );
                return None;
            };
            if let Some(allowed) = allowed_values
                && !allowed.iter().any(|candidate| candidate == value)
            {
                parameter_domain_finding(
                    value_kind,
                    &definition.parameter_id,
                    location,
                    "text value is outside its declared choice set",
                    Some(allowed.clone()),
                    findings,
                );
                return None;
            }
            Some(CompiledParameterValue::Text {
                value: value.into(),
            })
        }
        ParameterType::Quantity { kind, min, max } => lower_quantity_parameter(
            value_kind, definition, authored, kind, min, max, location, kinds, findings,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_quantity_parameter(
    value_kind: &str,
    definition: &ParameterDefinition,
    authored: &serde_json::Value,
    kind: &str,
    min: &Option<QuantityBound>,
    max: &Option<QuantityBound>,
    location: SourceLocation,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<CompiledParameterValue> {
    let quantity = match serde_json::from_value::<QuantityValue>(authored.clone()) {
        Ok(quantity) => quantity,
        Err(error) => {
            let detail = error.to_string();
            parameter_type_finding(
                value_kind,
                &definition.parameter_id,
                "quantity object with exact string `value` and `unit`",
                location,
                Some(&detail),
                findings,
            );
            return None;
        }
    };
    let canonical = match kinds.scale_quantity(kind, &quantity.value, &quantity.unit) {
        Ok(canonical) => canonical,
        Err(error) => {
            let repair = error.repair().map(compiler_repair);
            parameter_type_finding(
                value_kind,
                &definition.parameter_id,
                &format!("quantity of kind `{kind}`"),
                location,
                Some(error.detail()),
                findings,
            );
            if let Some(repair) = repair
                && let Some(diagnostic) = findings.last_mut()
            {
                diagnostic.repairs.push(repair);
            }
            return None;
        }
    };
    match quantity_outside_domain(&canonical.value, kind, min, max, kinds) {
        Ok(true) => {
            parameter_domain_finding(
                value_kind,
                &definition.parameter_id,
                location,
                "quantity is outside its declared domain",
                None,
                findings,
            );
            None
        }
        Ok(false) => Some(CompiledParameterValue::Quantity {
            kind: kind.into(),
            value: canonical.value.canonical_rational(),
            unit: canonical.canonical_unit,
        }),
        Err(error) => {
            parameter_type_finding(
                value_kind,
                &definition.parameter_id,
                &format!("quantity of kind `{kind}` with a comparable domain"),
                location,
                Some(error.detail()),
                findings,
            );
            None
        }
    }
}

pub(super) fn integer_outside_domain(
    value: i64,
    min: &Option<IntegerBound>,
    max: &Option<IntegerBound>,
) -> bool {
    min.as_ref()
        .is_some_and(|bound| value < bound.value || (value == bound.value && !bound.inclusive))
        || max
            .as_ref()
            .is_some_and(|bound| value > bound.value || (value == bound.value && !bound.inclusive))
}

pub(super) fn exact_outside_domain(
    value: &ExactNumber,
    min: &Option<ExactBound>,
    max: &Option<ExactBound>,
) -> Result<bool, avila_core_kernel::KernelError> {
    if let Some(bound) = min {
        let ordering = value.checked_cmp(&bound.value)?;
        if ordering == Ordering::Less || (ordering == Ordering::Equal && !bound.inclusive) {
            return Ok(true);
        }
    }
    if let Some(bound) = max {
        let ordering = value.checked_cmp(&bound.value)?;
        if ordering == Ordering::Greater || (ordering == Ordering::Equal && !bound.inclusive) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn quantity_outside_domain(
    value: &ExactNumber,
    kind: &str,
    min: &Option<QuantityBound>,
    max: &Option<QuantityBound>,
    kinds: &KindRegistry,
) -> Result<bool, avila_core_kernel::KernelError> {
    if let Some(bound) = min {
        let canonical = kinds.scale_quantity(kind, &bound.value.value, &bound.value.unit)?;
        let ordering = value.checked_cmp(&canonical.value)?;
        if ordering == Ordering::Less || (ordering == Ordering::Equal && !bound.inclusive) {
            return Ok(true);
        }
    }
    if let Some(bound) = max {
        let canonical = kinds.scale_quantity(kind, &bound.value.value, &bound.value.unit)?;
        let ordering = value.checked_cmp(&canonical.value)?;
        if ordering == Ordering::Greater || (ordering == Ordering::Equal && !bound.inclusive) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn parameter_type_finding(
    value_kind: &str,
    value_id: &str,
    expected: &str,
    location: SourceLocation,
    detail: Option<&str>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut message = format!("{value_kind} `{value_id}` must be authored as {expected}");
    if let Some(detail) = detail {
        message.push_str(": ");
        message.push_str(detail);
    }
    findings.push(CoreDiagnostic::new(
        CORE_T2401,
        FindingClass::Invalid,
        "requester",
        location,
        message,
    ));
}

pub(super) fn parameter_domain_finding(
    value_kind: &str,
    value_id: &str,
    location: SourceLocation,
    detail: &str,
    candidates: Option<Vec<String>>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut diagnostic = CoreDiagnostic::new(
        CORE_T2402,
        FindingClass::Invalid,
        "requester",
        location,
        format!("{value_kind} `{value_id}` {detail}"),
    );
    if let Some(candidates) = candidates {
        diagnostic = diagnostic.with_repair(DiagnosticRepair {
            applicability: RepairApplicability::ConstrainedChoice,
            candidates,
        });
    }
    findings.push(diagnostic);
}

pub(super) fn compiler_repair(repair: &avila_core_kernel::Repair) -> DiagnosticRepair {
    DiagnosticRepair {
        applicability: match repair.applicability {
            avila_core_kernel::RepairApplicability::MechanicallySafe => {
                RepairApplicability::MechanicallySafe
            }
            avila_core_kernel::RepairApplicability::ConstrainedChoice => {
                RepairApplicability::ConstrainedChoice
            }
            avila_core_kernel::RepairApplicability::MethodOwnerJudgment => {
                RepairApplicability::MethodOwnerJudgment
            }
        },
        candidates: repair.candidates.clone(),
    }
}
