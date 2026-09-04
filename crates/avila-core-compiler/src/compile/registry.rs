//! Registry snapshot indexing and internal-consistency checks.

use super::findings::{
    invalid_value, registry_incomplete, registry_location, require_nonempty, review_incomplete,
    validate_versioned_ref,
};
use super::schema::validate_role_schema_definition;
use super::values::NOT_DEFINED_PLACEHOLDER;
use crate::diagnostic::{CoreDiagnostic, FindingClass};
use crate::document::{
    CapabilityTypeDefinition, ClaimModelDeclaration, ExactBound, ParameterType, PurposeDefinition,
    QuantityBound, RegistrySnapshot, RoleDefinition, VersionedRef,
};
use avila_core_kernel::{ExactNumber, KindDefinition, KindRegistry, UnitDefinition};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct RegistryIndex<'a> {
    pub(crate) kinds: KindRegistry,
    pub(crate) kind_classes: BTreeMap<&'a str, &'a str>,
    pub(crate) purposes: BTreeMap<VersionedRef, &'a PurposeDefinition>,
    pub(crate) roles: BTreeMap<VersionedRef, &'a RoleDefinition>,
    pub(crate) capability_types: BTreeMap<VersionedRef, &'a CapabilityTypeDefinition>,
}

impl<'a> RegistryIndex<'a> {
    pub(crate) fn build(
        registry: &'a RegistrySnapshot,
        findings: &mut Vec<CoreDiagnostic>,
    ) -> Self {
        require_nonempty(
            &registry.registry_id,
            registry_location("/registry_id"),
            "registry_owner",
            findings,
        );
        if registry.revision == 0 {
            invalid_value(
                registry_location("/revision"),
                "registry revision must be at least one",
                "registry_owner",
                findings,
            );
        }

        let mut kinds = KindRegistry::default();
        let mut kind_classes = BTreeMap::new();
        for (index, kind) in registry.kinds.iter().enumerate() {
            let pointer = format!("/kinds/{index}");
            require_nonempty(
                &kind.kind_id,
                registry_location(format!("{pointer}/kind_id")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &kind.unit_class,
                registry_location(format!("{pointer}/unit_class")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &kind.owner,
                registry_location(format!("{pointer}/owner")),
                "registry_owner",
                findings,
            );
            if kind_classes
                .insert(kind.kind_id.as_str(), kind.unit_class.as_str())
                .is_some()
            {
                invalid_value(
                    registry_location(format!("{pointer}/kind_id")),
                    format!("duplicate quantity kind `{}`", kind.kind_id),
                    "registry_owner",
                    findings,
                );
                continue;
            }
            let unit_definitions = kind
                .units
                .iter()
                .map(|unit| UnitDefinition::new(unit.symbol.clone(), unit.factor.clone()))
                .collect::<Result<Vec<_>, _>>();
            let definition = unit_definitions.and_then(|units| {
                KindDefinition::new(kind.kind_id.clone(), kind.canonical_unit.clone(), units)
            });
            match definition.and_then(|definition| kinds.insert_kind(definition)) {
                Ok(()) => {}
                Err(error) => findings.push(CoreDiagnostic::new(
                    error.code(),
                    FindingClass::Invalid,
                    "registry_owner",
                    registry_location(&pointer),
                    error.detail(),
                )),
            }
        }

        let mut purposes = BTreeMap::new();
        for (index, purpose) in registry.purposes.iter().enumerate() {
            let pointer = format!("/purposes/{index}");
            validate_versioned_ref(
                &purpose.purpose,
                registry_location(format!("{pointer}/purpose")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &purpose.owner,
                registry_location(format!("{pointer}/owner")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &purpose.description,
                registry_location(format!("{pointer}/description")),
                "registry_owner",
                findings,
            );
            if purposes.insert(purpose.purpose.clone(), purpose).is_some() {
                registry_incomplete(
                    registry_location(format!("{pointer}/purpose")),
                    format!(
                        "duplicate governed purpose `{}@{}`",
                        purpose.purpose.id, purpose.purpose.major
                    ),
                    findings,
                );
            }
        }

        let mut roles = BTreeMap::new();
        for (index, role) in registry.roles.iter().enumerate() {
            let pointer = format!("/roles/{index}");
            validate_versioned_ref(
                &role.role,
                registry_location(format!("{pointer}/role")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &role.owner,
                registry_location(format!("{pointer}/owner")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &role.validator,
                registry_location(format!("{pointer}/validator")),
                "registry_owner",
                findings,
            );
            if let Some(input_schema) = &role.input_schema {
                validate_role_schema_definition(
                    input_schema,
                    &format!("{pointer}/input_schema"),
                    findings,
                );
            }
            if role.accepted_media_types.is_empty() {
                registry_incomplete(
                    registry_location(format!("{pointer}/accepted_media_types")),
                    "evidence role must accept at least one media type",
                    findings,
                );
            }
            if role.permitted_claim_models.is_empty() {
                registry_incomplete(
                    registry_location(format!("{pointer}/permitted_claim_models")),
                    "evidence role must permit at least one explicit claim model",
                    findings,
                );
            }
            let mut categories = BTreeSet::new();
            for (category_index, category) in role.categorical_values.iter().enumerate() {
                let location =
                    registry_location(format!("{pointer}/categorical_values/{category_index}"));
                if category.trim().is_empty() {
                    registry_incomplete(
                        location,
                        "a categorical role value must not be empty",
                        findings,
                    );
                } else if !categories.insert(category.as_str()) {
                    registry_incomplete(
                        location,
                        format!("categorical role repeats value `{category}`"),
                        findings,
                    );
                }
            }
            if !role.categorical_values.is_empty()
                && role.permitted_claim_models.as_slice() != [ClaimModelDeclaration::Unquantified]
            {
                registry_incomplete(
                    registry_location(format!("{pointer}/permitted_claim_models")),
                    "a closed categorical role must permit only the unquantified claim model",
                    findings,
                );
            }
            if roles.insert(role.role.clone(), role).is_some() {
                invalid_value(
                    registry_location(format!("{pointer}/role")),
                    format!(
                        "duplicate evidence role `{}@{}`",
                        role.role.id, role.role.major
                    ),
                    "registry_owner",
                    findings,
                );
            }
            match (&role.quantity_kind, &role.unit_class) {
                (Some(kind), Some(unit_class)) => match kind_classes.get(kind.as_str()) {
                    Some(expected) if expected == &unit_class.as_str() => {}
                    Some(expected) => registry_incomplete(
                        registry_location(format!("{pointer}/unit_class")),
                        format!(
                            "role unit class `{unit_class}` does not match kind `{kind}` class `{expected}`"
                        ),
                        findings,
                    ),
                    None => registry_incomplete(
                        registry_location(format!("{pointer}/quantity_kind")),
                        format!("role references unknown quantity kind `{kind}`"),
                        findings,
                    ),
                },
                (None, None) => {}
                _ => registry_incomplete(
                    registry_location(&pointer),
                    "a quantity role must declare both quantity_kind and unit_class",
                    findings,
                ),
            }
            if !role.categorical_values.is_empty()
                && (role.quantity_kind.is_some() || role.unit_class.is_some())
            {
                registry_incomplete(
                    registry_location(format!("{pointer}/categorical_values")),
                    "a closed categorical role must be non-quantitative",
                    findings,
                );
            }
        }

        let mut capability_types = BTreeMap::new();
        for (index, capability) in registry.capability_types.iter().enumerate() {
            let pointer = format!("/capability_types/{index}");
            validate_versioned_ref(
                &capability.capability_type,
                registry_location(format!("{pointer}/capability_type")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &capability.owner,
                registry_location(format!("{pointer}/owner")),
                "registry_owner",
                findings,
            );
            if capability_types
                .insert(capability.capability_type.clone(), capability)
                .is_some()
            {
                invalid_value(
                    registry_location(format!("{pointer}/capability_type")),
                    format!(
                        "duplicate capability type `{}@{}`",
                        capability.capability_type.id, capability.capability_type.major
                    ),
                    "registry_owner",
                    findings,
                );
            }
            validate_slots(capability, index, &roles, &purposes, findings);
            validate_parameter_definitions(capability, index, &kinds, &kind_classes, findings);
            validate_reproducibility_declaration(
                capability,
                index,
                &kinds,
                &kind_classes,
                findings,
            );
            validate_review_declaration(capability, index, &roles, findings);
        }

        Self {
            kinds,
            kind_classes,
            purposes,
            roles,
            capability_types,
        }
    }
}

pub(super) fn validate_review_declaration(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    roles: &BTreeMap<VersionedRef, &RoleDefinition>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let Some(review) = &capability.review else {
        return;
    };
    let review_pointer = format!("/capability_types/{capability_index}/review");

    if capability.inputs.is_empty() {
        review_incomplete(
            registry_location(format!("{review_pointer}/presented_input_slots")),
            "a review dossier must contain at least one evidence input",
            "registry_owner",
            findings,
        );
    }

    let input_slots: BTreeSet<_> = capability
        .inputs
        .iter()
        .map(|slot| slot.slot_id.as_str())
        .collect();
    let mut presented = BTreeSet::new();
    for (index, slot_id) in review.presented_input_slots.iter().enumerate() {
        let location = registry_location(format!("{review_pointer}/presented_input_slots/{index}"));
        if slot_id.trim().is_empty() {
            review_incomplete(
                location,
                "a presented review slot must not be empty",
                "registry_owner",
                findings,
            );
            continue;
        }
        if !presented.insert(slot_id.as_str()) {
            review_incomplete(
                location,
                format!("review dossier repeats input slot `{slot_id}`"),
                "registry_owner",
                findings,
            );
        } else if !input_slots.contains(slot_id.as_str()) {
            review_incomplete(
                location,
                format!("review dossier references unknown input slot `{slot_id}`"),
                "registry_owner",
                findings,
            );
        }
    }
    for (input_index, input) in capability.inputs.iter().enumerate() {
        if !input.required {
            review_incomplete(
                registry_location(format!(
                    "/capability_types/{capability_index}/inputs/{input_index}/required"
                )),
                format!(
                    "review input slot `{}` must be required so the compiled dossier is exact",
                    input.slot_id
                ),
                "registry_owner",
                findings,
            );
        }
        if !presented.contains(input.slot_id.as_str()) {
            review_incomplete(
                registry_location(format!("{review_pointer}/presented_input_slots")),
                format!(
                    "review dossier omits declared input slot `{}`",
                    input.slot_id
                ),
                "registry_owner",
                findings,
            );
        }
    }

    if review.allowed_dispositions.is_empty() {
        review_incomplete(
            registry_location(format!("{review_pointer}/allowed_dispositions")),
            "a review must declare at least one governance disposition",
            "registry_owner",
            findings,
        );
    }
    let mut dispositions = BTreeSet::new();
    for (index, disposition) in review.allowed_dispositions.iter().enumerate() {
        if !dispositions.insert(*disposition) {
            review_incomplete(
                registry_location(format!("{review_pointer}/allowed_dispositions/{index}")),
                "a governance disposition may be declared only once",
                "registry_owner",
                findings,
            );
        }
    }
    if review.decision_output_slot.trim().is_empty() {
        review_incomplete(
            registry_location(format!("{review_pointer}/decision_output_slot")),
            "a review must name its decision output slot",
            "registry_owner",
            findings,
        );
        return;
    }
    let Some(output) = capability
        .outputs
        .iter()
        .find(|output| output.slot_id == review.decision_output_slot)
    else {
        review_incomplete(
            registry_location(format!("{review_pointer}/decision_output_slot")),
            format!(
                "review decision output `{}` is not declared by the capability type",
                review.decision_output_slot
            ),
            "registry_owner",
            findings,
        );
        return;
    };
    if capability.outputs.len() != 1 {
        review_incomplete(
            registry_location(format!("/capability_types/{capability_index}/outputs")),
            "the current R9 subset permits a review capability to emit only its decision record",
            "registry_owner",
            findings,
        );
    }
    if output.permitted_claim_models.as_slice() != [ClaimModelDeclaration::Unquantified] {
        review_incomplete(
            registry_location(format!(
                "/capability_types/{capability_index}/outputs/{}/permitted_claim_models",
                capability
                    .outputs
                    .iter()
                    .position(|candidate| candidate.slot_id == output.slot_id)
                    .unwrap_or(0)
            )),
            "a review disposition is governance evidence and must use only the unquantified claim model",
            "registry_owner",
            findings,
        );
    }
    if let Some(role) = roles.get(&output.role)
        && (role.quantity_kind.is_some() || role.unit_class.is_some())
    {
        review_incomplete(
            registry_location(format!("{review_pointer}/decision_output_slot")),
            "a review decision role must be non-quantitative and cannot serve as a technical requirement metric",
            "registry_owner",
            findings,
        );
    }
}

pub(super) fn validate_parameter_definitions(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    kinds: &KindRegistry,
    kind_classes: &BTreeMap<&str, &str>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut parameter_ids = BTreeSet::new();
    for (index, parameter) in capability.parameters.iter().enumerate() {
        let pointer = format!("/capability_types/{capability_index}/parameters/{index}");
        require_nonempty(
            &parameter.parameter_id,
            registry_location(format!("{pointer}/parameter_id")),
            "registry_owner",
            findings,
        );
        if !parameter_ids.insert(parameter.parameter_id.as_str()) {
            invalid_value(
                registry_location(format!("{pointer}/parameter_id")),
                format!("duplicate parameter `{}`", parameter.parameter_id),
                "registry_owner",
                findings,
            );
        }
        validate_value_type_definition(
            "parameter",
            &parameter.parameter_id,
            &parameter.value_type,
            &pointer,
            kinds,
            kind_classes,
            findings,
        );
    }
}

pub(super) fn validate_reproducibility_declaration(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    kinds: &KindRegistry,
    kind_classes: &BTreeMap<&str, &str>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut factor_ids = BTreeSet::new();
    for (index, factor) in capability
        .reproducibility
        .material_factors
        .iter()
        .enumerate()
    {
        let pointer = format!(
            "/capability_types/{capability_index}/reproducibility/material_factors/{index}"
        );
        require_nonempty(
            &factor.factor_id,
            registry_location(format!("{pointer}/factor_id")),
            "registry_owner",
            findings,
        );
        if !factor_ids.insert(factor.factor_id.as_str()) {
            invalid_value(
                registry_location(format!("{pointer}/factor_id")),
                format!("duplicate material execution factor `{}`", factor.factor_id),
                "registry_owner",
                findings,
            );
        }
        validate_value_type_definition(
            "material execution factor",
            &factor.factor_id,
            &factor.value_type,
            &pointer,
            kinds,
            kind_classes,
            findings,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_value_type_definition(
    value_kind: &str,
    value_id: &str,
    value_type: &ParameterType,
    pointer: &str,
    kinds: &KindRegistry,
    kind_classes: &BTreeMap<&str, &str>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    match value_type {
        ParameterType::Boolean => {}
        ParameterType::Integer { min, max } => {
            if let (Some(min), Some(max)) = (min, max)
                && range_is_empty(min.value.cmp(&max.value), min.inclusive, max.inclusive)
            {
                registry_incomplete(
                    registry_location(format!("{pointer}/value_type")),
                    format!("{value_kind} `{value_id}` declares an empty integer domain"),
                    findings,
                );
            }
        }
        ParameterType::ExactNumber { min, max } => {
            validate_exact_value_range(value_kind, value_id, min, max, pointer, findings);
        }
        ParameterType::Text { allowed_values } => {
            if let Some(values) = allowed_values {
                if values.is_empty() {
                    registry_incomplete(
                        registry_location(format!("{pointer}/value_type/allowed_values")),
                        format!(
                            "{value_kind} `{value_id}` declares an empty set of allowed text values"
                        ),
                        findings,
                    );
                }
                let mut unique = BTreeSet::new();
                if values.iter().any(|value| !unique.insert(value.as_str())) {
                    registry_incomplete(
                        registry_location(format!("{pointer}/value_type/allowed_values")),
                        format!("{value_kind} `{value_id}` declares duplicate allowed text values"),
                        findings,
                    );
                }
                if values.iter().any(|value| value == NOT_DEFINED_PLACEHOLDER) {
                    registry_incomplete(
                        registry_location(format!("{pointer}/value_type/allowed_values")),
                        format!(
                            "{value_kind} `{value_id}` cannot admit the reserved draft placeholder `{NOT_DEFINED_PLACEHOLDER}` as a text value"
                        ),
                        findings,
                    );
                }
            }
        }
        ParameterType::Quantity { kind, min, max } => {
            if !kind_classes.contains_key(kind.as_str()) {
                registry_incomplete(
                    registry_location(format!("{pointer}/value_type/kind")),
                    format!("{value_kind} `{value_id}` references unknown quantity kind `{kind}`"),
                    findings,
                );
                return;
            }
            validate_quantity_value_range(
                value_kind, value_id, kind, min, max, pointer, kinds, findings,
            );
        }
    }
}

pub(super) fn validate_exact_value_range(
    value_kind: &str,
    value_id: &str,
    min: &Option<ExactBound>,
    max: &Option<ExactBound>,
    pointer: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let (Some(min), Some(max)) = (min, max) else {
        return;
    };
    match min.value.checked_cmp(&max.value) {
        Ok(ordering) if range_is_empty(ordering, min.inclusive, max.inclusive) => {
            registry_incomplete(
                registry_location(format!("{pointer}/value_type")),
                format!("{value_kind} `{value_id}` declares an empty exact-number domain"),
                findings,
            );
        }
        Ok(_) => {}
        Err(error) => registry_incomplete(
            registry_location(format!("{pointer}/value_type")),
            format!(
                "{value_kind} `{value_id}` domain cannot be compared: {}",
                error.detail()
            ),
            findings,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_quantity_value_range(
    value_kind: &str,
    value_id: &str,
    kind: &str,
    min: &Option<QuantityBound>,
    max: &Option<QuantityBound>,
    pointer: &str,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let lower = min.as_ref().and_then(|bound| {
        validate_registry_quantity_bound(
            value_kind, value_id, kind, bound, "min", pointer, kinds, findings,
        )
    });
    let upper = max.as_ref().and_then(|bound| {
        validate_registry_quantity_bound(
            value_kind, value_id, kind, bound, "max", pointer, kinds, findings,
        )
    });
    let (Some(lower), Some(upper), Some(min), Some(max)) = (lower, upper, min, max) else {
        return;
    };
    match lower.checked_cmp(&upper) {
        Ok(ordering) if range_is_empty(ordering, min.inclusive, max.inclusive) => {
            registry_incomplete(
                registry_location(format!("{pointer}/value_type")),
                format!("{value_kind} `{value_id}` declares an empty quantity domain"),
                findings,
            );
        }
        Ok(_) => {}
        Err(error) => registry_incomplete(
            registry_location(format!("{pointer}/value_type")),
            format!(
                "{value_kind} `{value_id}` domain cannot be compared: {}",
                error.detail()
            ),
            findings,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_registry_quantity_bound(
    value_kind: &str,
    value_id: &str,
    kind: &str,
    bound: &QuantityBound,
    side: &str,
    pointer: &str,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<ExactNumber> {
    match kinds.scale_quantity(kind, &bound.value.value, &bound.value.unit) {
        Ok(quantity) => Some(quantity.value),
        Err(error) => {
            registry_incomplete(
                registry_location(format!("{pointer}/value_type/{side}")),
                format!(
                    "{value_kind} `{value_id}` has an invalid {side} quantity bound: {}",
                    error.detail()
                ),
                findings,
            );
            None
        }
    }
}

pub(super) fn range_is_empty(ordering: Ordering, min_inclusive: bool, max_inclusive: bool) -> bool {
    ordering == Ordering::Greater
        || (ordering == Ordering::Equal && !(min_inclusive && max_inclusive))
}

pub(super) fn validate_slots(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    roles: &BTreeMap<VersionedRef, &RoleDefinition>,
    purposes: &BTreeMap<VersionedRef, &PurposeDefinition>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut input_ids = BTreeSet::new();
    for (index, slot) in capability.inputs.iter().enumerate() {
        let pointer = format!("/capability_types/{capability_index}/inputs/{index}");
        require_nonempty(
            &slot.slot_id,
            registry_location(format!("{pointer}/slot_id")),
            "registry_owner",
            findings,
        );
        if !input_ids.insert(slot.slot_id.as_str()) {
            invalid_value(
                registry_location(format!("{pointer}/slot_id")),
                format!("duplicate input slot `{}`", slot.slot_id),
                "registry_owner",
                findings,
            );
        }
        match roles.get(&slot.role) {
            None => registry_incomplete(
                registry_location(format!("{pointer}/role")),
                format!(
                    "input slot references unknown role `{}@{}`",
                    slot.role.id, slot.role.major
                ),
                findings,
            ),
            Some(role) => {
                for media_type in &slot.accepted_media_types {
                    if !role.accepted_media_types.contains(media_type) {
                        registry_incomplete(
                            registry_location(format!("{pointer}/accepted_media_types")),
                            format!(
                                "input slot media type `{media_type}` is outside role `{}@{}`",
                                role.role.id, role.role.major
                            ),
                            findings,
                        );
                    }
                }
            }
        }
        if slot.accepted_media_types.is_empty() {
            registry_incomplete(
                registry_location(format!("{pointer}/accepted_media_types")),
                "input slot must accept at least one media type",
                findings,
            );
        }
    }

    let mut output_ids = BTreeSet::new();
    for (index, slot) in capability.outputs.iter().enumerate() {
        let pointer = format!("/capability_types/{capability_index}/outputs/{index}");
        require_nonempty(
            &slot.slot_id,
            registry_location(format!("{pointer}/slot_id")),
            "registry_owner",
            findings,
        );
        if !output_ids.insert(slot.slot_id.as_str()) {
            invalid_value(
                registry_location(format!("{pointer}/slot_id")),
                format!("duplicate output slot `{}`", slot.slot_id),
                "registry_owner",
                findings,
            );
        }
        match roles.get(&slot.role) {
            None => registry_incomplete(
                registry_location(format!("{pointer}/role")),
                format!(
                    "output slot references unknown role `{}@{}`",
                    slot.role.id, slot.role.major
                ),
                findings,
            ),
            Some(role) if !role.accepted_media_types.contains(&slot.media_type) => {
                registry_incomplete(
                    registry_location(format!("{pointer}/media_type")),
                    format!(
                        "output media type `{}` is outside role `{}@{}`",
                        slot.media_type, role.role.id, role.role.major
                    ),
                    findings,
                );
            }
            Some(role) => {
                if slot.permitted_claim_models.is_empty() {
                    registry_incomplete(
                        registry_location(format!("{pointer}/permitted_claim_models")),
                        "output slot must declare at least one permitted claim model",
                        findings,
                    );
                }
                for model in &slot.permitted_claim_models {
                    if !role.permitted_claim_models.contains(model) {
                        registry_incomplete(
                            registry_location(format!("{pointer}/permitted_claim_models")),
                            format!(
                                "output claim model is outside role `{}@{}`",
                                role.role.id, role.role.major
                            ),
                            findings,
                        );
                    }
                }
            }
        }
        require_nonempty(
            &slot.media_type,
            registry_location(format!("{pointer}/media_type")),
            "registry_owner",
            findings,
        );
        let mut excluded = BTreeSet::new();
        for (purpose_index, purpose) in slot.excluded_purposes.iter().enumerate() {
            let location =
                registry_location(format!("{pointer}/excluded_purposes/{purpose_index}"));
            validate_versioned_ref(purpose, location.clone(), "registry_owner", findings);
            if !excluded.insert(purpose) {
                registry_incomplete(
                    location,
                    format!(
                        "output slot repeats excluded purpose `{}@{}`",
                        purpose.id, purpose.major
                    ),
                    findings,
                );
            } else if !purposes.contains_key(purpose) {
                registry_incomplete(
                    location,
                    format!(
                        "output slot excludes unknown governed purpose `{}@{}`",
                        purpose.id, purpose.major
                    ),
                    findings,
                );
            }
        }
    }
}
