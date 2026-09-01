use std::collections::BTreeMap;

use avila_core_kernel::{ExactNumber, SEMANTIC_PROFILE};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CONTRACT_SCHEMA_VERSION: &str = "avila.core/evidence-contract/v0.2-draft";
pub const REGISTRY_SCHEMA_VERSION: &str = "avila.core/registry-snapshot/v0.2-draft";
pub const COMPILE_REPORT_SCHEMA_VERSION: &str = "avila.core/compile-report/v0.2-draft";
pub const COMPILED_CONTRACT_SCHEMA_VERSION: &str = "avila.core/compiled-contract/v0.2-draft";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionedRef {
    pub id: String,
    pub major: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Draft,
    InReview,
    Approved,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSource {
    pub schema_version: String,
    pub semantic_profile: String,
    pub contract_id: String,
    pub revision: u64,
    pub status: ContractStatus,
    #[serde(default)]
    pub inputs: Vec<ContractInput>,
    pub workflow: Vec<WorkflowStep>,
    pub requirements: Vec<RequirementSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractInput {
    pub input_id: String,
    pub role: VersionedRef,
    pub media_type: String,
    pub claim_model: ClaimModelDeclaration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowStep {
    pub step_id: String,
    pub capability_type: VersionedRef,
    #[serde(default)]
    pub bindings: Vec<AuthoredBinding>,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredBinding {
    pub input_slot: String,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceRef {
    ContractInput {
        input_id: String,
    },
    StepOutput {
        step_id: String,
        output_slot: String,
    },
}

impl SourceRef {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::ContractInput { input_id } => format!("input:{input_id}"),
            Self::StepOutput {
                step_id,
                output_slot,
            } => format!("step:{step_id}/{output_slot}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementSource {
    pub requirement_id: String,
    pub statement: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<SourceRef>,
    pub comparison: Comparison,
    pub limit: TypedQuantity,
    pub basis: RequirementBasis,
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
pub struct TypedQuantity {
    pub kind: String,
    pub value: ExactNumber,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementBasis {
    pub kind: BasisKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BasisKind {
    Bounded,
    Enclosure,
    Nominal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrySnapshot {
    pub schema_version: String,
    pub semantic_profile: String,
    pub registry_id: String,
    pub revision: u64,
    pub kinds: Vec<KindRecord>,
    pub roles: Vec<RoleDefinition>,
    pub capability_types: Vec<CapabilityTypeDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindRecord {
    pub kind_id: String,
    pub canonical_unit: String,
    pub unit_class: String,
    pub owner: String,
    pub units: Vec<UnitRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRecord {
    pub symbol: String,
    pub factor: ExactNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDefinition {
    pub role: VersionedRef,
    pub owner: String,
    pub validator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_class: Option<String>,
    pub accepted_media_types: Vec<String>,
    pub permitted_claim_models: Vec<ClaimModelDeclaration>,
    #[serde(default)]
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityTypeDefinition {
    pub capability_type: VersionedRef,
    pub owner: String,
    #[serde(default)]
    pub inputs: Vec<InputSlotDefinition>,
    #[serde(default)]
    pub outputs: Vec<OutputSlotDefinition>,
    #[serde(default)]
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSlotDefinition {
    pub slot_id: String,
    pub role: VersionedRef,
    pub accepted_media_types: Vec<String>,
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSlotDefinition {
    pub slot_id: String,
    pub role: VersionedRef,
    pub media_type: String,
    pub permitted_claim_models: Vec<ClaimModelDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClaimModelDeclaration {
    Exact,
    Interval {
        #[serde(default)]
        nominal: bool,
    },
    CoverageInterval,
    WorstCase {
        side: BoundSide,
        #[serde(default)]
        nominal: bool,
    },
    StandardUncertainty,
    Samples,
    Distribution,
    Unquantified,
}

impl ClaimModelDeclaration {
    #[must_use]
    pub const fn is_kernel_irreducible(&self) -> bool {
        matches!(
            self,
            Self::StandardUncertainty | Self::Samples | Self::Distribution
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundSide {
    Lower,
    Upper,
}

const fn default_true() -> bool {
    true
}

pub fn current_profile_contract(contract_id: impl Into<String>) -> ContractSource {
    ContractSource {
        schema_version: CONTRACT_SCHEMA_VERSION.into(),
        semantic_profile: SEMANTIC_PROFILE.into(),
        contract_id: contract_id.into(),
        revision: 1,
        status: ContractStatus::Draft,
        inputs: Vec::new(),
        workflow: Vec::new(),
        requirements: Vec::new(),
    }
}
