use std::collections::{BTreeMap, BTreeSet, VecDeque};

use avila_core_kernel::{
    KindDefinition, KindRegistry, SEMANTIC_PROFILE, UnitDefinition, canonicalize_json,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::diagnostic::{
    CORE_R3101, CORE_R3102, CORE_R3201, CORE_R3202, CORE_R3203, CORE_R3301, CORE_R3501, CORE_S1101,
    CORE_S1102, CORE_T2001, CORE_T2101, CORE_T2102, CORE_T2103, CORE_T2201, CORE_T2203, CORE_T2301,
    CoreDiagnostic, DiagnosticRepair, FindingClass, RepairApplicability, SourceLocation,
};
use crate::document::{
    BasisKind, BoundSide, COMPILE_REPORT_SCHEMA_VERSION, COMPILED_CONTRACT_SCHEMA_VERSION,
    CONTRACT_SCHEMA_VERSION, CapabilityTypeDefinition, ClaimModelDeclaration, Comparison,
    ContractInput, ContractSource, REGISTRY_SCHEMA_VERSION, RegistrySnapshot, RequirementBasis,
    RoleDefinition, SourceRef, VersionedRef,
};

pub const COMPILE_NOTICE: &str = "Compilation establishes structural and semantic consistency under the named draft profile only. It performs no execution, evidence admission, scientific qualification, or requirement verdict.";
const COMPILER_ID: &str = concat!("avila.core/compiler-rust@", env!("CARGO_PKG_VERSION"));
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_WORKFLOW_STEPS: usize = 2_048;

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
    pub registry_id: String,
    pub registry_revision: u64,
    pub registry_sha256: String,
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
    pub parameters: BTreeMap<String, serde_json::Value>,
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
    pub metric: SourceRef,
    pub metric_role: VersionedRef,
    pub comparison: Comparison,
    pub limit: CanonicalTypedQuantity,
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
        contract_bytes,
        &mut source_identities,
        &mut findings,
    );
    let registry = read_document::<RegistrySnapshot>(
        "registry",
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
    let registry_index = RegistryIndex::build(&registry, &mut findings);
    let invalid_sources =
        validate_contract_registry_refs(&contract, &registry_index, &mut findings);

    if contract.workflow.len() > MAX_WORKFLOW_STEPS {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            "contract_author",
            contract_location("/workflow"),
            format!(
                "workflow contains {} steps; the compiler work limit is {MAX_WORKFLOW_STEPS}",
                contract.workflow.len()
            ),
        ));
    }

    let candidates = collect_sources(&contract, &registry_index);
    let resolution = resolve_workflow(
        &contract,
        &registry_index,
        &candidates,
        &invalid_sources,
        &mut findings,
    );
    let order = validate_graph(&contract, &resolution.dependencies, &mut findings);
    let compiled_requirements = compile_requirements(
        &contract,
        &registry_index,
        &candidates,
        &invalid_sources,
        &mut findings,
    );

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
    let workflow = build_compiled_steps(&contract, &resolution.bindings, &order);
    let mut inputs = contract.inputs.clone();
    inputs.sort_by(|left, right| left.input_id.cmp(&right.input_id));
    let mut requirements = compiled_requirements;
    requirements.sort_by(|left, right| left.requirement_id.cmp(&right.requirement_id));

    let compiled = create_compiled_contract(
        &contract,
        &registry,
        contract_sha256,
        registry_sha256,
        inputs,
        workflow,
        requirements,
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

fn read_document<T: serde::de::DeserializeOwned>(
    document: &str,
    bytes: &[u8],
    identities: &mut Vec<DocumentIdentity>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<T> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            owner_for(document),
            SourceLocation::new(document, ""),
            format!(
                "document contains {} bytes; the compiler input limit is {MAX_DOCUMENT_BYTES}",
                bytes.len()
            ),
        ));
        return None;
    }
    let canonical = match canonicalize_json(bytes) {
        Ok(canonical) => canonical,
        Err(error) => {
            findings.push(CoreDiagnostic::new(
                error.code(),
                FindingClass::Invalid,
                owner_for(document),
                SourceLocation::new(document, ""),
                error.detail(),
            ));
            return None;
        }
    };
    identities.push(DocumentIdentity {
        document: document.into(),
        sha256: prefixed_sha256(&canonical),
    });

    match serde_json::from_slice(bytes) {
        Ok(value) => Some(value),
        Err(error) => {
            let detail = error.to_string();
            let code = if detail.contains("unknown field") {
                CORE_S1101
            } else {
                CORE_S1102
            };
            findings.push(CoreDiagnostic::new(
                code,
                FindingClass::Invalid,
                owner_for(document),
                SourceLocation::new(document, ""),
                detail,
            ));
            None
        }
    }
}

fn validate_document_headers(
    contract: &ContractSource,
    registry: &RegistrySnapshot,
    findings: &mut Vec<CoreDiagnostic>,
) {
    check_header(
        "contract",
        &contract.schema_version,
        CONTRACT_SCHEMA_VERSION,
        &contract.semantic_profile,
        findings,
    );
    check_header(
        "registry",
        &registry.schema_version,
        REGISTRY_SCHEMA_VERSION,
        &registry.semantic_profile,
        findings,
    );
}

fn check_header(
    document: &str,
    schema_version: &str,
    expected_schema: &str,
    semantic_profile: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    if schema_version != expected_schema {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            owner_for(document),
            SourceLocation::new(document, "/schema_version"),
            format!("unsupported schema `{schema_version}`; expected `{expected_schema}`"),
        ));
    }
    if semantic_profile != SEMANTIC_PROFILE {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            owner_for(document),
            SourceLocation::new(document, "/semantic_profile"),
            format!(
                "unsupported semantic profile `{semantic_profile}`; expected `{SEMANTIC_PROFILE}`"
            ),
        ));
    }
}

fn validate_contract_shape(contract: &ContractSource, findings: &mut Vec<CoreDiagnostic>) {
    require_nonempty(
        &contract.contract_id,
        contract_location("/contract_id"),
        "contract_author",
        findings,
    );
    if contract.revision == 0 {
        invalid_value(
            contract_location("/revision"),
            "contract revision must be at least one",
            "contract_author",
            findings,
        );
    }
    if contract.workflow.is_empty() {
        invalid_value(
            contract_location("/workflow"),
            "a contract workflow must contain at least one step",
            "contract_author",
            findings,
        );
    }
    if contract.requirements.is_empty() {
        invalid_value(
            contract_location("/requirements"),
            "a contract must contain at least one requirement",
            "contract_author",
            findings,
        );
    }

    find_duplicate_ids(
        contract.inputs.iter().map(|item| item.input_id.as_str()),
        "input",
        "/inputs",
        findings,
    );
    find_duplicate_ids(
        contract.workflow.iter().map(|item| item.step_id.as_str()),
        "workflow step",
        "/workflow",
        findings,
    );
    find_duplicate_ids(
        contract
            .requirements
            .iter()
            .map(|item| item.requirement_id.as_str()),
        "requirement",
        "/requirements",
        findings,
    );

    for (index, input) in contract.inputs.iter().enumerate() {
        require_nonempty(
            &input.input_id,
            contract_location(format!("/inputs/{index}/input_id")),
            "contract_author",
            findings,
        );
        require_nonempty(
            &input.media_type,
            contract_location(format!("/inputs/{index}/media_type")),
            "contract_author",
            findings,
        );
        validate_versioned_ref(
            &input.role,
            contract_location(format!("/inputs/{index}/role")),
            "contract_author",
            findings,
        );
    }
    for (index, step) in contract.workflow.iter().enumerate() {
        require_nonempty(
            &step.step_id,
            contract_location(format!("/workflow/{index}/step_id")),
            "contract_author",
            findings,
        );
        validate_versioned_ref(
            &step.capability_type,
            contract_location(format!("/workflow/{index}/capability_type")),
            "contract_author",
            findings,
        );
        let mut slots = BTreeSet::new();
        for (binding_index, binding) in step.bindings.iter().enumerate() {
            require_nonempty(
                &binding.input_slot,
                contract_location(format!(
                    "/workflow/{index}/bindings/{binding_index}/input_slot"
                )),
                "contract_author",
                findings,
            );
            if !slots.insert(binding.input_slot.as_str()) {
                invalid_value(
                    contract_location(format!(
                        "/workflow/{index}/bindings/{binding_index}/input_slot"
                    )),
                    format!(
                        "step `{}` binds input slot `{}` more than once",
                        step.step_id, binding.input_slot
                    ),
                    "contract_author",
                    findings,
                );
            }
        }
    }
    for (index, requirement) in contract.requirements.iter().enumerate() {
        require_nonempty(
            &requirement.requirement_id,
            contract_location(format!("/requirements/{index}/requirement_id")),
            "contract_author",
            findings,
        );
        require_nonempty(
            &requirement.statement,
            contract_location(format!("/requirements/{index}/statement")),
            "contract_author",
            findings,
        );
        require_nonempty(
            &requirement.limit.kind,
            contract_location(format!("/requirements/{index}/limit/kind")),
            "contract_author",
            findings,
        );
        require_nonempty(
            &requirement.limit.unit,
            contract_location(format!("/requirements/{index}/limit/unit")),
            "contract_author",
            findings,
        );
    }
}

struct RegistryIndex<'a> {
    kinds: KindRegistry,
    kind_classes: BTreeMap<&'a str, &'a str>,
    roles: BTreeMap<VersionedRef, &'a RoleDefinition>,
    capability_types: BTreeMap<VersionedRef, &'a CapabilityTypeDefinition>,
}

impl<'a> RegistryIndex<'a> {
    fn build(registry: &'a RegistrySnapshot, findings: &mut Vec<CoreDiagnostic>) -> Self {
        require_nonempty(
            &registry.registry_id,
            registry_location("/registry_id"),
            "registry_owner",
            findings,
        );
        if registry.revision == 0 {
            invalid_value(
                registry_location("/revision"),
                "registry revision must be at least one",
                "registry_owner",
                findings,
            );
        }

        let mut kinds = KindRegistry::default();
        let mut kind_classes = BTreeMap::new();
        for (index, kind) in registry.kinds.iter().enumerate() {
            let pointer = format!("/kinds/{index}");
            require_nonempty(
                &kind.kind_id,
                registry_location(format!("{pointer}/kind_id")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &kind.unit_class,
                registry_location(format!("{pointer}/unit_class")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &kind.owner,
                registry_location(format!("{pointer}/owner")),
                "registry_owner",
                findings,
            );
            if kind_classes
                .insert(kind.kind_id.as_str(), kind.unit_class.as_str())
                .is_some()
            {
                invalid_value(
                    registry_location(format!("{pointer}/kind_id")),
                    format!("duplicate quantity kind `{}`", kind.kind_id),
                    "registry_owner",
                    findings,
                );
                continue;
            }
            let unit_definitions = kind
                .units
                .iter()
                .map(|unit| UnitDefinition::new(unit.symbol.clone(), unit.factor.clone()))
                .collect::<Result<Vec<_>, _>>();
            let definition = unit_definitions.and_then(|units| {
                KindDefinition::new(kind.kind_id.clone(), kind.canonical_unit.clone(), units)
            });
            match definition.and_then(|definition| kinds.insert_kind(definition)) {
                Ok(()) => {}
                Err(error) => findings.push(CoreDiagnostic::new(
                    error.code(),
                    FindingClass::Invalid,
                    "registry_owner",
                    registry_location(pointer),
                    error.detail(),
                )),
            }
        }

        let mut roles = BTreeMap::new();
        for (index, role) in registry.roles.iter().enumerate() {
            let pointer = format!("/roles/{index}");
            validate_versioned_ref(
                &role.role,
                registry_location(format!("{pointer}/role")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &role.owner,
                registry_location(format!("{pointer}/owner")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &role.validator,
                registry_location(format!("{pointer}/validator")),
                "registry_owner",
                findings,
            );
            if role.accepted_media_types.is_empty() {
                registry_incomplete(
                    registry_location(format!("{pointer}/accepted_media_types")),
                    "evidence role must accept at least one media type",
                    findings,
                );
            }
            if role.permitted_claim_models.is_empty() {
                registry_incomplete(
                    registry_location(format!("{pointer}/permitted_claim_models")),
                    "evidence role must permit at least one explicit claim model",
                    findings,
                );
            }
            if roles.insert(role.role.clone(), role).is_some() {
                invalid_value(
                    registry_location(format!("{pointer}/role")),
                    format!(
                        "duplicate evidence role `{}@{}`",
                        role.role.id, role.role.major
                    ),
                    "registry_owner",
                    findings,
                );
            }
            match (&role.quantity_kind, &role.unit_class) {
                (Some(kind), Some(unit_class)) => match kind_classes.get(kind.as_str()) {
                    Some(expected) if expected == &unit_class.as_str() => {}
                    Some(expected) => registry_incomplete(
                        registry_location(format!("{pointer}/unit_class")),
                        format!(
                            "role unit class `{unit_class}` does not match kind `{kind}` class `{expected}`"
                        ),
                        findings,
                    ),
                    None => registry_incomplete(
                        registry_location(format!("{pointer}/quantity_kind")),
                        format!("role references unknown quantity kind `{kind}`"),
                        findings,
                    ),
                },
                (None, None) => {}
                _ => registry_incomplete(
                    registry_location(pointer),
                    "a quantity role must declare both quantity_kind and unit_class",
                    findings,
                ),
            }
        }

        let mut capability_types = BTreeMap::new();
        for (index, capability) in registry.capability_types.iter().enumerate() {
            let pointer = format!("/capability_types/{index}");
            validate_versioned_ref(
                &capability.capability_type,
                registry_location(format!("{pointer}/capability_type")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &capability.owner,
                registry_location(format!("{pointer}/owner")),
                "registry_owner",
                findings,
            );
            if capability_types
                .insert(capability.capability_type.clone(), capability)
                .is_some()
            {
                invalid_value(
                    registry_location(format!("{pointer}/capability_type")),
                    format!(
                        "duplicate capability type `{}@{}`",
                        capability.capability_type.id, capability.capability_type.major
                    ),
                    "registry_owner",
                    findings,
                );
            }
            validate_slots(capability, index, &roles, findings);
        }

        Self {
            kinds,
            kind_classes,
            roles,
            capability_types,
        }
    }
}

fn validate_slots(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    roles: &BTreeMap<VersionedRef, &RoleDefinition>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut input_ids = BTreeSet::new();
    for (index, slot) in capability.inputs.iter().enumerate() {
        let pointer = format!("/capability_types/{capability_index}/inputs/{index}");
        require_nonempty(
            &slot.slot_id,
            registry_location(format!("{pointer}/slot_id")),
            "registry_owner",
            findings,
        );
        if !input_ids.insert(slot.slot_id.as_str()) {
            invalid_value(
                registry_location(format!("{pointer}/slot_id")),
                format!("duplicate input slot `{}`", slot.slot_id),
                "registry_owner",
                findings,
            );
        }
        match roles.get(&slot.role) {
            None => registry_incomplete(
                registry_location(format!("{pointer}/role")),
                format!(
                    "input slot references unknown role `{}@{}`",
                    slot.role.id, slot.role.major
                ),
                findings,
            ),
            Some(role) => {
                for media_type in &slot.accepted_media_types {
                    if !role.accepted_media_types.contains(media_type) {
                        registry_incomplete(
                            registry_location(format!("{pointer}/accepted_media_types")),
                            format!(
                                "input slot media type `{media_type}` is outside role `{}@{}`",
                                role.role.id, role.role.major
                            ),
                            findings,
                        );
                    }
                }
            }
        }
        if slot.accepted_media_types.is_empty() {
            registry_incomplete(
                registry_location(format!("{pointer}/accepted_media_types")),
                "input slot must accept at least one media type",
                findings,
            );
        }
    }

    let mut output_ids = BTreeSet::new();
    for (index, slot) in capability.outputs.iter().enumerate() {
        let pointer = format!("/capability_types/{capability_index}/outputs/{index}");
        require_nonempty(
            &slot.slot_id,
            registry_location(format!("{pointer}/slot_id")),
            "registry_owner",
            findings,
        );
        if !output_ids.insert(slot.slot_id.as_str()) {
            invalid_value(
                registry_location(format!("{pointer}/slot_id")),
                format!("duplicate output slot `{}`", slot.slot_id),
                "registry_owner",
                findings,
            );
        }
        match roles.get(&slot.role) {
            None => registry_incomplete(
                registry_location(format!("{pointer}/role")),
                format!(
                    "output slot references unknown role `{}@{}`",
                    slot.role.id, slot.role.major
                ),
                findings,
            ),
            Some(role) if !role.accepted_media_types.contains(&slot.media_type) => {
                registry_incomplete(
                    registry_location(format!("{pointer}/media_type")),
                    format!(
                        "output media type `{}` is outside role `{}@{}`",
                        slot.media_type, role.role.id, role.role.major
                    ),
                    findings,
                );
            }
            Some(role) => {
                if slot.permitted_claim_models.is_empty() {
                    registry_incomplete(
                        registry_location(format!("{pointer}/permitted_claim_models")),
                        "output slot must declare at least one permitted claim model",
                        findings,
                    );
                }
                for model in &slot.permitted_claim_models {
                    if !role.permitted_claim_models.contains(model) {
                        registry_incomplete(
                            registry_location(format!("{pointer}/permitted_claim_models")),
                            format!(
                                "output claim model is outside role `{}@{}`",
                                role.role.id, role.role.major
                            ),
                            findings,
                        );
                    }
                }
            }
        }
        require_nonempty(
            &slot.media_type,
            registry_location(format!("{pointer}/media_type")),
            "registry_owner",
            findings,
        );
    }
}

fn validate_contract_registry_refs(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> BTreeSet<SourceRef> {
    let mut invalid_sources = BTreeSet::new();
    for (index, input) in contract.inputs.iter().enumerate() {
        match registry.roles.get(&input.role) {
            None => {
                invalid_sources.insert(SourceRef::ContractInput {
                    input_id: input.input_id.clone(),
                });
                findings.push(CoreDiagnostic::new(
                    CORE_R3101,
                    FindingClass::Unsatisfied,
                    "contract_author",
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
                    "contract_author",
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
                    "contract_author",
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
struct Candidate {
    source: SourceRef,
    role: VersionedRef,
    media_type: String,
    claim_models: Vec<ClaimModelDeclaration>,
}

fn collect_sources(contract: &ContractSource, registry: &RegistryIndex<'_>) -> Vec<Candidate> {
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
        }));
    }
    candidates.sort_by(|left, right| left.source.cmp(&right.source));
    candidates
}

#[derive(Default)]
struct WorkflowResolution {
    bindings: BTreeMap<String, Vec<ResolvedBinding>>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
}

fn resolve_workflow(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    candidates: &[Candidate],
    invalid_sources: &BTreeSet<SourceRef>,
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
                "contract_author",
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
                    "contract_author",
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
                        "contract_author",
                        contract_location(pointer),
                        format!("step `{}` depends on its own output", step.step_id),
                    ));
                    continue;
                }
                let Some(candidate) = find_exact_candidate(candidates, &binding.source) else {
                    let code = match binding.source {
                        SourceRef::StepOutput { .. } => CORE_R3203,
                        SourceRef::ContractInput { .. } => CORE_R3101,
                    };
                    findings.push(CoreDiagnostic::new(
                        code,
                        FindingClass::Missing,
                        "contract_author",
                        contract_location(pointer),
                        format!("source `{}` does not exist", binding.source.label()),
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
                        "contract_author",
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
                        "contract_author",
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
                [] if slot.required && !blocked_by_invalid_source => {
                    findings.push(CoreDiagnostic::new(
                        CORE_R3101,
                        FindingClass::Missing,
                        "contract_author",
                        logical_input_location(step_index, &slot.slot_id),
                        format!(
                            "required input slot `{}` has no compatible source",
                            slot.slot_id
                        ),
                    ))
                }
                [_, _, ..] if slot.required => {
                    let candidate_labels = matches
                        .iter()
                        .map(|candidate| candidate.source.label())
                        .collect();
                    findings.push(
                        CoreDiagnostic::new(
                            CORE_R3102,
                            FindingClass::Invalid,
                            "contract_author",
                            logical_input_location(step_index, &slot.slot_id),
                            format!(
                                "required input slot `{}` has multiple compatible sources",
                                slot.slot_id
                            ),
                        )
                        .with_repair(DiagnosticRepair {
                            applicability: RepairApplicability::ConstrainedChoice,
                            candidates: candidate_labels,
                        }),
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

fn add_dependency(resolution: &mut WorkflowResolution, destination_step: &str, source: &SourceRef) {
    if let SourceRef::StepOutput { step_id, .. } = source {
        resolution
            .dependencies
            .entry(destination_step.into())
            .or_default()
            .insert(step_id.clone());
    }
}

fn validate_graph(
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
                "contract_author",
                contract_location(format!("/workflow/{primary_index}/bindings")),
                format!("workflow bindings contain a dependency cycle: {cyclic:?}"),
            )
            .with_related(related),
        );
    }
    order
}

fn cyclic_nodes(
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

fn can_reach_self(
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

fn compile_requirements(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    candidates: &[Candidate],
    invalid_sources: &BTreeSet<SourceRef>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Vec<CompiledRequirement> {
    let mut compiled = Vec::new();
    for (index, requirement) in contract.requirements.iter().enumerate() {
        let Some(metric) = &requirement.metric else {
            findings.push(CoreDiagnostic::new(
                CORE_R3301,
                FindingClass::Missing,
                "contract_author",
                contract_location(format!("/requirements/{index}/metric")),
                format!(
                    "requirement `{}` does not name a metric source",
                    requirement.requirement_id
                ),
            ));
            continue;
        };
        let Some(candidate) = find_exact_candidate(candidates, metric) else {
            findings.push(CoreDiagnostic::new(
                CORE_R3301,
                FindingClass::Missing,
                "contract_author",
                contract_location(format!("/requirements/{index}/metric")),
                format!("metric source `{}` does not exist", metric.label()),
            ));
            continue;
        };
        if invalid_sources.contains(&candidate.source) {
            continue;
        }
        if !candidate
            .claim_models
            .iter()
            .any(|model| claim_model_satisfies(model, &requirement.basis, requirement.comparison))
        {
            let irreducible = !candidate.claim_models.is_empty()
                && candidate
                    .claim_models
                    .iter()
                    .all(ClaimModelDeclaration::is_kernel_irreducible);
            let mut diagnostic = CoreDiagnostic::new(
                if irreducible { CORE_T2203 } else { CORE_T2201 },
                FindingClass::Unsatisfied,
                "contract_author",
                contract_location(format!("/requirements/{index}/basis")),
                if irreducible {
                    "metric source permits only claim models that the semantic kernel cannot reduce"
                } else {
                    "metric source cannot emit a claim model sufficient for this comparison basis"
                },
            );
            if irreducible {
                diagnostic = diagnostic.with_repair(DiagnosticRepair {
                    applicability: RepairApplicability::MethodOwnerJudgment,
                    candidates: vec!["core.uncertainty.expand@1".into()],
                });
            }
            findings.push(diagnostic);
        }
        let Some(role) = registry.roles.get(&candidate.role) else {
            continue;
        };
        let Some(metric_kind) = role.quantity_kind.as_deref() else {
            findings.push(CoreDiagnostic::new(
                CORE_T2102,
                FindingClass::Invalid,
                "contract_author",
                contract_location(format!("/requirements/{index}/metric")),
                format!(
                    "metric role `{}@{}` is not a quantity role",
                    role.role.id, role.role.major
                ),
            ));
            continue;
        };
        if metric_kind != requirement.limit.kind {
            findings.push(CoreDiagnostic::new(
                CORE_T2102,
                FindingClass::Invalid,
                "contract_author",
                contract_location(format!("/requirements/{index}/limit/kind")),
                format!(
                    "limit kind `{}` does not match metric kind `{metric_kind}`",
                    requirement.limit.kind
                ),
            ));
            continue;
        }
        if !registry.kind_classes.contains_key(metric_kind) {
            continue;
        }
        let canonical = match registry.kinds.scale_quantity(
            metric_kind,
            &requirement.limit.value,
            &requirement.limit.unit,
        ) {
            Ok(canonical) => canonical,
            Err(error) => {
                let code = match error.code() {
                    avila_core_kernel::CORE_T2001 => CORE_T2001,
                    avila_core_kernel::CORE_T2102 => CORE_T2103,
                    other => other,
                };
                let repair = error.repair().map(|repair| DiagnosticRepair {
                    applicability: match repair.applicability {
                        avila_core_kernel::RepairApplicability::MechanicallySafe => {
                            RepairApplicability::MechanicallySafe
                        }
                        avila_core_kernel::RepairApplicability::ConstrainedChoice => {
                            RepairApplicability::ConstrainedChoice
                        }
                        avila_core_kernel::RepairApplicability::MethodOwnerJudgment => {
                            RepairApplicability::MethodOwnerJudgment
                        }
                    },
                    candidates: repair.candidates.clone(),
                });
                let mut diagnostic = CoreDiagnostic::new(
                    code,
                    FindingClass::Invalid,
                    "contract_author",
                    contract_location(format!("/requirements/{index}/limit/unit")),
                    error.detail(),
                );
                if let Some(repair) = repair {
                    diagnostic = diagnostic.with_repair(repair);
                }
                findings.push(diagnostic);
                continue;
            }
        };
        compiled.push(CompiledRequirement {
            requirement_id: requirement.requirement_id.clone(),
            statement: requirement.statement.clone(),
            metric: metric.clone(),
            metric_role: candidate.role.clone(),
            comparison: requirement.comparison,
            limit: CanonicalTypedQuantity {
                kind: metric_kind.into(),
                value: canonical.value.canonical_rational(),
                unit: canonical.canonical_unit,
            },
            basis: requirement.basis.clone(),
        });
    }
    compiled
}

fn claim_model_satisfies(
    model: &ClaimModelDeclaration,
    basis: &RequirementBasis,
    comparison: Comparison,
) -> bool {
    match basis.kind {
        BasisKind::Bounded => match model {
            ClaimModelDeclaration::Exact
            | ClaimModelDeclaration::Interval { .. }
            | ClaimModelDeclaration::CoverageInterval => true,
            ClaimModelDeclaration::WorstCase { side, .. } => match comparison {
                Comparison::LessThan | Comparison::LessThanOrEqual => *side == BoundSide::Upper,
                Comparison::GreaterThan | Comparison::GreaterThanOrEqual => {
                    *side == BoundSide::Lower
                }
                Comparison::Equal => false,
            },
            ClaimModelDeclaration::StandardUncertainty
            | ClaimModelDeclaration::Samples
            | ClaimModelDeclaration::Distribution
            | ClaimModelDeclaration::Unquantified => false,
        },
        BasisKind::Enclosure => matches!(
            model,
            ClaimModelDeclaration::Exact | ClaimModelDeclaration::Interval { .. }
        ),
        BasisKind::Nominal => match model {
            ClaimModelDeclaration::Exact
            | ClaimModelDeclaration::CoverageInterval
            | ClaimModelDeclaration::Unquantified => true,
            ClaimModelDeclaration::Interval { nominal }
            | ClaimModelDeclaration::WorstCase { nominal, .. } => *nominal,
            ClaimModelDeclaration::StandardUncertainty
            | ClaimModelDeclaration::Samples
            | ClaimModelDeclaration::Distribution => false,
        },
    }
}

fn find_exact_candidate<'a>(
    candidates: &'a [Candidate],
    source: &SourceRef,
) -> Option<&'a Candidate> {
    candidates
        .iter()
        .find(|candidate| &candidate.source == source)
}

fn build_compiled_steps(
    contract: &ContractSource,
    bindings: &BTreeMap<String, Vec<ResolvedBinding>>,
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
                parameters: step.parameters.clone(),
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
) -> Result<CompiledContract, CompilerError> {
    #[derive(Serialize)]
    struct IdentityBody<'a> {
        schema_version: &'a str,
        semantic_profile: &'a str,
        compiler: &'a str,
        contract_id: &'a str,
        contract_revision: u64,
        contract_sha256: &'a str,
        registry_id: &'a str,
        registry_revision: u64,
        registry_sha256: &'a str,
        inputs: &'a [ContractInput],
        workflow: &'a [CompiledStep],
        requirements: &'a [CompiledRequirement],
    }

    let body = IdentityBody {
        schema_version: COMPILED_CONTRACT_SCHEMA_VERSION,
        semantic_profile: SEMANTIC_PROFILE,
        compiler: COMPILER_ID,
        contract_id: &contract.contract_id,
        contract_revision: contract.revision,
        contract_sha256: &contract_sha256,
        registry_id: &registry.registry_id,
        registry_revision: registry.revision,
        registry_sha256: &registry_sha256,
        inputs: &inputs,
        workflow: &workflow,
        requirements: &requirements,
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
        registry_id: registry.registry_id.clone(),
        registry_revision: registry.revision,
        registry_sha256,
        inputs,
        workflow,
        requirements,
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

fn find_duplicate_ids<'a>(
    values: impl Iterator<Item = &'a str>,
    kind: &str,
    collection_pointer: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for (index, value) in values.enumerate() {
        if !seen.insert(value) {
            invalid_value(
                contract_location(format!("{collection_pointer}/{index}")),
                format!("duplicate {kind} identifier `{value}`"),
                "contract_author",
                findings,
            );
        }
    }
}

fn validate_versioned_ref(
    reference: &VersionedRef,
    location: SourceLocation,
    owner: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    if reference.id.trim().is_empty() || reference.major == 0 {
        invalid_value(
            location,
            "a versioned reference requires a nonempty id and major version of at least one",
            owner,
            findings,
        );
    }
}

fn require_nonempty(
    value: &str,
    location: SourceLocation,
    owner: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    if value.trim().is_empty() {
        invalid_value(location, "value must not be empty", owner, findings);
    }
}

fn invalid_value(
    location: SourceLocation,
    message: impl Into<String>,
    owner: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_S1102,
        FindingClass::Invalid,
        owner,
        location,
        message,
    ));
}

fn registry_incomplete(
    location: SourceLocation,
    message: impl Into<String>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_R3501,
        FindingClass::Invalid,
        "registry_owner",
        location,
        message,
    ));
}

fn logical_input_location(step_index: usize, slot: &str) -> SourceLocation {
    contract_location(format!(
        "/workflow/{step_index}/inputs/{}",
        escape_pointer_token(slot)
    ))
}

fn contract_location(pointer: impl Into<String>) -> SourceLocation {
    SourceLocation::new("contract", pointer)
}

fn registry_location(pointer: impl Into<String>) -> SourceLocation {
    SourceLocation::new("registry", pointer)
}

fn owner_for(document: &str) -> &'static str {
    match document {
        "registry" => "registry_owner",
        _ => "contract_author",
    }
}

fn escape_pointer_token(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn sort_findings(findings: &mut [CoreDiagnostic]) {
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

fn step_index(contract: &ContractSource, step_id: &str) -> Option<usize> {
    contract
        .workflow
        .iter()
        .position(|step| step.step_id == step_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthoredBinding, ContractStatus};

    const CONTRACT: &[u8] = include_bytes!(
        "../../../fixtures/semantic-core/types/types.R1.resolved.pass.contract.json"
    );
    const REGISTRY: &[u8] =
        include_bytes!("../../../fixtures/semantic-core/types/compiler.registry.v1.json");

    fn contract() -> ContractSource {
        serde_json::from_slice(CONTRACT).unwrap()
    }

    fn registry() -> RegistrySnapshot {
        serde_json::from_slice(REGISTRY).unwrap()
    }

    fn compile_contract(contract: &ContractSource) -> CompileReport {
        compile_documents(&serde_json::to_vec(contract).unwrap(), REGISTRY).unwrap()
    }

    fn compile_with_registry(
        contract: &ContractSource,
        registry: &RegistrySnapshot,
    ) -> CompileReport {
        compile_documents(
            &serde_json::to_vec(contract).unwrap(),
            &serde_json::to_vec(registry).unwrap(),
        )
        .unwrap()
    }

    fn codes(report: &CompileReport) -> BTreeSet<&str> {
        report
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect()
    }

    #[test]
    fn resolved_fixture_compiles_deterministically() {
        let first = compile_documents(CONTRACT, REGISTRY).unwrap();
        let second = compile_documents(CONTRACT, REGISTRY).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.status, CompilationStatus::Compiled);
        assert!(first.findings.is_empty());
        let compiled = first.compiled.unwrap();
        assert_eq!(compiled.workflow.len(), 2);
        assert_eq!(compiled.workflow[1].bindings.len(), 1);
        assert_eq!(compiled.requirements[0].limit.value, "1/36000000");
        assert_eq!(compiled.requirements[0].limit.unit, "Sv/s");
        assert_eq!(
            compiled.snapshot_sha256,
            "sha256:87b8c9ec8abf99904a84abdd65716f0f88415331c53f17359de25cd1e1d9d0d8"
        );
    }

    #[test]
    fn authoritative_failures_become_findings() {
        let report = compile_documents(br#"{"value":1.0}"#, REGISTRY).unwrap();
        assert_eq!(report.status, CompilationStatus::Rejected);
        assert_eq!(report.findings[0].code, CORE_S1102);
        assert!(report.compiled.is_none());
    }

    #[test]
    fn required_slots_fail_closed_when_unresolved_or_ambiguous() {
        let mut unresolved = contract();
        unresolved.inputs.clear();
        let report = compile_contract(&unresolved);
        assert!(codes(&report).contains(CORE_R3101));

        let mut ambiguous = contract();
        let mut second = ambiguous.inputs[0].clone();
        second.input_id = "second-case".into();
        ambiguous.inputs.push(second);
        let report = compile_contract(&ambiguous);
        assert!(codes(&report).contains(CORE_R3102));
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.code == CORE_R3102)
            .unwrap();
        assert_eq!(
            finding.repairs[0].applicability,
            RepairApplicability::ConstrainedChoice
        );
        assert_eq!(finding.repairs[0].candidates.len(), 2);
    }

    #[test]
    fn explicit_binding_resolves_ambiguity_without_mutating_source() {
        let mut source = contract();
        let mut second = source.inputs[0].clone();
        second.input_id = "second-case".into();
        source.inputs.push(second);
        source.workflow[0].bindings.push(AuthoredBinding {
            input_slot: "source".into(),
            source: SourceRef::ContractInput {
                input_id: "case".into(),
            },
        });

        let report = compile_contract(&source);
        assert_eq!(report.status, CompilationStatus::Compiled);
        let compiled = report.compiled.unwrap();
        assert_eq!(
            compiled.workflow[0].bindings[0].source,
            SourceRef::ContractInput {
                input_id: "case".into()
            }
        );
        assert_eq!(source.inputs.len(), 2);
    }

    #[test]
    fn explicit_binding_checks_nominal_role_and_media_independently() {
        let mut role_mismatch = contract();
        role_mismatch.inputs[0].role = VersionedRef {
            id: "fixture.alternate_source_document".into(),
            major: 1,
        };
        role_mismatch.workflow[0].bindings.push(AuthoredBinding {
            input_slot: "source".into(),
            source: SourceRef::ContractInput {
                input_id: "case".into(),
            },
        });
        let report = compile_contract(&role_mismatch);
        assert!(codes(&report).contains(CORE_T2101));
        assert!(!codes(&report).contains(CORE_T2301));

        let mut media_mismatch = contract();
        media_mismatch.inputs[0].media_type = "application/octet-stream".into();
        media_mismatch.workflow[0].bindings.push(AuthoredBinding {
            input_slot: "source".into(),
            source: SourceRef::ContractInput {
                input_id: "case".into(),
            },
        });
        let report = compile_contract(&media_mismatch);
        assert!(codes(&report).contains(CORE_T2301));
        assert!(!codes(&report).contains(CORE_T2101));
        let media_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.code == CORE_T2301)
            .collect();
        assert_eq!(media_findings.len(), 1);
        assert_eq!(media_findings[0].primary.pointer, "/inputs/0/media_type");
    }

    #[test]
    fn graph_shape_rejects_self_unknown_and_cyclic_dependencies() {
        let mut self_dependency = contract();
        self_dependency.workflow[1].bindings.push(AuthoredBinding {
            input_slot: "dose_rate".into(),
            source: SourceRef::StepOutput {
                step_id: "bound".into(),
                output_slot: "bounded_dose_rate".into(),
            },
        });
        assert!(codes(&compile_contract(&self_dependency)).contains(CORE_R3201));

        let mut unknown = contract();
        unknown.workflow[1].bindings.push(AuthoredBinding {
            input_slot: "dose_rate".into(),
            source: SourceRef::StepOutput {
                step_id: "missing".into(),
                output_slot: "bounded_dose_rate".into(),
            },
        });
        assert!(codes(&compile_contract(&unknown)).contains(CORE_R3203));

        let mut cycle = contract();
        cycle.inputs.clear();
        cycle.workflow = vec![cycle.workflow[1].clone(), cycle.workflow[1].clone()];
        cycle.workflow[0].step_id = "left".into();
        cycle.workflow[1].step_id = "right".into();
        cycle.workflow[0].bindings = vec![AuthoredBinding {
            input_slot: "dose_rate".into(),
            source: SourceRef::StepOutput {
                step_id: "right".into(),
                output_slot: "bounded_dose_rate".into(),
            },
        }];
        cycle.workflow[1].bindings = vec![AuthoredBinding {
            input_slot: "dose_rate".into(),
            source: SourceRef::StepOutput {
                step_id: "left".into(),
                output_slot: "bounded_dose_rate".into(),
            },
        }];
        cycle.requirements[0].metric = Some(SourceRef::StepOutput {
            step_id: "right".into(),
            output_slot: "bounded_dose_rate".into(),
        });
        assert!(codes(&compile_contract(&cycle)).contains(CORE_R3202));
    }

    #[test]
    fn requirements_need_a_metric_and_exactly_compatible_limit() {
        let mut unbound = contract();
        unbound.requirements[0].metric = None;
        assert!(codes(&compile_contract(&unbound)).contains(CORE_R3301));

        let mut wrong_kind = contract();
        wrong_kind.requirements[0].limit.kind = "nuclear.absorbed_dose_rate".into();
        assert!(codes(&compile_contract(&wrong_kind)).contains(CORE_T2102));

        let mut wrong_unit_class = contract();
        wrong_unit_class.requirements[0].limit.unit = "Gy/s".into();
        assert!(codes(&compile_contract(&wrong_unit_class)).contains(CORE_T2103));

        let mut unknown_unit = contract();
        unknown_unit.requirements[0].limit.unit = "Mpa".into();
        assert!(codes(&compile_contract(&unknown_unit)).contains(CORE_T2001));
    }

    #[test]
    fn type_level_claim_models_must_satisfy_the_comparison_basis() {
        let source = contract();

        let mut irreducible = registry();
        irreducible.capability_types[1].outputs[0].permitted_claim_models =
            vec![ClaimModelDeclaration::StandardUncertainty];
        let report = compile_with_registry(&source, &irreducible);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.code == CORE_T2203)
            .unwrap();
        assert_eq!(finding.class, FindingClass::Unsatisfied);
        assert_eq!(
            finding.repairs[0].applicability,
            RepairApplicability::MethodOwnerJudgment
        );

        let mut wrong_side = registry();
        wrong_side.capability_types[1].outputs[0].permitted_claim_models =
            vec![ClaimModelDeclaration::WorstCase {
                side: BoundSide::Lower,
                nominal: false,
            }];
        assert!(codes(&compile_with_registry(&source, &wrong_side)).contains(CORE_T2201));

        let mut sufficient_side = registry();
        sufficient_side.capability_types[1].outputs[0].permitted_claim_models =
            vec![ClaimModelDeclaration::WorstCase {
                side: BoundSide::Upper,
                nominal: false,
            }];
        assert_eq!(
            compile_with_registry(&source, &sufficient_side).status,
            CompilationStatus::Compiled
        );

        let mut coverage_for_enclosure = registry();
        coverage_for_enclosure.capability_types[1].outputs[0].permitted_claim_models =
            vec![ClaimModelDeclaration::CoverageInterval];
        let mut enclosure_contract = source;
        enclosure_contract.requirements[0].basis.kind = BasisKind::Enclosure;
        assert!(
            codes(&compile_with_registry(
                &enclosure_contract,
                &coverage_for_enclosure
            ))
            .contains(CORE_T2201)
        );
    }

    #[test]
    fn independent_root_findings_are_reported_in_one_pass() {
        let mut source = contract();
        source.status = ContractStatus::Approved;
        source.inputs.clear();
        source.requirements[0].metric = None;
        source.workflow[1].bindings.push(AuthoredBinding {
            input_slot: "dose_rate".into(),
            source: SourceRef::StepOutput {
                step_id: "missing".into(),
                output_slot: "bounded_dose_rate".into(),
            },
        });
        let report = compile_contract(&source);
        let actual = codes(&report);
        assert!(actual.contains(CORE_R3101));
        assert!(actual.contains(CORE_R3203));
        assert!(actual.contains(CORE_R3301));
    }
}
