//! Non-blocking notices about declarations nothing consumes.

use super::findings::contract_location;
use super::ir::ResolvedBinding;
use super::registry::RegistryIndex;
use crate::diagnostic::{CORE_R3601, CORE_R3602, CoreDiagnostic, FindingClass};
use crate::document::{ContractSource, SourceRef};
use std::collections::{BTreeMap, BTreeSet};

/// Notices for declarations that would never enter the campaign: a contract
/// input no step binds, or a non-review step whose outputs feed neither
/// another step nor a requirement. Neither blocks compilation. They are
/// computed only when the contract is otherwise compilable, because an unfed
/// declaration is usually a consequence of a blocking failure reported
/// elsewhere.
pub(super) fn report_unconsumed_declarations(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    bindings: &BTreeMap<String, Vec<ResolvedBinding>>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let consumed: BTreeSet<SourceRef> = bindings
        .values()
        .flatten()
        .map(|binding| binding.source.clone())
        .chain(
            contract
                .requirements
                .iter()
                .filter_map(|requirement| requirement.metric.clone()),
        )
        .collect();
    for (index, input) in contract.inputs.iter().enumerate() {
        let source = SourceRef::ContractInput {
            input_id: input.input_id.clone(),
        };
        if !consumed.contains(&source) {
            findings.push(CoreDiagnostic::new(
                CORE_R3601,
                FindingClass::Notice,
                "requester",
                contract_location(format!("/inputs/{index}")),
                format!(
                    "contract input `{}` is bound to no step and enters no campaign evidence",
                    input.input_id
                ),
            ));
        }
    }
    for (index, step) in contract.workflow.iter().enumerate() {
        let is_review = registry
            .capability_types
            .get(&step.capability_type)
            .is_some_and(|capability| capability.review.is_some());
        if is_review {
            continue;
        }
        let consumed_output = consumed.iter().any(|source| {
            matches!(source, SourceRef::StepOutput { step_id, .. } if step_id == &step.step_id)
        });
        if !consumed_output {
            findings.push(CoreDiagnostic::new(
                CORE_R3602,
                FindingClass::Notice,
                "requester",
                contract_location(format!("/workflow/{index}")),
                format!(
                    "step `{}` produces no output consumed by another step or requirement",
                    step.step_id
                ),
            ));
        }
    }
}
