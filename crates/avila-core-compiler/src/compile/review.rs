//! Accountable-review obligations.

use super::findings::{contract_location, review_incomplete};
use super::ir::{CompiledReviewObligation, ResolvedBinding, ReviewFulfillment};
use super::registry::RegistryIndex;
use crate::diagnostic::{CORE_R3401, CoreDiagnostic, FindingClass};
use crate::document::{ContractSource, ReviewIndependence, ReviewParty};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn compile_review_obligations(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    bindings: &BTreeMap<String, Vec<ResolvedBinding>>,
    findings: &mut Vec<CoreDiagnostic>,
) -> BTreeMap<String, CompiledReviewObligation> {
    let mut compiled = BTreeMap::new();
    for (step_index, step) in contract.workflow.iter().enumerate() {
        let Some(capability) = registry.capability_types.get(&step.capability_type) else {
            continue;
        };
        let Some(declaration) = &capability.review else {
            if step.review.is_some() {
                review_incomplete(
                    contract_location(format!("/workflow/{step_index}/review")),
                    format!(
                        "capability type `{}@{}` does not declare accountable review semantics",
                        step.capability_type.id, step.capability_type.major
                    ),
                    "policy_owner",
                    findings,
                );
            }
            continue;
        };
        let Some(binding) = &step.review else {
            findings.push(CoreDiagnostic::new(
                CORE_R3401,
                FindingClass::Missing,
                "policy_owner",
                contract_location(format!("/workflow/{step_index}/review")),
                "review capability requires an explicit eligibility policy and independence declaration",
            ));
            continue;
        };

        let policy_pointer = format!("/workflow/{step_index}/review/reviewer_eligibility_policy");
        let mut valid = true;
        if binding
            .reviewer_eligibility_policy
            .policy_id
            .trim()
            .is_empty()
        {
            review_incomplete(
                contract_location(format!("{policy_pointer}/policy_id")),
                "reviewer eligibility policy id must not be empty",
                "policy_owner",
                findings,
            );
            valid = false;
        }
        if binding.reviewer_eligibility_policy.revision == 0 {
            review_incomplete(
                contract_location(format!("{policy_pointer}/revision")),
                "reviewer eligibility policy revision must be at least one",
                "policy_owner",
                findings,
            );
            valid = false;
        }
        if !is_sha256_identity(&binding.reviewer_eligibility_policy.sha256) {
            review_incomplete(
                contract_location(format!("{policy_pointer}/sha256")),
                "reviewer eligibility policy must be pinned by a lowercase sha256 identity",
                "policy_owner",
                findings,
            );
            valid = false;
        }

        if let ReviewIndependence::Constraints { requirements } = &binding.independence {
            let independence_pointer =
                format!("/workflow/{step_index}/review/independence/requirements");
            if requirements.is_empty() {
                review_incomplete(
                    contract_location(&independence_pointer),
                    "constraint-based review independence must name at least one party",
                    "policy_owner",
                    findings,
                );
                valid = false;
            }
            let mut parties = BTreeSet::<ReviewParty>::new();
            for (index, requirement) in requirements.iter().enumerate() {
                if !parties.insert(requirement.separated_from) {
                    review_incomplete(
                        contract_location(format!("{independence_pointer}/{index}/separated_from")),
                        "review independence may constrain each party only once",
                        "policy_owner",
                        findings,
                    );
                    valid = false;
                }
            }
        }

        let Some(output) = capability
            .outputs
            .iter()
            .find(|output| output.slot_id == declaration.decision_output_slot)
        else {
            continue;
        };
        let resolved: BTreeMap<_, _> = bindings
            .get(&step.step_id)
            .into_iter()
            .flatten()
            .map(|binding| (binding.input_slot.as_str(), binding))
            .collect();
        let presented_evidence: Vec<_> = declaration
            .presented_input_slots
            .iter()
            .filter_map(|slot_id| resolved.get(slot_id.as_str()).copied().cloned())
            .collect();
        if presented_evidence.len() != declaration.presented_input_slots.len() {
            continue;
        }
        if !valid {
            continue;
        }

        compiled.insert(
            step.step_id.clone(),
            CompiledReviewObligation {
                fulfillment: ReviewFulfillment::PendingExternalReview,
                presented_evidence,
                decision_output_slot: output.slot_id.clone(),
                decision_role: output.role.clone(),
                decision_media_type: output.media_type.clone(),
                allowed_dispositions: declaration.allowed_dispositions.clone(),
                reviewer_eligibility_policy: binding.reviewer_eligibility_policy.clone(),
                independence: binding.independence.clone(),
            },
        );
    }
    compiled
}

pub(super) fn is_sha256_identity(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
