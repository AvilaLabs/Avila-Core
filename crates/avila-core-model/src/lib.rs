//! Authoritative domain objects for Avila Core.
//!
//! This crate contains no graphical interface, solver implementation, or
//! scientific claim. It defines the documents that those components exchange.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const CONTRACT_SCHEMA_VERSION: &str = "avila.core/evidence-contract/v0.1";
pub const CAPABILITY_SCHEMA_VERSION: &str = "avila.core/capability-manifest/v0.1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceContract {
    pub schema_version: String,
    pub contract_id: String,
    pub title: String,
    pub question: String,
    pub status: ContractStatus,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<InputReference>,
    pub workflow: Vec<CapabilityRequest>,
    pub requirements: Vec<Requirement>,
    pub evidence_policy: EvidencePolicy,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl EvidenceContract {
    /// Performs structural validation only. Scientific validity belongs to
    /// qualified capabilities and review policy, never to this method.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version != CONTRACT_SCHEMA_VERSION {
            return Err(ModelError::UnsupportedSchema {
                expected: CONTRACT_SCHEMA_VERSION,
                actual: self.schema_version.clone(),
            });
        }
        require_nonempty("contract_id", &self.contract_id)?;
        require_nonempty("title", &self.title)?;
        require_nonempty("question", &self.question)?;
        if self.workflow.is_empty() {
            return Err(ModelError::EmptyCollection("workflow"));
        }
        if self.requirements.is_empty() {
            return Err(ModelError::EmptyCollection("requirements"));
        }

        ensure_unique(
            "workflow step",
            self.workflow.iter().map(|step| step.step_id.as_str()),
        )?;
        ensure_unique(
            "requirement",
            self.requirements
                .iter()
                .map(|item| item.requirement_id.as_str()),
        )?;
        ensure_unique(
            "input",
            self.inputs.iter().map(|item| item.input_id.as_str()),
        )?;

        let step_ids: BTreeSet<_> = self
            .workflow
            .iter()
            .map(|step| step.step_id.as_str())
            .collect();
        for step in &self.workflow {
            require_nonempty("workflow.step_id", &step.step_id)?;
            require_nonempty("workflow.capability_type", &step.capability_type)?;
            for dependency in &step.depends_on {
                if dependency == &step.step_id {
                    return Err(ModelError::SelfDependency(step.step_id.clone()));
                }
                if !step_ids.contains(dependency.as_str()) {
                    return Err(ModelError::UnknownDependency {
                        step: step.step_id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }

        for requirement in &self.requirements {
            require_nonempty("requirements.requirement_id", &requirement.requirement_id)?;
            require_nonempty("requirements.statement", &requirement.statement)?;
            require_nonempty("requirements.metric", &requirement.metric)?;
            require_nonempty("requirements.unit", &requirement.unit)?;
            if !requirement.limit.is_finite() {
                return Err(ModelError::NonFiniteLimit(
                    requirement.requirement_id.clone(),
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Draft,
    InReview,
    Approved,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputReference {
    pub input_id: String,
    pub role: String,
    pub uri: String,
    pub media_type: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequest {
    pub step_id: String,
    pub capability_type: String,
    #[serde(default)]
    pub implementation: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub requirement_id: String,
    pub statement: String,
    pub metric: String,
    pub comparison: Comparison,
    pub limit: f64,
    pub unit: String,
    #[serde(default)]
    pub evaluation_step: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePolicy {
    #[serde(default)]
    pub allow_unqualified_capabilities: bool,
    #[serde(default = "default_true")]
    pub require_complete_lineage: bool,
    #[serde(default = "default_true")]
    pub require_content_hashes: bool,
    #[serde(default)]
    pub required_review_roles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityManifest {
    pub schema_version: String,
    pub capability_id: String,
    pub capability_type: String,
    pub name: String,
    pub provider: String,
    pub version: String,
    pub maturity: CapabilityMaturity,
    pub execution: ExecutionSpec,
    #[serde(default)]
    pub accepts: Vec<String>,
    #[serde(default)]
    pub produces: Vec<String>,
    pub qualification: Qualification,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl CapabilityManifest {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version != CAPABILITY_SCHEMA_VERSION {
            return Err(ModelError::UnsupportedSchema {
                expected: CAPABILITY_SCHEMA_VERSION,
                actual: self.schema_version.clone(),
            });
        }
        require_nonempty("capability_id", &self.capability_id)?;
        require_nonempty("capability_type", &self.capability_type)?;
        require_nonempty("name", &self.name)?;
        require_nonempty("provider", &self.provider)?;
        require_nonempty("version", &self.version)?;
        if self.maturity == CapabilityMaturity::Qualified
            && self.qualification.status != QualificationStatus::Qualified
        {
            return Err(ModelError::InconsistentQualification(
                self.capability_id.clone(),
            ));
        }
        if let ExecutionSpec::Process { program, .. } = &self.execution {
            require_nonempty("execution.program", program)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMaturity {
    Specimen,
    Experimental,
    Qualified,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutionSpec {
    Unavailable {
        reason: String,
    },
    Process {
        program: String,
        #[serde(default)]
        arguments: Vec<String>,
    },
}

impl ExecutionSpec {
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Process { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Qualification {
    pub status: QualificationStatus,
    pub scope: String,
    #[serde(default)]
    pub limitations: Vec<String>,
    #[serde(default)]
    pub evidence_uri: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationStatus {
    NotAssessed,
    InReview,
    Qualified,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    Pass,
    Fail,
    Inconclusive,
    NotEvaluated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementVerdict {
    pub requirement_id: String,
    pub status: VerdictStatus,
    #[serde(default)]
    pub observed_value: Option<f64>,
    #[serde(default)]
    pub bound: Option<Bound>,
    pub unit: String,
    pub rationale: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    #[serde(default)]
    pub lower: Option<f64>,
    #[serde(default)]
    pub upper: Option<f64>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ModelError {
    #[error("unsupported schema `{actual}`; expected `{expected}`")]
    UnsupportedSchema {
        expected: &'static str,
        actual: String,
    },
    #[error("`{0}` must not be empty")]
    EmptyField(&'static str),
    #[error("`{0}` must contain at least one item")]
    EmptyCollection(&'static str),
    #[error("duplicate {kind} identifier `{id}`")]
    DuplicateIdentifier { kind: &'static str, id: String },
    #[error("workflow step `{0}` depends on itself")]
    SelfDependency(String),
    #[error("workflow step `{step}` refers to unknown dependency `{dependency}`")]
    UnknownDependency { step: String, dependency: String },
    #[error("requirement `{0}` has a non-finite limit")]
    NonFiniteLimit(String),
    #[error("capability `{0}` claims qualified maturity without a qualified record")]
    InconsistentQualification(String),
}

fn require_nonempty(field: &'static str, value: &str) -> Result<(), ModelError> {
    if value.trim().is_empty() {
        Err(ModelError::EmptyField(field))
    } else {
        Ok(())
    }
}

fn ensure_unique<'a>(
    kind: &'static str,
    values: impl Iterator<Item = &'a str>,
) -> Result<(), ModelError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ModelError::DuplicateIdentifier {
                kind,
                id: value.to_owned(),
            });
        }
    }
    Ok(())
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specimen_contract_deserializes_and_validates() {
        let contract: EvidenceContract = serde_json::from_str(include_str!(
            "../../../examples/contracts/shutdown-dose-specimen.json"
        ))
        .expect("specimen contract should deserialize");
        contract
            .validate()
            .expect("specimen contract should validate");
        assert_eq!(contract.status, ContractStatus::Draft);
    }

    #[test]
    fn specimen_capability_deserializes_and_validates() {
        let manifest: CapabilityManifest = serde_json::from_str(include_str!(
            "../../../examples/capabilities/openmc-transport.specimen.json"
        ))
        .expect("specimen manifest should deserialize");
        manifest
            .validate()
            .expect("specimen manifest should validate");
        assert!(!manifest.execution.is_available());
    }
}
