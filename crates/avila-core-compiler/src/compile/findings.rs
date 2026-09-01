//! Finding constructors and source-location helpers shared by every pass.

use crate::diagnostic::{
    CORE_R3401, CORE_R3501, CORE_S1102, CoreDiagnostic, FindingClass, SourceLocation,
};
use crate::document::VersionedRef;
use std::collections::BTreeSet;

pub(super) fn find_duplicate_ids<'a>(
    values: impl Iterator<Item = &'a str>,
    kind: &str,
    collection_pointer: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for (index, value) in values.enumerate() {
        if !seen.insert(value) {
            invalid_value(
                contract_location(format!("{collection_pointer}/{index}")),
                format!("duplicate {kind} identifier `{value}`"),
                "requester",
                findings,
            );
        }
    }
}

pub(super) fn validate_versioned_ref(
    reference: &VersionedRef,
    location: SourceLocation,
    owner: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    if reference.id.trim().is_empty() || reference.major == 0 {
        invalid_value(
            location,
            "a versioned reference requires a nonempty id and major version of at least one",
            owner,
            findings,
        );
    }
}

pub(super) fn require_nonempty(
    value: &str,
    location: SourceLocation,
    owner: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    if value.trim().is_empty() {
        invalid_value(location, "value must not be empty", owner, findings);
    }
}

pub(super) fn invalid_value(
    location: SourceLocation,
    message: impl Into<String>,
    owner: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_S1102,
        FindingClass::Invalid,
        owner,
        location,
        message,
    ));
}

pub(super) fn registry_incomplete(
    location: SourceLocation,
    message: impl Into<String>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_R3501,
        FindingClass::Invalid,
        "registry_owner",
        location,
        message,
    ));
}

pub(super) fn review_incomplete(
    location: SourceLocation,
    message: impl Into<String>,
    owner: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_R3401,
        FindingClass::Invalid,
        owner,
        location,
        message,
    ));
}

pub(super) fn logical_input_location(step_index: usize, slot: &str) -> SourceLocation {
    contract_location(format!(
        "/workflow/{step_index}/inputs/{}",
        escape_pointer_token(slot)
    ))
}

pub(super) fn parameter_location(step_index: usize, parameter_id: &str) -> SourceLocation {
    contract_location(format!(
        "/workflow/{step_index}/parameters/{}",
        escape_pointer_token(parameter_id)
    ))
}

pub(super) fn seed_location(step_index: usize) -> SourceLocation {
    contract_location(format!("/workflow/{step_index}/reproducibility/seed"))
}

pub(super) fn material_factor_location(step_index: usize, factor_id: &str) -> SourceLocation {
    contract_location(format!(
        "/workflow/{step_index}/reproducibility/material_factors/{}",
        escape_pointer_token(factor_id)
    ))
}

pub(super) fn contract_location(pointer: impl Into<String>) -> SourceLocation {
    SourceLocation::new("contract", pointer)
}

pub(super) fn registry_location(pointer: impl Into<String>) -> SourceLocation {
    SourceLocation::new("registry", pointer)
}

pub(crate) fn owner_for(document: &str) -> &'static str {
    match document {
        "registry" => "registry_owner",
        "claims" => "executor",
        _ => "requester",
    }
}

pub(super) fn escape_pointer_token(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
