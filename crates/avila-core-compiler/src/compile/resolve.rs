//! Slot resolution, the derived dependency graph, and graph-shape checks.

use super::MAX_WORKFLOW_STEPS;
use super::findings::{contract_location, invalid_value, logical_input_location};
use super::ir::ResolvedBinding;
use super::registry::RegistryIndex;
use crate::diagnostic::{
    CORE_R3101, CORE_R3102, CORE_R3201, CORE_R3202, CORE_R3203, CORE_S1102, CORE_T2101, CORE_T2201,
    CORE_T2301, CORE_T2601, CoreDiagnostic, DiagnosticRepair, FindingClass, RepairApplicability,
    RepairEdit,
};
use crate::document::{
    ClaimModelDeclaration, ContractSource, InputSlotDefinition, SourceRef, VersionedRef,
    WorkflowStep,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) fn validate_contract_registry_refs(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> BTreeSet<SourceRef> {
    let mut invalid_sources = BTreeSet::new();
    for (index, role) in contract
        .execution_policy
        .permitted_nondeterministic_roles
        .iter()
        .enumerate()
    {
        if !registry.roles.contains_key(role) {
            findings.push(CoreDiagnostic::new(
                CORE_S1102,
                FindingClass::Invalid,
                "policy_owner",
                contract_location(format!(
                    "/execution_policy/permitted_nondeterministic_roles/{index}"
                )),
                format!(
                    "nondeterminism policy references role `{}@{}` absent from the supplied registry snapshot",
                    role.id, role.major
                ),
            ));
        }
    }
    for (index, requirement) in contract.requirements.iter().enumerate() {
        if !registry.purposes.contains_key(&requirement.purpose) {
            findings.push(CoreDiagnostic::new(
                CORE_T2601,
                FindingClass::Invalid,
                "requester",
                contract_location(format!("/requirements/{index}/purpose")),
                format!(
                    "requirement references governed purpose `{}@{}` absent from the supplied registry snapshot",
                    requirement.purpose.id, requirement.purpose.major
                ),
            ));
        }
    }
    for (index, input) in contract.inputs.iter().enumerate() {
        match registry.roles.get(&input.role) {
            None => {
                invalid_sources.insert(SourceRef::ContractInput {
                    input_id: input.input_id.clone(),
                });
                findings.push(CoreDiagnostic::new(
                    CORE_R3101,
                    FindingClass::Unsatisfied,
                    "requester",
                    contract_location(format!("/inputs/{index}/role")),
                    format!(
                        "input references role `{}@{}` absent from the supplied registry snapshot",
                        input.role.id, input.role.major
                    ),
                ));
            }
            Some(role) if !role.accepted_media_types.contains(&input.media_type) => {
                invalid_sources.insert(SourceRef::ContractInput {
                    input_id: input.input_id.clone(),
                });
                findings.push(CoreDiagnostic::new(
                    CORE_T2301,
                    FindingClass::Invalid,
                    "requester",
                    contract_location(format!("/inputs/{index}/media_type")),
                    format!(
                        "media type `{}` is not accepted by role `{}@{}`",
                        input.media_type, input.role.id, input.role.major
                    ),
                ));
            }
            Some(role) if !role.permitted_claim_models.contains(&input.claim_model) => {
                invalid_sources.insert(SourceRef::ContractInput {
                    input_id: input.input_id.clone(),
                });
                findings.push(CoreDiagnostic::new(
                    CORE_T2201,
                    FindingClass::Invalid,
                    "requester",
                    contract_location(format!("/inputs/{index}/claim_model")),
                    format!(
                        "claim model is not permitted by role `{}@{}`",
                        input.role.id, input.role.major
                    ),
                ));
            }
            Some(_) => {}
        }
    }
    invalid_sources
}

#[derive(Clone)]
pub(super) struct Candidate {
    pub(super) source: SourceRef,
    pub(super) role: VersionedRef,
    pub(super) media_type: String,
    pub(super) claim_models: Vec<ClaimModelDeclaration>,
    pub(super) excluded_purposes: Vec<VersionedRef>,
}

pub(super) fn collect_sources(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
) -> Vec<Candidate> {
    let mut candidates: Vec<_> = contract
        .inputs
        .iter()
        .map(|input| Candidate {
            source: SourceRef::ContractInput {
                input_id: input.input_id.clone(),
            },
            role: input.role.clone(),
            media_type: input.media_type.clone(),
            claim_models: vec![input.claim_model.clone()],
            excluded_purposes: Vec::new(),
        })
        .collect();
    for step in &contract.workflow {
        let Some(capability) = registry.capability_types.get(&step.capability_type) else {
            continue;
        };
        candidates.extend(capability.outputs.iter().map(|output| Candidate {
            source: SourceRef::StepOutput {
                step_id: step.step_id.clone(),
                output_slot: output.slot_id.clone(),
            },
            role: output.role.clone(),
            media_type: output.media_type.clone(),
            claim_models: output.permitted_claim_models.clone(),
            excluded_purposes: output.excluded_purposes.clone(),
        }));
    }
    candidates.sort_by(|left, right| left.source.cmp(&right.source));
    candidates
}

#[derive(Default)]
pub(super) struct WorkflowResolution {
    pub(super) bindings: BTreeMap<String, Vec<ResolvedBinding>>,
    pub(super) dependencies: BTreeMap<String, BTreeSet<String>>,
}

pub(super) fn resolve_workflow(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    candidates: &[Candidate],
    invalid_sources: &BTreeSet<SourceRef>,
    unknown_type_steps: &BTreeSet<&str>,
    findings: &mut Vec<CoreDiagnostic>,
) -> WorkflowResolution {
    let mut result = WorkflowResolution::default();
    for step in &contract.workflow {
        result.dependencies.entry(step.step_id.clone()).or_default();
    }

    for (step_index, step) in contract.workflow.iter().enumerate() {
        let Some(capability) = registry.capability_types.get(&step.capability_type) else {
            findings.push(CoreDiagnostic::new(
                CORE_R3101,
                FindingClass::Unsatisfied,
                "requester",
                contract_location(format!("/workflow/{step_index}/capability_type")),
                format!(
                    "capability type `{}@{}` is absent from the supplied registry snapshot",
                    step.capability_type.id, step.capability_type.major
                ),
            ));
            continue;
        };

        let slot_definitions: BTreeMap<_, _> = capability
            .inputs
            .iter()
            .map(|slot| (slot.slot_id.as_str(), slot))
            .collect();
        for (binding_index, binding) in step.bindings.iter().enumerate() {
            if !slot_definitions.contains_key(binding.input_slot.as_str()) {
                invalid_value(
                    contract_location(format!(
                        "/workflow/{step_index}/bindings/{binding_index}/input_slot"
                    )),
                    format!(
                        "capability type `{}@{}` has no input slot `{}`",
                        step.capability_type.id, step.capability_type.major, binding.input_slot
                    ),
                    "requester",
                    findings,
                );
            }
        }

        let explicit: BTreeMap<_, _> = step
            .bindings
            .iter()
            .enumerate()
            .map(|(index, binding)| (binding.input_slot.as_str(), (index, binding)))
            .collect();
        let mut resolved = Vec::new();
        for slot in &capability.inputs {
            if let Some((binding_index, binding)) = explicit.get(slot.slot_id.as_str()) {
                let pointer = format!("/workflow/{step_index}/bindings/{binding_index}/source");
                if let SourceRef::StepOutput {
                    step_id: source_step,
                    ..
                } = &binding.source
                    && source_step == &step.step_id
                {
                    findings.push(CoreDiagnostic::new(
                        CORE_R3201,
                        FindingClass::Invalid,
                        "requester",
                        contract_location(pointer),
                        format!("step `{}` depends on its own output", step.step_id),
                    ));
                    continue;
                }
                let Some(candidate) = find_exact_candidate(candidates, &binding.source) else {
                    if produced_by_unknown_type(&binding.source, unknown_type_steps) {
                        continue;
                    }
                    let (code, message) =
                        missing_source_finding(contract, registry, &binding.source);
                    findings.push(CoreDiagnostic::new(
                        code,
                        FindingClass::Missing,
                        "requester",
                        contract_location(pointer),
                        message,
                    ));
                    continue;
                };
                if invalid_sources.contains(&candidate.source) {
                    continue;
                }
                let role_matches = candidate.role == slot.role;
                let media_matches = slot.accepted_media_types.contains(&candidate.media_type);
                if !role_matches {
                    findings.push(CoreDiagnostic::new(
                        CORE_T2101,
                        FindingClass::Invalid,
                        "requester",
                        contract_location(pointer.clone()),
                        format!(
                            "source role `{}@{}` cannot satisfy nominal role `{}@{}`",
                            candidate.role.id, candidate.role.major, slot.role.id, slot.role.major
                        ),
                    ));
                }
                if !media_matches {
                    findings.push(CoreDiagnostic::new(
                        CORE_T2301,
                        FindingClass::Invalid,
                        "requester",
                        contract_location(pointer),
                        format!(
                            "source media type `{}` is not accepted by input slot `{}`",
                            candidate.media_type, slot.slot_id
                        ),
                    ));
                }
                if role_matches && media_matches {
                    add_dependency(&mut result, &step.step_id, &candidate.source);
                    resolved.push(ResolvedBinding {
                        input_slot: slot.slot_id.clone(),
                        source: candidate.source.clone(),
                    });
                }
                continue;
            }

            let matches: Vec<_> = candidates
                .iter()
                .filter(|candidate| !invalid_sources.contains(&candidate.source))
                .filter(|candidate| {
                    !matches!(
                        &candidate.source,
                        SourceRef::StepOutput { step_id, .. } if step_id == &step.step_id
                    )
                })
                .filter(|candidate| candidate.role == slot.role)
                .filter(|candidate| slot.accepted_media_types.contains(&candidate.media_type))
                .collect();
            let blocked_by_invalid_source = candidates.iter().any(|candidate| {
                invalid_sources.contains(&candidate.source) && candidate.role == slot.role
            });
            match matches.as_slice() {
                [candidate] => {
                    add_dependency(&mut result, &step.step_id, &candidate.source);
                    resolved.push(ResolvedBinding {
                        input_slot: slot.slot_id.clone(),
                        source: candidate.source.clone(),
                    });
                }
                [] if slot.required && !blocked_by_invalid_source => findings.push(
                    CoreDiagnostic::new(
                        CORE_R3101,
                        FindingClass::Missing,
                        "requester",
                        logical_input_location(step_index, &slot.slot_id),
                        format!(
                            "required input slot `{}` (role `{}@{}`) has no compatible source",
                            slot.slot_id, slot.role.id, slot.role.major
                        ),
                    )
                    .with_repair(feeding_repair(contract, registry, slot)),
                ),
                [_, _, ..] if slot.required => {
                    let mut repair = DiagnosticRepair::labels(
                        RepairApplicability::ConstrainedChoice,
                        Vec::new(),
                    );
                    for candidate in &matches {
                        repair = repair.alternative(
                            candidate.source.label(),
                            binding_edit(step, step_index, &slot.slot_id, &candidate.source),
                        );
                    }
                    findings.push(
                        CoreDiagnostic::new(
                            CORE_R3102,
                            FindingClass::Invalid,
                            "requester",
                            logical_input_location(step_index, &slot.slot_id),
                            format!(
                                "required input slot `{}` has multiple compatible sources",
                                slot.slot_id
                            ),
                        )
                        .with_repair(repair),
                    );
                }
                _ => {}
            }
        }
        resolved.sort_by(|left, right| left.input_slot.cmp(&right.input_slot));
        result.bindings.insert(step.step_id.clone(), resolved);
    }
    result
}

/// Lists the ways a required slot could be fed under the pinned snapshot: a
/// contract input carrying the slot's role, or a step of any capability type
/// whose output produces that role in a media type the slot accepts. Each
/// alternative carries an edit when the compiler can state its exact bytes;
/// the list is a constrained choice for the requester, not a qualification
/// claim about any of the named types.
pub(super) fn feeding_repair(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    slot: &InputSlotDefinition,
) -> DiagnosticRepair {
    let mut repair = DiagnosticRepair::labels(RepairApplicability::ConstrainedChoice, Vec::new());
    let input_edit = registry.roles.get(&slot.role).and_then(|role| {
        let [media_type] = slot.accepted_media_types.as_slice() else {
            return None;
        };
        let [claim_model] = role.permitted_claim_models.as_slice() else {
            return None;
        };
        let input_id = unique_id(
            &slot.slot_id,
            contract.inputs.iter().map(|input| input.input_id.as_str()),
        );
        let input = serde_json::json!({
            "input_id": input_id,
            "role": slot.role,
            "media_type": media_type,
            "claim_model": claim_model,
        });
        Some(append_edit("/inputs", contract.inputs.is_empty(), input))
    });
    repair = repair.alternative(
        format!("declare_input:{}@{}", slot.role.id, slot.role.major),
        input_edit.unwrap_or_default(),
    );
    for (reference, capability) in &registry.capability_types {
        for output in &capability.outputs {
            if output.role == slot.role && slot.accepted_media_types.contains(&output.media_type) {
                let step_id = unique_id(
                    reference.id.rsplit('.').next().unwrap_or(&reference.id),
                    contract.workflow.iter().map(|step| step.step_id.as_str()),
                );
                let step = serde_json::json!({
                    "step_id": step_id,
                    "capability_type": reference,
                });
                repair = repair.alternative(
                    format!(
                        "add_step:{}@{}/{}",
                        reference.id, reference.major, output.slot_id
                    ),
                    append_edit("/workflow", contract.workflow.is_empty(), step),
                );
            }
        }
    }
    repair
}

/// The edit that binds `slot_id` of the step at `step_index` to `source`.
fn binding_edit(
    step: &WorkflowStep,
    step_index: usize,
    slot_id: &str,
    source: &SourceRef,
) -> Vec<RepairEdit> {
    let binding = serde_json::json!({ "input_slot": slot_id, "source": source });
    append_edit(
        &format!("/workflow/{step_index}/bindings"),
        step.bindings.is_empty(),
        binding,
    )
}

/// Appends `value` to the array at `path`. RFC 6902 `add` on an absent or
/// existing member sets it, so an empty array is written whole; a non-empty
/// one takes the `-` append form.
fn append_edit(path: &str, empty: bool, value: serde_json::Value) -> Vec<RepairEdit> {
    if empty {
        vec![RepairEdit::Add {
            path: path.to_owned(),
            value: serde_json::Value::Array(vec![value]),
        }]
    } else {
        vec![RepairEdit::Add {
            path: format!("{path}/-"),
            value,
        }]
    }
}

fn unique_id<'a>(preferred: &str, taken: impl Iterator<Item = &'a str>) -> String {
    let taken: Vec<&str> = taken.collect();
    if !taken.contains(&preferred) {
        return preferred.to_owned();
    }
    (2..)
        .map(|suffix| format!("{preferred}_{suffix}"))
        .find(|candidate| !taken.contains(&candidate.as_str()))
        .expect("an unused suffix exists")
}

/// A source is suppressed when it names an output of a step whose capability
/// type is absent from the snapshot. That root cause is already reported at the
/// step's `capability_type`; the reference would resolve once the type exists.
pub(super) fn produced_by_unknown_type(
    source: &SourceRef,
    unknown_type_steps: &BTreeSet<&str>,
) -> bool {
    matches!(
        source,
        SourceRef::StepOutput { step_id, .. } if unknown_type_steps.contains(step_id.as_str())
    )
}

/// Explains why a referenced source has no candidate, distinguishing an
/// undeclared input, a nonexistent step, and a known step with no such output.
pub(super) fn missing_source_finding(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    source: &SourceRef,
) -> (&'static str, String) {
    match source {
        SourceRef::ContractInput { input_id } => (
            CORE_R3101,
            format!("contract input `{input_id}` is not declared"),
        ),
        SourceRef::StepOutput {
            step_id,
            output_slot,
        } => {
            let Some(step) = contract
                .workflow
                .iter()
                .find(|step| &step.step_id == step_id)
            else {
                return (
                    CORE_R3203,
                    format!("workflow step `{step_id}` does not exist"),
                );
            };
            let declared: Vec<_> = registry
                .capability_types
                .get(&step.capability_type)
                .map(|capability| {
                    capability
                        .outputs
                        .iter()
                        .map(|output| output.slot_id.clone())
                        .collect()
                })
                .unwrap_or_default();
            (
                CORE_R3203,
                format!(
                    "step `{step_id}` of capability type `{}@{}` declares no output slot `{output_slot}`; declared outputs: {declared:?}",
                    step.capability_type.id, step.capability_type.major
                ),
            )
        }
    }
}

pub(super) fn add_dependency(
    resolution: &mut WorkflowResolution,
    destination_step: &str,
    source: &SourceRef,
) {
    if let SourceRef::StepOutput { step_id, .. } = source {
        resolution
            .dependencies
            .entry(destination_step.into())
            .or_default()
            .insert(step_id.clone());
    }
}

pub(super) fn validate_graph(
    contract: &ContractSource,
    dependencies: &BTreeMap<String, BTreeSet<String>>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Vec<String> {
    if contract.workflow.len() > MAX_WORKFLOW_STEPS {
        return Vec::new();
    }
    let mut indegrees: BTreeMap<String, usize> = dependencies
        .iter()
        .map(|(step, values)| (step.clone(), values.len()))
        .collect();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (step, values) in dependencies {
        for dependency in values {
            dependents.entry(dependency).or_default().push(step);
        }
    }
    for values in dependents.values_mut() {
        values.sort_unstable();
    }
    let mut ready: BTreeSet<String> = indegrees
        .iter()
        .filter_map(|(step, degree)| (*degree == 0).then_some(step.clone()))
        .collect();
    let mut order = Vec::with_capacity(indegrees.len());
    while let Some(step) = ready.pop_first() {
        order.push(step.clone());
        for dependent in dependents.get(step.as_str()).into_iter().flatten() {
            let degree = indegrees
                .get_mut(*dependent)
                .expect("dependents are built from known workflow steps");
            *degree -= 1;
            if *degree == 0 {
                ready.insert((*dependent).into());
            }
        }
    }

    if order.len() != indegrees.len() {
        let cyclic = cyclic_nodes(dependencies, &indegrees);
        let primary_step = cyclic
            .first()
            .expect("a failed topological ordering has a cyclic node");
        let primary_index = step_index(contract, primary_step).unwrap_or(0);
        let related = cyclic
            .iter()
            .skip(1)
            .filter_map(|step| step_index(contract, step))
            .map(|index| contract_location(format!("/workflow/{index}/bindings")))
            .collect();
        findings.push(
            CoreDiagnostic::new(
                CORE_R3202,
                FindingClass::Invalid,
                "requester",
                contract_location(format!("/workflow/{primary_index}/bindings")),
                format!("workflow bindings contain a dependency cycle: {cyclic:?}"),
            )
            .with_related(related),
        );
    }
    order
}

pub(super) fn cyclic_nodes(
    dependencies: &BTreeMap<String, BTreeSet<String>>,
    indegrees: &BTreeMap<String, usize>,
) -> Vec<String> {
    let remaining: BTreeSet<_> = indegrees
        .iter()
        .filter_map(|(step, degree)| (*degree > 0).then_some(step.clone()))
        .collect();
    remaining
        .iter()
        .filter(|start| can_reach_self(start, dependencies, &remaining))
        .cloned()
        .collect()
}

pub(super) fn can_reach_self(
    start: &str,
    dependencies: &BTreeMap<String, BTreeSet<String>>,
    permitted: &BTreeSet<String>,
) -> bool {
    let mut seen = BTreeSet::new();
    let mut queue: VecDeque<&str> = dependencies
        .get(start)
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    while let Some(step) = queue.pop_front() {
        if step == start {
            return true;
        }
        if !permitted.contains(step) || !seen.insert(step) {
            continue;
        }
        queue.extend(
            dependencies
                .get(step)
                .into_iter()
                .flatten()
                .map(String::as_str),
        );
    }
    false
}

pub(super) fn find_exact_candidate<'a>(
    candidates: &'a [Candidate],
    source: &SourceRef,
) -> Option<&'a Candidate> {
    candidates
        .iter()
        .find(|candidate| &candidate.source == source)
}

pub(super) fn step_index(contract: &ContractSource, step_id: &str) -> Option<usize> {
    contract
        .workflow
        .iter()
        .position(|step| step.step_id == step_id)
}
