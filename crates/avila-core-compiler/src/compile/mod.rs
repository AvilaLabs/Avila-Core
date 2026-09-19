//! Deterministic contract compilation, pass by pass.
//!
//! `compile_documents` is the entry point for ordinary contracts;
//! `compile_documents_with_material` additionally carries the bound
//! instantiation material an `instantiated_from` contract is checked
//! against (ADR-0022). Each pass lives in its own module, reports
//! independent findings, and never repairs a source document.

pub(crate) mod findings;
pub(crate) mod instantiation;
mod ir;
mod notices;
pub(crate) mod org_policy;
pub(crate) mod registry;
mod reproducibility;
mod requirements;
mod resolve;
pub(crate) mod review;
pub(crate) mod schema;
mod shape;
pub(crate) mod source;
mod values;

#[cfg(test)]
mod tests;

use self::findings::contract_location;
use self::notices::{report_require_signatures, report_unconsumed_declarations};
use self::registry::RegistryIndex;
use self::reproducibility::compile_reproducibility;
use self::requirements::{compile_categorical_requirements, compile_requirements};
use self::resolve::{
    collect_sources, resolve_workflow, validate_contract_registry_refs, validate_graph,
};
use self::review::compile_presentation_gates;
use self::schema::SchemaDocument;
use self::shape::validate_contract_shape;
use self::source::{read_document, validate_document_headers};
use self::values::compile_parameters;
use crate::diagnostic::{
    CORE_A4701, CORE_S1102, CoreDiagnostic, DiagnosticRepair, FindingClass, RepairApplicability,
    RepairEdit,
};
use crate::document::{
    COMPILE_REPORT_SCHEMA_VERSION, COMPILED_CONTRACT_SCHEMA_VERSION, CompletionBlock,
    ContractInput, ContractSource, ExecutionPolicy, RegistrySnapshot,
};
use avila_core_kernel::{SEMANTIC_PROFILE, VerdictStatus, canonicalize_json};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub use self::schema::validate_against_schema;
pub use ir::{
    COMPILE_NOTICE, CanonicalTypedQuantity, CompilationStatus, CompileReport,
    CompiledCategoricalRequirement, CompiledContract, CompiledParameterValue,
    CompiledPresentationGate, CompiledReproducibility, CompiledRequirement, CompiledStep,
    CompilerError, DocumentIdentity, PresentationGateState, ResolvedBinding,
};

/// Package-bound material a compile may consult beyond contract +
/// registry, as raw bytes: ADR-0022 instantiation records and templates,
/// ADR-0023 organization policies, and the bound `attestation` documents
/// a `replaces` relation names. The contract's own pins select what it is
/// checked against — material it does not name is ignored.
#[derive(Debug, Default)]
pub struct CompilationMaterial<'a> {
    /// Every bound `contract_instantiation` document.
    pub records: &'a [&'a [u8]],
    /// Every bound `contract_template` document.
    pub templates: &'a [&'a [u8]],
    /// Every bound `organization_policy` document (ADR-0023).
    pub organization_policies: &'a [&'a [u8]],
    /// Every bound `attestation` document — the pool a `replaces`
    /// relation's `replacement_attestation` digest resolves against
    /// (structural checks only; signatures are the runner's boundary).
    pub attestations: &'a [&'a [u8]],
}

const COMPILER_ID: &str = concat!("avila.core/compiler-rust@", env!("CARGO_PKG_VERSION"));

const MAX_WORKFLOW_STEPS: usize = 2_048;

/// Compile authoritative JSON documents against one explicit registry snapshot.
///
/// User-caused problems are returned as deterministic findings. `Err` is
/// reserved for a compiler-internal failure while constructing successful IR.
pub fn compile_documents(
    contract_bytes: &[u8],
    registry_bytes: &[u8],
) -> Result<CompileReport, CompilerError> {
    compile_documents_with_material(
        contract_bytes,
        registry_bytes,
        &instantiation::CompilationMaterial::default(),
    )
}

/// Compile a contract that may name package-bound material — an ADR-0022
/// `instantiated_from` record, or an ADR-0023 `organization_policy` floor.
/// `material` carries every bound document of each kind; the contract's
/// own pins select what it is checked against.
pub fn compile_documents_with_material(
    contract_bytes: &[u8],
    registry_bytes: &[u8],
    material: &instantiation::CompilationMaterial<'_>,
) -> Result<CompileReport, CompilerError> {
    let mut findings = Vec::new();
    let mut source_identities = Vec::new();

    let contract = read_document::<ContractSource>(
        "contract",
        SchemaDocument::Contract,
        contract_bytes,
        &mut source_identities,
        &mut findings,
    );
    let registry = read_document::<RegistrySnapshot>(
        "registry",
        SchemaDocument::Registry,
        registry_bytes,
        &mut source_identities,
        &mut findings,
    );

    let (Some(contract), Some(registry)) = (contract, registry) else {
        sort_findings(&mut findings);
        return Ok(rejected_report(source_identities, findings));
    };

    validate_document_headers(&contract, &registry, &mut findings);
    validate_contract_shape(&contract, &mut findings);
    report_require_signatures(&contract, &mut findings);
    let registry_index = RegistryIndex::build(&registry, &mut findings);
    let invalid_sources =
        validate_contract_registry_refs(&contract, &registry_index, &mut findings);
    let compiled_parameters = compile_parameters(&contract, &registry_index, &mut findings);
    let compiled_reproducibility =
        compile_reproducibility(&contract, &registry_index, &mut findings);

    if contract.workflow.len() > MAX_WORKFLOW_STEPS {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            "requester",
            contract_location("/workflow"),
            format!(
                "workflow contains {} steps; the compiler work limit is {MAX_WORKFLOW_STEPS}",
                contract.workflow.len()
            ),
        ));
    }

    let candidates = collect_sources(&contract, &registry_index);
    let unknown_type_steps: BTreeSet<&str> = contract
        .workflow
        .iter()
        .filter(|step| {
            !registry_index
                .capability_types
                .contains_key(&step.capability_type)
        })
        .map(|step| step.step_id.as_str())
        .collect();
    let resolution = resolve_workflow(
        &contract,
        &registry_index,
        &candidates,
        &invalid_sources,
        &unknown_type_steps,
        &mut findings,
    );
    let presentation_gates = compile_presentation_gates(
        &contract,
        &registry_index,
        &resolution.bindings,
        &mut findings,
    );
    let order = validate_graph(&contract, &resolution.dependencies, &mut findings);
    let compiled_requirements = compile_requirements(
        &contract,
        &registry_index,
        &candidates,
        &invalid_sources,
        &unknown_type_steps,
        &mut findings,
    );
    let compiled_categorical_requirements = compile_categorical_requirements(
        &contract,
        &registry_index,
        &candidates,
        &invalid_sources,
        &unknown_type_steps,
        &mut findings,
    );
    validate_completion_block(&contract, &mut findings);

    if contract.instantiated_from.is_some() {
        let contract_sha256 = identity_for(&source_identities, "contract")
            .expect("a parsed contract has a canonical identity")
            .to_owned();
        instantiation::check_instantiation(
            &contract,
            &contract_sha256,
            registry_bytes,
            &registry_index.kinds,
            material,
            &mut findings,
        );
    }

    // ADR-0023: the pinned `organization_policy` floor merges against the
    // contract's `execution_policy`; the merged result is what the
    // compiled snapshot enforces.
    let merged_policy = org_policy::check_org_policy(&contract, material, &mut findings);

    if !findings.iter().any(CoreDiagnostic::blocks_compilation) {
        report_unconsumed_declarations(
            &contract,
            &registry_index,
            &resolution.bindings,
            &mut findings,
        );
    }
    sort_findings(&mut findings);
    if findings.iter().any(CoreDiagnostic::blocks_compilation) {
        return Ok(rejected_report(source_identities, findings));
    }

    let contract_sha256 = identity_for(&source_identities, "contract")
        .expect("a parsed contract has a canonical identity")
        .to_owned();
    let registry_sha256 = identity_for(&source_identities, "registry")
        .expect("a parsed registry has a canonical identity")
        .to_owned();
    let workflow = build_compiled_steps(
        &contract,
        &resolution.bindings,
        &compiled_parameters,
        &compiled_reproducibility,
        &presentation_gates,
        &order,
    );
    let mut inputs = contract.inputs.clone();
    inputs.sort_by(|left, right| left.input_id.cmp(&right.input_id));
    let mut requirements = compiled_requirements;
    requirements.sort_by(|left, right| left.requirement_id.cmp(&right.requirement_id));
    let mut categorical_requirements = compiled_categorical_requirements;
    categorical_requirements.sort_by(|left, right| left.requirement_id.cmp(&right.requirement_id));

    let compiled = create_compiled_contract(
        &contract,
        &registry,
        contract_sha256,
        registry_sha256,
        inputs,
        workflow,
        requirements,
        categorical_requirements,
        merged_policy.as_ref().unwrap_or(&contract.execution_policy),
    )?;

    Ok(CompileReport {
        schema_version: COMPILE_REPORT_SCHEMA_VERSION.into(),
        semantic_profile: SEMANTIC_PROFILE.into(),
        status: CompilationStatus::Compiled,
        source_identities,
        findings,
        compiled: Some(compiled),
        notice: COMPILE_NOTICE.into(),
    })
}

/// SC-9 clause 6: a declared completion block must be internally possible.
/// `not_evaluated` never completes a substantive contract, and permitted
/// inconclusive reasons are meaningless unless `inconclusive` is declared
/// fulfilling — either shape is an unsatisfiable delivery statement.
fn validate_completion_block(contract: &ContractSource, findings: &mut Vec<CoreDiagnostic>) {
    let Some(block) = &contract.completion else {
        return;
    };
    if block
        .fulfilling_verdicts
        .contains(&VerdictStatus::NotEvaluated)
    {
        findings.push(
            CoreDiagnostic::new(
                CORE_A4701,
                FindingClass::Inadmissible,
                "policy_owner",
                contract_location("/completion/fulfilling_verdicts"),
                "`not_evaluated` never completes a substantive contract — it cannot be declared a fulfilling verdict",
            )
            .with_repair(
                DiagnosticRepair::labels(RepairApplicability::ConstrainedChoice, Vec::new())
                    .alternative(
                        "remove `not_evaluated` from `fulfilling_verdicts`",
                        vec![RepairEdit::Remove {
                            path: "/completion/fulfilling_verdicts".into(),
                        }],
                    ),
            ),
        );
    }
    if !block.permitted_inconclusive_reasons.is_empty()
        && !block
            .fulfilling_verdicts
            .contains(&VerdictStatus::Inconclusive)
    {
        findings.push(
            CoreDiagnostic::new(
                CORE_A4701,
                FindingClass::Inadmissible,
                "policy_owner",
                contract_location("/completion/permitted_inconclusive_reasons"),
                "`permitted_inconclusive_reasons` is declared but `inconclusive` is not a fulfilling verdict — no inconclusive verdict can complete this contract",
            )
            .with_repair(
                DiagnosticRepair::labels(RepairApplicability::ConstrainedChoice, Vec::new())
                    .alternative(
                        "add `inconclusive` to `fulfilling_verdicts`, or drop `permitted_inconclusive_reasons`",
                        Vec::new(),
                    ),
            ),
        );
    }
}

fn build_compiled_steps(
    contract: &ContractSource,
    bindings: &BTreeMap<String, Vec<ResolvedBinding>>,
    parameters: &BTreeMap<String, BTreeMap<String, CompiledParameterValue>>,
    reproducibility: &BTreeMap<String, CompiledReproducibility>,
    presentation_gates: &BTreeMap<String, CompiledPresentationGate>,
    order: &[String],
) -> Vec<CompiledStep> {
    let steps: BTreeMap<_, _> = contract
        .workflow
        .iter()
        .map(|step| (step.step_id.as_str(), step))
        .collect();
    order
        .iter()
        .map(|step_id| {
            let step = steps
                .get(step_id.as_str())
                .expect("topological order contains only known steps");
            CompiledStep {
                step_id: step.step_id.clone(),
                capability_type: step.capability_type.clone(),
                bindings: bindings.get(step_id).cloned().unwrap_or_default(),
                parameters: parameters.get(step_id).cloned().unwrap_or_default(),
                reproducibility: reproducibility
                    .get(step_id)
                    .cloned()
                    .expect("every compiled step has a reproducibility declaration"),
                presentation_gate: presentation_gates.get(step_id).cloned(),
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn create_compiled_contract(
    contract: &ContractSource,
    registry: &RegistrySnapshot,
    contract_sha256: String,
    registry_sha256: String,
    inputs: Vec<ContractInput>,
    workflow: Vec<CompiledStep>,
    requirements: Vec<CompiledRequirement>,
    categorical_requirements: Vec<CompiledCategoricalRequirement>,
    execution_policy: &ExecutionPolicy,
) -> Result<CompiledContract, CompilerError> {
    #[derive(Serialize)]
    struct IdentityBody<'a> {
        schema_version: &'a str,
        semantic_profile: &'a str,
        compiler: &'a str,
        contract_id: &'a str,
        contract_revision: u64,
        contract_sha256: &'a str,
        question: &'a str,
        assumptions: &'a [String],
        registry_id: &'a str,
        registry_revision: u64,
        registry_sha256: &'a str,
        execution_policy: &'a ExecutionPolicy,
        #[serde(skip_serializing_if = "Option::is_none")]
        completion: Option<&'a CompletionBlock>,
        inputs: &'a [ContractInput],
        workflow: &'a [CompiledStep],
        requirements: &'a [CompiledRequirement],
        #[serde(skip_serializing_if = "<[CompiledCategoricalRequirement]>::is_empty")]
        categorical_requirements: &'a [CompiledCategoricalRequirement],
    }

    let body = IdentityBody {
        schema_version: COMPILED_CONTRACT_SCHEMA_VERSION,
        semantic_profile: SEMANTIC_PROFILE,
        compiler: COMPILER_ID,
        contract_id: &contract.contract_id,
        contract_revision: contract.revision,
        contract_sha256: &contract_sha256,
        question: &contract.question,
        assumptions: &contract.assumptions,
        registry_id: &registry.registry_id,
        registry_revision: registry.revision,
        registry_sha256: &registry_sha256,
        execution_policy,
        completion: contract.completion.as_ref(),
        inputs: &inputs,
        workflow: &workflow,
        requirements: &requirements,
        categorical_requirements: &categorical_requirements,
    };
    let bytes = serde_json::to_vec(&body)
        .map_err(|error| CompilerError::Serialization(error.to_string()))?;
    let canonical = canonicalize_json(&bytes)
        .map_err(|error| CompilerError::InternalCanonicalization(error.to_string()))?;
    let snapshot_sha256 = prefixed_sha256(canonical);

    Ok(CompiledContract {
        schema_version: COMPILED_CONTRACT_SCHEMA_VERSION.into(),
        semantic_profile: SEMANTIC_PROFILE.into(),
        compiler: COMPILER_ID.into(),
        contract_id: contract.contract_id.clone(),
        contract_revision: contract.revision,
        contract_sha256,
        question: contract.question.clone(),
        assumptions: contract.assumptions.clone(),
        registry_id: registry.registry_id.clone(),
        registry_revision: registry.revision,
        registry_sha256,
        execution_policy: execution_policy.clone(),
        completion: contract.completion.clone(),
        inputs,
        workflow,
        requirements,
        categorical_requirements,
        snapshot_sha256,
    })
}

fn rejected_report(
    source_identities: Vec<DocumentIdentity>,
    findings: Vec<CoreDiagnostic>,
) -> CompileReport {
    CompileReport {
        schema_version: COMPILE_REPORT_SCHEMA_VERSION.into(),
        semantic_profile: SEMANTIC_PROFILE.into(),
        status: CompilationStatus::Rejected,
        source_identities,
        findings,
        compiled: None,
        notice: COMPILE_NOTICE.into(),
    }
}

pub(crate) fn sort_findings(findings: &mut [CoreDiagnostic]) {
    findings.sort_by(|left, right| {
        left.primary
            .cmp(&right.primary)
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.owner.cmp(&right.owner))
    });
}

fn identity_for<'a>(identities: &'a [DocumentIdentity], document: &str) -> Option<&'a str> {
    identities
        .iter()
        .find(|identity| identity.document == document)
        .map(|identity| identity.sha256.as_str())
}

pub(crate) fn prefixed_sha256(bytes: impl AsRef<[u8]>) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes.as_ref()))
}
