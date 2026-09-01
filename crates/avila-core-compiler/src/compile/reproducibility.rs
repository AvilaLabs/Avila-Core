//! Type-level determinism, seeds, and material execution factors.

use super::findings::{contract_location, material_factor_location, seed_location};
use super::ir::CompiledReproducibility;
use super::registry::RegistryIndex;
use super::values::{NOT_DEFINED_PLACEHOLDER, lower_typed_value};
use crate::diagnostic::{
    CORE_A4301, CORE_S1101, CORE_T2501, CoreDiagnostic, DiagnosticRepair, FindingClass,
    RepairApplicability,
};
use crate::document::{
    CapabilityTypeDefinition, ContractSource, DeterminismClass, ExecutionPolicy,
    ParameterDefinition,
};
use std::collections::BTreeMap;

pub(super) fn compile_reproducibility(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> BTreeMap<String, CompiledReproducibility> {
    let mut compiled_steps = BTreeMap::new();
    for (step_index, step) in contract.workflow.iter().enumerate() {
        let Some(capability) = registry.capability_types.get(&step.capability_type) else {
            continue;
        };
        let declaration = &capability.reproducibility;

        if declaration.determinism == DeterminismClass::Nondeterministic
            && !nondeterminism_permitted(&contract.execution_policy, capability)
        {
            findings.push(
                CoreDiagnostic::new(
                    CORE_A4301,
                    FindingClass::Inadmissible,
                    "policy_owner",
                    contract_location(format!("/workflow/{step_index}/capability_type")),
                    format!(
                        "capability type `{}@{}` is nondeterministic, but the contract execution policy does not permit nondeterminism for every produced role",
                        step.capability_type.id, step.capability_type.major
                    ),
                )
                .with_related(vec![contract_location(
                    "/execution_policy/permitted_nondeterministic_roles",
                )]),
            );
        }

        let seed = match declaration.determinism {
            DeterminismClass::SeededStochastic => match step.reproducibility.seed.as_deref() {
                Some(seed) if !seed.trim().is_empty() && seed != NOT_DEFINED_PLACEHOLDER => {
                    Some(seed.to_owned())
                }
                _ => {
                    findings.push(CoreDiagnostic::new(
                        CORE_T2501,
                        FindingClass::Missing,
                        "requester",
                        seed_location(step_index),
                        "seeded-stochastic capability requires an explicit nonempty seed",
                    ));
                    None
                }
            },
            DeterminismClass::Deterministic | DeterminismClass::Nondeterministic => {
                if step.reproducibility.seed.is_some() {
                    findings.push(CoreDiagnostic::new(
                        CORE_T2501,
                        FindingClass::Invalid,
                        "requester",
                        seed_location(step_index),
                        format!(
                            "a seed is not part of the invocation identity for a `{}` capability",
                            determinism_label(declaration.determinism)
                        ),
                    ));
                }
                None
            }
        };

        let definitions: BTreeMap<_, _> = declaration
            .material_factors
            .iter()
            .map(|factor| (factor.factor_id.as_str(), factor))
            .collect();
        for factor_id in step.reproducibility.material_factors.keys() {
            if !definitions.contains_key(factor_id.as_str()) {
                findings.push(
                    CoreDiagnostic::new(
                    CORE_S1101,
                    FindingClass::Invalid,
                    "requester",
                    material_factor_location(step_index, factor_id),
                    format!(
                        "capability type `{}@{}` does not declare material execution factor `{factor_id}`",
                        step.capability_type.id, step.capability_type.major
                    ),
                    )
                    .with_repair(DiagnosticRepair::removal(
                    RepairApplicability::ConstrainedChoice,
                    format!("remove material execution factor `{factor_id}`"),
                    &material_factor_location(step_index, factor_id).pointer,
                    )),
                    );
            }
        }

        let mut material_factors = BTreeMap::new();
        for factor in &declaration.material_factors {
            let location = material_factor_location(step_index, &factor.factor_id);
            let Some(authored) = step.reproducibility.material_factors.get(&factor.factor_id)
            else {
                findings.push(CoreDiagnostic::new(
                    CORE_T2501,
                    FindingClass::Missing,
                    "requester",
                    location,
                    format!(
                        "material execution factor `{}` is not bound",
                        factor.factor_id
                    ),
                ));
                continue;
            };
            if authored.as_str() == Some(NOT_DEFINED_PLACEHOLDER) {
                findings.push(CoreDiagnostic::new(
                    CORE_T2501,
                    FindingClass::Missing,
                    "requester",
                    location,
                    format!(
                        "material execution factor `{}` remains explicitly not defined",
                        factor.factor_id
                    ),
                ));
                continue;
            }
            let typed_definition = ParameterDefinition {
                parameter_id: factor.factor_id.clone(),
                required: true,
                value_type: factor.value_type.clone(),
            };
            if let Some(value) = lower_typed_value(
                "material execution factor",
                &typed_definition,
                authored,
                location,
                &registry.kinds,
                findings,
            ) {
                material_factors.insert(factor.factor_id.clone(), value);
            }
        }

        compiled_steps.insert(
            step.step_id.clone(),
            CompiledReproducibility {
                determinism: declaration.determinism,
                seed,
                material_factors,
            },
        );
    }
    compiled_steps
}

pub(super) fn nondeterminism_permitted(
    policy: &ExecutionPolicy,
    capability: &CapabilityTypeDefinition,
) -> bool {
    !capability.outputs.is_empty()
        && capability.outputs.iter().all(|output| {
            policy
                .permitted_nondeterministic_roles
                .contains(&output.role)
        })
}

pub(super) const fn determinism_label(determinism: DeterminismClass) -> &'static str {
    match determinism {
        DeterminismClass::Deterministic => "deterministic",
        DeterminismClass::SeededStochastic => "seeded_stochastic",
        DeterminismClass::Nondeterministic => "nondeterministic",
    }
}
