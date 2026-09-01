//! Compile reports and the immutable compiled-contract IR.

use crate::diagnostic::CoreDiagnostic;
use crate::document::{
    Comparison, ContractInput, DeterminismClass, ExecutionPolicy, ImmutablePolicyRef,
    RequirementBasis, ReviewDisposition, ReviewIndependence, SourceRef, VersionedRef,
};
use serde::Serialize;
use std::collections::BTreeMap;
use thiserror::Error;

pub const COMPILE_NOTICE: &str = "Compilation establishes structural and semantic consistency under the named draft profile only. It performs no execution, review fulfillment, reviewer-eligibility or trust evaluation, evidence admission, scientific qualification, or requirement verdict.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilationStatus {
    Compiled,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentIdentity {
    pub document: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompileReport {
    pub schema_version: String,
    pub semantic_profile: String,
    pub status: CompilationStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_identities: Vec<DocumentIdentity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<CoreDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled: Option<CompiledContract>,
    pub notice: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledContract {
    pub schema_version: String,
    pub semantic_profile: String,
    pub compiler: String,
    pub contract_id: String,
    pub contract_revision: u64,
    pub contract_sha256: String,
    pub question: String,
    pub assumptions: Vec<String>,
    pub registry_id: String,
    pub registry_revision: u64,
    pub registry_sha256: String,
    pub execution_policy: ExecutionPolicy,
    pub inputs: Vec<ContractInput>,
    pub workflow: Vec<CompiledStep>,
    pub requirements: Vec<CompiledRequirement>,
    pub snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledStep {
    pub step_id: String,
    pub capability_type: VersionedRef,
    pub bindings: Vec<ResolvedBinding>,
    pub parameters: BTreeMap<String, CompiledParameterValue>,
    pub reproducibility: CompiledReproducibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_obligation: Option<CompiledReviewObligation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledReviewObligation {
    pub fulfillment: ReviewFulfillment,
    pub presented_evidence: Vec<ResolvedBinding>,
    pub decision_output_slot: String,
    pub decision_role: VersionedRef,
    pub decision_media_type: String,
    pub allowed_dispositions: Vec<ReviewDisposition>,
    pub reviewer_eligibility_policy: ImmutablePolicyRef,
    pub independence: ReviewIndependence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewFulfillment {
    PendingExternalReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledReproducibility {
    pub determinism: DeterminismClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    pub material_factors: BTreeMap<String, CompiledParameterValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompiledParameterValue {
    Boolean {
        value: bool,
    },
    Integer {
        value: i64,
    },
    ExactNumber {
        value: String,
    },
    Text {
        value: String,
    },
    Quantity {
        kind: String,
        value: String,
        unit: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedBinding {
    pub input_slot: String,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledRequirement {
    pub requirement_id: String,
    pub statement: String,
    pub purpose: VersionedRef,
    pub metric: SourceRef,
    pub metric_role: VersionedRef,
    pub comparison: Comparison,
    pub limit: CanonicalTypedQuantity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<CanonicalTypedQuantity>,
    pub basis: RequirementBasis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalTypedQuantity {
    pub kind: String,
    pub value: String,
    pub unit: String,
}

#[derive(Debug, Error)]
pub enum CompilerError {
    #[error("failed to serialize compiled semantic record: {0}")]
    Serialization(String),
    #[error("compiler produced a record outside its own canonical profile: {0}")]
    InternalCanonicalization(String),
}
