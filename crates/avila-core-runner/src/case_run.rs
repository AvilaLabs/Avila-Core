//! The composed case workflow behind `avila-core run`.
//!
//! Stages, in order: package integrity, compilation, execution of the steps
//! the package declares (through case-specific adapters over exact
//! executables), generation of the claims document from package identities
//! and fresh outputs, identity binding, campaign evaluation, and replay
//! against the committed expectations. Every stage fails closed; a later
//! stage never runs over the output of a failed earlier one.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::Comparison;
use avila_core_compiler::{
    AdmissionState, BasisKind, CampaignReport, CampaignStatus, ClaimQualification, ClaimsDocument,
    CompilationStatus, CompileReport, CompiledContract, CompiledStep, CoverageDeclaration,
    CoverageReport, CoverageState, CoverageStatus, DeclaredOmission, EnvelopeAssessment,
    EnvelopeState, FindingClass, ImmutablePolicyRef, PresentationGateState, QualificationRecord,
    ResolvedBinding, ReviewDisposition, ReviewIndependence, ReviewerRole, SourceLocation,
    SourceRef, assess_coverage, compile_documents, evaluate_campaign, evaluate_envelope,
    parse_qualification, parse_requirement_set, registry_kinds, render_campaign_report,
    render_compile_report,
};
use avila_core_evidence::PackageArtifact;
use avila_core_evidence::{
    ArtifactCheck, CapabilityIdentity, CasePackageManifest, ExecutionReceipt, ExpectedInput,
    IntegrityCheckState, OutputState, PackageExecution, PackageIntegrityReport,
    PackageIntegrityStatus, ReceiptCheck, ReceiptCheckState, ReceiptExpectations, ReceiptInput,
    ReceiptOutput, ReceiptStatus, VerifiedCasePackage, parse_receipt, sha256_file,
    verify_case_package, verify_receipt,
};
use avila_core_kernel::{ExactNumber, KindRegistry, TruthValue, VerdictStatus};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::diagnostic::{
    CORE_X1001, CORE_X1002, CORE_X1101, CORE_X2001, CORE_X2101, CORE_X2201, CORE_X2301, CORE_X2401,
    CORE_X2402, CORE_X2501, CORE_X2601, CORE_X2701, CORE_X2801, CORE_X3001, CORE_X3101, CORE_X3201,
    CORE_X3301, CORE_X9001, RunFinding, RunStage,
};
use crate::execute::claims::{
    GeneratedClaim, canonical_decimal, canonical_identity, generate_claims,
};
use crate::execute::external_checker::{EXTERNAL_CHECKER_DOCUMENT_ROLE, ExternalCheckerAdapter};
use crate::execute::{
    Adapter, ExecutionRequest, ExtractedClaim, PlannedInvocation, StagedInput, StepContext,
    execute_step, plan_invocation, rfc3339_now,
};

const CASE_RUN_REPORT_SCHEMA_VERSION: &str = "avila.core/case-run-report/v0.3-draft";
const RUN_ATTEMPT_LOG_SCHEMA_VERSION: &str = "avila.core/run-attempt/v0.1-draft";
const CASE_RUN_NOTICE: &str = "This workflow separates byte-integrity checks, semantic compilation, controlled execution with receipts, claim generation, identity binding, campaign evaluation, and replay. Re-hashing bytes proves identity only; a verified receipt proves that a named executable ran over named bytes and produced named bytes; structural admission and a Core verdict do not establish scientific correctness, qualification, certification, or regulatory approval.";

/// How the runner is pointed at the outside world: named artifact roots,
/// named executables, and where to put the workspace.
#[derive(Debug, Clone)]
pub struct CaseRunOptions {
    pub source_roots: BTreeMap<String, PathBuf>,
    pub capabilities: BTreeMap<String, PathBuf>,
    pub workspace: Option<PathBuf>,
    /// Reuse a step from its committed receipt when the planned invocation
    /// identity matches and every recorded output still verifies at a bound
    /// identity (SC-12 execution memoization). Off forces fresh execution.
    pub reuse: bool,
    /// Report what would be reused or rerun, and why, without executing.
    pub plan_only: bool,
    /// Free contract inputs supplied for this run, by input id. Each is
    /// hashed and attested for this run; the committed expectations then
    /// describe a different candidate and are not replayed.
    pub inputs: BTreeMap<String, PathBuf>,
    /// Values for the environment keys the executions declare.
    pub environment: BTreeMap<String, String>,
    /// Append one JSON line describing this run to this file.
    pub log: Option<PathBuf>,
    /// Refuse the run before anything is compiled or executed unless the
    /// package manifest's digest equals this value: the requester's pin on
    /// the exact package a campaign is allowed to evaluate.
    pub expected_manifest_sha256: Option<String>,
}

impl Default for CaseRunOptions {
    fn default() -> Self {
        Self {
            source_roots: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            workspace: None,
            reuse: true,
            plan_only: false,
            inputs: BTreeMap::new(),
            environment: BTreeMap::new(),
            log: None,
            expected_manifest_sha256: None,
        }
    }
}

/// A free input supplied for this run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedInput {
    pub input_id: String,
    pub evidence_id: String,
    pub path: String,
    pub sha256: String,
}

/// One requirement's outcome with the numbers that decided it and the
/// distance to its limit, for search and for people. Read from the kernel's
/// verdict output; nothing here is re-derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictMargin {
    pub requirement_id: String,
    pub status: VerdictStatus,
    pub rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nominal: Option<String>,
    /// Limit minus the decisive bound for an upper limit, decisive bound
    /// minus limit for a lower limit: positive means inside, negative means
    /// outside. Absent when no number decided the verdict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseRunStatus {
    Evaluated,
    Rejected,
    /// A `--plan` run: the change analysis was reported and nothing ran.
    Planned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    Verified,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReport {
    pub status: BindingStatus,
    pub evidence_records: usize,
    pub bound_evidence_records: usize,
    /// Records produced this run for steps a supplied input reaches: their
    /// identity is the receipt's, not a package-declared one.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub receipted_evidence_records: usize,
    /// Package-bound records of steps a supplied input reaches that did not
    /// run: absent by design, because the reference input's results cannot
    /// speak for another input.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub withheld_evidence_records: usize,
    pub required_presentation_policies: usize,
    pub bound_presentation_policies: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayReport {
    pub document_id: String,
    pub matches: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Every declared execution ran or was reused, and every receipt verified.
    Executed,
    /// Every declared execution was reused from a committed receipt; nothing ran.
    Reused,
    /// A plan-only run: what would run, and why, was reported.
    Planned,
    /// No executable was supplied; committed claims are evaluated as recorded.
    NotRun,
    /// Some declared executions ran and others were not supplied.
    Partial,
    /// An execution could not be attempted safely: unknown adapter, wrong
    /// capability identity, or unchecked input bytes.
    Refused,
    /// An execution ran but did not complete with a verified receipt.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepExecutionState {
    Executed,
    /// The committed receipt's invocation identity equals the planned one
    /// and its outputs verify at bound identities; nothing ran.
    Reused,
    /// Plan only: the step would execute for the listed changes.
    Planned,
    NotRun,
    Refused,
    Failed,
}

/// The typed change classes of SC-12 that the runner can detect between a
/// committed receipt and the invocation it plans now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeClass {
    InputBytes,
    InputBinding,
    Parameters,
    Capability,
    Invocation,
    NoCommittedReceipt,
    ReceiptNotCompleted,
    OutputsUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeRecord {
    pub class: ChangeClass,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCheckState {
    Verified,
    Mismatch,
    Missing,
    NotSupplied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityCheck {
    pub capability_id: String,
    pub package_id: String,
    pub expected_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_sha256: Option<String>,
    pub state: CapabilityCheckState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StagedInputReport {
    pub input_slot: String,
    pub evidence_id: String,
    pub workspace_path: String,
    pub sha256: String,
    pub integrity: IntegrityCheckState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptSummary {
    pub workspace_path: String,
    pub sha256: String,
    pub invocation_sha256: String,
    pub status: ReceiptStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<i32>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputReport {
    pub output_id: String,
    pub workspace_path: String,
    pub state: OutputState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// Whether the fresh bytes equal the identity the package bound for the
    /// claims this output carries. `None` when nothing binds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reproduces_bound_artifact: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptReplayReport {
    pub document_id: String,
    pub matches: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub differences: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StepExecutionReport {
    pub step_id: String,
    pub adapter: String,
    pub capability_id: String,
    pub state: StepExecutionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<CapabilityCheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<StagedInputReport>,
    /// Identity of the invocation the runner planned from the current bound
    /// inputs, parameters, and capability, before deciding to reuse or run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_invocation_sha256: Option<String>,
    /// What differs from the committed receipt, by SC-12 change class.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<ChangeRecord>,
    /// The committed receipt document this step was reused from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reused_receipt: Option<String>,
    /// The producing capability's qualification envelope evaluated over this
    /// run's facts, when the package binds a qualification for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualification: Option<EnvelopeAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<ReceiptCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReceiptReplayReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<RunFinding>,
}

impl StepExecutionReport {
    fn add_finding(
        &mut self,
        code: &'static str,
        class: FindingClass,
        stage: RunStage,
        owner: &'static str,
        primary: SourceLocation,
        message: impl Into<String>,
    ) {
        self.findings.push(
            RunFinding::runtime(code, class, stage, owner, primary, message)
                .for_step(&self.step_id),
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NotExecutedStep {
    pub step_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReport {
    pub status: ExecutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    pub steps: Vec<StepExecutionReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_executed: Vec<NotExecutedStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimsReport {
    pub generated_sha256: String,
    pub committed_sha256: String,
    pub matches_committed: bool,
    pub input_attestations: usize,
    pub executed_claims: usize,
    pub reused_claims: usize,
    pub recorded_claims: usize,
    /// Committed claims not carried because a supplied input reaches their step.
    pub invalidated_claims: usize,
    /// The generated output claims, including categorical values. Artifact
    /// payloads remain separate; this is the compact semantic result surface
    /// available to reports and attempt logs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_claims: Vec<Value>,
}

/// Whether the exact evidence dossier for a compiled presentation gate is present.
/// Readiness does not imply a routing disposition has been recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationGateReadiness {
    ReadyForAgent,
    AwaitingEvidence,
}

/// One realized artifact in the exact dossier compiled for a presentation gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PresentedEvidence {
    pub input_slot: String,
    pub source: SourceRef,
    pub evidence_id: String,
    pub sha256: String,
    pub media_type: String,
}

/// A content-identified request for an optional agent practicality gate over
/// the exact realized dossier. The request is routing material, not technical
/// evidence, and cannot alter Core's verdicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationGateReport {
    pub request_sha256: String,
    pub compiled_snapshot_sha256: String,
    pub campaign_sha256: String,
    pub step_id: String,
    pub gate_state: PresentationGateState,
    pub reviewer_role: ReviewerRole,
    pub readiness: PresentationGateReadiness,
    pub presented_evidence: Vec<PresentedEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_evidence: Vec<SourceRef>,
    pub decision_role: avila_core_compiler::VersionedRef,
    pub decision_media_type: String,
    pub allowed_dispositions: Vec<ReviewDisposition>,
    pub reviewer_eligibility_policy: ImmutablePolicyRef,
    pub independence: ReviewIndependence,
    pub instructions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseRunReport {
    pub schema_version: String,
    pub case_id: String,
    pub title: String,
    pub status: CaseRunStatus,
    /// A stage-ordered, actionable view of every finding that prevented or
    /// qualified progress. Nested reports remain available as the detailed
    /// evidence; agents need only this collection to drive the next attempt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<RunFinding>,
    pub integrity: PackageIntegrityReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile: Option<CompileReport>,
    /// Coverage of the contract against the package's requirement set, when
    /// the package declares one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageReport>,
    /// Findings rendered as readable text with source locations, present
    /// whenever compilation or evaluation reported any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_findings: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims: Option<ClaimsReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bindings: Option<BindingReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign: Option<CampaignReport>,
    /// One entry per requirement verdict with its numbers and margin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub margins: Vec<VerdictMargin>,
    /// Realized, content-identified dossiers for optional practicality gates.
    /// These requests control presentation only, never a verdict.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presentation_gates: Vec<PresentationGateReport>,
    /// Free inputs supplied for this run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplied_inputs: Vec<SuppliedInput>,
    /// Steps a supplied input reaches; their committed claims and receipts
    /// describe a different candidate and are neither carried nor replayed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidated_steps: Vec<String>,
    /// Whether the committed expectations apply to this run at all.
    pub replay_applicable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplayReport>,
    pub notice: String,
}

impl CaseRunReport {
    pub fn succeeded(&self) -> bool {
        matches!(
            self.status,
            CaseRunStatus::Evaluated | CaseRunStatus::Planned
        )
    }
}

/// Parse repeated `NAME=PATH` arguments.
pub fn parse_named_paths(
    values: &[String],
    what: &str,
    example: &str,
) -> Result<BTreeMap<String, PathBuf>, Box<dyn Error>> {
    let mut paths = BTreeMap::new();
    for value in values {
        let Some((name, path)) = value.split_once('=') else {
            return Err(format!(
                "{what} `{value}` must have the form NAME=PATH (for example, {example})"
            )
            .into());
        };
        if name.is_empty() || path.is_empty() {
            return Err(format!("{what} `{value}` must contain a non-empty name and path").into());
        }
        if paths.insert(name.into(), PathBuf::from(path)).is_some() {
            return Err(format!("{what} `{name}` was supplied more than once").into());
        }
    }
    Ok(paths)
}

pub fn parse_source_roots(values: &[String]) -> Result<BTreeMap<String, PathBuf>, Box<dyn Error>> {
    parse_named_paths(values, "source root", "aftermatter=../project-aftermatter")
}

pub fn parse_capabilities(values: &[String]) -> Result<BTreeMap<String, PathBuf>, Box<dyn Error>> {
    parse_named_paths(
        values,
        "capability",
        "aftermatter-cli=../project-aftermatter/target/release/aftermatter",
    )
}

pub fn parse_inputs(values: &[String]) -> Result<BTreeMap<String, PathBuf>, Box<dyn Error>> {
    parse_named_paths(values, "input", "candidate=candidates/c-0001.json")
}

/// Parse repeated `KEY=VALUE` environment arguments.
pub fn parse_environment(values: &[String]) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut environment = BTreeMap::new();
    for value in values {
        let Some((key, value)) = value.split_once('=') else {
            return Err(format!("environment `{value}` must have the form KEY=VALUE").into());
        };
        if key.is_empty() {
            return Err("environment key must not be empty".into());
        }
        if environment
            .insert(key.to_string(), value.to_string())
            .is_some()
        {
            return Err(format!("environment `{key}` was supplied more than once").into());
        }
    }
    Ok(environment)
}

pub fn execute_case(
    case_or_manifest: &Path,
    options: &CaseRunOptions,
) -> Result<CaseRunReport, Box<dyn Error>> {
    match execute_case_inner(case_or_manifest, options) {
        Ok(mut report) => {
            collect_run_findings(&mut report);
            let workspace = report
                .execution
                .as_ref()
                .and_then(|execution| execution.workspace.as_deref())
                .map(Path::new);
            write_run_report(workspace, &report)?;
            append_log(options, case_or_manifest, &report)?;
            Ok(report)
        }
        Err(error) => {
            let finding = RunFinding::runtime(
                CORE_X9001,
                FindingClass::Invalid,
                RunStage::Infrastructure,
                "operator_or_runner",
                SourceLocation::new("case-run", ""),
                error.to_string(),
            );
            if let Err(log_error) = append_error_log(options, case_or_manifest, &finding) {
                return Err(format!(
                    "{error}; additionally, the run attempt could not be logged: {log_error}"
                )
                .into());
            }
            Err(error)
        }
    }
}

fn execute_case_inner(
    case_or_manifest: &Path,
    options: &CaseRunOptions,
) -> Result<CaseRunReport, Box<dyn Error>> {
    let manifest_path = if case_or_manifest.is_dir() {
        case_or_manifest.join("package.json")
    } else {
        case_or_manifest.to_path_buf()
    };
    let manifest_bytes = fs::read(&manifest_path)?;
    let package_root = manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut package = verify_case_package(&manifest_bytes, package_root, &options.source_roots)?;
    let supplied_inputs = supply_free_inputs(&mut package, options)?;
    let replay_applicable = supplied_inputs.is_empty();

    let mut report = CaseRunReport {
        schema_version: CASE_RUN_REPORT_SCHEMA_VERSION.into(),
        case_id: package.manifest.case_id.clone(),
        title: package.manifest.title.clone(),
        status: CaseRunStatus::Rejected,
        findings: Vec::new(),
        integrity: package.integrity.clone(),
        compile: None,
        coverage: None,
        rendered_findings: None,
        execution: None,
        claims: None,
        bindings: None,
        campaign: None,
        margins: Vec::new(),
        presentation_gates: Vec::new(),
        supplied_inputs: supplied_inputs.clone(),
        invalidated_steps: Vec::new(),
        replay_applicable,
        replay: None,
        notice: CASE_RUN_NOTICE.into(),
    };
    if let Some(expected) = &options.expected_manifest_sha256
        && expected != &report.integrity.manifest_sha256
    {
        report.notice = format!(
            "package manifest sha256 {} differs from the pinned {}; the run is refused before anything is compiled or executed",
            report.integrity.manifest_sha256, expected
        );
        report.findings.push(RunFinding::runtime(
            CORE_X1002,
            FindingClass::Inadmissible,
            RunStage::PackageIntegrity,
            "requester",
            SourceLocation::new("case-package", ""),
            report.notice.clone(),
        ));
        return Ok(report);
    }

    // A missing root is an explicit partial check. A supplied-but-missing or
    // different artifact is a failed integrity gate and nothing else runs.
    if package.integrity.status == PackageIntegrityStatus::Failed {
        return Ok(report);
    }

    let contract = required_document(&package, "contract")?;
    let registry = required_document(&package, "registry")?;
    let committed_claims_bytes = required_document(&package, "claims")?;
    let committed_claims: Value = serde_json::from_slice(committed_claims_bytes)?;

    let compile = compile_documents(contract, registry)?;
    if compile.status == CompilationStatus::Rejected {
        report.rendered_findings = Some(render_compile_report(
            &compile,
            &[("contract", contract), ("registry", registry)],
        ));
        report.compile = Some(compile);
        return Ok(report);
    }
    let compiled = compile
        .compiled
        .as_ref()
        .ok_or("compiler reported `compiled` without a compiled snapshot")?;
    let invalidated_steps = steps_reached_by_inputs(compiled, &supplied_inputs);
    report.invalidated_steps = invalidated_steps.iter().cloned().collect();

    // Coverage against the library requirement set, when declared. A search
    // optimizes exactly what is written; an unstated omission is refused
    // before any evaluation is spent on it.
    if let Some(declared) = &package.manifest.coverage {
        let set_document = package
            .manifest
            .documents
            .iter()
            .find(|document| document.document_id == declared.requirement_set)
            .ok_or("coverage names a document the manifest validation should have refused")?;
        let bytes = package
            .document_by_id(&declared.requirement_set)
            .ok_or("requirement set document bytes are missing")?;
        let set = parse_requirement_set(bytes)?;
        let declaration = CoverageDeclaration {
            mapping: declared.mapping.clone(),
            omissions: declared
                .omissions
                .iter()
                .map(|omission| DeclaredOmission {
                    set_requirement_id: omission.set_requirement_id.clone(),
                    reason: omission.reason.clone(),
                    accepted_by: omission.accepted_by.clone(),
                })
                .collect(),
        };
        let coverage = assess_coverage(compiled, &set, &set_document.sha256, &declaration);
        let incomplete = coverage.status == CoverageStatus::Incomplete;
        report.coverage = Some(coverage);
        if incomplete {
            report.compile = Some(compile.clone());
            return Ok(report);
        }
    }

    // Execute the steps the package declares. A refused or failed execution
    // stops the workflow: no claim is generated over an unverified run.
    let mut executed_claims = Vec::new();
    let mut workspace = None;
    let envelopes = Envelopes {
        records: load_qualifications(&package)?,
        kinds: registry_kinds(registry)?,
    };
    if !package.manifest.executions.is_empty() {
        let mut runner = Runner::new(
            &package,
            compiled,
            options,
            &committed_claims,
            &supplied_inputs,
            replay_applicable,
            &envelopes,
        );
        let execution = runner.run_all()?;
        executed_claims = runner.claims;
        workspace = runner.workspace;
        let stop = options.plan_only
            || matches!(
                execution.status,
                ExecutionStatus::Refused | ExecutionStatus::Failed | ExecutionStatus::Planned
            );
        if options.plan_only
            && !matches!(
                execution.status,
                ExecutionStatus::Refused | ExecutionStatus::Failed
            )
        {
            report.status = CaseRunStatus::Planned;
        }
        report.execution = Some(execution);
        report.compile = Some(compile.clone());
        if stop {
            return Ok(report);
        }
    } else {
        report.compile = Some(compile.clone());
    }

    // Generate the claims document from the package identities, the fresh
    // outputs, and the recorded attestations for steps that did not run.
    let generated = generate_claims(
        compiled,
        &package.manifest,
        &committed_claims,
        &executed_claims,
        &invalidated_steps,
    )?;
    let committed_sha256 = canonical_identity(committed_claims_bytes)?;
    let claims_match = generated.canonical_sha256 == committed_sha256;
    let evidence_claims = generated
        .value
        .get("claims")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    report.claims = Some(ClaimsReport {
        generated_sha256: generated.canonical_sha256.clone(),
        committed_sha256,
        matches_committed: claims_match,
        input_attestations: generated.input_attestations,
        executed_claims: generated.executed_claims,
        reused_claims: generated.reused_claims,
        recorded_claims: generated.recorded_claims,
        invalidated_claims: generated.invalidated_claims,
        evidence_claims,
    });
    if let Some(workspace) = workspace.as_deref() {
        let _ = fs::write(workspace.join("claims.json"), &generated.bytes);
    }

    let claims: ClaimsDocument = serde_json::from_slice(&generated.bytes)?;
    let bindings = verify_bindings(&package.manifest, &claims, compiled, &invalidated_steps);
    let bindings_failed = bindings.status == BindingStatus::Failed;
    report.bindings = Some(bindings);
    if bindings_failed {
        return Ok(report);
    }

    let campaign = evaluate_campaign(contract, registry, &generated.bytes)?;
    let campaign_rejected = campaign.status == CampaignStatus::Rejected;
    report.margins = margins(compiled, &campaign);
    report.presentation_gates = build_presentation_gates(compiled, &claims, &campaign)?;
    if !campaign.findings.is_empty() {
        report.rendered_findings = Some(render_campaign_report(
            &campaign,
            &[
                ("contract", contract),
                ("registry", registry),
                ("claims", &generated.bytes),
            ],
        ));
    }
    report.replay = if replay_applicable {
        replay_expected(&package, &campaign)?
    } else {
        None
    };
    let replay_failed = report.replay.as_ref().is_some_and(|replay| !replay.matches);
    if let Some(workspace) = workspace.as_deref()
        && let Ok(mut bytes) = serde_json::to_vec_pretty(&campaign)
    {
        bytes.push(b'\n');
        let _ = fs::write(workspace.join("campaign-report.json"), bytes);
    }
    report.campaign = Some(campaign);
    let receipts_drifted = report.execution.as_ref().is_some_and(|execution| {
        execution
            .steps
            .iter()
            .any(|step| step.replay.as_ref().is_some_and(|replay| !replay.matches))
    });
    let expectations_hold =
        !replay_applicable || (!replay_failed && claims_match && !receipts_drifted);
    if !campaign_rejected && expectations_hold {
        report.status = CaseRunStatus::Evaluated;
    }
    Ok(report)
}

fn build_presentation_gates(
    compiled: &CompiledContract,
    claims: &ClaimsDocument,
    campaign: &CampaignReport,
) -> Result<Vec<PresentationGateReport>, Box<dyn Error>> {
    let Some(campaign_sha256) = campaign.campaign_sha256.as_ref() else {
        return Ok(Vec::new());
    };
    let mut stages = Vec::new();
    for step in &compiled.workflow {
        let Some(gate) = &step.presentation_gate else {
            continue;
        };
        let mut presented_evidence = Vec::new();
        let mut missing_evidence = Vec::new();
        for binding in &gate.presented_evidence {
            match realize_presented_evidence(binding, claims) {
                Some(evidence) => presented_evidence.push(evidence),
                None => missing_evidence.push(binding.source.clone()),
            }
        }
        let readiness = if missing_evidence.is_empty() {
            PresentationGateReadiness::ReadyForAgent
        } else {
            PresentationGateReadiness::AwaitingEvidence
        };
        let mut gate = PresentationGateReport {
            request_sha256: String::new(),
            compiled_snapshot_sha256: compiled.snapshot_sha256.clone(),
            campaign_sha256: campaign_sha256.clone(),
            step_id: step.step_id.clone(),
            gate_state: gate.state,
            reviewer_role: gate.reviewer_role,
            readiness,
            presented_evidence,
            missing_evidence,
            decision_role: gate.decision_role.clone(),
            decision_media_type: gate.decision_media_type.clone(),
            allowed_dispositions: gate.allowed_dispositions.clone(),
            reviewer_eligibility_policy: gate.reviewer_eligibility_policy.clone(),
            independence: gate.independence.clone(),
            instructions: gate.instructions.clone(),
        };
        gate.request_sha256 = presentation_request_identity(&gate)?;
        stages.push(gate);
    }
    Ok(stages)
}

fn realize_presented_evidence(
    binding: &ResolvedBinding,
    claims: &ClaimsDocument,
) -> Option<PresentedEvidence> {
    match &binding.source {
        SourceRef::ContractInput { input_id } => {
            let mut matches = claims
                .inputs
                .iter()
                .filter(|input| input.input_id == *input_id);
            let input = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            Some(PresentedEvidence {
                input_slot: binding.input_slot.clone(),
                source: binding.source.clone(),
                evidence_id: format!("input:{input_id}"),
                sha256: input.artifact.sha256.clone(),
                media_type: input.artifact.media_type.clone(),
            })
        }
        SourceRef::StepOutput {
            step_id,
            output_slot,
        } => {
            let mut matches = claims
                .claims
                .iter()
                .filter(|claim| claim.step_id == *step_id && claim.output_slot == *output_slot);
            let output = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            Some(PresentedEvidence {
                input_slot: binding.input_slot.clone(),
                source: binding.source.clone(),
                evidence_id: output.claim_id.clone(),
                sha256: output.artifact.sha256.clone(),
                media_type: output.artifact.media_type.clone(),
            })
        }
    }
}

fn presentation_request_identity(gate: &PresentationGateReport) -> Result<String, Box<dyn Error>> {
    let mut value = serde_json::to_value(gate)?;
    value
        .as_object_mut()
        .expect("a presentation gate serializes as an object")
        .remove("request_sha256");
    let bytes = serde_json::to_vec(&value)?;
    let canonical = avila_core_kernel::canonicalize_json(&bytes)?;
    Ok(format!("sha256:{:x}", Sha256::digest(&canonical)))
}

/// Hash each supplied free input and let it stand for its contract input in
/// this run: the manifest's artifact and the integrity check for that
/// evidence record are replaced by the supplied bytes' identity.
fn supply_free_inputs(
    package: &mut VerifiedCasePackage,
    options: &CaseRunOptions,
) -> Result<Vec<SuppliedInput>, Box<dyn Error>> {
    let mut supplied = Vec::new();
    for (input_id, path) in &options.inputs {
        if !package.manifest.free_inputs.contains(input_id) {
            return Err(format!(
                "input `{input_id}` is not a free input of this package; free inputs: [{}]",
                package.manifest.free_inputs.join(", ")
            )
            .into());
        }
        let canonical = fs::canonicalize(path)
            .map_err(|error| format!("input `{input_id}` at `{}`: {error}", path.display()))?;
        let (sha256, _) = sha256_file(&canonical)?;
        let evidence_id = format!("input:{input_id}");
        let display = canonical.display().to_string();
        match package
            .manifest
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.evidence_ids.contains(&evidence_id))
        {
            Some(artifact) if artifact.evidence_ids.len() > 1 => {
                return Err(format!(
                    "input `{input_id}` is bound by artifact `{}` together with other evidence; it cannot be supplied separately",
                    artifact.artifact_id
                )
                .into());
            }
            Some(artifact) => {
                artifact.source_root = "supplied".into();
                artifact.path = display.clone();
                artifact.sha256 = sha256.clone();
            }
            None => package.manifest.artifacts.push(PackageArtifact {
                artifact_id: evidence_id.clone(),
                evidence_ids: vec![evidence_id.clone()],
                source_root: "supplied".into(),
                path: display.clone(),
                sha256: sha256.clone(),
            }),
        }
        match package
            .integrity
            .artifacts
            .iter_mut()
            .find(|check| check.evidence_ids.contains(&evidence_id))
        {
            Some(check) => {
                check.source_root = "supplied".into();
                check.path = display.clone();
                check.expected_sha256 = sha256.clone();
                check.actual_sha256 = Some(sha256.clone());
                check.state = IntegrityCheckState::Verified;
            }
            None => package.integrity.artifacts.push(ArtifactCheck {
                artifact_id: evidence_id.clone(),
                evidence_ids: vec![evidence_id.clone()],
                source_root: "supplied".into(),
                path: display.clone(),
                expected_sha256: sha256.clone(),
                actual_sha256: Some(sha256.clone()),
                state: IntegrityCheckState::Verified,
            }),
        }
        supplied.push(SuppliedInput {
            input_id: input_id.clone(),
            evidence_id,
            path: display,
            sha256,
        });
    }
    Ok(supplied)
}

/// Every step a supplied input reaches through the compiled bindings, so
/// their committed claims are not carried and their receipts not replayed.
fn steps_reached_by_inputs(
    compiled: &CompiledContract,
    supplied: &[SuppliedInput],
) -> BTreeSet<String> {
    let inputs: BTreeSet<&str> = supplied
        .iter()
        .map(|input| input.input_id.as_str())
        .collect();
    let mut reached = BTreeSet::new();
    for step in &compiled.workflow {
        let hit = step.bindings.iter().any(|binding| match &binding.source {
            SourceRef::ContractInput { input_id } => inputs.contains(input_id.as_str()),
            SourceRef::StepOutput { step_id, .. } => reached.contains(step_id),
        });
        if hit {
            reached.insert(step.step_id.clone());
        }
    }
    reached
}

/// The numbers behind each verdict and the distance to the limit.
fn margins(compiled: &CompiledContract, campaign: &CampaignReport) -> Vec<VerdictMargin> {
    campaign
        .verdicts
        .iter()
        .map(|verdict| {
            let comparison = compiled
                .requirements
                .iter()
                .find(|requirement| requirement.requirement_id == verdict.requirement_id)
                .map(|requirement| requirement.comparison);
            let output = &verdict.verdict;
            let decisive_upper = output
                .upper_canonical
                .as_deref()
                .or(match output.basis_visible {
                    Some(avila_core_kernel::BasisKind::Nominal) => {
                        output.nominal_canonical.as_deref()
                    }
                    _ => None,
                });
            let decisive_lower = output
                .lower_canonical
                .as_deref()
                .or(match output.basis_visible {
                    Some(avila_core_kernel::BasisKind::Nominal) => {
                        output.nominal_canonical.as_deref()
                    }
                    _ => None,
                });
            let margin = match (comparison, output.limit_canonical.as_deref()) {
                (Some(Comparison::LessThan | Comparison::LessThanOrEqual), Some(limit)) => {
                    decisive_upper.and_then(|upper| exact_difference(limit, upper))
                }
                (Some(Comparison::GreaterThan | Comparison::GreaterThanOrEqual), Some(limit)) => {
                    decisive_lower.and_then(|lower| exact_difference(lower, limit))
                }
                _ => None,
            };
            VerdictMargin {
                requirement_id: verdict.requirement_id.clone(),
                status: output.status,
                rule: output.rule.clone(),
                unit: output.canonical_unit.clone(),
                limit: output.limit_canonical.clone(),
                lower: output.lower_canonical.clone(),
                upper: output.upper_canonical.clone(),
                nominal: output.nominal_canonical.clone(),
                margin,
            }
        })
        .collect()
}

/// Renders an exact canonical value for people: the terminating decimal when
/// it is short, otherwise a value rounded to four decimal places and marked
/// approximate. The report keeps the exact value; only the summary rounds.
pub fn display_number(text: &str) -> String {
    let Ok(value) = ExactNumber::from_canonical(text) else {
        return text.to_string();
    };
    if let Ok(decimal) = canonical_decimal(&value)
        && decimal.len() <= 12
    {
        return decimal;
    }
    let Ok(scaled) = value
        .checked_mul_integer(10_000)
        .and_then(|scaled| scaled.round_half_even_integer())
    else {
        return text.to_string();
    };
    let sign = if scaled < 0 { "-" } else { "" };
    let magnitude = scaled.unsigned_abs();
    format!("~{sign}{}.{:04}", magnitude / 10_000, magnitude % 10_000)
}

fn exact_difference(left: &str, right: &str) -> Option<String> {
    let left = ExactNumber::from_canonical(left).ok()?;
    let right = ExactNumber::from_canonical(right).ok()?;
    let difference = left.checked_sub(&right).ok()?;
    Some(canonical_decimal(&difference).unwrap_or_else(|_| difference.canonical_rational()))
}

/// Append one line describing this run to the campaign log.
fn write_coverage_summary(out: &mut String, coverage: &CoverageReport) {
    let label = match coverage.status {
        CoverageStatus::Complete => "COMPLETE",
        CoverageStatus::Incomplete => "INCOMPLETE",
    };
    let _ = writeln!(
        out,
        "   coverage of requirement set {} revision {} ({}): [{label}] {} covered, {} omitted with a stated reason, {} omissible, {} unstated, {} covered only on a weaker basis",
        coverage.set_id,
        coverage.set_revision,
        coverage.set_sha256,
        coverage.count(CoverageState::Covered),
        coverage.count(CoverageState::OmittedStated),
        coverage.count(CoverageState::Omissible),
        coverage.count(CoverageState::OmittedUnstated),
        coverage.count(CoverageState::CoveredUnderBasis),
    );
    for issue in &coverage.issues {
        let _ = writeln!(out, "      issue: {issue}");
    }
    for entry in &coverage.entries {
        let state = match entry.state {
            CoverageState::Covered => "COVERED",
            CoverageState::CoveredUnderBasis => "UNDER BASIS",
            CoverageState::OmittedStated => "OMITTED",
            CoverageState::Omissible => "OMISSIBLE",
            CoverageState::OmittedUnstated => "UNSTATED",
        };
        let detail = match entry.state {
            CoverageState::Covered | CoverageState::CoveredUnderBasis => entry
                .covered_by
                .iter()
                .map(|cover| {
                    format!(
                        "{} ({}{})",
                        cover.requirement_id,
                        basis_word(cover.basis),
                        if cover.adequate {
                            ""
                        } else {
                            ", below the set's minimum basis"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
            CoverageState::OmittedStated => format!(
                "{} (accepted by {})",
                entry.reason.as_deref().unwrap_or(""),
                entry.accepted_by.as_deref().unwrap_or("")
            ),
            CoverageState::Omissible => "the set permits silent omission".into(),
            CoverageState::OmittedUnstated => "no reason stated".into(),
        };
        let _ = writeln!(
            out,
            "      [{state}] {} — {detail}",
            entry.set_requirement_id
        );
        for issue in &entry.issues {
            let _ = writeln!(out, "         issue: {issue}");
        }
    }
    if !coverage.additional_requirements.is_empty() {
        let _ = writeln!(
            out,
            "      beyond the set: {}",
            coverage.additional_requirements.join(", ")
        );
    }
}

fn basis_word(basis: BasisKind) -> &'static str {
    match basis {
        BasisKind::Nominal => "nominal",
        BasisKind::Bounded => "bounded",
        BasisKind::Enclosure => "enclosure",
    }
}

/// Build the single feedback stream consumed by people and iterating agents.
/// Detailed stage reports remain authoritative; this is a lossless-enough,
/// actionable index over the blockers and drift they contain.
fn collect_run_findings(report: &mut CaseRunReport) {
    for document in &report.integrity.documents {
        let (class, state) = match document.state {
            IntegrityCheckState::Missing => (FindingClass::Missing, "missing"),
            IntegrityCheckState::Mismatch => (FindingClass::Inadmissible, "does not match"),
            IntegrityCheckState::Verified | IntegrityCheckState::NotChecked => continue,
        };
        report.findings.push(RunFinding::runtime(
            CORE_X1001,
            class,
            RunStage::PackageIntegrity,
            "case_author",
            SourceLocation::new(
                "case-package",
                format!("/documents/{}", json_pointer_segment(&document.document_id)),
            ),
            format!(
                "package document `{}` at `{}` is {state}; expected {}, observed {}",
                document.document_id,
                document.path,
                document.expected_sha256,
                document.actual_sha256.as_deref().unwrap_or("no bytes")
            ),
        ));
    }
    for artifact in &report.integrity.artifacts {
        let (class, state) = match artifact.state {
            IntegrityCheckState::Missing => (FindingClass::Missing, "missing"),
            IntegrityCheckState::Mismatch => (FindingClass::Inadmissible, "does not match"),
            IntegrityCheckState::Verified | IntegrityCheckState::NotChecked => continue,
        };
        report.findings.push(RunFinding::runtime(
            CORE_X1001,
            class,
            RunStage::PackageIntegrity,
            "operator_or_case_author",
            SourceLocation::new(
                "case-package",
                format!("/artifacts/{}", json_pointer_segment(&artifact.artifact_id)),
            ),
            format!(
                "artifact `{}` at root `{}` path `{}` is {state}; expected {}, observed {}",
                artifact.artifact_id,
                artifact.source_root,
                artifact.path,
                artifact.expected_sha256,
                artifact.actual_sha256.as_deref().unwrap_or("no bytes")
            ),
        ));
    }

    if let Some(compile) = &report.compile {
        report.findings.extend(
            compile
                .findings
                .iter()
                .map(|finding| RunFinding::from_core(RunStage::Compilation, finding)),
        );
    }

    if let Some(coverage) = &report.coverage
        && coverage.status == CoverageStatus::Incomplete
    {
        let messages = coverage
            .issues
            .iter()
            .cloned()
            .chain(coverage.entries.iter().flat_map(|entry| {
                entry
                    .issues
                    .iter()
                    .map(|issue| format!("{}: {issue}", entry.set_requirement_id))
            }));
        for message in messages {
            report.findings.push(RunFinding::runtime(
                CORE_X1101,
                FindingClass::Inadmissible,
                RunStage::Coverage,
                "requester",
                SourceLocation::new("case-package", "/coverage"),
                message,
            ));
        }
    }

    if let Some(execution) = &report.execution {
        for step in &execution.steps {
            report.findings.extend(step.findings.iter().cloned());
            if let Some(replay) = &step.replay
                && !replay.matches
            {
                report.findings.push(
                    RunFinding::runtime(
                        CORE_X3201,
                        FindingClass::Inadmissible,
                        RunStage::Replay,
                        "capability_provider_or_case_author",
                        SourceLocation::new("execution-receipt", ""),
                        format!(
                            "step `{}` differs from committed receipt `{}`: {}",
                            step.step_id,
                            replay.document_id,
                            replay.differences.join("; ")
                        ),
                    )
                    .for_step(&step.step_id),
                );
            }
        }
    }

    if report.replay_applicable
        && let Some(claims) = &report.claims
        && !claims.matches_committed
    {
        report.findings.push(RunFinding::runtime(
            CORE_X3101,
            FindingClass::Inadmissible,
            RunStage::Replay,
            "capability_provider_or_case_author",
            SourceLocation::new("claims", ""),
            format!(
                "generated claims {} differ from committed claims {}",
                claims.generated_sha256, claims.committed_sha256
            ),
        ));
    }

    if let Some(bindings) = &report.bindings
        && bindings.status == BindingStatus::Failed
    {
        for issue in &bindings.issues {
            report.findings.push(RunFinding::runtime(
                CORE_X3001,
                FindingClass::Inadmissible,
                RunStage::EvidenceBinding,
                "case_author_or_capability_provider",
                SourceLocation::new("claims", ""),
                issue,
            ));
        }
    }

    if let Some(campaign) = &report.campaign {
        report.findings.extend(
            campaign
                .findings
                .iter()
                .map(|finding| RunFinding::from_core(RunStage::CampaignEvaluation, finding)),
        );
    }

    if let Some(replay) = &report.replay
        && !replay.matches
    {
        report.findings.push(RunFinding::runtime(
            CORE_X3301,
            FindingClass::Inadmissible,
            RunStage::Replay,
            "case_author",
            SourceLocation::new("campaign-report", ""),
            format!(
                "fresh campaign result differs from committed document `{}`",
                replay.document_id
            ),
        ));
    }

    report.findings.sort_by(|left, right| {
        left.stage
            .cmp(&right.stage)
            .then_with(|| left.step_id.cmp(&right.step_id))
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.primary.cmp(&right.primary))
            .then_with(|| left.message.cmp(&right.message))
    });
    report.findings.dedup();
}

fn json_pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn append_log(
    options: &CaseRunOptions,
    case_or_manifest: &Path,
    report: &CaseRunReport,
) -> Result<(), Box<dyn Error>> {
    let Some(path) = &options.log else {
        return Ok(());
    };
    #[derive(Serialize)]
    struct LogEntry<'a> {
        schema_version: &'static str,
        recorded_at: String,
        case_path: String,
        case_id: &'a str,
        status: CaseRunStatus,
        integrity_status: PackageIntegrityStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        compilation_status: Option<CompilationStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        execution_status: Option<ExecutionStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        binding_status: Option<BindingStatus>,
        #[serde(skip_serializing_if = "Option::is_none")]
        campaign_status: Option<CampaignStatus>,
        supplied_inputs: &'a [SuppliedInput],
        steps: Vec<StepLog<'a>>,
        findings: &'a [RunFinding],
        claims: &'a [Value],
        verdicts: &'a [VerdictMargin],
        presentation_gates: Vec<PresentationGateLog<'a>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        coverage: Option<CoverageLog<'a>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        campaign_sha256: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        workspace: Option<&'a str>,
        /// The identities this run was evaluated under, so a rewritten
        /// package or contract is visible in the record as a different one.
        manifest_sha256: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        compiled_snapshot_sha256: Option<&'a str>,
        documents: Vec<DocumentLog<'a>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        reused_from: Vec<(&'a str, &'a str)>,
    }
    #[derive(Serialize)]
    struct StepLog<'a> {
        step_id: &'a str,
        adapter: &'a str,
        capability_id: &'a str,
        state: StepExecutionState,
        #[serde(skip_serializing_if = "Option::is_none")]
        planned_invocation_sha256: Option<&'a str>,
        changes: &'a [ChangeRecord],
        #[serde(skip_serializing_if = "Option::is_none")]
        receipt: Option<&'a ReceiptSummary>,
        outputs: &'a [OutputReport],
    }
    #[derive(Serialize)]
    struct DocumentLog<'a> {
        document_id: &'a str,
        role: &'a str,
        sha256: &'a str,
    }
    #[derive(Serialize)]
    struct CoverageLog<'a> {
        status: CoverageStatus,
        set_id: &'a str,
        set_sha256: &'a str,
    }
    #[derive(Serialize)]
    struct PresentationGateLog<'a> {
        step_id: &'a str,
        reviewer_role: ReviewerRole,
        readiness: PresentationGateReadiness,
        request_sha256: &'a str,
    }
    let entry = LogEntry {
        schema_version: RUN_ATTEMPT_LOG_SCHEMA_VERSION,
        recorded_at: rfc3339_now(),
        case_path: case_or_manifest.display().to_string(),
        case_id: &report.case_id,
        status: report.status,
        integrity_status: report.integrity.status,
        compilation_status: report.compile.as_ref().map(|compile| compile.status),
        execution_status: report.execution.as_ref().map(|execution| execution.status),
        binding_status: report.bindings.as_ref().map(|bindings| bindings.status),
        campaign_status: report.campaign.as_ref().map(|campaign| campaign.status),
        supplied_inputs: &report.supplied_inputs,
        steps: report
            .execution
            .as_ref()
            .map(|execution| {
                execution
                    .steps
                    .iter()
                    .map(|step| StepLog {
                        step_id: &step.step_id,
                        adapter: &step.adapter,
                        capability_id: &step.capability_id,
                        state: step.state,
                        planned_invocation_sha256: step.planned_invocation_sha256.as_deref(),
                        changes: &step.changes,
                        receipt: step.receipt.as_ref(),
                        outputs: &step.outputs,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        findings: &report.findings,
        claims: report
            .claims
            .as_ref()
            .map_or(&[], |claims| claims.evidence_claims.as_slice()),
        verdicts: &report.margins,
        presentation_gates: report
            .presentation_gates
            .iter()
            .map(|gate| PresentationGateLog {
                step_id: &gate.step_id,
                reviewer_role: gate.reviewer_role,
                readiness: gate.readiness,
                request_sha256: &gate.request_sha256,
            })
            .collect(),
        coverage: report.coverage.as_ref().map(|coverage| CoverageLog {
            status: coverage.status,
            set_id: &coverage.set_id,
            set_sha256: &coverage.set_sha256,
        }),
        campaign_sha256: report
            .campaign
            .as_ref()
            .and_then(|campaign| campaign.campaign_sha256.as_deref()),
        workspace: report
            .execution
            .as_ref()
            .and_then(|execution| execution.workspace.as_deref()),
        manifest_sha256: &report.integrity.manifest_sha256,
        compiled_snapshot_sha256: report
            .compile
            .as_ref()
            .and_then(|compile| compile.compiled.as_ref())
            .map(|compiled| compiled.snapshot_sha256.as_str()),
        documents: report
            .integrity
            .documents
            .iter()
            .map(|document| DocumentLog {
                document_id: &document.document_id,
                role: &document.role,
                sha256: document
                    .actual_sha256
                    .as_deref()
                    .unwrap_or(&document.expected_sha256),
            })
            .collect(),
        reused_from: report
            .execution
            .as_ref()
            .map(|execution| {
                execution
                    .steps
                    .iter()
                    .filter_map(|step| {
                        step.reused_receipt
                            .as_deref()
                            .map(|receipt| (step.step_id.as_str(), receipt))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    append_log_line(path, &serde_json::to_string(&entry)?)
}

fn append_error_log(
    options: &CaseRunOptions,
    case_or_manifest: &Path,
    finding: &RunFinding,
) -> Result<(), Box<dyn Error>> {
    let Some(path) = &options.log else {
        return Ok(());
    };
    #[derive(Serialize)]
    struct ErrorLogEntry<'a> {
        schema_version: &'static str,
        recorded_at: String,
        case_path: String,
        status: &'static str,
        findings: [&'a RunFinding; 1],
    }
    let entry = ErrorLogEntry {
        schema_version: RUN_ATTEMPT_LOG_SCHEMA_VERSION,
        recorded_at: rfc3339_now(),
        case_path: case_or_manifest.display().to_string(),
        status: "error",
        findings: [finding],
    };
    append_log_line(path, &serde_json::to_string(&entry)?)
}

fn append_log_line(path: &Path, line: &str) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    std::io::Write::write_all(&mut file, line.as_bytes())?;
    std::io::Write::write_all(&mut file, b"\n")?;
    Ok(())
}

fn write_run_report(
    workspace: Option<&Path>,
    report: &CaseRunReport,
) -> Result<(), Box<dyn Error>> {
    let Some(workspace) = workspace else {
        return Ok(());
    };
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    fs::write(workspace.join("run-report.json"), bytes)?;
    Ok(())
}

fn required_document<'a>(
    package: &'a VerifiedCasePackage,
    role: &str,
) -> Result<&'a [u8], Box<dyn Error>> {
    package
        .document_by_role(role)
        .ok_or_else(|| format!("verified package has no readable `{role}` document").into())
}

/// A fresh output of an executed step, available to later steps and to
/// claim generation.
#[derive(Debug, Clone)]
struct FreshOutput {
    evidence_id: String,
    path: PathBuf,
    sha256: String,
    media_type: String,
}

/// A resolved artifact for a step input: where the verified bytes are and
/// what identity they must have.
#[derive(Debug, Clone)]
struct ResolvedArtifact {
    evidence_id: String,
    path: PathBuf,
    sha256: String,
    media_type: String,
    integrity: IntegrityCheckState,
}

struct Runner<'a> {
    package: &'a VerifiedCasePackage,
    compiled: &'a CompiledContract,
    options: &'a CaseRunOptions,
    committed_claims: &'a Value,
    artifact_checks: BTreeMap<String, &'a ArtifactCheck>,
    canonical_roots: BTreeMap<String, PathBuf>,
    fresh_outputs: BTreeMap<(String, String), FreshOutput>,
    workspace: Option<PathBuf>,
    claims: Vec<GeneratedClaim>,
    /// Supplied free inputs by evidence id, resolved to their files.
    supplied: BTreeMap<String, PathBuf>,
    /// Whether committed receipts describe this run's candidate.
    replay_applicable: bool,
    /// The package's qualification records and the kinds their facts scale by.
    envelopes: &'a Envelopes,
    /// The envelope assessment of the step being run, attached to its claims.
    current_qualification: Option<Value>,
}

/// A qualification record the package binds, already checked against the
/// capability it names.
pub struct BoundQualification {
    pub sha256: String,
    pub record: QualificationRecord,
}

/// What envelope evaluation needs: the bound records and the registry's
/// quantity kinds.
pub struct Envelopes {
    pub records: Vec<BoundQualification>,
    pub kinds: KindRegistry,
}

/// Load the package's `qualification` documents and refuse any whose bound
/// executable is not the one the package binds under that capability id.
fn load_qualifications(
    package: &VerifiedCasePackage,
) -> Result<Vec<BoundQualification>, Box<dyn Error>> {
    let mut bound = Vec::new();
    for document in package
        .manifest
        .documents
        .iter()
        .filter(|document| document.role == "qualification")
    {
        let bytes = package
            .document_by_id(&document.document_id)
            .ok_or_else(|| {
                format!(
                    "qualification document `{}` has no bytes",
                    document.document_id
                )
            })?;
        let record = parse_qualification(bytes)
            .map_err(|error| format!("document `{}`: {error}", document.document_id))?;
        let capability = package
            .manifest
            .capabilities
            .iter()
            .find(|capability| capability.capability_id == record.capability.capability_id)
            .ok_or_else(|| {
                format!(
                    "qualification `{}` names capability `{}`, which the package does not bind",
                    record.qualification_id, record.capability.capability_id
                )
            })?;
        if capability.executable_sha256 != record.capability.executable_sha256 {
            return Err(format!(
                "qualification `{}` covers executable {} but the package binds {} as `{}`",
                record.qualification_id,
                record.capability.executable_sha256,
                capability.executable_sha256,
                capability.capability_id
            )
            .into());
        }
        if !package.manifest.executions.iter().any(|execution| {
            execution.adapter == record.adapter
                && execution.capability_id == record.capability.capability_id
        }) {
            return Err(format!(
                "qualification `{}` covers adapter `{}` under `{}`, but no execution uses that pair",
                record.qualification_id, record.adapter, record.capability.capability_id
            )
            .into());
        }
        bound.push(BoundQualification {
            sha256: document.sha256.clone(),
            record,
        });
    }
    Ok(bound)
}

impl<'a> Runner<'a> {
    fn new(
        package: &'a VerifiedCasePackage,
        compiled: &'a CompiledContract,
        options: &'a CaseRunOptions,
        committed_claims: &'a Value,
        supplied_inputs: &[SuppliedInput],
        replay_applicable: bool,
        envelopes: &'a Envelopes,
    ) -> Self {
        let artifact_checks = package
            .integrity
            .artifacts
            .iter()
            .flat_map(|check| {
                check
                    .evidence_ids
                    .iter()
                    .map(move |evidence_id| (evidence_id.clone(), check))
            })
            .collect();
        let canonical_roots = options
            .source_roots
            .iter()
            .filter_map(|(name, path)| {
                fs::canonicalize(path)
                    .ok()
                    .map(|canonical| (name.clone(), canonical))
            })
            .collect();
        Self {
            package,
            compiled,
            options,
            committed_claims,
            artifact_checks,
            canonical_roots,
            fresh_outputs: BTreeMap::new(),
            workspace: None,
            claims: Vec::new(),
            supplied: supplied_inputs
                .iter()
                .map(|input| (input.evidence_id.clone(), PathBuf::from(&input.path)))
                .collect(),
            replay_applicable,
            envelopes,
            current_qualification: None,
        }
    }

    fn resolve_adapter(&self, adapter_id: &str) -> Result<Adapter, String> {
        let document = self.package.manifest.documents.iter().find(|document| {
            document.role == EXTERNAL_CHECKER_DOCUMENT_ROLE && document.document_id == adapter_id
        });
        if let Some(adapter) = Adapter::by_id(adapter_id) {
            if document.is_some() {
                return Err(format!(
                    "adapter `{adapter_id}` is built into this runner and cannot be shadowed by an `{EXTERNAL_CHECKER_DOCUMENT_ROLE}` document"
                ));
            }
            return Ok(adapter);
        }
        let document = document.ok_or_else(|| {
            format!(
                "adapter `{adapter_id}` is neither built into this runner nor declared by a verified `{EXTERNAL_CHECKER_DOCUMENT_ROLE}` document whose document_id matches the adapter id"
            )
        })?;
        let bytes = self
            .package
            .document_by_id(&document.document_id)
            .ok_or_else(|| format!("adapter document `{adapter_id}` has no verified bytes"))?;
        let adapter = ExternalCheckerAdapter::from_bytes(bytes)
            .map_err(|error| format!("adapter document `{adapter_id}`: {error}"))?;
        if adapter.adapter_id != adapter_id {
            return Err(format!(
                "adapter document `{adapter_id}` declares adapter_id `{}`",
                adapter.adapter_id
            ));
        }
        Ok(Adapter::ExternalChecker {
            adapter: Box::new(adapter),
            descriptor_sha256: document.sha256.clone(),
        })
    }

    fn run_all(&mut self) -> Result<ExecutionReport, Box<dyn Error>> {
        let executions: BTreeMap<&str, &PackageExecution> = self
            .package
            .manifest
            .executions
            .iter()
            .map(|execution| (execution.step_id.as_str(), execution))
            .collect();
        let mut steps = Vec::new();
        let mut not_executed = Vec::new();
        // Compiled order is topological, so a fresh output exists before any
        // later executed step binds it.
        for step in &self.compiled.workflow {
            match executions.get(step.step_id.as_str()) {
                Some(execution) => steps.push(self.run_step(step, execution)?),
                None => not_executed.push(NotExecutedStep {
                    step_id: step.step_id.clone(),
                    reason: if step.presentation_gate.is_some() {
                        "optional agent practicality gate; the runner materializes its exact dossier for the connected agent".into()
                    } else {
                        "no execution declared; the committed claim is evaluated as a recorded attestation".into()
                    },
                }),
            }
        }
        for execution in &self.package.manifest.executions {
            if !self
                .compiled
                .workflow
                .iter()
                .any(|step| step.step_id == execution.step_id)
            {
                steps.push(StepExecutionReport {
                    step_id: execution.step_id.clone(),
                    adapter: execution.adapter.clone(),
                    capability_id: execution.capability_id.clone(),
                    state: StepExecutionState::Refused,
                    capability: None,
                    inputs: Vec::new(),
                    planned_invocation_sha256: None,
                    changes: Vec::new(),
                    reused_receipt: None,
                    qualification: None,
                    receipt: None,
                    outputs: Vec::new(),
                    verification: None,
                    replay: None,
                    findings: vec![RunFinding::runtime(
                        CORE_X2001,
                        FindingClass::Invalid,
                        RunStage::ExecutionPlanning,
                        "case_author",
                        SourceLocation::new("case-package", "/executions"),
                        format!(
                            "step `{}` is declared for execution but the compiled workflow has no such step",
                            execution.step_id
                        ),
                    )
                    .for_step(&execution.step_id)],
                });
            }
        }

        let states: Vec<StepExecutionState> = steps.iter().map(|step| step.state).collect();
        let status = if states.contains(&StepExecutionState::Refused) {
            ExecutionStatus::Refused
        } else if states.contains(&StepExecutionState::Failed) {
            ExecutionStatus::Failed
        } else if states.contains(&StepExecutionState::Planned) {
            ExecutionStatus::Planned
        } else if states
            .iter()
            .all(|state| *state == StepExecutionState::NotRun)
        {
            ExecutionStatus::NotRun
        } else if states
            .iter()
            .all(|state| *state == StepExecutionState::Reused)
        {
            ExecutionStatus::Reused
        } else if states.iter().all(|state| {
            matches!(
                state,
                StepExecutionState::Executed | StepExecutionState::Reused
            )
        }) {
            ExecutionStatus::Executed
        } else {
            ExecutionStatus::Partial
        };
        Ok(ExecutionReport {
            status,
            workspace: self
                .workspace
                .as_ref()
                .map(|path| path.display().to_string()),
            steps,
            not_executed,
        })
    }

    fn run_step(
        &mut self,
        step: &CompiledStep,
        execution: &PackageExecution,
    ) -> Result<StepExecutionReport, Box<dyn Error>> {
        let mut report = StepExecutionReport {
            step_id: step.step_id.clone(),
            adapter: execution.adapter.clone(),
            capability_id: execution.capability_id.clone(),
            state: StepExecutionState::Refused,
            capability: None,
            inputs: Vec::new(),
            planned_invocation_sha256: None,
            changes: Vec::new(),
            reused_receipt: None,
            qualification: None,
            receipt: None,
            outputs: Vec::new(),
            verification: None,
            replay: None,
            findings: Vec::new(),
        };

        let Some(declared) = self
            .package
            .manifest
            .capabilities
            .iter()
            .find(|capability| capability.capability_id == execution.capability_id)
        else {
            report.add_finding(
                CORE_X2001,
                FindingClass::Invalid,
                RunStage::ExecutionPlanning,
                "case_author",
                SourceLocation::new("case-package", "/capabilities"),
                format!("capability `{}` is not declared", execution.capability_id),
            );
            return Ok(report);
        };
        let identity = CapabilityIdentity {
            capability_id: declared.capability_id.clone(),
            package_id: declared.package_id.clone(),
            source_repository: declared.source_repository.clone(),
            source_commit: declared.source_commit.clone(),
            executable_sha256: declared.executable_sha256.clone(),
        };
        let not_supplied = CapabilityCheck {
            capability_id: declared.capability_id.clone(),
            package_id: declared.package_id.clone(),
            expected_sha256: declared.executable_sha256.clone(),
            actual_sha256: None,
            state: CapabilityCheckState::NotSupplied,
        };
        let adapter = match self.resolve_adapter(&execution.adapter) {
            Ok(adapter) => adapter,
            Err(issue) => {
                report.add_finding(
                    CORE_X2001,
                    FindingClass::Invalid,
                    RunStage::ExecutionPlanning,
                    "case_author",
                    SourceLocation::new("case-package", "/executions"),
                    issue,
                );
                return Ok(report);
            }
        };
        let expected_type = adapter.capability_type();
        if step.capability_type.id != expected_type.id
            || step.capability_type.major != expected_type.major
        {
            report.add_finding(
                CORE_X2001,
                FindingClass::Invalid,
                RunStage::ExecutionPlanning,
                "case_author",
                SourceLocation::new("case-package", "/executions"),
                format!(
                    "adapter `{}` implements `{}@{}`, but step `{}` compiles to `{}@{}`",
                    execution.adapter,
                    expected_type.id,
                    expected_type.major,
                    step.step_id,
                    step.capability_type.id,
                    step.capability_type.major
                ),
            );
        }
        if step.presentation_gate.is_some() {
            report.add_finding(
                CORE_X2001,
                FindingClass::Invalid,
                RunStage::ExecutionPlanning,
                "case_author",
                SourceLocation::new("case-package", "/executions"),
                "an optional agent practicality gate consumes the runner's materialized review request, not a package execution declaration",
            );
        }

        // Every bound input slot must be staged from verified bytes.
        let staging: BTreeMap<&str, &str> = execution
            .inputs
            .iter()
            .map(|input| (input.input_slot.as_str(), input.workspace_path.as_str()))
            .collect();
        let bound_slots: BTreeSet<&str> = step
            .bindings
            .iter()
            .map(|binding| binding.input_slot.as_str())
            .collect();
        for slot in staging.keys() {
            if !bound_slots.contains(slot) {
                report.add_finding(
                    CORE_X2001,
                    FindingClass::Invalid,
                    RunStage::ExecutionPlanning,
                    "case_author",
                    SourceLocation::new("case-package", "/executions"),
                    format!(
                        "package stages input slot `{slot}`, which the compiled step does not bind"
                    ),
                );
            }
        }
        let mut staged = Vec::new();
        let mut unverified = Vec::new();
        for binding in &step.bindings {
            let Some(workspace_path) = staging.get(binding.input_slot.as_str()) else {
                report.add_finding(
                    CORE_X2001,
                    FindingClass::Missing,
                    RunStage::ExecutionPlanning,
                    "case_author",
                    SourceLocation::new("case-package", "/executions"),
                    format!(
                        "bound input slot `{}` has no staging path in the package",
                        binding.input_slot
                    ),
                );
                continue;
            };
            if !adapter.accepts_input(&binding.input_slot) {
                report.add_finding(
                    CORE_X2001,
                    FindingClass::Invalid,
                    RunStage::ExecutionPlanning,
                    "case_author",
                    SourceLocation::new("case-package", "/executions"),
                    format!(
                        "adapter `{}` does not accept input slot `{}`",
                        execution.adapter, binding.input_slot
                    ),
                );
                continue;
            }
            match self.resolve_source(&binding.source) {
                Ok(artifact) => {
                    if artifact.integrity != IntegrityCheckState::Verified {
                        unverified.push(format!(
                            "input slot `{}` bytes (`{}`) were not verified: {:?}; the runner neither executes over nor reuses unchecked bytes",
                            binding.input_slot, artifact.evidence_id, artifact.integrity
                        ));
                    }
                    report.inputs.push(StagedInputReport {
                        input_slot: binding.input_slot.clone(),
                        evidence_id: artifact.evidence_id.clone(),
                        workspace_path: (*workspace_path).to_string(),
                        sha256: artifact.sha256.clone(),
                        integrity: artifact.integrity,
                    });
                    staged.push(StagedInput {
                        input_slot: binding.input_slot.clone(),
                        evidence_id: artifact.evidence_id,
                        source_path: artifact.path,
                        workspace_path: (*workspace_path).to_string(),
                        media_type: artifact.media_type,
                        expected_sha256: artifact.sha256,
                    });
                }
                Err(issue) => report.add_finding(
                    CORE_X2101,
                    FindingClass::Missing,
                    RunStage::ExecutionPlanning,
                    "operator",
                    SourceLocation::new("case-run", "/execution"),
                    format!("input slot `{}`: {issue}", binding.input_slot),
                ),
            }
        }
        let adapter_outputs = adapter.outputs();
        for output in &adapter_outputs {
            if staging
                .values()
                .any(|input_path| *input_path == output.workspace_path.as_str())
            {
                report.add_finding(
                    CORE_X2001,
                    FindingClass::Invalid,
                    RunStage::ExecutionPlanning,
                    "case_author",
                    SourceLocation::new("case-package", "/executions"),
                    format!(
                        "adapter output `{}` collides with staged input path `{}`",
                        output.output_id, output.workspace_path
                    ),
                );
            }
        }
        let declared_slots: BTreeSet<&str> = execution
            .outputs
            .iter()
            .map(|output| output.output_slot.as_str())
            .collect();
        let adapter_output_slots = adapter.output_slots();
        let adapter_slots: BTreeSet<&str> = adapter_output_slots.iter().copied().collect();
        if declared_slots != adapter_slots {
            report.add_finding(
                CORE_X2801,
                FindingClass::Invalid,
                RunStage::ExecutionPlanning,
                "case_author",
                SourceLocation::new("case-package", "/executions"),
                format!(
                    "package binds output slots {:?}, but adapter `{}` produces {:?}",
                    declared_slots, execution.adapter, adapter_slots
                ),
            );
        }
        if !report.findings.is_empty() {
            return Ok(report);
        }
        let claim_ids_by_slot: BTreeMap<&str, &str> = execution
            .outputs
            .iter()
            .map(|output| (output.output_slot.as_str(), output.claim_id.as_str()))
            .collect();

        // Unchecked bytes: without an executable the step is simply not run;
        // with one, the runner refuses, because it executes only over bytes
        // it verified and reuses only against them.
        let executable = self
            .options
            .capabilities
            .get(&execution.capability_id)
            .cloned();
        if !unverified.is_empty() {
            match executable {
                None => {
                    report.capability = Some(not_supplied);
                    report.state = StepExecutionState::NotRun;
                }
                Some(_) => {
                    for issue in unverified {
                        report.add_finding(
                            CORE_X2101,
                            FindingClass::Inadmissible,
                            RunStage::ExecutionPlanning,
                            "operator",
                            SourceLocation::new("case-run", "/execution"),
                            issue,
                        );
                    }
                }
            }
            return Ok(report);
        }

        // The environment keys the adapter or the package declares are part
        // of the planned invocation by name. Their values are needed only if
        // the step actually runs, so a reuse or a not-run step needs none.
        let required_keys: BTreeSet<&str> = adapter
            .required_environment()
            .iter()
            .copied()
            .chain(execution.environment.iter().map(String::as_str))
            .collect();
        let required_environment: Vec<String> =
            required_keys.iter().map(|key| (*key).to_string()).collect();
        let mut supplied_environment = BTreeMap::new();
        let mut missing_environment = Vec::new();
        for key in required_keys {
            match self.options.environment.get(key) {
                Some(value) => {
                    supplied_environment.insert(key.to_string(), value.clone());
                }
                None => missing_environment.push(key.to_string()),
            }
        }

        // Plan the invocation from the bound inputs, the parameters, and the
        // package's capability identity; the executable is not needed yet.
        let parameters: BTreeMap<String, Value> = step
            .parameters
            .iter()
            .map(|(id, value)| serde_json::to_value(value).map(|value| (id.clone(), value)))
            .collect::<Result<_, _>>()?;
        let context = StepContext {
            parameters: parameters.clone(),
            seed: step.reproducibility.seed.clone(),
        };
        let executable = executable.map(|path| fs::canonicalize(&path).unwrap_or(path));
        let program = executable
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| declared.capability_id.clone());
        let plan = match plan_invocation(
            &adapter,
            &identity,
            &program,
            &context,
            &required_environment,
            &supplied_environment,
            &staged,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                report.add_finding(
                    CORE_X2201,
                    FindingClass::Invalid,
                    RunStage::ExecutionPlanning,
                    "adapter_owner",
                    SourceLocation::new("case-run", "/execution"),
                    format!("the invocation could not be planned: {error}"),
                );
                return Ok(report);
            }
        };
        report.planned_invocation_sha256 = Some(plan.invocation_sha256.clone());

        // The producer's qualification envelope over this run's facts, when
        // the package binds one for this adapter and capability. Evaluated
        // before anything runs so a plan can already say "outside".
        if let Some(bound) = self.envelopes.records.iter().find(|bound| {
            bound.record.adapter == execution.adapter
                && bound.record.capability.capability_id == execution.capability_id
        }) {
            let mut staged_bytes = Vec::with_capacity(staged.len());
            for input in &staged {
                staged_bytes.push((
                    input.input_slot.clone(),
                    input.media_type.clone(),
                    input.expected_sha256.clone(),
                    fs::read(&input.source_path)?,
                ));
            }
            match adapter.applicability(&staged_bytes, &plan.invocation_sha256) {
                Ok(context) => {
                    report.qualification = Some(evaluate_envelope(
                        &bound.record,
                        &bound.sha256,
                        &self.envelopes.kinds,
                        &context,
                    ));
                }
                Err(error) => {
                    report.add_finding(
                        CORE_X2301,
                        FindingClass::Inadmissible,
                        RunStage::ExecutionPlanning,
                        "method_owner",
                        SourceLocation::new("case-run", "/execution"),
                        format!("facts for the qualification envelope: {error}"),
                    );
                    return Ok(report);
                }
            }
        }
        self.current_qualification = report.qualification.as_ref().map(|assessment| {
            serde_json::to_value(ClaimQualification::from(assessment)).unwrap_or(Value::Null)
        });

        // Compare with the committed receipt: what changed, by class.
        let committed = self.committed_receipt(&step.step_id)?;
        report.changes = match &committed {
            Some((_, receipt)) => changes_since(receipt, &plan, &identity, &parameters),
            None => vec![ChangeRecord {
                class: ChangeClass::NoCommittedReceipt,
                detail: "the package commits no execution receipt for this step".into(),
            }],
        };

        // Reuse: same invocation identity, a completed receipt, and every
        // recorded output verifiable at a bound identity (SC-12 memoization).
        if self.options.reuse
            && report.changes.is_empty()
            && let Some((document_id, receipt)) = &committed
        {
            match self.reusable_outputs(receipt) {
                Ok(outputs) => {
                    let mut output_bytes = BTreeMap::new();
                    for (output, path) in &outputs {
                        output_bytes.insert(output.output_id.clone(), fs::read(path)?);
                    }
                    let extracted = match adapter.extract_claims(&output_bytes, &context) {
                        Ok(extracted) => extracted,
                        Err(issue) => {
                            report.add_finding(
                                CORE_X2701,
                                FindingClass::Invalid,
                                RunStage::ClaimGeneration,
                                "adapter_owner",
                                SourceLocation::new("case-run", "/execution"),
                                format!("claim extraction over reused outputs: {issue}"),
                            );
                            report.state = StepExecutionState::Failed;
                            return Ok(report);
                        }
                    };
                    let extracted_slots: BTreeSet<&str> = extracted
                        .iter()
                        .map(|claim| claim.output_slot.as_str())
                        .collect();
                    if extracted_slots != adapter_slots {
                        report.add_finding(
                            CORE_X2801,
                            FindingClass::Invalid,
                            RunStage::ClaimGeneration,
                            "adapter_owner",
                            SourceLocation::new("case-run", "/execution"),
                            format!(
                                "adapter extracted claims for {:?}, but declares {:?}",
                                extracted_slots, adapter_slots
                            ),
                        );
                        report.state = StepExecutionState::Failed;
                        return Ok(report);
                    }
                    for (output, _) in &outputs {
                        report.outputs.push(OutputReport {
                            output_id: output.output_id.clone(),
                            workspace_path: output.workspace_path.clone(),
                            state: output.state,
                            sha256: output.sha256.clone(),
                            bytes: output.bytes,
                            reproduces_bound_artifact: Some(true),
                        });
                    }
                    report.receipt = Some(ReceiptSummary {
                        workspace_path: self.document_path(document_id),
                        sha256: self.document_sha256(document_id),
                        invocation_sha256: receipt.invocation_sha256.clone(),
                        status: receipt.status,
                        exit_status: receipt.process.exit_status,
                        duration_ms: receipt.process.duration_ms,
                    });
                    self.promote(
                        step,
                        &identity,
                        &claim_ids_by_slot,
                        &extracted,
                        &outputs,
                        true,
                    )?;
                    report.reused_receipt = Some(document_id.clone());
                    report.state = StepExecutionState::Reused;
                    return Ok(report);
                }
                Err(detail) => report.changes.push(ChangeRecord {
                    class: ChangeClass::OutputsUnavailable,
                    detail,
                }),
            }
        }

        if self.options.plan_only {
            report.state = StepExecutionState::Planned;
            return Ok(report);
        }

        // Without an executable there is nothing to run: the recorded claims
        // stand as attestations and the gap is reported, not hidden.
        let Some(executable) = executable else {
            report.capability = Some(not_supplied);
            report.state = StepExecutionState::NotRun;
            return Ok(report);
        };

        // Running needs a value for every required key.
        if !missing_environment.is_empty() {
            for key in &missing_environment {
                report.add_finding(
                    CORE_X2402,
                    FindingClass::Missing,
                    RunStage::ExecutionPlanning,
                    "operator",
                    SourceLocation::new("case-run", "/execution"),
                    format!(
                        "environment `{key}` is required to execute this step and was not supplied; pass --env {key}=VALUE"
                    ),
                );
            }
            return Ok(report);
        }

        // The executable must be exactly the bytes the package binds.
        let capability_check = match sha256_file(&executable) {
            Ok((actual, _)) => {
                let state = if actual == declared.executable_sha256 {
                    CapabilityCheckState::Verified
                } else {
                    CapabilityCheckState::Mismatch
                };
                CapabilityCheck {
                    capability_id: declared.capability_id.clone(),
                    package_id: declared.package_id.clone(),
                    expected_sha256: declared.executable_sha256.clone(),
                    actual_sha256: Some(actual),
                    state,
                }
            }
            Err(_) => CapabilityCheck {
                capability_id: declared.capability_id.clone(),
                package_id: declared.package_id.clone(),
                expected_sha256: declared.executable_sha256.clone(),
                actual_sha256: None,
                state: CapabilityCheckState::Missing,
            },
        };
        match capability_check.state {
            CapabilityCheckState::Verified => {}
            CapabilityCheckState::Mismatch => report.add_finding(
                CORE_X2401,
                FindingClass::Inadmissible,
                RunStage::ExecutionPlanning,
                "operator",
                SourceLocation::new("case-run", "/execution"),
                format!(
                    "executable `{}` hashes to {}, but the package binds {}",
                    executable.display(),
                    capability_check.actual_sha256.as_deref().unwrap_or("?"),
                    declared.executable_sha256
                ),
            ),
            _ => report.add_finding(
                CORE_X2401,
                FindingClass::Missing,
                RunStage::ExecutionPlanning,
                "operator",
                SourceLocation::new("case-run", "/execution"),
                format!("executable `{}` cannot be read", executable.display()),
            ),
        }
        report.capability = Some(capability_check);
        if !report.findings.is_empty() {
            return Ok(report);
        }

        // Run.
        let workspace = self.workspace_dir()?;
        let step_dir = workspace.join(&step.step_id);
        let request = ExecutionRequest {
            case_id: self.package.manifest.case_id.clone(),
            compiled_snapshot_sha256: self.compiled.snapshot_sha256.clone(),
            step_id: step.step_id.clone(),
            adapter: adapter.clone(),
            capability: identity.clone(),
            executable: executable.clone(),
            context: context.clone(),
            required_environment: required_environment.clone(),
            environment: supplied_environment.clone(),
            inputs: staged.clone(),
        };
        let outcome = match execute_step(&step_dir, &request) {
            Ok(outcome) => outcome,
            Err(error) => {
                report.add_finding(
                    CORE_X2501,
                    FindingClass::Unsatisfied,
                    RunStage::Execution,
                    "capability_provider",
                    SourceLocation::new("case-run", "/execution"),
                    format!("execution could not be completed: {error}"),
                );
                report.state = StepExecutionState::Failed;
                return Ok(report);
            }
        };

        // Verify from bytes, not from memory: re-read the receipt the runner
        // wrote and re-hash everything it names.
        let receipt_bytes = fs::read(&outcome.receipt_path)?;
        let receipt = parse_receipt(&receipt_bytes)?;
        let (receipt_sha256, _) = sha256_file(&outcome.receipt_path)?;
        report.receipt = Some(ReceiptSummary {
            workspace_path: format!("{}/receipt.json", step.step_id),
            sha256: receipt_sha256,
            invocation_sha256: receipt.invocation_sha256.clone(),
            status: receipt.status,
            exit_status: receipt.process.exit_status,
            duration_ms: receipt.process.duration_ms,
        });
        if receipt.invocation_sha256 != plan.invocation_sha256 {
            report.add_finding(
                CORE_X2601,
                FindingClass::Inadmissible,
                RunStage::ReceiptVerification,
                "runner",
                SourceLocation::new("execution-receipt", "/invocation_sha256"),
                format!(
                    "the receipt records invocation {} but the runner planned {}",
                    receipt.invocation_sha256, plan.invocation_sha256
                ),
            );
        }
        let expectations = ReceiptExpectations {
            case_id: self.package.manifest.case_id.clone(),
            compiled_snapshot_sha256: self.compiled.snapshot_sha256.clone(),
            step_id: step.step_id.clone(),
            capability_type: expected_type,
            adapter: adapter.id().into(),
            adapter_sha256: adapter.descriptor_sha256().map(str::to_owned),
            capability: identity.clone(),
            inputs: staged
                .iter()
                .map(|input| {
                    (
                        input.input_slot.clone(),
                        ExpectedInput {
                            evidence_id: input.evidence_id.clone(),
                            sha256: input.expected_sha256.clone(),
                            media_type: input.media_type.clone(),
                        },
                    )
                })
                .collect(),
            outputs: adapter_outputs
                .iter()
                .map(|output| output.output_id.to_string())
                .collect(),
        };
        let verification = verify_receipt(&receipt, &outcome.step_dir, &expectations)?;
        let verified = verification.state == ReceiptCheckState::Verified;
        for issue in &verification.issues {
            report.add_finding(
                CORE_X2601,
                FindingClass::Inadmissible,
                RunStage::ReceiptVerification,
                "capability_provider",
                SourceLocation::new("execution-receipt", ""),
                issue,
            );
        }
        report.verification = Some(verification);

        let mut extracted = Vec::new();
        if verified {
            let mut output_bytes = BTreeMap::new();
            for output in &receipt.outputs {
                if output.state == OutputState::Collected {
                    output_bytes.insert(
                        output.output_id.clone(),
                        fs::read(outcome.step_dir.join(&output.workspace_path))?,
                    );
                }
            }
            match adapter.extract_claims(&output_bytes, &context) {
                Ok(claims) => extracted = claims,
                Err(issue) => report.add_finding(
                    CORE_X2701,
                    FindingClass::Invalid,
                    RunStage::ClaimGeneration,
                    "adapter_owner",
                    SourceLocation::new("case-run", "/execution"),
                    format!("claim extraction: {issue}"),
                ),
            }
            let extracted_slots: BTreeSet<&str> = extracted
                .iter()
                .map(|claim| claim.output_slot.as_str())
                .collect();
            if !extracted.is_empty() && extracted_slots != adapter_slots {
                report.add_finding(
                    CORE_X2801,
                    FindingClass::Invalid,
                    RunStage::ClaimGeneration,
                    "adapter_owner",
                    SourceLocation::new("case-run", "/execution"),
                    format!(
                        "adapter extracted claims for {:?}, but declares {:?}",
                        extracted_slots, adapter_slots
                    ),
                );
            }
        }

        for output in &receipt.outputs {
            let bound: BTreeSet<&str> = extracted
                .iter()
                .filter(|claim| claim.output_id == output.output_id)
                .filter_map(|claim| claim_ids_by_slot.get(claim.output_slot.as_str()).copied())
                .collect();
            let bound_identities: BTreeSet<&str> = bound
                .iter()
                .filter_map(|claim_id| self.artifact_checks.get(*claim_id))
                .map(|check| check.expected_sha256.as_str())
                .collect();
            let reproduces_bound_artifact = match (&output.sha256, bound_identities.len()) {
                (Some(sha256), 1) if self.replay_applicable => {
                    Some(bound_identities.contains(sha256.as_str()))
                }
                _ => None,
            };
            report.outputs.push(OutputReport {
                output_id: output.output_id.clone(),
                workspace_path: output.workspace_path.clone(),
                state: output.state,
                sha256: output.sha256.clone(),
                bytes: output.bytes,
                reproduces_bound_artifact,
            });
        }

        report.replay = self.replay_receipt(&step.step_id, &receipt)?;

        if !verified || !report.findings.is_empty() {
            report.state = StepExecutionState::Failed;
            return Ok(report);
        }

        let produced: Vec<(ReceiptOutput, PathBuf)> = receipt
            .outputs
            .iter()
            .map(|output| {
                (
                    output.clone(),
                    outcome.step_dir.join(&output.workspace_path),
                )
            })
            .collect();
        self.promote(
            step,
            &identity,
            &claim_ids_by_slot,
            &extracted,
            &produced,
            false,
        )?;
        report.state = StepExecutionState::Executed;
        Ok(report)
    }

    /// Promote produced or reused outputs: they become available to later
    /// steps, and their extracted claims enter the generated document.
    fn promote(
        &mut self,
        step: &CompiledStep,
        identity: &CapabilityIdentity,
        claim_ids_by_slot: &BTreeMap<&str, &str>,
        extracted: &[ExtractedClaim],
        produced: &[(ReceiptOutput, PathBuf)],
        reused: bool,
    ) -> Result<(), Box<dyn Error>> {
        let qualification = self.current_qualification.clone();
        for claim in extracted {
            let (output, path) = produced
                .iter()
                .find(|(output, _)| output.output_id == claim.output_id)
                .ok_or_else(|| format!("adapter named unknown output `{}`", claim.output_id))?;
            let sha256 = output
                .sha256
                .clone()
                .ok_or_else(|| format!("collected output `{}` has no digest", claim.output_id))?;
            let claim_id = claim_ids_by_slot
                .get(claim.output_slot.as_str())
                .ok_or_else(|| format!("output slot `{}` has no claim id", claim.output_slot))?;
            self.fresh_outputs.insert(
                (step.step_id.clone(), claim.output_slot.clone()),
                FreshOutput {
                    evidence_id: (*claim_id).to_string(),
                    path: path.clone(),
                    sha256: sha256.clone(),
                    media_type: output.media_type.clone(),
                },
            );
            self.claims.push(GeneratedClaim {
                claim_id: (*claim_id).to_string(),
                step_id: step.step_id.clone(),
                output_slot: claim.output_slot.clone(),
                artifact_sha256: sha256,
                media_type: output.media_type.clone(),
                producer_package_id: identity.package_id.clone(),
                producer_sha256: identity.executable_sha256.clone(),
                claim: claim.claim.clone(),
                qualification: qualification.clone(),
                reused,
            });
        }
        Ok(())
    }

    /// The committed receipt document for a step, parsed.
    fn committed_receipt(
        &self,
        step_id: &str,
    ) -> Result<Option<(String, ExecutionReceipt)>, Box<dyn Error>> {
        let Some(document) = self.package.manifest.documents.iter().find(|document| {
            document.role == "execution_receipt" && document.step_id.as_deref() == Some(step_id)
        }) else {
            return Ok(None);
        };
        let bytes = self
            .package
            .document_by_id(&document.document_id)
            .ok_or("committed receipt was not readable after package verification")?;
        Ok(Some((document.document_id.clone(), parse_receipt(bytes)?)))
    }

    fn document_path(&self, document_id: &str) -> String {
        self.package
            .manifest
            .documents
            .iter()
            .find(|document| document.document_id == document_id)
            .map(|document| document.path.clone())
            .unwrap_or_default()
    }

    fn document_sha256(&self, document_id: &str) -> String {
        self.package
            .manifest
            .documents
            .iter()
            .find(|document| document.document_id == document_id)
            .map(|document| document.sha256.clone())
            .unwrap_or_default()
    }

    /// Every output a completed receipt recorded, located as a verified bound
    /// artifact with the same digest. An output that cannot be located that
    /// way makes the receipt unusable for reuse.
    fn reusable_outputs(
        &self,
        receipt: &ExecutionReceipt,
    ) -> Result<Vec<(ReceiptOutput, PathBuf)>, String> {
        if receipt.status != ReceiptStatus::Completed || receipt.process.exit_status != Some(0) {
            return Err("the committed receipt did not complete with exit status 0".into());
        }
        let mut outputs = Vec::new();
        for output in &receipt.outputs {
            let Some(sha256) = output.sha256.as_deref() else {
                return Err(format!("output `{}` carries no digest", output.output_id));
            };
            let located = self.package.integrity.artifacts.iter().find(|check| {
                check.state == IntegrityCheckState::Verified && check.expected_sha256 == sha256
            });
            let Some(check) = located else {
                return Err(format!(
                    "output `{}` ({sha256}) is not available as a verified bound artifact",
                    output.output_id
                ));
            };
            let path = if check.source_root == "supplied" {
                self.supplied
                    .iter()
                    .find(|(evidence_id, _)| check.evidence_ids.contains(evidence_id))
                    .map(|(_, path)| path.clone())
                    .ok_or_else(|| "a supplied artifact has no path".to_string())?
            } else {
                self.canonical_roots
                    .get(&check.source_root)
                    .map(|root| root.join(&check.path))
                    .ok_or_else(|| format!("root `{}` is not resolved", check.source_root))?
            };
            outputs.push((output.clone(), path));
        }
        Ok(outputs)
    }

    fn workspace_dir(&mut self) -> Result<PathBuf, Box<dyn Error>> {
        if let Some(workspace) = &self.workspace {
            return Ok(workspace.clone());
        }
        let workspace = match &self.options.workspace {
            Some(path) => path.clone(),
            None => {
                let stamp: String = rfc3339_now()
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric())
                    .collect();
                PathBuf::from("workspaces")
                    .join(&self.package.manifest.case_id)
                    .join(format!("{stamp}-{}", std::process::id()))
            }
        };
        if workspace.exists() && fs::read_dir(&workspace)?.next().is_some() {
            return Err(format!(
                "workspace `{}` exists and is not empty; the runner never reuses a workspace",
                workspace.display()
            )
            .into());
        }
        fs::create_dir_all(&workspace)?;
        // Absolute, so confinement checks agree with the paths staged under it
        // when the operator names a relative workspace.
        let workspace = fs::canonicalize(&workspace)?;
        self.workspace = Some(workspace.clone());
        Ok(workspace)
    }

    fn resolve_source(&self, source: &SourceRef) -> Result<ResolvedArtifact, String> {
        match source {
            SourceRef::ContractInput { input_id } => {
                let media_type = self
                    .compiled
                    .inputs
                    .iter()
                    .find(|input| &input.input_id == input_id)
                    .map(|input| input.media_type.clone())
                    .ok_or_else(|| format!("contract input `{input_id}` is not compiled"))?;
                self.resolve_artifact(&format!("input:{input_id}"), media_type)
            }
            SourceRef::StepOutput {
                step_id,
                output_slot,
            } => {
                if let Some(fresh) = self
                    .fresh_outputs
                    .get(&(step_id.clone(), output_slot.clone()))
                {
                    return Ok(ResolvedArtifact {
                        evidence_id: fresh.evidence_id.clone(),
                        path: fresh.path.clone(),
                        sha256: fresh.sha256.clone(),
                        media_type: fresh.media_type.clone(),
                        integrity: IntegrityCheckState::Verified,
                    });
                }
                let recorded = self
                    .committed_claims
                    .get("claims")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find(|claim| {
                        claim.get("step_id").and_then(Value::as_str) == Some(step_id)
                            && claim.get("output_slot").and_then(Value::as_str)
                                == Some(output_slot)
                    })
                    .ok_or_else(|| {
                        format!(
                            "step `{step_id}` output `{output_slot}` was neither executed nor recorded in the committed claims"
                        )
                    })?;
                let claim_id = recorded
                    .get("claim_id")
                    .and_then(Value::as_str)
                    .ok_or("recorded claim has no claim_id")?;
                let media_type = recorded
                    .pointer("/artifact/media_type")
                    .and_then(Value::as_str)
                    .ok_or("recorded claim has no artifact media type")?;
                self.resolve_artifact(claim_id, media_type.to_string())
            }
        }
    }

    fn resolve_artifact(
        &self,
        evidence_id: &str,
        media_type: String,
    ) -> Result<ResolvedArtifact, String> {
        let check = self
            .artifact_checks
            .get(evidence_id)
            .ok_or_else(|| format!("evidence record `{evidence_id}` has no package artifact"))?;
        let path = if check.source_root == "supplied" {
            self.supplied.get(evidence_id).cloned().unwrap_or_default()
        } else {
            self.canonical_roots
                .get(&check.source_root)
                .map(|root| root.join(&check.path))
                .unwrap_or_default()
        };
        Ok(ResolvedArtifact {
            evidence_id: evidence_id.to_string(),
            path,
            sha256: check.expected_sha256.clone(),
            media_type,
            integrity: check.state,
        })
    }

    /// Compare a fresh receipt with the receipt the package committed for
    /// the same step: same request identity, same capability, same outputs.
    /// Timestamps and durations are observations and are not compared.
    fn replay_receipt(
        &self,
        step_id: &str,
        fresh: &ExecutionReceipt,
    ) -> Result<Option<ReceiptReplayReport>, Box<dyn Error>> {
        if !self.replay_applicable {
            return Ok(None);
        }
        let Some(document) = self.package.manifest.documents.iter().find(|document| {
            document.role == "execution_receipt" && document.step_id.as_deref() == Some(step_id)
        }) else {
            return Ok(None);
        };
        let bytes = self
            .package
            .document_by_id(&document.document_id)
            .ok_or("committed receipt was not readable after package verification")?;
        let committed = parse_receipt(bytes)?;
        let mut differences = Vec::new();
        if committed.invocation_sha256 != fresh.invocation_sha256 {
            differences.push(format!(
                "invocation identity {} → {}",
                committed.invocation_sha256, fresh.invocation_sha256
            ));
        }
        if committed.capability != fresh.capability {
            differences.push(format!(
                "capability {} → {}",
                committed.capability.executable_sha256, fresh.capability.executable_sha256
            ));
        }
        if committed.status != fresh.status {
            differences.push(format!(
                "status {:?} → {:?}",
                committed.status, fresh.status
            ));
        }
        let outputs = |receipt: &ExecutionReceipt| -> BTreeMap<String, Option<String>> {
            receipt
                .outputs
                .iter()
                .map(|output| (output.output_id.clone(), output.sha256.clone()))
                .collect()
        };
        let committed_outputs = outputs(&committed);
        let fresh_outputs = outputs(fresh);
        for (output_id, sha256) in &committed_outputs {
            match fresh_outputs.get(output_id) {
                Some(actual) if actual == sha256 => {}
                Some(actual) => differences.push(format!(
                    "output `{output_id}` {} → {}",
                    sha256.as_deref().unwrap_or("missing"),
                    actual.as_deref().unwrap_or("missing")
                )),
                None => differences.push(format!(
                    "output `{output_id}` is absent from the fresh receipt"
                )),
            }
        }
        for output_id in fresh_outputs.keys() {
            if !committed_outputs.contains_key(output_id) {
                differences.push(format!(
                    "output `{output_id}` is absent from the committed receipt"
                ));
            }
        }
        Ok(Some(ReceiptReplayReport {
            document_id: document.document_id.clone(),
            matches: differences.is_empty(),
            differences,
        }))
    }
}

/// What differs between a committed receipt and the invocation planned now,
/// by SC-12 change class. Empty means the receipt describes exactly this
/// request.
fn changes_since(
    committed: &ExecutionReceipt,
    plan: &PlannedInvocation,
    capability: &CapabilityIdentity,
    parameters: &BTreeMap<String, Value>,
) -> Vec<ChangeRecord> {
    let mut changes = Vec::new();
    if committed.capability != *capability {
        changes.push(ChangeRecord {
            class: ChangeClass::Capability,
            detail: format!(
                "executable {} → {}",
                committed.capability.executable_sha256, capability.executable_sha256
            ),
        });
    }
    let keys: BTreeSet<&String> = committed
        .parameters
        .keys()
        .chain(parameters.keys())
        .collect();
    for key in keys {
        if committed.parameters.get(key) != parameters.get(key) {
            changes.push(ChangeRecord {
                class: ChangeClass::Parameters,
                detail: format!(
                    "parameter `{key}` {} → {}",
                    committed
                        .parameters
                        .get(key)
                        .map_or("absent".to_string(), Value::to_string),
                    parameters
                        .get(key)
                        .map_or("absent".to_string(), Value::to_string)
                ),
            });
        }
    }
    let before: BTreeMap<&str, &ReceiptInput> = committed
        .inputs
        .iter()
        .map(|input| (input.input_slot.as_str(), input))
        .collect();
    let after: BTreeMap<&str, &ReceiptInput> = plan
        .inputs
        .iter()
        .map(|input| (input.input_slot.as_str(), input))
        .collect();
    let slots: BTreeSet<&str> = before.keys().chain(after.keys()).copied().collect();
    for slot in slots {
        match (before.get(slot), after.get(slot)) {
            (Some(old), Some(new)) => {
                if old.evidence_id != new.evidence_id {
                    changes.push(ChangeRecord {
                        class: ChangeClass::InputBinding,
                        detail: format!(
                            "slot `{slot}` bound `{}` → `{}`",
                            old.evidence_id, new.evidence_id
                        ),
                    });
                }
                if old.sha256 != new.sha256 || old.bytes != new.bytes {
                    changes.push(ChangeRecord {
                        class: ChangeClass::InputBytes,
                        detail: format!("slot `{slot}` bytes {} → {}", old.sha256, new.sha256),
                    });
                } else if old.workspace_path != new.workspace_path
                    || old.media_type != new.media_type
                {
                    changes.push(ChangeRecord {
                        class: ChangeClass::Invocation,
                        detail: format!("slot `{slot}` staging path or media type differs"),
                    });
                }
            }
            (Some(_), None) => changes.push(ChangeRecord {
                class: ChangeClass::InputBinding,
                detail: format!("slot `{slot}` is no longer bound"),
            }),
            (None, Some(_)) => changes.push(ChangeRecord {
                class: ChangeClass::InputBinding,
                detail: format!("slot `{slot}` is newly bound"),
            }),
            (None, None) => {}
        }
    }
    let old = &committed.invocation;
    let new = &plan.invocation;
    if old.arguments != new.arguments
        || old.working_directory != new.working_directory
        || old.environment != new.environment
        || old.required_environment != new.required_environment
        || old.timeout_ms != new.timeout_ms
    {
        changes.push(ChangeRecord {
            class: ChangeClass::Invocation,
            detail: "the adapter's arguments, environment, working directory, or timeout differ"
                .into(),
        });
    }
    if committed.status != ReceiptStatus::Completed || committed.process.exit_status != Some(0) {
        changes.push(ChangeRecord {
            class: ChangeClass::ReceiptNotCompleted,
            detail: "the committed receipt did not complete with exit status 0".into(),
        });
    }
    changes
}

fn verify_bindings(
    manifest: &CasePackageManifest,
    claims: &ClaimsDocument,
    compiled: &CompiledContract,
    invalidated_steps: &BTreeSet<String>,
) -> BindingReport {
    let mut issues = Vec::new();
    let mut expected = BTreeMap::<String, String>::new();
    for input in &claims.inputs {
        insert_evidence(
            &mut expected,
            format!("input:{}", input.input_id),
            input.artifact.sha256.clone(),
            &mut issues,
        );
    }
    // Claims for steps a supplied input reaches carry the identity their
    // receipt recorded; the package's declared identity describes the
    // reference candidate and is not compared.
    let mut receipted: BTreeSet<String> = BTreeSet::new();
    let withheld: BTreeSet<&str> = manifest
        .executions
        .iter()
        .filter(|execution| invalidated_steps.contains(&execution.step_id))
        .flat_map(|execution| {
            execution
                .outputs
                .iter()
                .map(|output| output.claim_id.as_str())
        })
        .collect();
    for claim in &claims.claims {
        if invalidated_steps.contains(&claim.step_id) {
            receipted.insert(claim.claim_id.clone());
        }
        insert_evidence(
            &mut expected,
            claim.claim_id.clone(),
            claim.artifact.sha256.clone(),
            &mut issues,
        );
    }

    let mut seen = BTreeSet::new();
    let mut bound_evidence_records = 0;
    let mut receipted_evidence_records = 0;
    let mut withheld_evidence_records = 0;
    for artifact in &manifest.artifacts {
        for evidence_id in &artifact.evidence_ids {
            let Some(expected_sha256) = expected.get(evidence_id) else {
                if withheld.contains(evidence_id.as_str()) {
                    withheld_evidence_records += 1;
                    continue;
                }
                issues.push(format!(
                    "artifact `{}` binds unknown evidence record `{evidence_id}`",
                    artifact.artifact_id
                ));
                continue;
            };
            seen.insert(evidence_id.clone());
            if receipted.contains(evidence_id) {
                receipted_evidence_records += 1;
            } else if expected_sha256 == &artifact.sha256 {
                bound_evidence_records += 1;
            } else {
                issues.push(format!(
                    "artifact `{}` digest does not match evidence record `{evidence_id}`",
                    artifact.artifact_id
                ));
            }
        }
    }
    for evidence_id in expected.keys() {
        if !seen.contains(evidence_id) {
            issues.push(format!(
                "evidence record `{evidence_id}` has no package artifact binding"
            ));
        }
    }

    let required_policies: BTreeSet<String> = compiled
        .workflow
        .iter()
        .filter_map(|step| step.presentation_gate.as_ref())
        .map(|review| review.reviewer_eligibility_policy.sha256.clone())
        .collect();
    let package_policies: BTreeSet<String> = manifest
        .documents
        .iter()
        .filter(|document| document.role == "review_policy")
        .map(|document| document.sha256.clone())
        .collect();
    let bound_presentation_policies = required_policies.intersection(&package_policies).count();
    for digest in required_policies.difference(&package_policies) {
        issues.push(format!(
            "compiled presentation gate requires policy `{digest}`, but the package does not contain it"
        ));
    }
    for digest in package_policies.difference(&required_policies) {
        issues.push(format!(
            "package presentation policy `{digest}` is not referenced by the compiled contract"
        ));
    }

    BindingReport {
        status: if issues.is_empty() {
            BindingStatus::Verified
        } else {
            BindingStatus::Failed
        },
        evidence_records: expected.len(),
        bound_evidence_records,
        receipted_evidence_records,
        withheld_evidence_records,
        required_presentation_policies: required_policies.len(),
        bound_presentation_policies,
        issues,
    }
}

fn insert_evidence(
    expected: &mut BTreeMap<String, String>,
    evidence_id: String,
    sha256: String,
    issues: &mut Vec<String>,
) {
    if expected.insert(evidence_id.clone(), sha256).is_some() {
        issues.push(format!(
            "claims document repeats evidence record `{evidence_id}`"
        ));
    }
}

fn replay_expected(
    package: &VerifiedCasePackage,
    campaign: &CampaignReport,
) -> Result<Option<ReplayReport>, Box<dyn Error>> {
    let Some(document) = package
        .manifest
        .documents
        .iter()
        .find(|document| document.role == "expected_campaign_report")
    else {
        return Ok(None);
    };
    let bytes = package
        .document_by_id(&document.document_id)
        .ok_or("expected campaign report was not readable after package verification")?;
    let expected: Value = serde_json::from_slice(bytes)?;
    let actual = serde_json::to_value(campaign)?;
    Ok(Some(ReplayReport {
        document_id: document.document_id.clone(),
        matches: expected == actual,
    }))
}

pub fn human_summary(report: &CaseRunReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Avila Core case workflow");
    let _ = writeln!(out, "{} — {}", report.case_id, report.title);

    let verified_documents = report
        .integrity
        .documents
        .iter()
        .filter(|check| check.state == IntegrityCheckState::Verified)
        .count();
    let verified_artifacts: Vec<_> = report
        .integrity
        .artifacts
        .iter()
        .filter(|check| check.state == IntegrityCheckState::Verified)
        .collect();
    let verified_evidence: usize = verified_artifacts
        .iter()
        .map(|check| check.evidence_ids.len())
        .sum();
    let total_evidence: usize = report
        .integrity
        .artifacts
        .iter()
        .map(|check| check.evidence_ids.len())
        .sum();
    let unchecked_roots: BTreeSet<_> = report
        .integrity
        .artifacts
        .iter()
        .filter(|check| check.state == IntegrityCheckState::NotChecked)
        .map(|check| check.source_root.as_str())
        .collect();

    let _ = writeln!(out, "\n1. PACKAGE INTEGRITY");
    let _ = writeln!(
        out,
        "   [{}] {verified_documents}/{} package documents re-hashed",
        if verified_documents == report.integrity.documents.len() {
            "VERIFIED"
        } else {
            "FAILED"
        },
        report.integrity.documents.len()
    );
    let _ = writeln!(
        out,
        "   [{}] {}/{} external artifacts re-hashed ({verified_evidence}/{total_evidence} evidence records)",
        integrity_label(report.integrity.status),
        verified_artifacts.len(),
        report.integrity.artifacts.len()
    );
    if !unchecked_roots.is_empty() {
        let _ = writeln!(
            out,
            "   not checked: source root(s) {}",
            unchecked_roots.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    for input in &report.supplied_inputs {
        let _ = writeln!(
            out,
            "   supplied: input `{}` = {} {}",
            input.input_id, input.path, input.sha256
        );
    }
    for check in report.integrity.documents.iter().filter(|check| {
        matches!(
            check.state,
            IntegrityCheckState::Missing | IntegrityCheckState::Mismatch
        )
    }) {
        let _ = writeln!(out, "   {:?}: {}", check.state, check.path);
    }
    for check in report.integrity.artifacts.iter().filter(|check| {
        matches!(
            check.state,
            IntegrityCheckState::Missing | IntegrityCheckState::Mismatch
        )
    }) {
        let _ = writeln!(
            out,
            "   {:?}: {}:{}",
            check.state, check.source_root, check.path
        );
    }

    let _ = writeln!(out, "\n2. COMPILE");
    match report.compile.as_ref() {
        Some(compile) => match compile.compiled.as_ref() {
            Some(compiled) => {
                let _ = writeln!(
                    out,
                    "   [COMPILED] {} revision {}",
                    compiled.contract_id, compiled.contract_revision
                );
                for (index, step) in compiled.workflow.iter().enumerate() {
                    let connector = if index + 1 == compiled.workflow.len() {
                        "└─"
                    } else {
                        "├─"
                    };
                    let _ = writeln!(
                        out,
                        "   {connector} {} [{}@{}]",
                        step.step_id, step.capability_type.id, step.capability_type.major
                    );
                }
                let _ = writeln!(
                    out,
                    "   {} requirement(s); snapshot {}",
                    compiled.requirements.len(),
                    compiled.snapshot_sha256
                );
                if let Some(coverage) = &report.coverage {
                    write_coverage_summary(&mut out, coverage);
                }
            }
            None => {
                let _ = writeln!(
                    out,
                    "   [REJECTED] {} compiler finding(s)",
                    compile.findings.len()
                );
                if let Some(rendered) = &report.rendered_findings {
                    for line in rendered.lines() {
                        let _ = writeln!(out, "   {line}");
                    }
                }
            }
        },
        None => {
            let _ = writeln!(out, "   [NOT RUN]");
        }
    }

    let _ = writeln!(out, "\n3. EXECUTE");
    match report.execution.as_ref() {
        Some(execution) => {
            for step in &execution.steps {
                match step.state {
                    StepExecutionState::Executed => {
                        let _ = writeln!(
                            out,
                            "   [EXECUTED] {} via {} ({}, {})",
                            step.step_id,
                            step.capability_id,
                            step.capability
                                .as_ref()
                                .map_or("?", |capability| capability.package_id.as_str()),
                            step.capability
                                .as_ref()
                                .and_then(|capability| capability.actual_sha256.as_deref())
                                .unwrap_or("?")
                        );
                        let verified_inputs = step
                            .inputs
                            .iter()
                            .filter(|input| input.integrity == IntegrityCheckState::Verified)
                            .count();
                        if let Some(receipt) = &step.receipt {
                            let _ = writeln!(
                                out,
                                "      {verified_inputs}/{} staged inputs verified; exit {} in {} ms; {}/{} declared outputs collected",
                                step.inputs.len(),
                                receipt
                                    .exit_status
                                    .map_or("none".to_string(), |code| code.to_string()),
                                receipt.duration_ms,
                                step.outputs
                                    .iter()
                                    .filter(|output| output.state == OutputState::Collected)
                                    .count(),
                                step.outputs.len()
                            );
                        }
                        for output in &step.outputs {
                            let _ = writeln!(
                                out,
                                "      {} {}{}",
                                output.workspace_path,
                                output.sha256.as_deref().unwrap_or("missing"),
                                match output.reproduces_bound_artifact {
                                    Some(true) => " — reproduces the bound artifact",
                                    Some(false) => " — DIFFERS from the bound artifact",
                                    None => "",
                                }
                            );
                        }
                        if let Some(receipt) = &step.receipt {
                            let _ = writeln!(
                                out,
                                "      receipt {} {} [{}]{}",
                                receipt.workspace_path,
                                receipt.sha256,
                                step.verification.as_ref().map_or("NOT VERIFIED", |check| {
                                    match check.state {
                                        ReceiptCheckState::Verified => "VERIFIED",
                                        ReceiptCheckState::Failed => "FAILED",
                                    }
                                }),
                                match &step.replay {
                                    Some(replay) if replay.matches =>
                                        "; [MATCH] committed receipt".to_string(),
                                    Some(replay) => format!(
                                        "; [DRIFT] committed receipt: {}",
                                        replay.differences.join("; ")
                                    ),
                                    None => String::new(),
                                }
                            );
                        }
                    }
                    StepExecutionState::Reused => {
                        let _ = writeln!(
                            out,
                            "   [REUSED] {} — committed receipt {} matches the planned invocation {}; {} output(s) verified at their bound identities; nothing ran",
                            step.step_id,
                            step.receipt
                                .as_ref()
                                .map_or("?", |receipt| receipt.workspace_path.as_str()),
                            step.planned_invocation_sha256.as_deref().unwrap_or("?"),
                            step.outputs.len()
                        );
                    }
                    StepExecutionState::Planned => {
                        let _ = writeln!(
                            out,
                            "   [PLANNED] {} would execute (invocation {})",
                            step.step_id,
                            step.planned_invocation_sha256.as_deref().unwrap_or("?")
                        );
                    }
                    StepExecutionState::NotRun => {
                        if report.invalidated_steps.contains(&step.step_id) {
                            let _ = writeln!(
                                out,
                                "   [NOT RUN] {} — capability `{}` not supplied; a supplied input reaches this step, so its committed claims describe the reference input and are not carried",
                                step.step_id, step.capability_id
                            );
                        } else {
                            let _ = writeln!(
                                out,
                                "   [NOT RUN] {} — capability `{}` not supplied; its committed claims are evaluated as recorded attestations",
                                step.step_id, step.capability_id
                            );
                        }
                    }
                    StepExecutionState::Refused => {
                        let _ = writeln!(out, "   [REFUSED] {}", step.step_id);
                    }
                    StepExecutionState::Failed => {
                        let _ = writeln!(out, "   [FAILED] {}", step.step_id);
                    }
                }
                if !step.changes.is_empty()
                    && matches!(
                        step.state,
                        StepExecutionState::Executed
                            | StepExecutionState::Planned
                            | StepExecutionState::NotRun
                    )
                {
                    let _ = writeln!(
                        out,
                        "      {}: {}",
                        if step.state == StepExecutionState::Executed {
                            "rerun because"
                        } else {
                            "would rerun because"
                        },
                        step.changes
                            .iter()
                            .map(|change| format!("{:?}: {}", change.class, change.detail))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
                if let Some(assessment) = &step.qualification {
                    let state = match assessment.state {
                        EnvelopeState::Inside => "INSIDE",
                        EnvelopeState::Outside => "OUTSIDE",
                        EnvelopeState::Unknown => "UNKNOWN",
                    };
                    let failed: Vec<String> = assessment
                        .terms
                        .iter()
                        .filter(|term| term.result != TruthValue::True)
                        .map(|term| format!("{} -> {:?}", term.predicate, term.result))
                        .collect();
                    let _ = writeln!(
                        out,
                        "      envelope {} rev {}: [{state}] {}/{} terms hold{}",
                        assessment.qualification_id,
                        assessment.revision,
                        assessment.terms.len() - failed.len(),
                        assessment.terms.len(),
                        if failed.is_empty() {
                            String::new()
                        } else {
                            format!("; {}", failed.join("; "))
                        }
                    );
                    for issue in &assessment.issues {
                        let _ = writeln!(out, "         issue: {issue}");
                    }
                }
                for finding in &step.findings {
                    let _ = writeln!(
                        out,
                        "      [{}] {}\n         next: {}",
                        finding.code, finding.message, finding.next_action
                    );
                }
            }
            if !execution.not_executed.is_empty() {
                let _ = writeln!(
                    out,
                    "   not executed: {}",
                    execution
                        .not_executed
                        .iter()
                        .map(|step| format!("{} ({})", step.step_id, step.reason))
                        .collect::<Vec<_>>()
                        .join("; ")
                );
            }
            if let Some(workspace) = &execution.workspace {
                let _ = writeln!(out, "   workspace {workspace}");
            }
        }
        None => {
            let _ = writeln!(
                out,
                "   [NOT RUN] {}",
                if report
                    .compile
                    .as_ref()
                    .is_some_and(|c| c.compiled.is_some())
                {
                    "the package declares no execution"
                } else {
                    "compilation did not pass its gate"
                }
            );
        }
    }

    let _ = writeln!(out, "\n4. CLAIMS");
    match (&report.claims, &report.bindings) {
        (Some(claims), bindings) => {
            let _ = writeln!(
                out,
                "   [GENERATED] {} input attestations from package identities; {} claims from executed outputs; {} from reused outputs; {} recorded claims carried",
                claims.input_attestations,
                claims.executed_claims,
                claims.reused_claims,
                claims.recorded_claims
            );
            for claim in &claims.evidence_claims {
                if let (Some(slot), Some(value)) = (
                    claim.get("output_slot").and_then(Value::as_str),
                    claim.pointer("/claim/value").and_then(Value::as_str),
                ) {
                    let _ = writeln!(out, "   category {slot}: {value}");
                }
            }
            if claims.invalidated_claims > 0 {
                let _ = writeln!(
                    out,
                    "   not carried: {} recorded claim(s) for step(s) reached by a supplied input ({})",
                    claims.invalidated_claims,
                    report.invalidated_steps.join(", ")
                );
            }
            if report.replay_applicable {
                let _ = writeln!(
                    out,
                    "   [{}] generated claims {} committed claims.json",
                    if claims.matches_committed {
                        "MATCH"
                    } else {
                        "MISMATCH"
                    },
                    if claims.matches_committed {
                        "match"
                    } else {
                        "differ from"
                    }
                );
            } else {
                let _ = writeln!(
                    out,
                    "   [NOT APPLICABLE] committed claims describe the reference candidate, not the supplied input(s)"
                );
            }
            match bindings {
                Some(bindings) => {
                    let _ = writeln!(
                        out,
                        "   [{}] {}/{} evidence identities bound{}; {}/{} presentation-policy identities",
                        binding_label(bindings.status),
                        bindings.bound_evidence_records,
                        bindings.evidence_records,
                        match (
                            bindings.receipted_evidence_records,
                            bindings.withheld_evidence_records
                        ) {
                            (0, 0) => String::new(),
                            (receipted, 0) => {
                                format!(
                                    " ({receipted} carried by receipt for supplied-input steps)"
                                )
                            }
                            (0, withheld) => {
                                format!(
                                    " ({withheld} withheld: reached by a supplied input and not run)"
                                )
                            }
                            (receipted, withheld) => format!(
                                " ({receipted} carried by receipt for supplied-input steps; {withheld} withheld, not run)"
                            ),
                        },
                        bindings.bound_presentation_policies,
                        bindings.required_presentation_policies
                    );
                    for issue in &bindings.issues {
                        let _ = writeln!(out, "   issue: {issue}");
                    }
                }
                None => {
                    let _ = writeln!(out, "   [NOT BOUND]");
                }
            }
        }
        (None, _) if report.status == CaseRunStatus::Planned => {
            let _ = writeln!(out, "   [NOT RUN] plan only");
        }
        (None, _) => {
            let _ = writeln!(out, "   [NOT RUN] an earlier gate did not pass");
        }
    }

    let _ = writeln!(out, "\n5. EVALUATE");
    match report.campaign.as_ref() {
        None if report.status == CaseRunStatus::Planned => {
            let _ = writeln!(out, "   [NOT RUN] plan only");
        }
        Some(campaign) => {
            let admitted = campaign
                .admissions
                .iter()
                .filter(|record| record.state == AdmissionState::Admitted)
                .count();
            let _ = writeln!(
                out,
                "   [EVALUATED] {admitted}/{} evidence records structurally admitted",
                campaign.admissions.len()
            );
            for verdict in &campaign.verdicts {
                let margin = report
                    .margins
                    .iter()
                    .find(|margin| margin.requirement_id == verdict.requirement_id);
                let numbers = margin.map_or(String::new(), |margin| {
                    let unit = margin.unit.as_deref().unwrap_or("");
                    let mut parts = Vec::new();
                    if let (Some(lower), Some(upper)) = (&margin.lower, &margin.upper) {
                        parts.push(format!(
                            "[{}, {}] {unit}",
                            display_number(lower),
                            display_number(upper)
                        ));
                    } else if let Some(nominal) = &margin.nominal {
                        parts.push(format!("nominal {} {unit}", display_number(nominal)));
                    }
                    if let Some(limit) = &margin.limit {
                        parts.push(format!("limit {} {unit}", display_number(limit)));
                    }
                    if let Some(value) = &margin.margin {
                        parts.push(format!("margin {} {unit}", display_number(value)));
                    }
                    if parts.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", parts.join("; "))
                    }
                });
                let _ = writeln!(
                    out,
                    "   [{}] {} — {}{numbers}",
                    verdict_label(verdict.verdict.status),
                    verdict.requirement_id,
                    verdict.verdict.rule
                );
                let reasons: Vec<String> = verdict
                    .verdict
                    .reasons
                    .iter()
                    .filter_map(|reason| match reason {
                        avila_core_kernel::VerdictReason::EvidenceState { evidence_id, state } => {
                            Some(format!("{evidence_id}: {state}"))
                        }
                        avila_core_kernel::VerdictReason::CodeOwner { code, owner } => {
                            Some(format!("{code} (owner {owner})"))
                        }
                        _ => None,
                    })
                    .collect();
                if verdict.verdict.status == VerdictStatus::NotEvaluated && !reasons.is_empty() {
                    let _ = writeln!(out, "      because: {}", reasons.join("; "));
                }
            }
            if let Some(identity) = &campaign.campaign_sha256 {
                let _ = writeln!(out, "   campaign {identity}");
            }
        }
        None => {
            let _ = writeln!(out, "   [NOT RUN]");
        }
    }

    if !report.presentation_gates.is_empty() {
        let _ = writeln!(out, "\n6. OPTIONAL PRESENTATION");
        for gate in &report.presentation_gates {
            let state = match gate.readiness {
                PresentationGateReadiness::ReadyForAgent => "READY FOR AGENT",
                PresentationGateReadiness::AwaitingEvidence => "AWAITING EVIDENCE",
            };
            let _ = writeln!(
                out,
                "   [{state}] {} — optional agent practicality gate; {}/{} dossier artifacts present; request {}",
                gate.step_id,
                gate.presented_evidence.len(),
                gate.presented_evidence.len() + gate.missing_evidence.len(),
                gate.request_sha256,
            );
            for instruction in &gate.instructions {
                let _ = writeln!(out, "      instruction: {instruction}");
            }
            for source in &gate.missing_evidence {
                let _ = writeln!(out, "      missing: {}", source.label());
            }
        }
    }

    let replay_section = if report.presentation_gates.is_empty() {
        6
    } else {
        7
    };
    if !report.replay_applicable && report.campaign.is_some() {
        let _ = writeln!(out, "\n{replay_section}. REPLAY");
        let _ = writeln!(
            out,
            "   [NOT APPLICABLE] free input(s) supplied; committed expectations describe the reference candidate"
        );
    }
    if let Some(replay) = &report.replay {
        let _ = writeln!(out, "\n{replay_section}. REPLAY");
        let _ = writeln!(
            out,
            "   [{}] generated campaign report {} committed expectation",
            if replay.matches { "MATCH" } else { "MISMATCH" },
            if replay.matches {
                "matches"
            } else {
                "differs from"
            }
        );
    }

    if !report.findings.is_empty() {
        let _ = writeln!(
            out,
            "\nACTIONABLE FEEDBACK ({} finding(s))",
            report.findings.len()
        );
        for finding in &report.findings {
            let step = finding
                .step_id
                .as_deref()
                .map(|step_id| format!(" step {step_id}"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "   [{} · {}{step} · owner {}] {}",
                finding.code,
                run_stage_label(finding.stage),
                finding.owner,
                finding.message
            );
            let _ = writeln!(out, "      next: {}", finding.next_action);
        }
    }

    let execution_phrase = match report.execution.as_ref().map(|execution| execution.status) {
        Some(ExecutionStatus::Executed) => {
            "every declared step executed or was reused under a verified receipt"
        }
        Some(ExecutionStatus::Reused) => {
            "every declared step was reused from a committed receipt whose invocation identity and outputs still verify, so nothing ran"
        }
        Some(ExecutionStatus::Planned) => "execution was planned only",
        Some(ExecutionStatus::NotRun) => "no step was executed",
        Some(ExecutionStatus::Partial) => {
            "some declared steps executed and others were not supplied"
        }
        Some(ExecutionStatus::Refused) => "execution was refused",
        Some(ExecutionStatus::Failed) => "execution failed",
        None if report
            .compile
            .as_ref()
            .is_some_and(|compile| compile.compiled.is_some()) =>
        {
            "no execution is declared"
        }
        None => "execution was not reached",
    };
    let _ = writeln!(
        out,
        "\nOutcome: {}. Byte integrity is {}; {execution_phrase}; {}",
        match report.status {
            CaseRunStatus::Evaluated => "workflow evaluated",
            CaseRunStatus::Rejected => "workflow rejected",
            CaseRunStatus::Planned => "workflow planned, not run",
        },
        match report.integrity.status {
            PackageIntegrityStatus::Complete => "complete for every declared artifact",
            PackageIntegrityStatus::Partial =>
                "partial because at least one source root was not supplied",
            PackageIntegrityStatus::Failed => "failed",
        },
        if report.campaign.is_some() {
            "requirement results remain exactly as reported above."
        } else {
            "no requirement result was produced."
        },
    );
    out
}

fn integrity_label(status: PackageIntegrityStatus) -> &'static str {
    match status {
        PackageIntegrityStatus::Complete => "COMPLETE",
        PackageIntegrityStatus::Partial => "PARTIAL",
        PackageIntegrityStatus::Failed => "FAILED",
    }
}

fn binding_label(status: BindingStatus) -> &'static str {
    match status {
        BindingStatus::Verified => "VERIFIED",
        BindingStatus::Failed => "FAILED",
    }
}

fn verdict_label(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Pass => "PASS",
        VerdictStatus::Fail => "FAIL",
        VerdictStatus::Inconclusive => "INCONCLUSIVE",
        VerdictStatus::NotEvaluated => "NOT_EVALUATED",
    }
}

fn run_stage_label(stage: RunStage) -> &'static str {
    match stage {
        RunStage::PackageIntegrity => "package_integrity",
        RunStage::Compilation => "compilation",
        RunStage::Coverage => "coverage",
        RunStage::ExecutionPlanning => "execution_planning",
        RunStage::Execution => "execution",
        RunStage::ReceiptVerification => "receipt_verification",
        RunStage::ClaimGeneration => "claim_generation",
        RunStage::EvidenceBinding => "evidence_binding",
        RunStage::CampaignEvaluation => "campaign_evaluation",
        RunStage::Replay => "replay",
        RunStage::Infrastructure => "infrastructure",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case_000() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-000-actinv-aftermatter")
    }

    fn case_001() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-001-shield-search")
    }

    #[test]
    fn case_001_materializes_an_exact_optional_practical_review_request() {
        let report = execute_case(&case_001(), &CaseRunOptions::default()).unwrap();
        assert_eq!(report.status, CaseRunStatus::Evaluated);
        assert!(report.replay.as_ref().unwrap().matches);

        let [stage] = report.presentation_gates.as_slice() else {
            panic!("CASE-001 must materialize exactly one staged review request");
        };
        assert_eq!(stage.step_id, "practical-review");
        assert_eq!(stage.reviewer_role, ReviewerRole::Agent);
        assert_eq!(stage.gate_state, PresentationGateState::AwaitingAgent);
        assert_eq!(stage.readiness, PresentationGateReadiness::ReadyForAgent);
        assert_eq!(
            stage.request_sha256,
            "sha256:3c998b6cf03fea23e9fc6cf37366aa21969899e69c5aae1805bc976cfa7a8445"
        );
        assert_eq!(
            stage
                .presented_evidence
                .iter()
                .map(|evidence| evidence.evidence_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "input:reviewer-script",
                "input:candidate",
                "screen-result",
                "transport-result",
            ]
        );
        assert!(stage.missing_evidence.is_empty());
        assert_eq!(stage.instructions.len(), 4);
        assert_eq!(
            stage.allowed_dispositions,
            vec![
                ReviewDisposition::PresentToUser,
                ReviewDisposition::RequestChanges,
                ReviewDisposition::Abstain,
            ]
        );

        let transport = report
            .campaign
            .as_ref()
            .unwrap()
            .verdicts
            .iter()
            .find(|verdict| verdict.requirement_id == "SHIELD-R2-transport")
            .unwrap();
        assert_eq!(transport.verdict.status, VerdictStatus::Fail);

        let contract = fs::read(case_001().join("contract.json")).unwrap();
        let registry = fs::read(case_001().join("registry.json")).unwrap();
        let compile = compile_documents(&contract, &registry).unwrap();
        let compiled = compile.compiled.as_ref().unwrap();
        let mut claims: ClaimsDocument =
            serde_json::from_slice(&fs::read(case_001().join("claims.json")).unwrap()).unwrap();
        claims
            .claims
            .retain(|claim| claim.claim_id != "transport-result");
        let claims_bytes = serde_json::to_vec(&claims).unwrap();
        let campaign = evaluate_campaign(&contract, &registry, &claims_bytes).unwrap();
        let stages = build_presentation_gates(compiled, &claims, &campaign).unwrap();
        assert_eq!(
            stages[0].readiness,
            PresentationGateReadiness::AwaitingEvidence
        );
        assert_eq!(
            stages[0].missing_evidence,
            vec![SourceRef::StepOutput {
                step_id: "transport".into(),
                output_slot: "transport-result".into(),
            }]
        );
    }

    #[test]
    fn case_000_runs_without_external_roots_and_names_every_gap() {
        let report = execute_case(&case_000(), &CaseRunOptions::default()).unwrap();
        assert_eq!(report.status, CaseRunStatus::Evaluated);
        assert_eq!(report.integrity.status, PackageIntegrityStatus::Partial);
        assert!(
            report
                .integrity
                .documents
                .iter()
                .all(|check| { check.state == IntegrityCheckState::Verified })
        );
        assert!(
            report
                .integrity
                .artifacts
                .iter()
                .all(|check| { check.state == IntegrityCheckState::NotChecked })
        );
        let execution = report.execution.as_ref().unwrap();
        assert_eq!(execution.status, ExecutionStatus::NotRun);
        assert!(execution.workspace.is_none());
        let claims = report.claims.as_ref().unwrap();
        assert!(claims.matches_committed);
        let bindings = report.bindings.as_ref().unwrap();
        assert_eq!(bindings.status, BindingStatus::Verified);
        assert_eq!(bindings.bound_evidence_records, 20);
        assert_eq!(bindings.bound_presentation_policies, 0);
        assert_eq!(report.campaign.as_ref().unwrap().verdicts.len(), 2);
        assert!(report.replay.as_ref().unwrap().matches);

        let summary = human_summary(&report);
        assert!(summary.contains("activation [aftermatter.r0-inventory-build@1]"));
        assert!(summary.contains("[NOT RUN] activation"));
        assert!(summary.contains("[NOT RUN] classification"));
        assert!(summary.contains("CASE-000-R1 — bounded.lt.within"));
        assert!(summary.contains("source root(s) actinv-data, actinv-release, aftermatter"));
    }

    /// Executes CASE-000 for real when the bound executables and artifact
    /// roots are available locally. Set `AVILA_CORE_CASE_000_AFTERMATTER`
    /// (the Aftermatter executable), `AVILA_CORE_CASE_000_PYTHON3` (the
    /// interpreter that runs the R0 builder),
    /// `AVILA_CORE_CASE_000_AFTERMATTER_ROOT` (the Aftermatter checkout),
    /// `AVILA_CORE_CASE_000_ACTINV_DATA` (the data release), and
    /// `AVILA_CORE_CASE_000_ACTINV_RELEASE` (the directory holding the
    /// `actinv` and `dump` release builds) to run it; it is skipped, visibly,
    /// otherwise.
    #[test]
    fn case_000_executes_both_tools_when_available() {
        let (Ok(executable), Ok(python3), Ok(aftermatter), Ok(actinv_data), Ok(actinv_release)) = (
            std::env::var("AVILA_CORE_CASE_000_AFTERMATTER"),
            std::env::var("AVILA_CORE_CASE_000_PYTHON3"),
            std::env::var("AVILA_CORE_CASE_000_AFTERMATTER_ROOT"),
            std::env::var("AVILA_CORE_CASE_000_ACTINV_DATA"),
            std::env::var("AVILA_CORE_CASE_000_ACTINV_RELEASE"),
        ) else {
            eprintln!("skipped: AVILA_CORE_CASE_000_* not set; CASE-000 was not executed");
            return;
        };
        let workspace =
            std::env::temp_dir().join(format!("avila-core-case-000-{}", std::process::id()));
        let _ = fs::remove_dir_all(&workspace);
        let options = CaseRunOptions {
            source_roots: BTreeMap::from([
                ("aftermatter".to_string(), PathBuf::from(aftermatter)),
                ("actinv-data".to_string(), PathBuf::from(actinv_data)),
                ("actinv-release".to_string(), PathBuf::from(actinv_release)),
            ]),
            capabilities: BTreeMap::from([
                ("aftermatter-cli".to_string(), PathBuf::from(executable)),
                ("python3".to_string(), PathBuf::from(python3)),
            ]),
            workspace: Some(workspace.clone()),
            reuse: false,
            plan_only: false,
            inputs: BTreeMap::new(),
            environment: BTreeMap::new(),
            log: None,
            expected_manifest_sha256: None,
        };
        let report = execute_case(&case_000(), &options).unwrap();
        let summary = human_summary(&report);
        assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
        assert_eq!(report.integrity.status, PackageIntegrityStatus::Complete);
        let execution = report.execution.as_ref().unwrap();
        assert_eq!(execution.status, ExecutionStatus::Executed);
        assert_eq!(execution.steps.len(), 2);
        for step in &execution.steps {
            assert_eq!(step.state, StepExecutionState::Executed, "{summary}");
            assert!(step.changes.is_empty(), "{summary}");
            assert!(
                step.outputs
                    .iter()
                    .all(|output| output.reproduces_bound_artifact == Some(true))
            );
            assert!(step.replay.as_ref().unwrap().matches);
        }
        let claims = report.claims.as_ref().unwrap();
        assert!(claims.matches_committed);
        assert_eq!(claims.executed_claims, 6);
        assert_eq!(claims.recorded_claims, 0);
        assert!(report.replay.as_ref().unwrap().matches);
        assert!(summary.contains("[EXECUTED] activation via python3"));
        assert!(summary.contains("[EXECUTED] classification via aftermatter-cli"));
        let _ = fs::remove_dir_all(&workspace);

        // The same request again: both receipts match, nothing runs.
        let reuse = CaseRunOptions {
            reuse: true,
            workspace: Some(workspace.clone()),
            ..options
        };
        let report = execute_case(&case_000(), &reuse).unwrap();
        let summary = human_summary(&report);
        assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
        let execution = report.execution.as_ref().unwrap();
        assert_eq!(execution.status, ExecutionStatus::Reused);
        assert!(execution.workspace.is_none());
        assert_eq!(report.claims.as_ref().unwrap().reused_claims, 6);
        assert!(report.claims.as_ref().unwrap().matches_committed);
    }
}
