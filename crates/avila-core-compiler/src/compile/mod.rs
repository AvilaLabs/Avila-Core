//! Deterministic contract compilation, pass by pass.
//!
//! `compile_documents` is the only entry point. Each pass lives in its own
//! module, reports independent findings, and never repairs a source document.

pub(crate) mod findings;
mod ir;
mod notices;
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
use crate::diagnostic::{CORE_S1102, CoreDiagnostic, FindingClass};
use crate::document::{
    COMPILE_REPORT_SCHEMA_VERSION, COMPILED_CONTRACT_SCHEMA_VERSION, ContractInput, ContractSource,
    ExecutionPolicy, RegistrySnapshot,
};
use avila_core_kernel::{SEMANTIC_PROFILE, canonicalize_json};
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
        execution_policy: &contract.execution_policy,
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
        execution_policy: contract.execution_policy.clone(),
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

fn prefixed_sha256(bytes: impl AsRef<[u8]>) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes.as_ref()))
}
