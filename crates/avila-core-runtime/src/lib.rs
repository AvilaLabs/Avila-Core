//! Deterministic planning for Avila Core campaigns.
//!
//! Process execution is intentionally outside this initial scaffold. A plan may
//! identify an available process capability, but only a future reviewed runner
//! may execute it and produce evidence.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use avila_core_model::{
    CapabilityManifest, CapabilityMaturity, CapabilityRequest, EvidenceContract, ExecutionSpec,
    ModelError, QualificationStatus,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Default)]
pub struct CampaignPlanner;

impl CampaignPlanner {
    pub fn plan(
        &self,
        contract: &EvidenceContract,
        manifests: &[CapabilityManifest],
    ) -> Result<CampaignPlan, PlannerError> {
        contract.validate()?;
        let mut manifest_ids = BTreeSet::new();
        for manifest in manifests {
            manifest.validate()?;
            if !manifest_ids.insert(manifest.capability_id.as_str()) {
                return Err(PlannerError::DuplicateCapability(
                    manifest.capability_id.clone(),
                ));
            }
        }

        let order = topological_order(&contract.workflow)?;
        let requests: BTreeMap<_, _> = contract
            .workflow
            .iter()
            .map(|request| (request.step_id.as_str(), request))
            .collect();
        let mut planned_states: BTreeMap<String, StepState> = BTreeMap::new();
        let mut steps = Vec::with_capacity(order.len());

        for step_id in order {
            let request = requests
                .get(step_id.as_str())
                .expect("topological order only contains known steps");
            let selection = select_capability(request, manifests, contract);

            let dependency_block = request.depends_on.iter().find_map(|dependency| {
                match planned_states.get(dependency) {
                    Some(StepState::Blocked { .. }) => Some(format!(
                        "dependency `{dependency}` is blocked; downstream execution is inadmissible"
                    )),
                    _ => None,
                }
            });
            let state = dependency_block.unwrap_or_else(|| selection.state.reason_or_empty());
            let state = if state.is_empty() {
                selection.state
            } else {
                StepState::Blocked { reason: state }
            };

            planned_states.insert(step_id.clone(), state.clone());
            steps.push(CampaignStep {
                step_id,
                capability_type: request.capability_type.clone(),
                capability_id: selection.capability_id,
                provider: selection.provider,
                depends_on: request.depends_on.clone(),
                state,
            });
        }

        let status = if steps
            .iter()
            .all(|step| matches!(step.state, StepState::Ready))
        {
            PlanStatus::Ready
        } else {
            PlanStatus::Blocked
        };

        Ok(CampaignPlan {
            contract_id: contract.contract_id.clone(),
            status,
            steps,
            notice: "Planning only: no capability was executed and no scientific result exists."
                .into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignPlan {
    pub contract_id: String,
    pub status: PlanStatus,
    pub steps: Vec<CampaignStep>,
    pub notice: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Ready,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignStep {
    pub step_id: String,
    pub capability_type: String,
    #[serde(default)]
    pub capability_id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    pub depends_on: Vec<String>,
    pub state: StepState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum StepState {
    Ready,
    Blocked { reason: String },
}

impl StepState {
    fn reason_or_empty(&self) -> String {
        match self {
            Self::Ready => String::new(),
            Self::Blocked { reason } => reason.clone(),
        }
    }
}

struct Selection {
    capability_id: Option<String>,
    provider: Option<String>,
    state: StepState,
}

fn select_capability(
    request: &CapabilityRequest,
    manifests: &[CapabilityManifest],
    contract: &EvidenceContract,
) -> Selection {
    let mut candidates: Vec<_> = manifests
        .iter()
        .filter(|manifest| manifest.capability_type == request.capability_type)
        .filter(|manifest| {
            request
                .implementation
                .as_ref()
                .is_none_or(|requested| requested == &manifest.capability_id)
        })
        .filter(|manifest| manifest.maturity != CapabilityMaturity::Retired)
        .collect();
    candidates.sort_by(|left, right| {
        maturity_rank(right.maturity)
            .cmp(&maturity_rank(left.maturity))
            .then_with(|| left.capability_id.cmp(&right.capability_id))
    });

    let Some(manifest) = candidates.first() else {
        return Selection {
            capability_id: None,
            provider: None,
            state: StepState::Blocked {
                reason: format!(
                    "no capability manifest satisfies type `{}`{}",
                    request.capability_type,
                    request
                        .implementation
                        .as_ref()
                        .map(|id| format!(" and implementation `{id}`"))
                        .unwrap_or_default()
                ),
            },
        };
    };

    let identity = || Selection {
        capability_id: Some(manifest.capability_id.clone()),
        provider: Some(manifest.provider.clone()),
        state: StepState::Ready,
    };

    if !contract.evidence_policy.allow_unqualified_capabilities
        && (manifest.maturity != CapabilityMaturity::Qualified
            || manifest.qualification.status != QualificationStatus::Qualified)
    {
        return Selection {
            state: StepState::Blocked {
                reason: format!(
                    "capability `{}` is not qualified under this contract's evidence policy",
                    manifest.capability_id
                ),
            },
            ..identity()
        };
    }

    if let ExecutionSpec::Unavailable { reason } = &manifest.execution {
        return Selection {
            state: StepState::Blocked {
                reason: format!(
                    "capability `{}` is declared unavailable: {reason}",
                    manifest.capability_id
                ),
            },
            ..identity()
        };
    }

    identity()
}

const fn maturity_rank(maturity: CapabilityMaturity) -> u8 {
    match maturity {
        CapabilityMaturity::Specimen => 0,
        CapabilityMaturity::Experimental => 1,
        CapabilityMaturity::Qualified => 2,
        CapabilityMaturity::Retired => 0,
    }
}

fn topological_order(workflow: &[CapabilityRequest]) -> Result<Vec<String>, PlannerError> {
    let mut indegree: BTreeMap<String, usize> = workflow
        .iter()
        .map(|request| (request.step_id.clone(), request.depends_on.len()))
        .collect();
    let mut dependents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for request in workflow {
        for dependency in &request.depends_on {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(request.step_id.clone());
        }
    }

    let mut ready: BTreeSet<String> = indegree
        .iter()
        .filter_map(|(step, degree)| (*degree == 0).then_some(step.clone()))
        .collect();
    let mut order = Vec::with_capacity(workflow.len());

    while let Some(step) = ready.pop_first() {
        order.push(step.clone());
        for dependent in dependents.get(&step).into_iter().flatten() {
            let degree = indegree
                .get_mut(dependent)
                .expect("dependent was drawn from known workflow");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(dependent.clone());
            }
        }
    }

    if order.len() != workflow.len() {
        let cycle_members = indegree
            .into_iter()
            .filter_map(|(step, degree)| (degree > 0).then_some(step))
            .collect();
        return Err(PlannerError::DependencyCycle(cycle_members));
    }
    Ok(order)
}

#[derive(Debug, Error)]
pub enum PlannerError {
    #[error(transparent)]
    InvalidModel(#[from] ModelError),
    #[error("duplicate capability manifest `{0}`")]
    DuplicateCapability(String),
    #[error("workflow dependency cycle includes: {0:?}")]
    DependencyCycle(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specimen_contract() -> EvidenceContract {
        serde_json::from_str(include_str!(
            "../../../examples/contracts/shutdown-dose-specimen.json"
        ))
        .unwrap()
    }

    fn specimen_manifests() -> Vec<CapabilityManifest> {
        [
            include_str!("../../../examples/capabilities/openmc-transport.specimen.json"),
            include_str!("../../../examples/capabilities/actinv-activation.specimen.json"),
            include_str!("../../../examples/capabilities/avila-dose.specimen.json"),
            include_str!("../../../examples/capabilities/avify-bounds.specimen.json"),
            include_str!("../../../examples/capabilities/core-requirement.specimen.json"),
        ]
        .into_iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect()
    }

    #[test]
    fn specimen_plan_is_explicitly_blocked() {
        let plan = CampaignPlanner
            .plan(&specimen_contract(), &specimen_manifests())
            .unwrap();
        assert_eq!(plan.status, PlanStatus::Blocked);
        assert!(
            plan.steps
                .iter()
                .all(|step| matches!(step.state, StepState::Blocked { .. }))
        );
    }

    #[test]
    fn dependency_cycle_is_rejected() {
        let mut contract = specimen_contract();
        contract.workflow[0].depends_on = vec!["evaluate-requirement".into()];
        let error = CampaignPlanner
            .plan(&contract, &specimen_manifests())
            .unwrap_err();
        assert!(matches!(error, PlannerError::DependencyCycle(_)));
    }
}
