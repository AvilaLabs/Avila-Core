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

use avila_core_compiler::{
    AdmissionState, CampaignReport, CampaignStatus, ClaimsDocument, CompilationStatus,
    CompileReport, CompiledContract, CompiledStep, SourceRef, compile_documents, evaluate_campaign,
    render_campaign_report, render_compile_report,
};
use avila_core_evidence::{
    ArtifactCheck, CapabilityIdentity, CasePackageManifest, ExecutionReceipt, ExpectedInput,
    IntegrityCheckState, OutputState, PackageExecution, PackageIntegrityReport,
    PackageIntegrityStatus, ReceiptCheck, ReceiptCheckState, ReceiptExpectations, ReceiptInput,
    ReceiptOutput, ReceiptStatus, VerifiedCasePackage, parse_receipt, sha256_file,
    verify_case_package, verify_receipt,
};
use avila_core_kernel::VerdictStatus;
use serde::Serialize;
use serde_json::Value;

use crate::execute::claims::{GeneratedClaim, canonical_identity, generate_claims};
use crate::execute::{
    Adapter, ExecutionRequest, ExtractedClaim, PlannedInvocation, StagedInput, execute_step,
    plan_invocation, rfc3339_now,
};

const CASE_RUN_REPORT_SCHEMA_VERSION: &str = "avila.core/case-run-report/v0.2-draft";
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
}

impl Default for CaseRunOptions {
    fn default() -> Self {
        Self {
            source_roots: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            workspace: None,
            reuse: true,
            plan_only: false,
        }
    }
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
    pub required_review_policies: usize,
    pub bound_review_policies: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<ReceiptCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReceiptReplayReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
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
    pub decisions: usize,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseRunReport {
    pub schema_version: String,
    pub case_id: String,
    pub title: String,
    pub status: CaseRunStatus,
    pub integrity: PackageIntegrityReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile: Option<CompileReport>,
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

pub fn execute_case(
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
    let package = verify_case_package(&manifest_bytes, package_root, &options.source_roots)?;

    let mut report = CaseRunReport {
        schema_version: CASE_RUN_REPORT_SCHEMA_VERSION.into(),
        case_id: package.manifest.case_id.clone(),
        title: package.manifest.title.clone(),
        status: CaseRunStatus::Rejected,
        integrity: package.integrity.clone(),
        compile: None,
        rendered_findings: None,
        execution: None,
        claims: None,
        bindings: None,
        campaign: None,
        replay: None,
        notice: CASE_RUN_NOTICE.into(),
    };

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

    // Execute the steps the package declares. A refused or failed execution
    // stops the workflow: no claim is generated over an unverified run.
    let mut executed_claims = Vec::new();
    let mut workspace = None;
    if !package.manifest.executions.is_empty() {
        let mut runner = Runner::new(&package, compiled, options, &committed_claims);
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
            write_run_report(workspace.as_deref(), &report);
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
    )?;
    let committed_sha256 = canonical_identity(committed_claims_bytes)?;
    let claims_match = generated.canonical_sha256 == committed_sha256;
    report.claims = Some(ClaimsReport {
        generated_sha256: generated.canonical_sha256.clone(),
        committed_sha256,
        matches_committed: claims_match,
        input_attestations: generated.input_attestations,
        executed_claims: generated.executed_claims,
        reused_claims: generated.reused_claims,
        recorded_claims: generated.recorded_claims,
        decisions: generated.decisions,
    });
    if let Some(workspace) = workspace.as_deref() {
        let _ = fs::write(workspace.join("claims.json"), &generated.bytes);
    }

    let claims: ClaimsDocument = serde_json::from_slice(&generated.bytes)?;
    let bindings = verify_bindings(&package.manifest, &claims, compiled);
    let bindings_failed = bindings.status == BindingStatus::Failed;
    report.bindings = Some(bindings);
    if bindings_failed {
        write_run_report(workspace.as_deref(), &report);
        return Ok(report);
    }

    let campaign = evaluate_campaign(contract, registry, &generated.bytes)?;
    let campaign_rejected = campaign.status == CampaignStatus::Rejected;
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
    report.replay = replay_expected(&package, &campaign)?;
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
    if !campaign_rejected && !replay_failed && claims_match && !receipts_drifted {
        report.status = CaseRunStatus::Evaluated;
    }
    write_run_report(workspace.as_deref(), &report);
    Ok(report)
}

fn write_run_report(workspace: Option<&Path>, report: &CaseRunReport) {
    if let Some(workspace) = workspace
        && let Ok(mut bytes) = serde_json::to_vec_pretty(report)
    {
        bytes.push(b'\n');
        let _ = fs::write(workspace.join("run-report.json"), bytes);
    }
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
}

impl<'a> Runner<'a> {
    fn new(
        package: &'a VerifiedCasePackage,
        compiled: &'a CompiledContract,
        options: &'a CaseRunOptions,
        committed_claims: &'a Value,
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
        }
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
                    reason: if step.review_obligation.is_some() {
                        "pending external review; never executed by the runner".into()
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
                    receipt: None,
                    outputs: Vec::new(),
                    verification: None,
                    replay: None,
                    issues: vec![format!(
                        "step `{}` is declared for execution but the compiled workflow has no such step",
                        execution.step_id
                    )],
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
            receipt: None,
            outputs: Vec::new(),
            verification: None,
            replay: None,
            issues: Vec::new(),
        };

        let Some(declared) = self
            .package
            .manifest
            .capabilities
            .iter()
            .find(|capability| capability.capability_id == execution.capability_id)
        else {
            report.issues.push(format!(
                "capability `{}` is not declared",
                execution.capability_id
            ));
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
        let Some(adapter) = Adapter::by_id(&execution.adapter) else {
            report.issues.push(format!(
                "adapter `{}` is not known to this runner",
                execution.adapter
            ));
            return Ok(report);
        };
        let expected_type = adapter.capability_type();
        if step.capability_type.id != expected_type.id
            || step.capability_type.major != expected_type.major
        {
            report.issues.push(format!(
                "adapter `{}` implements `{}@{}`, but step `{}` compiles to `{}@{}`",
                execution.adapter,
                expected_type.id,
                expected_type.major,
                step.step_id,
                step.capability_type.id,
                step.capability_type.major
            ));
        }
        if step.review_obligation.is_some() {
            report
                .issues
                .push("a review obligation is never executed by the runner".into());
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
                report.issues.push(format!(
                    "package stages input slot `{slot}`, which the compiled step does not bind"
                ));
            }
        }
        let mut staged = Vec::new();
        let mut unverified = Vec::new();
        for binding in &step.bindings {
            let Some(workspace_path) = staging.get(binding.input_slot.as_str()) else {
                report.issues.push(format!(
                    "bound input slot `{}` has no staging path in the package",
                    binding.input_slot
                ));
                continue;
            };
            if !adapter.input_slots().contains(&binding.input_slot.as_str()) {
                report.issues.push(format!(
                    "adapter `{}` does not accept input slot `{}`",
                    execution.adapter, binding.input_slot
                ));
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
                Err(issue) => report
                    .issues
                    .push(format!("input slot `{}`: {issue}", binding.input_slot)),
            }
        }
        let declared_slots: BTreeSet<&str> = execution
            .outputs
            .iter()
            .map(|output| output.output_slot.as_str())
            .collect();
        let adapter_slots: BTreeSet<&str> = adapter.output_slots().iter().copied().collect();
        if declared_slots != adapter_slots {
            report.issues.push(format!(
                "package binds output slots {:?}, but adapter `{}` produces {:?}",
                declared_slots, execution.adapter, adapter_slots
            ));
        }
        if !report.issues.is_empty() {
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
                Some(_) => report.issues = unverified,
            }
            return Ok(report);
        }

        // Plan the invocation from the bound inputs, the parameters, and the
        // package's capability identity; the executable is not needed yet.
        let parameters: BTreeMap<String, Value> = step
            .parameters
            .iter()
            .map(|(id, value)| serde_json::to_value(value).map(|value| (id.clone(), value)))
            .collect::<Result<_, _>>()?;
        let executable = executable.map(|path| fs::canonicalize(&path).unwrap_or(path));
        let program = executable
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| declared.capability_id.clone());
        let plan = plan_invocation(adapter, &identity, &program, &parameters, &staged)?;
        report.planned_invocation_sha256 = Some(plan.invocation_sha256.clone());

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
                    let extracted = match adapter.extract_claims(&output_bytes, &parameters) {
                        Ok(extracted) => extracted,
                        Err(issue) => {
                            report
                                .issues
                                .push(format!("claim extraction over reused outputs: {issue}"));
                            report.state = StepExecutionState::Failed;
                            return Ok(report);
                        }
                    };
                    let extracted_slots: BTreeSet<&str> = extracted
                        .iter()
                        .map(|claim| claim.output_slot.as_str())
                        .collect();
                    if extracted_slots != adapter_slots {
                        report.issues.push(format!(
                            "adapter extracted claims for {:?}, but declares {:?}",
                            extracted_slots, adapter_slots
                        ));
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
            CapabilityCheckState::Mismatch => report.issues.push(format!(
                "executable `{}` hashes to {}, but the package binds {}",
                executable.display(),
                capability_check.actual_sha256.as_deref().unwrap_or("?"),
                declared.executable_sha256
            )),
            _ => report.issues.push(format!(
                "executable `{}` cannot be read",
                executable.display()
            )),
        }
        report.capability = Some(capability_check);
        if !report.issues.is_empty() {
            return Ok(report);
        }

        // Run.
        let workspace = self.workspace_dir()?;
        let step_dir = workspace.join(&step.step_id);
        let request = ExecutionRequest {
            case_id: self.package.manifest.case_id.clone(),
            compiled_snapshot_sha256: self.compiled.snapshot_sha256.clone(),
            step_id: step.step_id.clone(),
            adapter,
            capability: identity.clone(),
            executable: executable.clone(),
            parameters: parameters.clone(),
            inputs: staged.clone(),
        };
        let outcome = match execute_step(&step_dir, &request) {
            Ok(outcome) => outcome,
            Err(error) => {
                report
                    .issues
                    .push(format!("execution could not be completed: {error}"));
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
            report.issues.push(format!(
                "the receipt records invocation {} but the runner planned {}",
                receipt.invocation_sha256, plan.invocation_sha256
            ));
        }
        let expectations = ReceiptExpectations {
            case_id: self.package.manifest.case_id.clone(),
            compiled_snapshot_sha256: self.compiled.snapshot_sha256.clone(),
            step_id: step.step_id.clone(),
            capability_type: expected_type,
            adapter: adapter.id().into(),
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
            outputs: adapter
                .outputs()
                .iter()
                .map(|output| output.output_id.to_string())
                .collect(),
        };
        let verification = verify_receipt(&receipt, &outcome.step_dir, &expectations)?;
        let verified = verification.state == ReceiptCheckState::Verified;
        report.issues.extend(verification.issues.iter().cloned());
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
            match adapter.extract_claims(&output_bytes, &parameters) {
                Ok(claims) => extracted = claims,
                Err(issue) => report.issues.push(format!("claim extraction: {issue}")),
            }
            let extracted_slots: BTreeSet<&str> = extracted
                .iter()
                .map(|claim| claim.output_slot.as_str())
                .collect();
            if !extracted.is_empty() && extracted_slots != adapter_slots {
                report.issues.push(format!(
                    "adapter extracted claims for {:?}, but declares {:?}",
                    extracted_slots, adapter_slots
                ));
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
                (Some(sha256), 1) => Some(bound_identities.contains(sha256.as_str())),
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

        if !verified || !report.issues.is_empty() {
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
            let path = self
                .canonical_roots
                .get(&check.source_root)
                .map(|root| root.join(&check.path))
                .ok_or_else(|| format!("root `{}` is not resolved", check.source_root))?;
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
        let path = self
            .canonical_roots
            .get(&check.source_root)
            .map(|root| root.join(&check.path))
            .unwrap_or_default();
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
    for claim in &claims.claims {
        insert_evidence(
            &mut expected,
            claim.claim_id.clone(),
            claim.artifact.sha256.clone(),
            &mut issues,
        );
    }

    let mut seen = BTreeSet::new();
    let mut bound_evidence_records = 0;
    for artifact in &manifest.artifacts {
        for evidence_id in &artifact.evidence_ids {
            let Some(expected_sha256) = expected.get(evidence_id) else {
                issues.push(format!(
                    "artifact `{}` binds unknown evidence record `{evidence_id}`",
                    artifact.artifact_id
                ));
                continue;
            };
            seen.insert(evidence_id.clone());
            if expected_sha256 == &artifact.sha256 {
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
        .filter_map(|step| step.review_obligation.as_ref())
        .map(|review| review.reviewer_eligibility_policy.sha256.clone())
        .collect();
    let package_policies: BTreeSet<String> = manifest
        .documents
        .iter()
        .filter(|document| document.role == "review_policy")
        .map(|document| document.sha256.clone())
        .collect();
    let bound_review_policies = required_policies.intersection(&package_policies).count();
    for digest in required_policies.difference(&package_policies) {
        issues.push(format!(
            "compiled review obligation requires policy `{digest}`, but the package does not contain it"
        ));
    }
    for digest in package_policies.difference(&required_policies) {
        issues.push(format!(
            "package review policy `{digest}` is not referenced by the compiled contract"
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
        required_review_policies: required_policies.len(),
        bound_review_policies,
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
                        let _ = writeln!(
                            out,
                            "   [NOT RUN] {} — capability `{}` not supplied; its committed claims are evaluated as recorded attestations",
                            step.step_id, step.capability_id
                        );
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
                for issue in &step.issues {
                    let _ = writeln!(out, "      issue: {issue}");
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
                "   [GENERATED] {} input attestations from package identities; {} claims from executed outputs; {} from reused outputs; {} recorded claims carried; {} decisions",
                claims.input_attestations,
                claims.executed_claims,
                claims.reused_claims,
                claims.recorded_claims,
                claims.decisions
            );
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
            match bindings {
                Some(bindings) => {
                    let _ = writeln!(
                        out,
                        "   [{}] {}/{} evidence identities bound; {}/{} review-policy identities",
                        binding_label(bindings.status),
                        bindings.bound_evidence_records,
                        bindings.evidence_records,
                        bindings.bound_review_policies,
                        bindings.required_review_policies
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
                let _ = writeln!(
                    out,
                    "   [{}] {} — {}",
                    verdict_label(verdict.verdict.status),
                    verdict.requirement_id,
                    verdict.verdict.rule
                );
            }
            if let Some(identity) = &campaign.campaign_sha256 {
                let _ = writeln!(out, "   campaign {identity}");
            }
        }
        None => {
            let _ = writeln!(out, "   [NOT RUN]");
        }
    }

    if let Some(replay) = &report.replay {
        let _ = writeln!(out, "\n6. REPLAY");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn case_000() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-000-actinv-aftermatter")
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
        assert_eq!(bindings.bound_review_policies, 1);
        assert_eq!(report.campaign.as_ref().unwrap().verdicts.len(), 2);
        assert!(report.replay.as_ref().unwrap().matches);

        let summary = human_summary(&report);
        assert!(summary.contains("activation [aftermatter.r0-inventory-build@1]"));
        assert!(summary.contains("[NOT RUN] activation"));
        assert!(summary.contains("[NOT RUN] classification"));
        assert!(summary.contains("CASE-000-R1 — not_evaluated.review_pending"));
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
