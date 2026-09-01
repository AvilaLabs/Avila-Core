use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use avila_core_kernel::{
    CanonicalJsonValue, EXACT_NUMBER_DECODE_PREFIX, ExactNumber, KindDefinition, KindRegistry,
    SEMANTIC_PROFILE, UnitDefinition, canonicalize_json, read_authoritative_decimal,
    read_authoritative_json,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::diagnostic::{
    CORE_A4301, CORE_R3101, CORE_R3102, CORE_R3201, CORE_R3202, CORE_R3203, CORE_R3301, CORE_R3401,
    CORE_R3501, CORE_S1101, CORE_S1102, CORE_S1301, CORE_T2001, CORE_T2101, CORE_T2102, CORE_T2103,
    CORE_T2104, CORE_T2201, CORE_T2203, CORE_T2301, CORE_T2401, CORE_T2402, CORE_T2501, CORE_T2601,
    CoreDiagnostic, DiagnosticRepair, FindingClass, RepairApplicability, SourceLocation,
};
use crate::document::{
    BasisKind, BoundSide, COMPILE_REPORT_SCHEMA_VERSION, COMPILED_CONTRACT_SCHEMA_VERSION,
    CONTRACT_SCHEMA_VERSION, CapabilityTypeDefinition, ClaimModelDeclaration, Comparison,
    ContractInput, ContractSource, ContractStatus, DeterminismClass, ExactBound, ExecutionPolicy,
    ImmutablePolicyRef, IntegerBound, ParameterDefinition, ParameterType, PurposeDefinition,
    QuantityBound, QuantityValue, REGISTRY_SCHEMA_VERSION, RegistrySnapshot, RequirementBasis,
    RequirementSource, ReviewDisposition, ReviewIndependence, ReviewParty, RoleDefinition,
    SourceRef, TypedQuantity, VersionedRef,
};

pub const COMPILE_NOTICE: &str = "Compilation establishes structural and semantic consistency under the named draft profile only. It performs no execution, review fulfillment, reviewer-eligibility or trust evaluation, evidence admission, scientific qualification, or requirement verdict.";
const COMPILER_ID: &str = concat!("avila.core/compiler-rust@", env!("CARGO_PKG_VERSION"));
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_WORKFLOW_STEPS: usize = 2_048;
const NOT_DEFINED_PLACEHOLDER: &str = "not_defined";

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
    let compiled_parameters = compile_parameters(&contract, &registry_index, &mut findings);
    let compiled_reproducibility =
        compile_reproducibility(&contract, &registry_index, &mut findings);

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
    let compiled_reviews = compile_review_obligations(
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
    let workflow = build_compiled_steps(
        &contract,
        &resolution.bindings,
        &compiled_parameters,
        &compiled_reproducibility,
        &compiled_reviews,
        &order,
    );
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
    let canonical_value = match read_authoritative_json(bytes) {
        Ok(value) => value,
        Err(error) => {
            findings.push(CoreDiagnostic::new(
                error.code(),
                FindingClass::Invalid,
                owner_for(document),
                SourceLocation::new(document, error.pointer().unwrap_or_default()),
                error.detail(),
            ));
            return None;
        }
    };
    let canonical = match serde_json::to_vec(&canonical_value) {
        Ok(canonical) => canonical,
        Err(error) => {
            findings.push(CoreDiagnostic::new(
                CORE_S1102,
                FindingClass::Invalid,
                owner_for(document),
                SourceLocation::new(document, ""),
                error.to_string(),
            ));
            return None;
        }
    };
    identities.push(DocumentIdentity {
        document: document.into(),
        sha256: prefixed_sha256(&canonical),
    });

    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    match serde_path_to_error::deserialize::<_, T>(&mut deserializer) {
        Ok(value) => Some(value),
        Err(error) => {
            let detail = error.inner().to_string();
            let mut pointer = pointer_from_decode_path(error.path());
            let code = if let Some(field) = unknown_field_name(&detail) {
                let token = escape_pointer_token(field);
                if pointer.rsplit('/').next() != Some(token.as_str()) {
                    pointer.push('/');
                    pointer.push_str(&token);
                }
                CORE_S1101
            } else {
                CORE_S1102
            };
            let repair = canonical_number_repair(&canonical_value, &pointer, &detail);
            let mut diagnostic = CoreDiagnostic::new(
                code,
                FindingClass::Invalid,
                owner_for(document),
                SourceLocation::new(document, pointer),
                detail,
            );
            if let Some(repair) = repair {
                diagnostic = diagnostic.with_repair(repair);
            }
            findings.push(diagnostic);
            None
        }
    }
}

/// Lowers the path at which typed decoding stopped to a JSON Pointer.
///
/// Enum-variant segments are not JSON keys and are omitted. Decoding inside an
/// internally tagged enum is buffered by serde, so a failure there points at
/// the enum value rather than the field inside it.
fn pointer_from_decode_path(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;

    let mut pointer = String::new();
    for segment in path.iter() {
        match segment {
            Segment::Seq { index } => {
                pointer.push('/');
                pointer.push_str(&index.to_string());
            }
            Segment::Map { key } => {
                pointer.push('/');
                pointer.push_str(&escape_pointer_token(key));
            }
            Segment::Enum { .. } | Segment::Unknown => {}
        }
    }
    pointer
}

/// Extracts the field name from serde's stable `unknown field` message so the
/// finding can point at the offending key even when the decode path stopped at
/// the parent object.
fn unknown_field_name(detail: &str) -> Option<&str> {
    let rest = detail.strip_prefix("unknown field `")?;
    let end = rest.find('`')?;
    Some(&rest[..end])
}

/// Recovers the mechanically safe canonical form for a number that failed
/// typed decoding, by re-reading the raw authored string at the pointer.
fn canonical_number_repair(
    document: &CanonicalJsonValue,
    pointer: &str,
    detail: &str,
) -> Option<DiagnosticRepair> {
    if !detail.starts_with(EXACT_NUMBER_DECODE_PREFIX) {
        return None;
    }
    let CanonicalJsonValue::String(raw) = document.pointer(pointer)? else {
        return None;
    };
    ExactNumber::from_canonical(raw)
        .err()?
        .repair()
        .map(compiler_repair)
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

    let mut permitted_roles = BTreeSet::new();
    for (index, role) in contract
        .execution_policy
        .permitted_nondeterministic_roles
        .iter()
        .enumerate()
    {
        let location = contract_location(format!(
            "/execution_policy/permitted_nondeterministic_roles/{index}"
        ));
        validate_versioned_ref(role, location.clone(), "policy_owner", findings);
        if !permitted_roles.insert(role) {
            invalid_value(
                location,
                format!(
                    "nondeterminism permission repeats role `{}@{}`",
                    role.id, role.major
                ),
                "policy_owner",
                findings,
            );
        }
    }

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
        validate_versioned_ref(
            &requirement.purpose,
            contract_location(format!("/requirements/{index}/purpose")),
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
        if let Some(tolerance) = &requirement.tolerance {
            require_nonempty(
                &tolerance.kind,
                contract_location(format!("/requirements/{index}/tolerance/kind")),
                "contract_author",
                findings,
            );
            require_nonempty(
                &tolerance.unit,
                contract_location(format!("/requirements/{index}/tolerance/unit")),
                "contract_author",
                findings,
            );
        }
    }
}

struct RegistryIndex<'a> {
    kinds: KindRegistry,
    kind_classes: BTreeMap<&'a str, &'a str>,
    purposes: BTreeMap<VersionedRef, &'a PurposeDefinition>,
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

        let mut purposes = BTreeMap::new();
        for (index, purpose) in registry.purposes.iter().enumerate() {
            let pointer = format!("/purposes/{index}");
            validate_versioned_ref(
                &purpose.purpose,
                registry_location(format!("{pointer}/purpose")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &purpose.owner,
                registry_location(format!("{pointer}/owner")),
                "registry_owner",
                findings,
            );
            require_nonempty(
                &purpose.description,
                registry_location(format!("{pointer}/description")),
                "registry_owner",
                findings,
            );
            if purposes.insert(purpose.purpose.clone(), purpose).is_some() {
                registry_incomplete(
                    registry_location(format!("{pointer}/purpose")),
                    format!(
                        "duplicate governed purpose `{}@{}`",
                        purpose.purpose.id, purpose.purpose.major
                    ),
                    findings,
                );
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
            validate_slots(capability, index, &roles, &purposes, findings);
            validate_parameter_definitions(capability, index, &kinds, &kind_classes, findings);
            validate_reproducibility_declaration(
                capability,
                index,
                &kinds,
                &kind_classes,
                findings,
            );
            validate_review_declaration(capability, index, &roles, findings);
        }

        Self {
            kinds,
            kind_classes,
            purposes,
            roles,
            capability_types,
        }
    }
}

fn validate_review_declaration(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    roles: &BTreeMap<VersionedRef, &RoleDefinition>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let Some(review) = &capability.review else {
        return;
    };
    let review_pointer = format!("/capability_types/{capability_index}/review");

    if capability.reproducibility.determinism != DeterminismClass::Nondeterministic {
        review_incomplete(
            registry_location(format!(
                "/capability_types/{capability_index}/reproducibility/determinism"
            )),
            "an accountable review capability must remain nondeterministic",
            "registry_owner",
            findings,
        );
    }
    if capability.inputs.is_empty() {
        review_incomplete(
            registry_location(format!("{review_pointer}/presented_input_slots")),
            "a review dossier must contain at least one evidence input",
            "registry_owner",
            findings,
        );
    }

    let input_slots: BTreeSet<_> = capability
        .inputs
        .iter()
        .map(|slot| slot.slot_id.as_str())
        .collect();
    let mut presented = BTreeSet::new();
    for (index, slot_id) in review.presented_input_slots.iter().enumerate() {
        let location = registry_location(format!("{review_pointer}/presented_input_slots/{index}"));
        if slot_id.trim().is_empty() {
            review_incomplete(
                location,
                "a presented review slot must not be empty",
                "registry_owner",
                findings,
            );
            continue;
        }
        if !presented.insert(slot_id.as_str()) {
            review_incomplete(
                location,
                format!("review dossier repeats input slot `{slot_id}`"),
                "registry_owner",
                findings,
            );
        } else if !input_slots.contains(slot_id.as_str()) {
            review_incomplete(
                location,
                format!("review dossier references unknown input slot `{slot_id}`"),
                "registry_owner",
                findings,
            );
        }
    }
    for (input_index, input) in capability.inputs.iter().enumerate() {
        if !input.required {
            review_incomplete(
                registry_location(format!(
                    "/capability_types/{capability_index}/inputs/{input_index}/required"
                )),
                format!(
                    "review input slot `{}` must be required so the compiled dossier is exact",
                    input.slot_id
                ),
                "registry_owner",
                findings,
            );
        }
        if !presented.contains(input.slot_id.as_str()) {
            review_incomplete(
                registry_location(format!("{review_pointer}/presented_input_slots")),
                format!(
                    "review dossier omits declared input slot `{}`",
                    input.slot_id
                ),
                "registry_owner",
                findings,
            );
        }
    }

    if review.allowed_dispositions.is_empty() {
        review_incomplete(
            registry_location(format!("{review_pointer}/allowed_dispositions")),
            "a review must declare at least one governance disposition",
            "registry_owner",
            findings,
        );
    }
    let mut dispositions = BTreeSet::new();
    for (index, disposition) in review.allowed_dispositions.iter().enumerate() {
        if !dispositions.insert(*disposition) {
            review_incomplete(
                registry_location(format!("{review_pointer}/allowed_dispositions/{index}")),
                "a governance disposition may be declared only once",
                "registry_owner",
                findings,
            );
        }
    }

    if review.decision_output_slot.trim().is_empty() {
        review_incomplete(
            registry_location(format!("{review_pointer}/decision_output_slot")),
            "a review must name its decision output slot",
            "registry_owner",
            findings,
        );
        return;
    }
    let Some(output) = capability
        .outputs
        .iter()
        .find(|output| output.slot_id == review.decision_output_slot)
    else {
        review_incomplete(
            registry_location(format!("{review_pointer}/decision_output_slot")),
            format!(
                "review decision output `{}` is not declared by the capability type",
                review.decision_output_slot
            ),
            "registry_owner",
            findings,
        );
        return;
    };
    if capability.outputs.len() != 1 {
        review_incomplete(
            registry_location(format!("/capability_types/{capability_index}/outputs")),
            "the current R9 subset permits a review capability to emit only its decision record",
            "registry_owner",
            findings,
        );
    }
    if output.permitted_claim_models.as_slice() != [ClaimModelDeclaration::Unquantified] {
        review_incomplete(
            registry_location(format!(
                "/capability_types/{capability_index}/outputs/{}/permitted_claim_models",
                capability
                    .outputs
                    .iter()
                    .position(|candidate| candidate.slot_id == output.slot_id)
                    .unwrap_or(0)
            )),
            "a review disposition is governance evidence and must use only the unquantified claim model",
            "registry_owner",
            findings,
        );
    }
    if let Some(role) = roles.get(&output.role)
        && (role.quantity_kind.is_some() || role.unit_class.is_some())
    {
        review_incomplete(
            registry_location(format!("{review_pointer}/decision_output_slot")),
            "a review decision role must be non-quantitative and cannot serve as a technical requirement metric",
            "registry_owner",
            findings,
        );
    }
}

fn validate_parameter_definitions(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    kinds: &KindRegistry,
    kind_classes: &BTreeMap<&str, &str>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut parameter_ids = BTreeSet::new();
    for (index, parameter) in capability.parameters.iter().enumerate() {
        let pointer = format!("/capability_types/{capability_index}/parameters/{index}");
        require_nonempty(
            &parameter.parameter_id,
            registry_location(format!("{pointer}/parameter_id")),
            "registry_owner",
            findings,
        );
        if !parameter_ids.insert(parameter.parameter_id.as_str()) {
            invalid_value(
                registry_location(format!("{pointer}/parameter_id")),
                format!("duplicate parameter `{}`", parameter.parameter_id),
                "registry_owner",
                findings,
            );
        }
        validate_value_type_definition(
            "parameter",
            &parameter.parameter_id,
            &parameter.value_type,
            &pointer,
            kinds,
            kind_classes,
            findings,
        );
    }
}

fn validate_reproducibility_declaration(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    kinds: &KindRegistry,
    kind_classes: &BTreeMap<&str, &str>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut factor_ids = BTreeSet::new();
    for (index, factor) in capability
        .reproducibility
        .material_factors
        .iter()
        .enumerate()
    {
        let pointer = format!(
            "/capability_types/{capability_index}/reproducibility/material_factors/{index}"
        );
        require_nonempty(
            &factor.factor_id,
            registry_location(format!("{pointer}/factor_id")),
            "registry_owner",
            findings,
        );
        if !factor_ids.insert(factor.factor_id.as_str()) {
            invalid_value(
                registry_location(format!("{pointer}/factor_id")),
                format!("duplicate material execution factor `{}`", factor.factor_id),
                "registry_owner",
                findings,
            );
        }
        validate_value_type_definition(
            "material execution factor",
            &factor.factor_id,
            &factor.value_type,
            &pointer,
            kinds,
            kind_classes,
            findings,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_value_type_definition(
    value_kind: &str,
    value_id: &str,
    value_type: &ParameterType,
    pointer: &str,
    kinds: &KindRegistry,
    kind_classes: &BTreeMap<&str, &str>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    match value_type {
        ParameterType::Boolean => {}
        ParameterType::Integer { min, max } => {
            if let (Some(min), Some(max)) = (min, max)
                && range_is_empty(min.value.cmp(&max.value), min.inclusive, max.inclusive)
            {
                registry_incomplete(
                    registry_location(format!("{pointer}/value_type")),
                    format!("{value_kind} `{value_id}` declares an empty integer domain"),
                    findings,
                );
            }
        }
        ParameterType::ExactNumber { min, max } => {
            validate_exact_value_range(value_kind, value_id, min, max, pointer, findings);
        }
        ParameterType::Text { allowed_values } => {
            if let Some(values) = allowed_values {
                if values.is_empty() {
                    registry_incomplete(
                        registry_location(format!("{pointer}/value_type/allowed_values")),
                        format!(
                            "{value_kind} `{value_id}` declares an empty set of allowed text values"
                        ),
                        findings,
                    );
                }
                let mut unique = BTreeSet::new();
                if values.iter().any(|value| !unique.insert(value.as_str())) {
                    registry_incomplete(
                        registry_location(format!("{pointer}/value_type/allowed_values")),
                        format!("{value_kind} `{value_id}` declares duplicate allowed text values"),
                        findings,
                    );
                }
                if values.iter().any(|value| value == NOT_DEFINED_PLACEHOLDER) {
                    registry_incomplete(
                        registry_location(format!("{pointer}/value_type/allowed_values")),
                        format!(
                            "{value_kind} `{value_id}` cannot admit the reserved draft placeholder `{NOT_DEFINED_PLACEHOLDER}` as a text value"
                        ),
                        findings,
                    );
                }
            }
        }
        ParameterType::Quantity { kind, min, max } => {
            if !kind_classes.contains_key(kind.as_str()) {
                registry_incomplete(
                    registry_location(format!("{pointer}/value_type/kind")),
                    format!("{value_kind} `{value_id}` references unknown quantity kind `{kind}`"),
                    findings,
                );
                return;
            }
            validate_quantity_value_range(
                value_kind, value_id, kind, min, max, pointer, kinds, findings,
            );
        }
    }
}

fn validate_exact_value_range(
    value_kind: &str,
    value_id: &str,
    min: &Option<ExactBound>,
    max: &Option<ExactBound>,
    pointer: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let (Some(min), Some(max)) = (min, max) else {
        return;
    };
    match min.value.checked_cmp(&max.value) {
        Ok(ordering) if range_is_empty(ordering, min.inclusive, max.inclusive) => {
            registry_incomplete(
                registry_location(format!("{pointer}/value_type")),
                format!("{value_kind} `{value_id}` declares an empty exact-number domain"),
                findings,
            );
        }
        Ok(_) => {}
        Err(error) => registry_incomplete(
            registry_location(format!("{pointer}/value_type")),
            format!(
                "{value_kind} `{value_id}` domain cannot be compared: {}",
                error.detail()
            ),
            findings,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_quantity_value_range(
    value_kind: &str,
    value_id: &str,
    kind: &str,
    min: &Option<QuantityBound>,
    max: &Option<QuantityBound>,
    pointer: &str,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let lower = min.as_ref().and_then(|bound| {
        validate_registry_quantity_bound(
            value_kind, value_id, kind, bound, "min", pointer, kinds, findings,
        )
    });
    let upper = max.as_ref().and_then(|bound| {
        validate_registry_quantity_bound(
            value_kind, value_id, kind, bound, "max", pointer, kinds, findings,
        )
    });
    let (Some(lower), Some(upper), Some(min), Some(max)) = (lower, upper, min, max) else {
        return;
    };
    match lower.checked_cmp(&upper) {
        Ok(ordering) if range_is_empty(ordering, min.inclusive, max.inclusive) => {
            registry_incomplete(
                registry_location(format!("{pointer}/value_type")),
                format!("{value_kind} `{value_id}` declares an empty quantity domain"),
                findings,
            );
        }
        Ok(_) => {}
        Err(error) => registry_incomplete(
            registry_location(format!("{pointer}/value_type")),
            format!(
                "{value_kind} `{value_id}` domain cannot be compared: {}",
                error.detail()
            ),
            findings,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_registry_quantity_bound(
    value_kind: &str,
    value_id: &str,
    kind: &str,
    bound: &QuantityBound,
    side: &str,
    pointer: &str,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<ExactNumber> {
    match kinds.scale_quantity(kind, &bound.value.value, &bound.value.unit) {
        Ok(quantity) => Some(quantity.value),
        Err(error) => {
            registry_incomplete(
                registry_location(format!("{pointer}/value_type/{side}")),
                format!(
                    "{value_kind} `{value_id}` has an invalid {side} quantity bound: {}",
                    error.detail()
                ),
                findings,
            );
            None
        }
    }
}

fn range_is_empty(ordering: Ordering, min_inclusive: bool, max_inclusive: bool) -> bool {
    ordering == Ordering::Greater
        || (ordering == Ordering::Equal && !(min_inclusive && max_inclusive))
}

fn validate_slots(
    capability: &CapabilityTypeDefinition,
    capability_index: usize,
    roles: &BTreeMap<VersionedRef, &RoleDefinition>,
    purposes: &BTreeMap<VersionedRef, &PurposeDefinition>,
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
        let mut excluded = BTreeSet::new();
        for (purpose_index, purpose) in slot.excluded_purposes.iter().enumerate() {
            let location =
                registry_location(format!("{pointer}/excluded_purposes/{purpose_index}"));
            validate_versioned_ref(purpose, location.clone(), "registry_owner", findings);
            if !excluded.insert(purpose) {
                registry_incomplete(
                    location,
                    format!(
                        "output slot repeats excluded purpose `{}@{}`",
                        purpose.id, purpose.major
                    ),
                    findings,
                );
            } else if !purposes.contains_key(purpose) {
                registry_incomplete(
                    location,
                    format!(
                        "output slot excludes unknown governed purpose `{}@{}`",
                        purpose.id, purpose.major
                    ),
                    findings,
                );
            }
        }
    }
}

fn validate_contract_registry_refs(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> BTreeSet<SourceRef> {
    let mut invalid_sources = BTreeSet::new();
    for (index, role) in contract
        .execution_policy
        .permitted_nondeterministic_roles
        .iter()
        .enumerate()
    {
        if !registry.roles.contains_key(role) {
            findings.push(CoreDiagnostic::new(
                CORE_S1102,
                FindingClass::Invalid,
                "policy_owner",
                contract_location(format!(
                    "/execution_policy/permitted_nondeterministic_roles/{index}"
                )),
                format!(
                    "nondeterminism policy references role `{}@{}` absent from the supplied registry snapshot",
                    role.id, role.major
                ),
            ));
        }
    }
    for (index, requirement) in contract.requirements.iter().enumerate() {
        if !registry.purposes.contains_key(&requirement.purpose) {
            findings.push(CoreDiagnostic::new(
                CORE_T2601,
                FindingClass::Invalid,
                "contract_author",
                contract_location(format!("/requirements/{index}/purpose")),
                format!(
                    "requirement references governed purpose `{}@{}` absent from the supplied registry snapshot",
                    requirement.purpose.id, requirement.purpose.major
                ),
            ));
        }
    }
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
    excluded_purposes: Vec<VersionedRef>,
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
            excluded_purposes: Vec::new(),
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
            excluded_purposes: output.excluded_purposes.clone(),
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

fn compile_review_obligations(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    bindings: &BTreeMap<String, Vec<ResolvedBinding>>,
    findings: &mut Vec<CoreDiagnostic>,
) -> BTreeMap<String, CompiledReviewObligation> {
    let mut compiled = BTreeMap::new();
    for (step_index, step) in contract.workflow.iter().enumerate() {
        let Some(capability) = registry.capability_types.get(&step.capability_type) else {
            continue;
        };
        let Some(declaration) = &capability.review else {
            if step.review.is_some() {
                review_incomplete(
                    contract_location(format!("/workflow/{step_index}/review")),
                    format!(
                        "capability type `{}@{}` does not declare accountable review semantics",
                        step.capability_type.id, step.capability_type.major
                    ),
                    "policy_owner",
                    findings,
                );
            }
            continue;
        };
        let Some(binding) = &step.review else {
            findings.push(CoreDiagnostic::new(
                CORE_R3401,
                FindingClass::Missing,
                "policy_owner",
                contract_location(format!("/workflow/{step_index}/review")),
                "review capability requires an explicit eligibility policy and independence declaration",
            ));
            continue;
        };

        let policy_pointer = format!("/workflow/{step_index}/review/reviewer_eligibility_policy");
        let mut valid = true;
        if binding
            .reviewer_eligibility_policy
            .policy_id
            .trim()
            .is_empty()
        {
            review_incomplete(
                contract_location(format!("{policy_pointer}/policy_id")),
                "reviewer eligibility policy id must not be empty",
                "policy_owner",
                findings,
            );
            valid = false;
        }
        if binding.reviewer_eligibility_policy.revision == 0 {
            review_incomplete(
                contract_location(format!("{policy_pointer}/revision")),
                "reviewer eligibility policy revision must be at least one",
                "policy_owner",
                findings,
            );
            valid = false;
        }
        if !is_sha256_identity(&binding.reviewer_eligibility_policy.sha256) {
            review_incomplete(
                contract_location(format!("{policy_pointer}/sha256")),
                "reviewer eligibility policy must be pinned by a lowercase sha256 identity",
                "policy_owner",
                findings,
            );
            valid = false;
        }

        if let ReviewIndependence::Constraints { requirements } = &binding.independence {
            let independence_pointer =
                format!("/workflow/{step_index}/review/independence/requirements");
            if requirements.is_empty() {
                review_incomplete(
                    contract_location(&independence_pointer),
                    "constraint-based review independence must name at least one party",
                    "policy_owner",
                    findings,
                );
                valid = false;
            }
            let mut parties = BTreeSet::<ReviewParty>::new();
            for (index, requirement) in requirements.iter().enumerate() {
                if !parties.insert(requirement.separated_from) {
                    review_incomplete(
                        contract_location(format!("{independence_pointer}/{index}/separated_from")),
                        "review independence may constrain each party only once",
                        "policy_owner",
                        findings,
                    );
                    valid = false;
                }
            }
        }

        let Some(output) = capability
            .outputs
            .iter()
            .find(|output| output.slot_id == declaration.decision_output_slot)
        else {
            continue;
        };
        let resolved: BTreeMap<_, _> = bindings
            .get(&step.step_id)
            .into_iter()
            .flatten()
            .map(|binding| (binding.input_slot.as_str(), binding))
            .collect();
        let presented_evidence: Vec<_> = declaration
            .presented_input_slots
            .iter()
            .filter_map(|slot_id| resolved.get(slot_id.as_str()).copied().cloned())
            .collect();
        if presented_evidence.len() != declaration.presented_input_slots.len() {
            continue;
        }
        if !valid {
            continue;
        }

        compiled.insert(
            step.step_id.clone(),
            CompiledReviewObligation {
                fulfillment: ReviewFulfillment::PendingExternalReview,
                presented_evidence,
                decision_output_slot: output.slot_id.clone(),
                decision_role: output.role.clone(),
                decision_media_type: output.media_type.clone(),
                allowed_dispositions: declaration.allowed_dispositions.clone(),
                reviewer_eligibility_policy: binding.reviewer_eligibility_policy.clone(),
                independence: binding.independence.clone(),
            },
        );
    }
    compiled
}

fn is_sha256_identity(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

fn compile_reproducibility(
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
                findings.push(CoreDiagnostic::new(
                    CORE_S1101,
                    FindingClass::Invalid,
                    "requester",
                    material_factor_location(step_index, factor_id),
                    format!(
                        "capability type `{}@{}` does not declare material execution factor `{factor_id}`",
                        step.capability_type.id, step.capability_type.major
                    ),
                ));
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

fn nondeterminism_permitted(
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

const fn determinism_label(determinism: DeterminismClass) -> &'static str {
    match determinism {
        DeterminismClass::Deterministic => "deterministic",
        DeterminismClass::SeededStochastic => "seeded_stochastic",
        DeterminismClass::Nondeterministic => "nondeterministic",
    }
}

fn compile_parameters(
    contract: &ContractSource,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> BTreeMap<String, BTreeMap<String, CompiledParameterValue>> {
    let mut compiled_steps = BTreeMap::new();
    for (step_index, step) in contract.workflow.iter().enumerate() {
        let Some(capability) = registry.capability_types.get(&step.capability_type) else {
            continue;
        };
        let definitions: BTreeMap<_, _> = capability
            .parameters
            .iter()
            .map(|definition| (definition.parameter_id.as_str(), definition))
            .collect();
        let mut compiled = BTreeMap::new();

        for parameter_id in step.parameters.keys() {
            if !definitions.contains_key(parameter_id.as_str()) {
                findings.push(CoreDiagnostic::new(
                    CORE_S1101,
                    FindingClass::Invalid,
                    "requester",
                    parameter_location(step_index, parameter_id),
                    format!(
                        "capability type `{}@{}` does not declare parameter `{parameter_id}`",
                        step.capability_type.id, step.capability_type.major
                    ),
                ));
            }
        }

        for definition in &capability.parameters {
            let Some(value) = step.parameters.get(&definition.parameter_id) else {
                if definition.required {
                    add_missing_parameter_finding(
                        contract.status,
                        step_index,
                        &definition.parameter_id,
                        findings,
                    );
                }
                continue;
            };
            if value.as_str() == Some(NOT_DEFINED_PLACEHOLDER) {
                add_placeholder_parameter_finding(
                    contract.status,
                    step_index,
                    &definition.parameter_id,
                    findings,
                );
                continue;
            }
            if let Some(value) =
                lower_parameter_value(definition, value, step_index, &registry.kinds, findings)
            {
                compiled.insert(definition.parameter_id.clone(), value);
            }
        }
        compiled_steps.insert(step.step_id.clone(), compiled);
    }
    compiled_steps
}

fn add_missing_parameter_finding(
    status: ContractStatus,
    step_index: usize,
    parameter_id: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_S1301,
        placeholder_finding_class(status),
        "requester",
        parameter_location(step_index, parameter_id),
        format!("required parameter `{parameter_id}` is not defined"),
    ));
}

fn add_placeholder_parameter_finding(
    status: ContractStatus,
    step_index: usize,
    parameter_id: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_S1301,
        placeholder_finding_class(status),
        "requester",
        parameter_location(step_index, parameter_id),
        if status == ContractStatus::Draft {
            format!("parameter `{parameter_id}` remains explicitly not defined in this draft")
        } else {
            format!(
                "parameter `{parameter_id}` cannot remain `not_defined` when contract status is `{}`",
                contract_status_label(status)
            )
        },
    ));
}

const fn placeholder_finding_class(status: ContractStatus) -> FindingClass {
    if matches!(status, ContractStatus::Draft) {
        FindingClass::Missing
    } else {
        FindingClass::Invalid
    }
}

const fn contract_status_label(status: ContractStatus) -> &'static str {
    match status {
        ContractStatus::Draft => "draft",
        ContractStatus::InReview => "in_review",
        ContractStatus::Approved => "approved",
        ContractStatus::Retired => "retired",
    }
}

fn lower_parameter_value(
    definition: &ParameterDefinition,
    authored: &serde_json::Value,
    step_index: usize,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<CompiledParameterValue> {
    let location = parameter_location(step_index, &definition.parameter_id);
    lower_typed_value("parameter", definition, authored, location, kinds, findings)
}

fn lower_typed_value(
    value_kind: &str,
    definition: &ParameterDefinition,
    authored: &serde_json::Value,
    location: SourceLocation,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<CompiledParameterValue> {
    match &definition.value_type {
        ParameterType::Boolean => authored.as_bool().map_or_else(
            || {
                parameter_type_finding(
                    value_kind,
                    &definition.parameter_id,
                    "boolean",
                    location,
                    None,
                    findings,
                );
                None
            },
            |value| Some(CompiledParameterValue::Boolean { value }),
        ),
        ParameterType::Integer { min, max } => {
            let Some(value) = authored.as_i64() else {
                parameter_type_finding(
                    value_kind,
                    &definition.parameter_id,
                    "integer",
                    location,
                    None,
                    findings,
                );
                return None;
            };
            if integer_outside_domain(value, min, max) {
                parameter_domain_finding(
                    value_kind,
                    &definition.parameter_id,
                    location,
                    "integer value is outside its declared domain",
                    None,
                    findings,
                );
                return None;
            }
            Some(CompiledParameterValue::Integer { value })
        }
        ParameterType::ExactNumber { min, max } => {
            let Some(text) = authored.as_str() else {
                parameter_type_finding(
                    value_kind,
                    &definition.parameter_id,
                    "exact-number string",
                    location,
                    None,
                    findings,
                );
                return None;
            };
            let value = match ExactNumber::from_canonical(text) {
                Ok(value) => value,
                Err(error) => {
                    parameter_type_finding(
                        value_kind,
                        &definition.parameter_id,
                        "exact-number string",
                        location,
                        Some(error.detail()),
                        findings,
                    );
                    return None;
                }
            };
            match exact_outside_domain(&value, min, max) {
                Ok(true) => {
                    parameter_domain_finding(
                        value_kind,
                        &definition.parameter_id,
                        location,
                        "exact number is outside its declared domain",
                        None,
                        findings,
                    );
                    None
                }
                Ok(false) => Some(CompiledParameterValue::ExactNumber {
                    value: value.canonical_rational(),
                }),
                Err(error) => {
                    parameter_type_finding(
                        value_kind,
                        &definition.parameter_id,
                        "comparable exact-number value",
                        location,
                        Some(error.detail()),
                        findings,
                    );
                    None
                }
            }
        }
        ParameterType::Text { allowed_values } => {
            let Some(value) = authored.as_str() else {
                parameter_type_finding(
                    value_kind,
                    &definition.parameter_id,
                    "text",
                    location,
                    None,
                    findings,
                );
                return None;
            };
            if let Some(allowed) = allowed_values
                && !allowed.iter().any(|candidate| candidate == value)
            {
                parameter_domain_finding(
                    value_kind,
                    &definition.parameter_id,
                    location,
                    "text value is outside its declared choice set",
                    Some(allowed.clone()),
                    findings,
                );
                return None;
            }
            Some(CompiledParameterValue::Text {
                value: value.into(),
            })
        }
        ParameterType::Quantity { kind, min, max } => lower_quantity_parameter(
            value_kind, definition, authored, kind, min, max, location, kinds, findings,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_quantity_parameter(
    value_kind: &str,
    definition: &ParameterDefinition,
    authored: &serde_json::Value,
    kind: &str,
    min: &Option<QuantityBound>,
    max: &Option<QuantityBound>,
    location: SourceLocation,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<CompiledParameterValue> {
    let quantity = match serde_json::from_value::<QuantityValue>(authored.clone()) {
        Ok(quantity) => quantity,
        Err(error) => {
            let detail = error.to_string();
            parameter_type_finding(
                value_kind,
                &definition.parameter_id,
                "quantity object with exact string `value` and `unit`",
                location,
                Some(&detail),
                findings,
            );
            return None;
        }
    };
    let canonical = match kinds.scale_quantity(kind, &quantity.value, &quantity.unit) {
        Ok(canonical) => canonical,
        Err(error) => {
            let repair = error.repair().map(compiler_repair);
            parameter_type_finding(
                value_kind,
                &definition.parameter_id,
                &format!("quantity of kind `{kind}`"),
                location,
                Some(error.detail()),
                findings,
            );
            if let Some(repair) = repair
                && let Some(diagnostic) = findings.last_mut()
            {
                diagnostic.repairs.push(repair);
            }
            return None;
        }
    };
    match quantity_outside_domain(&canonical.value, kind, min, max, kinds) {
        Ok(true) => {
            parameter_domain_finding(
                value_kind,
                &definition.parameter_id,
                location,
                "quantity is outside its declared domain",
                None,
                findings,
            );
            None
        }
        Ok(false) => Some(CompiledParameterValue::Quantity {
            kind: kind.into(),
            value: canonical.value.canonical_rational(),
            unit: canonical.canonical_unit,
        }),
        Err(error) => {
            parameter_type_finding(
                value_kind,
                &definition.parameter_id,
                &format!("quantity of kind `{kind}` with a comparable domain"),
                location,
                Some(error.detail()),
                findings,
            );
            None
        }
    }
}

fn integer_outside_domain(
    value: i64,
    min: &Option<IntegerBound>,
    max: &Option<IntegerBound>,
) -> bool {
    min.as_ref()
        .is_some_and(|bound| value < bound.value || (value == bound.value && !bound.inclusive))
        || max
            .as_ref()
            .is_some_and(|bound| value > bound.value || (value == bound.value && !bound.inclusive))
}

fn exact_outside_domain(
    value: &ExactNumber,
    min: &Option<ExactBound>,
    max: &Option<ExactBound>,
) -> Result<bool, avila_core_kernel::KernelError> {
    if let Some(bound) = min {
        let ordering = value.checked_cmp(&bound.value)?;
        if ordering == Ordering::Less || (ordering == Ordering::Equal && !bound.inclusive) {
            return Ok(true);
        }
    }
    if let Some(bound) = max {
        let ordering = value.checked_cmp(&bound.value)?;
        if ordering == Ordering::Greater || (ordering == Ordering::Equal && !bound.inclusive) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn quantity_outside_domain(
    value: &ExactNumber,
    kind: &str,
    min: &Option<QuantityBound>,
    max: &Option<QuantityBound>,
    kinds: &KindRegistry,
) -> Result<bool, avila_core_kernel::KernelError> {
    if let Some(bound) = min {
        let canonical = kinds.scale_quantity(kind, &bound.value.value, &bound.value.unit)?;
        let ordering = value.checked_cmp(&canonical.value)?;
        if ordering == Ordering::Less || (ordering == Ordering::Equal && !bound.inclusive) {
            return Ok(true);
        }
    }
    if let Some(bound) = max {
        let canonical = kinds.scale_quantity(kind, &bound.value.value, &bound.value.unit)?;
        let ordering = value.checked_cmp(&canonical.value)?;
        if ordering == Ordering::Greater || (ordering == Ordering::Equal && !bound.inclusive) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parameter_type_finding(
    value_kind: &str,
    value_id: &str,
    expected: &str,
    location: SourceLocation,
    detail: Option<&str>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut message = format!("{value_kind} `{value_id}` must be authored as {expected}");
    if let Some(detail) = detail {
        message.push_str(": ");
        message.push_str(detail);
    }
    findings.push(CoreDiagnostic::new(
        CORE_T2401,
        FindingClass::Invalid,
        "requester",
        location,
        message,
    ));
}

fn parameter_domain_finding(
    value_kind: &str,
    value_id: &str,
    location: SourceLocation,
    detail: &str,
    candidates: Option<Vec<String>>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut diagnostic = CoreDiagnostic::new(
        CORE_T2402,
        FindingClass::Invalid,
        "requester",
        location,
        format!("{value_kind} `{value_id}` {detail}"),
    );
    if let Some(candidates) = candidates {
        diagnostic = diagnostic.with_repair(DiagnosticRepair {
            applicability: RepairApplicability::ConstrainedChoice,
            candidates,
        });
    }
    findings.push(diagnostic);
}

fn compiler_repair(repair: &avila_core_kernel::Repair) -> DiagnosticRepair {
    DiagnosticRepair {
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
    }
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
        let coverage_valid = validate_basis_coverage(index, requirement, findings);
        let tolerance_present_correctly = validate_tolerance_presence(index, requirement, findings);
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
        if registry.purposes.contains_key(&requirement.purpose)
            && candidate.excluded_purposes.contains(&requirement.purpose)
        {
            findings.push(CoreDiagnostic::new(
                CORE_T2601,
                FindingClass::Unsatisfied,
                "contract_author",
                contract_location(format!("/requirements/{index}/purpose")),
                format!(
                    "metric source `{}` explicitly excludes governed purpose `{}@{}`",
                    candidate.source.label(),
                    requirement.purpose.id,
                    requirement.purpose.major
                ),
            ));
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
        let limit = lower_requirement_quantity(
            index,
            "limit",
            &requirement.limit,
            metric_kind,
            registry,
            findings,
        );
        let tolerance = match (&requirement.tolerance, requirement.comparison) {
            (Some(quantity), Comparison::Equal) => lower_requirement_quantity(
                index,
                "tolerance",
                quantity,
                metric_kind,
                registry,
                findings,
            )
            .filter(|canonical| {
                tolerance_is_nonnegative(index, canonical, &quantity.value, findings)
            }),
            _ => None,
        };
        let (Some(limit), true, true) = (limit, coverage_valid, tolerance_present_correctly) else {
            continue;
        };
        if requirement.comparison == Comparison::Equal && tolerance.is_none() {
            continue;
        }
        compiled.push(CompiledRequirement {
            requirement_id: requirement.requirement_id.clone(),
            statement: requirement.statement.clone(),
            purpose: requirement.purpose.clone(),
            metric: metric.clone(),
            metric_role: candidate.role.clone(),
            comparison: requirement.comparison,
            limit,
            tolerance,
            basis: requirement.basis.clone(),
        });
    }
    compiled
}

/// Checks that a `coverage` basis is a canonical decimal in `(0, 1]` and is
/// declared only on a `bounded` basis. The kernel repeats this check at verdict
/// time; catching it here keeps an unevaluable requirement from compiling.
fn validate_basis_coverage(
    index: usize,
    requirement: &RequirementSource,
    findings: &mut Vec<CoreDiagnostic>,
) -> bool {
    let Some(coverage) = requirement.basis.coverage.as_deref() else {
        return true;
    };
    let location = contract_location(format!("/requirements/{index}/basis/coverage"));
    if requirement.basis.kind != BasisKind::Bounded {
        invalid_value(
            location,
            format!(
                "coverage is only meaningful for a `bounded` basis, not `{}`",
                basis_label(requirement.basis.kind)
            ),
            "contract_author",
            findings,
        );
        return false;
    }
    match read_authoritative_decimal(coverage) {
        Ok(value) => {
            let one = ExactNumber::from_canonical("1").expect("`1` is canonical");
            let in_range = value.is_positive()
                && value
                    .checked_cmp(&one)
                    .is_ok_and(|ordering| ordering != Ordering::Greater);
            if in_range {
                true
            } else {
                invalid_value(
                    location,
                    "coverage must lie in the interval (0, 1]",
                    "contract_author",
                    findings,
                );
                false
            }
        }
        Err(error) => {
            let mut diagnostic = CoreDiagnostic::new(
                CORE_S1102,
                FindingClass::Invalid,
                "contract_author",
                location,
                format!(
                    "coverage must be a canonical decimal in the interval (0, 1]: {}",
                    error.detail()
                ),
            );
            if let Some(repair) = error.repair() {
                diagnostic = diagnostic.with_repair(compiler_repair(repair));
            }
            findings.push(diagnostic);
            false
        }
    }
}

/// An equality comparison needs a tolerance to be evaluable at all; any other
/// comparison must not carry one, because it would silently mean nothing.
fn validate_tolerance_presence(
    index: usize,
    requirement: &RequirementSource,
    findings: &mut Vec<CoreDiagnostic>,
) -> bool {
    let location = contract_location(format!("/requirements/{index}/tolerance"));
    match (requirement.comparison, &requirement.tolerance) {
        (Comparison::Equal, None) => {
            findings.push(CoreDiagnostic::new(
                CORE_T2104,
                FindingClass::Missing,
                "contract_author",
                location,
                "an `equal` comparison requires an exact tolerance quantity of the metric kind",
            ));
            false
        }
        (Comparison::Equal, Some(_)) | (_, None) => true,
        (comparison, Some(_)) => {
            findings.push(CoreDiagnostic::new(
                CORE_T2104,
                FindingClass::Invalid,
                "contract_author",
                location,
                format!(
                    "a tolerance is only meaningful for an `equal` comparison, not `{}`",
                    comparison_label(comparison)
                ),
            ));
            false
        }
    }
}

/// Lowers a requirement-level quantity (`limit` or `tolerance`) into the
/// metric kind's canonical unit, reporting kind and unit mismatches at the
/// exact field.
fn lower_requirement_quantity(
    index: usize,
    field: &str,
    quantity: &TypedQuantity,
    metric_kind: &str,
    registry: &RegistryIndex<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<CanonicalTypedQuantity> {
    if quantity.kind != metric_kind {
        findings.push(CoreDiagnostic::new(
            CORE_T2102,
            FindingClass::Invalid,
            "contract_author",
            contract_location(format!("/requirements/{index}/{field}/kind")),
            format!(
                "{field} kind `{}` does not match metric kind `{metric_kind}`",
                quantity.kind
            ),
        ));
        return None;
    }
    if !registry.kind_classes.contains_key(metric_kind) {
        return None;
    }
    match registry
        .kinds
        .scale_quantity(metric_kind, &quantity.value, &quantity.unit)
    {
        Ok(canonical) => Some(CanonicalTypedQuantity {
            kind: metric_kind.into(),
            value: canonical.value.canonical_rational(),
            unit: canonical.canonical_unit,
        }),
        Err(error) => {
            let code = match error.code() {
                avila_core_kernel::CORE_T2001 => CORE_T2001,
                avila_core_kernel::CORE_T2102 => CORE_T2103,
                other => other,
            };
            let repair = error.repair().map(compiler_repair);
            let mut diagnostic = CoreDiagnostic::new(
                code,
                FindingClass::Invalid,
                "contract_author",
                contract_location(format!("/requirements/{index}/{field}/unit")),
                error.detail(),
            );
            if let Some(repair) = repair {
                diagnostic = diagnostic.with_repair(repair);
            }
            findings.push(diagnostic);
            None
        }
    }
}

fn tolerance_is_nonnegative(
    index: usize,
    canonical: &CanonicalTypedQuantity,
    authored: &ExactNumber,
    findings: &mut Vec<CoreDiagnostic>,
) -> bool {
    // Unit factors are positive, so the sign of the authored value decides.
    if authored.is_zero() || authored.is_positive() {
        return true;
    }
    invalid_value(
        contract_location(format!("/requirements/{index}/tolerance/value")),
        format!(
            "tolerance cannot be negative; canonical value is `{}` {}",
            canonical.value, canonical.unit
        ),
        "contract_author",
        findings,
    );
    false
}

const fn basis_label(kind: BasisKind) -> &'static str {
    match kind {
        BasisKind::Bounded => "bounded",
        BasisKind::Enclosure => "enclosure",
        BasisKind::Nominal => "nominal",
    }
}

const fn comparison_label(comparison: Comparison) -> &'static str {
    match comparison {
        Comparison::LessThan => "less_than",
        Comparison::LessThanOrEqual => "less_than_or_equal",
        Comparison::GreaterThan => "greater_than",
        Comparison::GreaterThanOrEqual => "greater_than_or_equal",
        Comparison::Equal => "equal",
    }
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
    parameters: &BTreeMap<String, BTreeMap<String, CompiledParameterValue>>,
    reproducibility: &BTreeMap<String, CompiledReproducibility>,
    review_obligations: &BTreeMap<String, CompiledReviewObligation>,
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
                review_obligation: review_obligations.get(step_id).cloned(),
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
        execution_policy: &'a ExecutionPolicy,
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
        execution_policy: &contract.execution_policy,
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
        execution_policy: contract.execution_policy.clone(),
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

fn review_incomplete(
    location: SourceLocation,
    message: impl Into<String>,
    owner: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        CORE_R3401,
        FindingClass::Invalid,
        owner,
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

fn parameter_location(step_index: usize, parameter_id: &str) -> SourceLocation {
    contract_location(format!(
        "/workflow/{step_index}/parameters/{}",
        escape_pointer_token(parameter_id)
    ))
}

fn seed_location(step_index: usize) -> SourceLocation {
    contract_location(format!("/workflow/{step_index}/reproducibility/seed"))
}

fn material_factor_location(step_index: usize, factor_id: &str) -> SourceLocation {
    contract_location(format!(
        "/workflow/{step_index}/reproducibility/material_factors/{}",
        escape_pointer_token(factor_id)
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
    const PARAMETER_CONTRACT: &[u8] = include_bytes!(
        "../../../fixtures/semantic-core/types/types.R7.parameters.pass.contract.json"
    );
    const PARAMETER_REGISTRY: &[u8] = include_bytes!(
        "../../../fixtures/semantic-core/types/compiler.parameters.registry.v1.json"
    );
    const REVIEW_CONTRACT: &[u8] = include_bytes!(
        "../../../fixtures/semantic-core/types/types.R9.review-bound.pass.contract.json"
    );
    const REVIEW_REGISTRY: &[u8] =
        include_bytes!("../../../fixtures/semantic-core/types/compiler.review.registry.v1.json");
    const PURPOSE_CONTRACT: &[u8] = include_bytes!(
        "../../../fixtures/semantic-core/types/types.R10.allowed.pass.contract.json"
    );
    const PURPOSE_REGISTRY: &[u8] =
        include_bytes!("../../../fixtures/semantic-core/types/compiler.purpose.registry.v1.json");

    fn contract() -> ContractSource {
        serde_json::from_slice(CONTRACT).unwrap()
    }

    fn registry() -> RegistrySnapshot {
        serde_json::from_slice(REGISTRY).unwrap()
    }

    fn parameter_contract() -> ContractSource {
        serde_json::from_slice(PARAMETER_CONTRACT).unwrap()
    }

    fn parameter_registry() -> RegistrySnapshot {
        serde_json::from_slice(PARAMETER_REGISTRY).unwrap()
    }

    fn review_contract() -> ContractSource {
        serde_json::from_slice(REVIEW_CONTRACT).unwrap()
    }

    fn review_registry() -> RegistrySnapshot {
        serde_json::from_slice(REVIEW_REGISTRY).unwrap()
    }

    fn purpose_contract() -> ContractSource {
        serde_json::from_slice(PURPOSE_CONTRACT).unwrap()
    }

    fn purpose_registry() -> RegistrySnapshot {
        serde_json::from_slice(PURPOSE_REGISTRY).unwrap()
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
            "sha256:7a6968cc0e3cf02ea2b7835a500418813175b17a383767481e2758376199b775"
        );
    }

    #[test]
    fn authoritative_failures_become_findings() {
        let report = compile_documents(br#"{"value":1.0}"#, REGISTRY).unwrap();
        assert_eq!(report.status, CompilationStatus::Rejected);
        assert_eq!(report.findings[0].code, CORE_S1102);
        assert_eq!(report.findings[0].primary.pointer, "/value");
        assert!(report.compiled.is_none());
    }

    #[test]
    fn source_layer_findings_name_the_offending_value() {
        let mut float_parameter: serde_json::Value = serde_json::from_slice(CONTRACT).unwrap();
        float_parameter["workflow"][0]["parameters"]["x"] = serde_json::json!(1.5);
        let report =
            compile_documents(&serde_json::to_vec(&float_parameter).unwrap(), REGISTRY).unwrap();
        assert_eq!(report.findings[0].code, CORE_S1102);
        assert_eq!(
            report.findings[0].primary.pointer,
            "/workflow/0/parameters/x"
        );

        let mut unknown_field: serde_json::Value = serde_json::from_slice(CONTRACT).unwrap();
        unknown_field["workflow"][1]["bogus_field"] = serde_json::json!(1);
        let report =
            compile_documents(&serde_json::to_vec(&unknown_field).unwrap(), REGISTRY).unwrap();
        assert_eq!(report.findings[0].code, CORE_S1101);
        assert_eq!(
            report.findings[0].primary.pointer,
            "/workflow/1/bogus_field"
        );

        let mut wrong_variant: serde_json::Value = serde_json::from_slice(CONTRACT).unwrap();
        wrong_variant["requirements"][0]["comparison"] = serde_json::json!("lessthan");
        let report =
            compile_documents(&serde_json::to_vec(&wrong_variant).unwrap(), REGISTRY).unwrap();
        assert_eq!(report.findings[0].code, CORE_S1102);
        assert_eq!(
            report.findings[0].primary.pointer,
            "/requirements/0/comparison"
        );
        assert!(report.findings[0].repairs.is_empty());

        let mut noncanonical: serde_json::Value = serde_json::from_slice(CONTRACT).unwrap();
        noncanonical["requirements"][0]["limit"]["value"] = serde_json::json!("100.0");
        let report =
            compile_documents(&serde_json::to_vec(&noncanonical).unwrap(), REGISTRY).unwrap();
        assert_eq!(report.findings[0].code, CORE_S1102);
        assert_eq!(
            report.findings[0].primary.pointer,
            "/requirements/0/limit/value"
        );
        assert_eq!(
            report.findings[0].repairs,
            vec![DiagnosticRepair {
                applicability: RepairApplicability::MechanicallySafe,
                candidates: vec!["100".into()],
            }]
        );
    }

    #[test]
    fn invalid_parameter_declarations_fail_as_registry_findings() {
        let source = parameter_contract();

        let mut empty_integer_domain = parameter_registry();
        let ParameterType::Integer { min: Some(min), .. } =
            &mut empty_integer_domain.capability_types[0].parameters[1].value_type
        else {
            panic!("fixture parameter must be an integer");
        };
        min.value = 101;
        assert!(codes(&compile_with_registry(&source, &empty_integer_domain)).contains(CORE_R3501));

        let mut reserved_choice = parameter_registry();
        let ParameterType::Text {
            allowed_values: Some(values),
        } = &mut reserved_choice.capability_types[0].parameters[3].value_type
        else {
            panic!("fixture parameter must be text");
        };
        values.push(NOT_DEFINED_PLACEHOLDER.into());
        assert!(codes(&compile_with_registry(&source, &reserved_choice)).contains(CORE_R3501));

        let mut unknown_kind = parameter_registry();
        let ParameterType::Quantity { kind, .. } =
            &mut unknown_kind.capability_types[0].parameters[0].value_type
        else {
            panic!("fixture parameter must be a quantity");
        };
        *kind = "fixture.unknown_kind".into();
        assert!(codes(&compile_with_registry(&source, &unknown_kind)).contains(CORE_R3501));
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
    fn review_obligation_never_becomes_a_technical_verdict() {
        let report = compile_documents(REVIEW_CONTRACT, REVIEW_REGISTRY).unwrap();
        assert_eq!(report.status, CompilationStatus::Compiled);
        let compiled = report.compiled.unwrap();
        let review = compiled
            .workflow
            .iter()
            .find(|step| step.step_id == "review")
            .and_then(|step| step.review_obligation.as_ref())
            .unwrap();
        assert_eq!(review.fulfillment, ReviewFulfillment::PendingExternalReview);
        assert_eq!(review.presented_evidence[0].input_slot, "trace");
        assert_eq!(review.presented_evidence[1].input_slot, "result");
        assert!(compiled.requirements.iter().all(|requirement| {
            !matches!(
                &requirement.metric,
                SourceRef::StepOutput { step_id, .. } if step_id == "review"
            )
        }));
    }

    #[test]
    fn incomplete_review_type_declarations_fail_closed() {
        let source = review_contract();

        let mut deterministic = review_registry();
        deterministic.capability_types[1]
            .reproducibility
            .determinism = DeterminismClass::Deterministic;
        assert!(codes(&compile_with_registry(&source, &deterministic)).contains(CORE_R3401));

        let mut hidden_input = review_registry();
        hidden_input.capability_types[1]
            .review
            .as_mut()
            .unwrap()
            .presented_input_slots
            .pop();
        assert!(codes(&compile_with_registry(&source, &hidden_input)).contains(CORE_R3401));

        let mut optional_input = review_registry();
        optional_input.capability_types[1].inputs[0].required = false;
        assert!(codes(&compile_with_registry(&source, &optional_input)).contains(CORE_R3401));

        let mut technical_decision = review_registry();
        technical_decision.capability_types[1].outputs[0].permitted_claim_models =
            vec![ClaimModelDeclaration::Interval { nominal: false }];
        assert!(codes(&compile_with_registry(&source, &technical_decision)).contains(CORE_R3401));

        let mut quantitative_decision = review_registry();
        quantitative_decision.capability_types[1].outputs[0].role =
            quantitative_decision.roles[0].role.clone();
        quantitative_decision.capability_types[1].outputs[0].media_type =
            "application/vnd.fixture.quantity+json".into();
        assert!(
            codes(&compile_with_registry(&source, &quantitative_decision)).contains(CORE_R3401)
        );

        let mut duplicate_disposition = review_registry();
        let declaration = duplicate_disposition.capability_types[1]
            .review
            .as_mut()
            .unwrap();
        declaration
            .allowed_dispositions
            .push(declaration.allowed_dispositions[0]);
        assert!(
            codes(&compile_with_registry(&source, &duplicate_disposition)).contains(CORE_R3401)
        );
    }

    #[test]
    fn purpose_exclusions_are_nominal_and_fail_closed() {
        let allowed = compile_documents(PURPOSE_CONTRACT, PURPOSE_REGISTRY).unwrap();
        assert_eq!(allowed.status, CompilationStatus::Compiled);
        assert_eq!(
            allowed.compiled.unwrap().requirements[0].purpose,
            VersionedRef {
                id: "fixture.design_compliance".into(),
                major: 1,
            }
        );

        let mut excluded = purpose_contract();
        excluded.requirements[0].purpose = VersionedRef {
            id: "fixture.screening".into(),
            major: 1,
        };
        assert!(codes(&compile_with_registry(&excluded, &purpose_registry())).contains(CORE_T2601));

        let mut similar_name = purpose_contract();
        similar_name.requirements[0].purpose = VersionedRef {
            id: "fixture.screening_research".into(),
            major: 1,
        };
        assert_eq!(
            compile_with_registry(&similar_name, &purpose_registry()).status,
            CompilationStatus::Compiled
        );

        let mut unknown = purpose_contract();
        unknown.requirements[0].purpose = VersionedRef {
            id: "fixture.unknown".into(),
            major: 1,
        };
        assert!(codes(&compile_with_registry(&unknown, &purpose_registry())).contains(CORE_T2601));
    }

    #[test]
    fn invalid_purpose_vocabularies_and_exclusions_are_registry_findings() {
        let source = purpose_contract();

        let mut duplicate_purpose = purpose_registry();
        duplicate_purpose
            .purposes
            .push(duplicate_purpose.purposes[0].clone());
        assert!(codes(&compile_with_registry(&source, &duplicate_purpose)).contains(CORE_R3501));

        let mut duplicate_exclusion = purpose_registry();
        let repeated =
            duplicate_exclusion.capability_types[0].outputs[0].excluded_purposes[0].clone();
        duplicate_exclusion.capability_types[0].outputs[0]
            .excluded_purposes
            .push(repeated);
        assert!(codes(&compile_with_registry(&source, &duplicate_exclusion)).contains(CORE_R3501));

        let mut unknown_exclusion = purpose_registry();
        unknown_exclusion.capability_types[0].outputs[0].excluded_purposes[0] = VersionedRef {
            id: "fixture.unknown".into(),
            major: 1,
        };
        assert!(codes(&compile_with_registry(&source, &unknown_exclusion)).contains(CORE_R3501));
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
    fn equality_requires_a_nonnegative_tolerance_of_the_metric_kind() {
        let tolerance = |value: &str, kind: &str, unit: &str| TypedQuantity {
            kind: kind.into(),
            value: ExactNumber::from_canonical(value).unwrap(),
            unit: unit.into(),
        };
        let metric_kind = "nuclear.dose_equivalent_rate";

        let mut missing = contract();
        missing.requirements[0].comparison = Comparison::Equal;
        let report = compile_contract(&missing);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.code == CORE_T2104)
            .unwrap();
        assert_eq!(finding.class, FindingClass::Missing);
        assert_eq!(finding.primary.pointer, "/requirements/0/tolerance");

        let mut misplaced = contract();
        misplaced.requirements[0].tolerance = Some(tolerance("1", metric_kind, "uSv/h"));
        let report = compile_contract(&misplaced);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.code == CORE_T2104)
            .unwrap();
        assert_eq!(finding.class, FindingClass::Invalid);

        let mut negative = contract();
        negative.requirements[0].comparison = Comparison::Equal;
        negative.requirements[0].tolerance = Some(tolerance("-1", metric_kind, "uSv/h"));
        let report = compile_contract(&negative);
        assert!(report.findings.iter().any(|finding| {
            finding.code == CORE_S1102
                && finding.primary.pointer == "/requirements/0/tolerance/value"
        }));

        let mut wrong_kind = contract();
        wrong_kind.requirements[0].comparison = Comparison::Equal;
        wrong_kind.requirements[0].tolerance =
            Some(tolerance("1", "nuclear.absorbed_dose_rate", "Gy/s"));
        let report = compile_contract(&wrong_kind);
        assert!(report.findings.iter().any(|finding| {
            finding.code == CORE_T2102
                && finding.primary.pointer == "/requirements/0/tolerance/kind"
        }));

        let mut wrong_unit = contract();
        wrong_unit.requirements[0].comparison = Comparison::Equal;
        wrong_unit.requirements[0].tolerance = Some(tolerance("1", metric_kind, "rem/h"));
        let report = compile_contract(&wrong_unit);
        assert!(report.findings.iter().any(|finding| {
            finding.code == CORE_T2001
                && finding.primary.pointer == "/requirements/0/tolerance/unit"
        }));

        let mut valid = contract();
        valid.requirements[0].comparison = Comparison::Equal;
        valid.requirements[0].tolerance = Some(tolerance("1", metric_kind, "uSv/h"));
        let report = compile_contract(&valid);
        assert_eq!(report.status, CompilationStatus::Compiled);
        let compiled = report.compiled.unwrap();
        let tolerance = compiled.requirements[0].tolerance.as_ref().unwrap();
        assert_eq!(tolerance.value, "1/3600000000");
        assert_eq!(tolerance.unit, "Sv/s");
    }

    #[test]
    fn coverage_must_be_a_canonical_decimal_in_the_unit_interval_on_a_bounded_basis() {
        let with_coverage = |kind: BasisKind, coverage: &str| {
            let mut source = contract();
            source.requirements[0].basis = RequirementBasis {
                kind,
                coverage: Some(coverage.into()),
            };
            compile_contract(&source)
        };
        let coverage_finding = |report: &CompileReport| {
            report
                .findings
                .iter()
                .find(|finding| finding.primary.pointer == "/requirements/0/basis/coverage")
                .cloned()
        };

        for out_of_range in ["1.2", "0", "-0.5", "abc"] {
            let report = with_coverage(BasisKind::Bounded, out_of_range);
            let finding = coverage_finding(&report).unwrap_or_else(|| panic!("{out_of_range}"));
            assert_eq!(finding.code, CORE_S1102, "{out_of_range}");
            assert!(finding.repairs.is_empty(), "{out_of_range}");
        }

        let noncanonical = with_coverage(BasisKind::Bounded, "0.950");
        let finding = coverage_finding(&noncanonical).unwrap();
        assert_eq!(finding.code, CORE_S1102);
        assert_eq!(
            finding.repairs,
            vec![DiagnosticRepair {
                applicability: RepairApplicability::MechanicallySafe,
                candidates: vec!["0.95".into()],
            }]
        );

        let misplaced = with_coverage(BasisKind::Enclosure, "0.95");
        assert_eq!(coverage_finding(&misplaced).unwrap().code, CORE_S1102);

        for valid in ["0.95", "1"] {
            let report = with_coverage(BasisKind::Bounded, valid);
            assert_eq!(report.status, CompilationStatus::Compiled, "{valid}");
            assert_eq!(
                report.compiled.unwrap().requirements[0]
                    .basis
                    .coverage
                    .as_deref(),
                Some(valid)
            );
        }
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
