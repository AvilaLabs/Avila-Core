//! Compile reports and the immutable compiled-contract IR.
//!
//! `CompileReport` is ordinary serializable data: any client may read or
//! print it. `CompiledContract` is the opposite — a *checked* semantic
//! value. Its fields are private to this crate, it cannot be deserialized,
//! defaulted, or constructed outside `compile::lower`, so a value of this
//! type is evidence that the named contract and registry bytes passed
//! every compile pass. Cloning preserves the invariant because the value
//! is immutable.

use crate::diagnostic::CoreDiagnostic;
use crate::document::{
    CategoricalPredicate, Comparison, CompletionBlock, ContractInput, DeterminismClass,
    ExecutionPolicy, ImmutablePolicyRef, RequirementBasis, ReviewDisposition, ReviewIndependence,
    ReviewerRole, SourceRef, VersionedRef,
};
use avila_core_kernel::ExactNumber;
use serde::Serialize;
use std::collections::BTreeMap;
use thiserror::Error;

pub const COMPILE_NOTICE: &str = "Compilation establishes structural and semantic consistency under the named draft profile only. It performs no execution, optional presentation-gate run, agent-identity or trust evaluation, evidence admission, scientific qualification, or requirement verdict.";

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

/// The closed outcome of compiling authored documents. `Compiled` carries
/// the checked contract; `Rejected` carries the findings-only report. The
/// two cannot be mixed: a rejected report has no `CompiledContract` to
/// smuggle downstream, and a compiled one always carries its report.
#[derive(Debug, Clone, PartialEq)]
pub enum Compilation {
    Compiled(CheckedCompilation),
    Rejected(CompileReport),
}

impl Compilation {
    /// The wire report, whichever variant the outcome took.
    pub fn report(&self) -> &CompileReport {
        match self {
            Self::Compiled(compilation) => &compilation.report,
            Self::Rejected(report) => report,
        }
    }

    /// Consume the outcome into the wire report.
    pub fn into_report(self) -> CompileReport {
        match self {
            Self::Compiled(compilation) => compilation.report,
            Self::Rejected(report) => report,
        }
    }

    /// The checked contract when the outcome compiled.
    ///
    /// ```compile_fail
    /// // A checked contract cannot be forged in a client crate: there is
    /// // no `Default`, no deserialization, and no public constructor.
    /// let forged = avila_core_compiler::CompiledContract::default();
    /// ```
    pub fn contract(&self) -> Option<&CompiledContract> {
        match self {
            Self::Compiled(compilation) => Some(compilation.contract()),
            Self::Rejected(_) => None,
        }
    }
}

/// A successful compilation: the report plus the checked contract it
/// produced. Constructed only inside the compiler, so the invariant
/// `report.status == Compiled && report.compiled.is_some()` cannot be
/// broken by a downstream crate.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedCompilation {
    report: CompileReport,
}

impl CheckedCompilation {
    pub(crate) fn new(report: CompileReport) -> Self {
        debug_assert_eq!(report.status, CompilationStatus::Compiled);
        debug_assert!(report.compiled.is_some());
        Self { report }
    }

    pub fn report(&self) -> &CompileReport {
        &self.report
    }

    /// The checked contract. Access is read-only; the value can be cloned
    /// but never mutated through this handle.
    ///
    /// ```
    /// let contract = include_bytes!("../../../../fixtures/semantic-core/types/types.R1.resolved.pass.contract.json");
    /// let registry = include_bytes!("../../../../fixtures/semantic-core/types/compiler.registry.v1.json");
    /// let compilation = avila_core_compiler::compile_documents(contract, registry).unwrap();
    /// let compiled = compilation.contract().expect("the fixture compiles");
    /// assert_eq!(compiled.requirements()[0].requirement_id, "R-001");
    /// ```
    pub fn contract(&self) -> &CompiledContract {
        // CheckedCompilation is only constructed where the report carries a
        // compiled result.
        self.report.compiled.as_ref().expect("checked outcome")
    }
}

/// The compiled form of one contract under one registry snapshot.
///
/// This is a checked value: the compiler produces it and no downstream
/// crate can construct, deserialize, or mutate it. Use it where semantics
/// require a compiled result — a serializable report field is only a
/// description of one.
///
/// ```compile_fail
/// // Mutating a checked contract is not a supported route to a different
/// // compilation:
/// # let contract = include_bytes!("../../../../fixtures/semantic-core/types/types.R1.resolved.pass.contract.json");
/// # let registry = include_bytes!("../../../../fixtures/semantic-core/types/compiler.registry.v1.json");
/// # let compilation = avila_core_compiler::compile_documents(contract, registry).unwrap();
/// let compiled = compilation.contract().unwrap();
/// compiled.snapshot_sha256 = "sha256:00".to_string();
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompiledContract {
    schema_version: String,
    semantic_profile: String,
    compiler: String,
    contract_id: String,
    contract_revision: u64,
    contract_sha256: String,
    question: String,
    assumptions: Vec<String>,
    registry_id: String,
    registry_revision: u64,
    registry_sha256: String,
    execution_policy: ExecutionPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    completion: Option<CompletionBlock>,
    inputs: Vec<ContractInput>,
    workflow: Vec<CompiledStep>,
    requirements: Vec<CompiledRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    categorical_requirements: Vec<CompiledCategoricalRequirement>,
    snapshot_sha256: String,
}

impl CompiledContract {
    /// Construct the checked contract. `pub(crate)` so only the compile
    /// pipeline — which has just run every pass — can make one.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        schema_version: String,
        semantic_profile: String,
        compiler: String,
        contract_id: String,
        contract_revision: u64,
        contract_sha256: String,
        question: String,
        assumptions: Vec<String>,
        registry_id: String,
        registry_revision: u64,
        registry_sha256: String,
        execution_policy: ExecutionPolicy,
        completion: Option<CompletionBlock>,
        inputs: Vec<ContractInput>,
        workflow: Vec<CompiledStep>,
        requirements: Vec<CompiledRequirement>,
        categorical_requirements: Vec<CompiledCategoricalRequirement>,
        snapshot_sha256: String,
    ) -> Self {
        Self {
            schema_version,
            semantic_profile,
            compiler,
            contract_id,
            contract_revision,
            contract_sha256,
            question,
            assumptions,
            registry_id,
            registry_revision,
            registry_sha256,
            execution_policy,
            completion,
            inputs,
            workflow,
            requirements,
            categorical_requirements,
            snapshot_sha256,
        }
    }

    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }
    pub fn semantic_profile(&self) -> &str {
        &self.semantic_profile
    }
    pub fn compiler(&self) -> &str {
        &self.compiler
    }
    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }
    pub fn contract_revision(&self) -> u64 {
        self.contract_revision
    }
    pub fn contract_sha256(&self) -> &str {
        &self.contract_sha256
    }
    pub fn question(&self) -> &str {
        &self.question
    }
    pub fn assumptions(&self) -> &[String] {
        &self.assumptions
    }
    pub fn registry_id(&self) -> &str {
        &self.registry_id
    }
    pub fn registry_revision(&self) -> u64 {
        self.registry_revision
    }
    pub fn registry_sha256(&self) -> &str {
        &self.registry_sha256
    }
    pub fn execution_policy(&self) -> &ExecutionPolicy {
        &self.execution_policy
    }
    pub fn completion(&self) -> Option<&CompletionBlock> {
        self.completion.as_ref()
    }
    pub fn inputs(&self) -> &[ContractInput] {
        &self.inputs
    }
    pub fn workflow(&self) -> &[CompiledStep] {
        &self.workflow
    }
    pub fn requirements(&self) -> &[CompiledRequirement] {
        &self.requirements
    }
    pub fn categorical_requirements(&self) -> &[CompiledCategoricalRequirement] {
        &self.categorical_requirements
    }
    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }
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
    pub presentation_gate: Option<CompiledPresentationGate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledPresentationGate {
    pub state: PresentationGateState,
    pub reviewer_role: ReviewerRole,
    pub presented_evidence: Vec<ResolvedBinding>,
    pub decision_output_slot: String,
    pub decision_role: VersionedRef,
    pub decision_media_type: String,
    pub allowed_dispositions: Vec<ReviewDisposition>,
    pub reviewer_eligibility_policy: ImmutablePolicyRef,
    pub independence: ReviewIndependence,
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationGateState {
    AwaitingAgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledReproducibility {
    pub determinism: DeterminismClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    pub material_factors: BTreeMap<String, CompiledParameterValue>,
}

/// A resolved parameter value. Exact numbers and typed quantities retain
/// `ExactNumber` — the checked form compilation established — and serialize
/// back to the same canonical rational string the report carried before.
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
        value: ExactNumber,
    },
    Text {
        value: String,
    },
    Quantity {
        kind: String,
        value: ExactNumber,
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
pub struct CompiledCategoricalRequirement {
    pub requirement_id: String,
    pub statement: String,
    pub purpose: VersionedRef,
    pub metric: SourceRef,
    pub metric_role: VersionedRef,
    pub predicate: CategoricalPredicate,
}

/// A quantity lowered into canonical exact form: the value is an
/// `ExactNumber` produced by the kind's exact scaling, never a float and
/// never a string to re-parse. Constructed only inside the compiler.
///
/// ```compile_fail
/// // A checked quantity cannot be fabricated downstream:
/// let limit = avila_core_compiler::CanonicalTypedQuantity {
///     kind: "nuclear.dose_equivalent_rate".into(),
///     value: "1/36000000".into(),
///     unit: "Sv/s".into(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanonicalTypedQuantity {
    kind: String,
    value: ExactNumber,
    unit: String,
}

impl CanonicalTypedQuantity {
    pub(crate) fn new(kind: String, value: ExactNumber, unit: String) -> Self {
        Self { kind, value, unit }
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The checked exact value. Serializes as the canonical rational
    /// string, so report bytes and snapshot identities are unchanged.
    ///
    /// ```
    /// # let contract = include_bytes!("../../../../fixtures/semantic-core/types/types.R1.resolved.pass.contract.json");
    /// # let registry = include_bytes!("../../../../fixtures/semantic-core/types/compiler.registry.v1.json");
    /// # let compilation = avila_core_compiler::compile_documents(contract, registry).unwrap();
    /// let limit = &compilation.contract().unwrap().requirements()[0].limit;
    /// assert_eq!(limit.value().to_string(), "1/36000000");
    /// assert_eq!(limit.unit(), "Sv/s");
    /// ```
    pub fn value(&self) -> &ExactNumber {
        &self.value
    }
    pub fn unit(&self) -> &str {
        &self.unit
    }
}

#[derive(Debug, Error)]
pub enum CompilerError {
    #[error("failed to serialize compiled semantic record: {0}")]
    Serialization(String),
    #[error("compiler produced a record outside its own canonical profile: {0}")]
    InternalCanonicalization(String),
}
