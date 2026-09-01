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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicy {
    #[serde(default)]
    pub permitted_nondeterministic_roles: Vec<VersionedRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSource {
    pub schema_version: String,
    pub semantic_profile: String,
    pub contract_id: String,
    pub revision: u64,
    pub status: ContractStatus,
    /// The bounded question the contract exists to resolve. Prose the
    /// compiler carries into the compiled boundary and never interprets.
    pub question: String,
    /// Conditions accepted without being established by the campaign. Prose,
    /// never interpreted; typed facts with provenance are a separate record.
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub execution_policy: ExecutionPolicy,
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
    #[serde(default)]
    pub reproducibility: ReproducibilityBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewPolicyBinding>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReproducibilityBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(default)]
    pub material_factors: BTreeMap<String, Value>,
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
    pub purpose: VersionedRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<SourceRef>,
    pub comparison: Comparison,
    pub limit: TypedQuantity,
    /// Required by, and only meaningful for, an `equal` comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<TypedQuantity>,
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
    pub purposes: Vec<PurposeDefinition>,
    pub roles: Vec<RoleDefinition>,
    pub capability_types: Vec<CapabilityTypeDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurposeDefinition {
    pub purpose: VersionedRef,
    pub owner: String,
    pub description: String,
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
    pub reproducibility: ReproducibilityDeclaration,
    #[serde(default)]
    pub inputs: Vec<InputSlotDefinition>,
    #[serde(default)]
    pub outputs: Vec<OutputSlotDefinition>,
    #[serde(default)]
    pub parameters: Vec<ParameterDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewDeclaration>,
    #[serde(default)]
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDeclaration {
    pub presented_input_slots: Vec<String>,
    pub decision_output_slot: String,
    pub allowed_dispositions: Vec<ReviewDisposition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDisposition {
    ApproveForUse,
    RejectForUse,
    RequestChanges,
    Abstain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPolicyBinding {
    pub reviewer_eligibility_policy: ImmutablePolicyRef,
    pub independence: ReviewIndependence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImmutablePolicyRef {
    pub policy_id: String,
    pub revision: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewIndependence {
    None,
    Constraints {
        requirements: Vec<IndependenceRequirement>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependenceRequirement {
    pub separated_from: ReviewParty,
    pub minimum_separation: SeparationLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewParty {
    Requester,
    MethodOwner,
    CapabilityProvider,
    Executor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeparationLevel {
    DifferentPerson,
    DifferentOrganization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReproducibilityDeclaration {
    pub determinism: DeterminismClass,
    #[serde(default)]
    pub material_factors: Vec<ExecutionFactorDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismClass {
    Deterministic,
    SeededStochastic,
    Nondeterministic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFactorDefinition {
    pub factor_id: String,
    pub value_type: ParameterType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterDefinition {
    pub parameter_id: String,
    pub required: bool,
    pub value_type: ParameterType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParameterType {
    Boolean,
    Integer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<IntegerBound>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<IntegerBound>,
    },
    ExactNumber {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<ExactBound>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<ExactBound>,
    },
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        allowed_values: Option<Vec<String>>,
    },
    Quantity {
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<QuantityBound>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<QuantityBound>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegerBound {
    pub value: i64,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactBound {
    pub value: ExactNumber,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityBound {
    pub value: QuantityValue,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityValue {
    pub value: ExactNumber,
    pub unit: String,
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
    #[serde(default)]
    pub excluded_purposes: Vec<VersionedRef>,
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
        question: String::new(),
        assumptions: Vec::new(),
        execution_policy: ExecutionPolicy::default(),
        inputs: Vec::new(),
        workflow: Vec::new(),
        requirements: Vec::new(),
    }
}
