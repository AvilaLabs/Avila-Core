//! Contract-shape checks that need no registry.

use super::findings::{
    contract_location, find_duplicate_ids, invalid_value, require_nonempty, validate_versioned_ref,
};
use crate::diagnostic::CoreDiagnostic;
use crate::document::ContractSource;
use std::collections::BTreeSet;

pub(super) fn validate_contract_shape(
    contract: &ContractSource,
    findings: &mut Vec<CoreDiagnostic>,
) {
    require_nonempty(
        &contract.contract_id,
        contract_location("/contract_id"),
        "requester",
        findings,
    );
    if contract.revision == 0 {
        invalid_value(
            contract_location("/revision"),
            "contract revision must be at least one",
            "requester",
            findings,
        );
    }
    if contract.workflow.is_empty() {
        invalid_value(
            contract_location("/workflow"),
            "a contract workflow must contain at least one step",
            "requester",
            findings,
        );
    }
    if contract.requirements.is_empty() {
        invalid_value(
            contract_location("/requirements"),
            "a contract must contain at least one requirement",
            "requester",
            findings,
        );
    }

    find_duplicate_ids(
        contract.inputs.iter().map(|item| item.input_id.as_str()),
        "input",
        "/inputs",
        findings,
    );
    find_duplicate_ids(
        contract.workflow.iter().map(|item| item.step_id.as_str()),
        "workflow step",
        "/workflow",
        findings,
    );
    find_duplicate_ids(
        contract
            .requirements
            .iter()
            .map(|item| item.requirement_id.as_str()),
        "requirement",
        "/requirements",
        findings,
    );

    let mut permitted_roles = BTreeSet::new();
    for (index, role) in contract
        .execution_policy
        .permitted_nondeterministic_roles
        .iter()
        .enumerate()
    {
        let location = contract_location(format!(
            "/execution_policy/permitted_nondeterministic_roles/{index}"
        ));
        validate_versioned_ref(role, location.clone(), "policy_owner", findings);
        if !permitted_roles.insert(role) {
            invalid_value(
                location,
                format!(
                    "nondeterminism permission repeats role `{}@{}`",
                    role.id, role.major
                ),
                "policy_owner",
                findings,
            );
        }
    }

    for (index, input) in contract.inputs.iter().enumerate() {
        require_nonempty(
            &input.input_id,
            contract_location(format!("/inputs/{index}/input_id")),
            "requester",
            findings,
        );
        require_nonempty(
            &input.media_type,
            contract_location(format!("/inputs/{index}/media_type")),
            "requester",
            findings,
        );
        validate_versioned_ref(
            &input.role,
            contract_location(format!("/inputs/{index}/role")),
            "requester",
            findings,
        );
    }
    for (index, step) in contract.workflow.iter().enumerate() {
        require_nonempty(
            &step.step_id,
            contract_location(format!("/workflow/{index}/step_id")),
            "requester",
            findings,
        );
        validate_versioned_ref(
            &step.capability_type,
            contract_location(format!("/workflow/{index}/capability_type")),
            "requester",
            findings,
        );
        let mut slots = BTreeSet::new();
        for (binding_index, binding) in step.bindings.iter().enumerate() {
            require_nonempty(
                &binding.input_slot,
                contract_location(format!(
                    "/workflow/{index}/bindings/{binding_index}/input_slot"
                )),
                "requester",
                findings,
            );
            if !slots.insert(binding.input_slot.as_str()) {
                invalid_value(
                    contract_location(format!(
                        "/workflow/{index}/bindings/{binding_index}/input_slot"
                    )),
                    format!(
                        "step `{}` binds input slot `{}` more than once",
                        step.step_id, binding.input_slot
                    ),
                    "requester",
                    findings,
                );
            }
        }
    }
    for (index, requirement) in contract.requirements.iter().enumerate() {
        require_nonempty(
            &requirement.requirement_id,
            contract_location(format!("/requirements/{index}/requirement_id")),
            "requester",
            findings,
        );
        require_nonempty(
            &requirement.statement,
            contract_location(format!("/requirements/{index}/statement")),
            "requester",
            findings,
        );
        validate_versioned_ref(
            &requirement.purpose,
            contract_location(format!("/requirements/{index}/purpose")),
            "requester",
            findings,
        );
        require_nonempty(
            &requirement.limit.kind,
            contract_location(format!("/requirements/{index}/limit/kind")),
            "requester",
            findings,
        );
        require_nonempty(
            &requirement.limit.unit,
            contract_location(format!("/requirements/{index}/limit/unit")),
            "requester",
            findings,
        );
        if let Some(tolerance) = &requirement.tolerance {
            require_nonempty(
                &tolerance.kind,
                contract_location(format!("/requirements/{index}/tolerance/kind")),
                "requester",
                findings,
            );
            require_nonempty(
                &tolerance.unit,
                contract_location(format!("/requirements/{index}/tolerance/unit")),
                "requester",
                findings,
            );
        }
    }
}
