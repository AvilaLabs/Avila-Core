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
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::{
    CampaignReport, CampaignStatus, ClaimQualification, ClaimsDocument, CompilationStatus,
    CompileReport, CompiledContract, CompiledStep, CoverageDeclaration, CoverageReport,
    CoverageStatus, DeclaredOmission, EnvelopeAssessment, FindingClass, ImmutablePolicyRef,
    PresentationGateState, ReviewDisposition, ReviewIndependence, ReviewerRole, SourceLocation,
    SourceRef, assess_coverage, compile_documents, evaluate_campaign, evaluate_envelope,
    parse_requirement_set, registry_kinds, render_campaign_report, render_compile_report,
};
use avila_core_evidence::signature::{self, TrustRoot};
use avila_core_evidence::{
    ArtifactCheck, CapabilityIdentity, ExecutionReceipt, ExpectedInput, HashCache,
    HashCacheContext, IntegrityCheckState, OutputState, PackageExecution, PackageIntegrityReport,
    PackageIntegrityStatus, ReceiptCheck, ReceiptCheckState, ReceiptExpectations, ReceiptOutput,
    ReceiptStatus, VerifiedCasePackage, load_hash_cache, parse_receipt, save_hash_cache,
    sha256_file, verify_case_package, verify_receipt,
};
use serde::Serialize;
use serde_json::Value;

#[cfg(test)]
use crate::attempt::{
    ATTEMPT_COMPARISON_SCHEMA_VERSION, AttemptMarginUnavailableReason, AttemptVerdictTransition,
};
use crate::attempt::{AttemptComparison, AttemptLineageRequest, AttemptRecord, prepare_attempt};
use crate::diagnostic::{
    CORE_X1001, CORE_X1002, CORE_X1003, CORE_X1004, CORE_X1005, CORE_X1101, CORE_X1201, CORE_X1301,
    CORE_X2001, CORE_X2101, CORE_X2201, CORE_X2301, CORE_X2401, CORE_X2402, CORE_X2501, CORE_X2601,
    CORE_X2701, CORE_X2801, CORE_X3001, CORE_X3101, CORE_X3201, CORE_X3301, CORE_X9001, RunFinding,
    RunStage,
};
use crate::execute::claims::{GeneratedClaim, canonical_identity, generate_claims};
use crate::execute::external_checker::{EXTERNAL_CHECKER_DOCUMENT_ROLE, ExternalCheckerAdapter};
use crate::execute::{
    Adapter, ExecutionRequest, ExtractedClaim, StagedInput, StepContext, execute_step,
    plan_invocation, rfc3339_now,
};
#[cfg(test)]
use avila_core_kernel::VerdictStatus;

mod log;
#[cfg(test)]
use log::append_log_line;
use log::{append_error_log, append_log};
mod qualification;
#[allow(unused_imports)]
use qualification::BoundQualification;
use qualification::{Envelopes, load_qualifications};
mod signing;
pub use signing::SignatureStatus;
mod inputs;
use inputs::{steps_reached_by_inputs, supply_free_inputs, validate_free_inputs};
mod compare;
pub use compare::VerdictMargin;
pub(crate) use compare::compare_attempt_results;
#[cfg(test)]
use compare::write_attempt_comparison;
use compare::{compare_attempt_to_parent, margins};
mod gates;
use gates::build_presentation_gates;
mod replay;
use replay::{replay_expected, verify_bindings};
mod runner;
use runner::Runner;
mod report;
mod stderr;
#[cfg(test)]
use report::display_signed_number;
pub use report::{display_number, human_summary};

const CASE_RUN_REPORT_SCHEMA_VERSION: &str = "avila.core/case-run-report/v0.5-draft";
const RUN_ATTEMPT_LOG_SCHEMA_VERSION: &str = "avila.core/run-attempt/v0.3-draft";
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
    /// Optional identity-bound placement of this run in a candidate lineage.
    pub attempt: Option<AttemptLineageRequest>,
    /// An operator-owned JSON file caching verified digests of large,
    /// unchanging artifacts resolved under a `--source-root`, keyed by exact
    /// path, size, and modification time (S-038). Off unless supplied.
    /// Package documents and anything resolved from inside the case
    /// directory are always re-hashed regardless of this setting. See
    /// `avila_core_evidence::hash_cache` for the trust this accepts.
    pub hash_cache: Option<PathBuf>,
    /// The requester and runner public keys this run accepts (ADR-0015).
    /// With it, the manifest signature must verify against a listed
    /// requester key or the run is refused before compilation, and a
    /// committed receipt is reused under SC-12 only when its signature
    /// verifies against a listed runner key. Without it, every signature is
    /// checked for internal consistency only and reported `unsigned` or
    /// `signature not checked`, never `verified`.
    pub trust_root: Option<PathBuf>,
    /// A runner seed key (32 raw bytes). When supplied, a freshly executed
    /// step's receipt is signed and the signature is written next to it in
    /// the workspace; campaign log lines this run appends are signed the
    /// same way.
    pub runner_key: Option<PathBuf>,
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
            attempt: None,
            hash_cache: None,
            trust_root: None,
            runner_key: None,
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
    /// A trust root was supplied and the committed receipt carries no
    /// signature document verified against a listed runner key at all.
    ReceiptUnsigned,
    /// A trust root was supplied and the committed receipt's signature
    /// document fails internal consistency, names an unlisted or
    /// wrong-role key, or does not cryptographically verify.
    ReceiptSignatureInvalid,
    /// The committed receipt's `case_id` or `compiled_snapshot_sha256`
    /// differs from this run's, even though nothing else about the planned
    /// invocation changed: it was produced for a different case (a "donor"
    /// receipt copied from elsewhere) and does not describe this one.
    DifferentCase,
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
    /// The signature status of this step's operative receipt: the committed
    /// one when reused or left `not_run`, or the one this run just produced
    /// when executed fresh. Absent when no receipt exists to sign at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_signature: Option<SignatureStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputReport>,
    /// Optional adapter claims whose pointers resolved to no value this run:
    /// their evidence is legitimately absent, so dependent requirements see
    /// missing evidence rather than an adapter defect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub absent_slots: Vec<String>,
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
    /// This run's identity-bound place in a candidate search, when requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<AttemptRecord>,
    /// Core's comparison of this child with the exact parent log record its
    /// attempt binds. Roots and refused attempt requests have no comparison.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_comparison: Option<AttemptComparison>,
    /// A stage-ordered, actionable view of every finding that prevented or
    /// qualified progress. Nested reports remain available as the detailed
    /// evidence; agents need only this collection to drive the next attempt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<RunFinding>,
    pub integrity: PackageIntegrityReport,
    /// The package manifest's requester-signature status (ADR-0015). Present
    /// as soon as package integrity is checked, whether or not
    /// `--trust-root` was supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_signature: Option<SignatureStatus>,
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

/// Load `--trust-root FILE`, if supplied. A missing, unreadable, or invalid
/// trust root is a hard error: unlike the hash cache, degrading it to
/// "unchecked" would weaken exactly the check the operator asked for, so
/// this never falls back silently.
fn load_trust_root(options: &CaseRunOptions) -> Result<Option<TrustRoot>, Box<dyn Error>> {
    let Some(path) = &options.trust_root else {
        return Ok(None);
    };
    let bytes =
        fs::read(path).map_err(|error| format!("trust root `{}`: {error}", path.display()))?;
    let trust_root = TrustRoot::parse(&bytes)
        .map_err(|error| format!("trust root `{}`: {error}", path.display()))?;
    Ok(Some(trust_root))
}

/// Load `--runner-key FILE`, if supplied: a raw 32-byte seed. Never printed
/// or logged; held only in memory for the life of this run.
fn load_runner_key(options: &CaseRunOptions) -> Result<Option<[u8; 32]>, Box<dyn Error>> {
    let Some(path) = &options.runner_key else {
        return Ok(None);
    };
    let bytes =
        fs::read(path).map_err(|error| format!("runner key `{}`: {error}", path.display()))?;
    let seed = signature::parse_seed_bytes(&bytes)
        .map_err(|error| format!("runner key `{}`: {error}", path.display()))?;
    Ok(Some(seed))
}

pub fn execute_case(
    case_or_manifest: &Path,
    options: &CaseRunOptions,
) -> Result<CaseRunReport, Box<dyn Error>> {
    let trust_root = load_trust_root(options)?;
    let runner_key = load_runner_key(options)?;
    match execute_case_inner(case_or_manifest, options, trust_root.as_ref(), runner_key) {
        Ok(mut report) => {
            collect_run_findings(&mut report);
            report.attempt_comparison = compare_attempt_to_parent(options, &report)?;
            let workspace = report
                .execution
                .as_ref()
                .and_then(|execution| execution.workspace.as_deref())
                .map(Path::new);
            write_run_report(workspace, &report)?;
            append_log(
                options,
                case_or_manifest,
                &report,
                trust_root.as_ref(),
                runner_key,
            )?;
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
            if let Err(log_error) = append_error_log(
                options,
                case_or_manifest,
                &finding,
                trust_root.as_ref(),
                runner_key,
            ) {
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
    trust_root: Option<&TrustRoot>,
    runner_key: Option<[u8; 32]>,
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

    // The hash cache is opt-in and off by default (S-038). A corrupt or
    // unwritable cache file never fails the run: it is ignored, a notice
    // finding says so, and every artifact is hashed fresh as if no cache had
    // been supplied.
    let (mut hash_cache, cache_load_finding) = match &options.hash_cache {
        Some(path) => match load_hash_cache(path) {
            Ok(cache) => (Some(cache), None),
            Err(error) => (
                Some(HashCache::new()),
                Some(RunFinding::runtime(
                    CORE_X1003,
                    FindingClass::Notice,
                    RunStage::PackageIntegrity,
                    "operator",
                    SourceLocation::new(path.display().to_string(), ""),
                    format!(
                        "hash cache ignored, every artifact under a source root is hashed fresh this run: {error}"
                    ),
                )),
            ),
        },
        None => (None, None),
    };
    let hash_cache_verified_at = rfc3339_now();
    let mut package = verify_case_package(
        &manifest_bytes,
        package_root,
        &options.source_roots,
        hash_cache.as_mut().map(|cache| HashCacheContext {
            cache,
            verified_at: &hash_cache_verified_at,
        }),
    )?;
    let cache_save_finding = match (&options.hash_cache, &hash_cache) {
        (Some(path), Some(cache)) => save_hash_cache(path, cache).err().map(|error| {
            RunFinding::runtime(
                CORE_X1003,
                FindingClass::Notice,
                RunStage::PackageIntegrity,
                "operator",
                SourceLocation::new(path.display().to_string(), ""),
                format!(
                    "freshly hashed digests could not be written back to the hash cache: {error}"
                ),
            )
        }),
        _ => None,
    };
    let supplied_inputs = supply_free_inputs(&mut package, options)?;
    let replay_applicable = supplied_inputs.is_empty();

    let mut report = CaseRunReport {
        schema_version: CASE_RUN_REPORT_SCHEMA_VERSION.into(),
        case_id: package.manifest.case_id.clone(),
        title: package.manifest.title.clone(),
        status: CaseRunStatus::Rejected,
        attempt: None,
        attempt_comparison: None,
        findings: Vec::new(),
        integrity: package.integrity.clone(),
        manifest_signature: None,
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
    report.findings.extend(cache_load_finding);
    report.findings.extend(cache_save_finding);
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

    // The manifest's requester signature (ADR-0015). Reported whether or not
    // a trust root was supplied; refused before anything is compiled only
    // when a trust root was supplied and it did not verify against a listed
    // requester key, exactly where the requester's manifest pin refuses
    // today.
    let manifest_signature_status =
        signing::manifest_signature_status(&package, &manifest_bytes, trust_root);
    report.manifest_signature = Some(manifest_signature_status.clone());
    if trust_root.is_some() && !manifest_signature_status.is_verified() {
        report.notice = format!(
            "package manifest signature is not verified against a listed requester key: {}",
            manifest_signature_status.describe()
        );
        report.findings.push(RunFinding::runtime(
            CORE_X1004,
            FindingClass::Inadmissible,
            RunStage::PackageIntegrity,
            "requester",
            SourceLocation::new("case-package", "/documents"),
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

    // A contract that requires signed execution cannot be run at all without
    // a trust root to check against: falling back to the default "reused or
    // rerun, visibly" behavior would defeat the policy silently.
    if compiled.execution_policy.require_signatures && trust_root.is_none() {
        report.notice = "the contract's execution policy sets require_signatures, but no --trust-root was supplied to verify against".into();
        report.findings.push(RunFinding::runtime(
            CORE_X1005,
            FindingClass::Inadmissible,
            RunStage::Compilation,
            "requester_or_operator",
            SourceLocation::new("contract", "/execution_policy/require_signatures"),
            report.notice.clone(),
        ));
        report.compile = Some(compile.clone());
        return Ok(report);
    }

    // Every supplied free input is validated against its role's declared
    // schema before anything is staged or executed. A role with no declared
    // schema is unaffected: this is a structural check, not a general
    // input-format contract, and material vocabulary stays the capability's
    // job.
    let free_input_findings = validate_free_inputs(compiled, registry, &supplied_inputs)?;
    if !free_input_findings.is_empty() {
        report.notice = "a supplied free input violates its role's declared schema; the run is refused before anything is staged or executed".into();
        report.findings.extend(free_input_findings);
        report.compile = Some(compile.clone());
        return Ok(report);
    }

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

    report.compile = Some(compile.clone());
    if let Some(request) = &options.attempt {
        let candidate_path = options
            .inputs
            .get(&request.candidate_input)
            .map(PathBuf::as_path);
        let supplied_candidate_sha256 = supplied_inputs
            .iter()
            .find(|input| input.input_id == request.candidate_input)
            .map(|input| input.sha256.as_str());
        match prepare_attempt(
            request,
            options.log.as_deref(),
            candidate_path,
            supplied_candidate_sha256,
            &report.integrity.manifest_sha256,
            &compiled.snapshot_sha256,
            trust_root,
        ) {
            Ok(attempt) => report.attempt = Some(attempt),
            Err(issue) => {
                report.notice =
                    format!("attempt lineage was refused before capability execution: {issue}");
                let lineage_document = options
                    .log
                    .as_ref()
                    .map_or_else(|| "campaign-log".into(), |path| path.display().to_string());
                report.findings.push(RunFinding::runtime(
                    CORE_X1201,
                    FindingClass::Inadmissible,
                    RunStage::AttemptPlanning,
                    "designer_or_log_custodian",
                    SourceLocation::new(lineage_document, "/attempt"),
                    issue,
                ));
                return Ok(report);
            }
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
            trust_root,
            runner_key,
        );
        let execution = runner.run_all()?;
        executed_claims = runner.claims;
        workspace = runner.workspace;

        // execution_policy.require_signatures makes an unsigned outcome a
        // refusal rather than the default's visible fallback to a rerun or
        // `not_run`. By this point require_signatures already implies
        // trust_root is Some (refused earlier otherwise), so every status
        // here is Unsigned, Invalid, or Verified, never NotChecked.
        if compiled.execution_policy.require_signatures && !options.plan_only {
            for step in &execution.steps {
                if !matches!(
                    step.state,
                    StepExecutionState::Reused
                        | StepExecutionState::Executed
                        | StepExecutionState::NotRun
                ) {
                    continue;
                }
                let signed = step
                    .receipt_signature
                    .as_ref()
                    .is_some_and(SignatureStatus::is_verified);
                if !signed {
                    report.findings.push(
                        RunFinding::runtime(
                            CORE_X1005,
                            FindingClass::Inadmissible,
                            RunStage::ReceiptVerification,
                            "requester_or_operator",
                            SourceLocation::new("execution-receipt", ""),
                            format!(
                                "step `{}` evidence is {}, and the contract's execution policy requires signed execution",
                                step.step_id,
                                step.receipt_signature
                                    .as_ref()
                                    .map_or_else(|| "unsigned".to_string(), SignatureStatus::describe)
                            ),
                        )
                        .for_step(&step.step_id),
                    );
                }
            }
        }
        let signature_policy_violation = compiled.execution_policy.require_signatures
            && !options.plan_only
            && report
                .findings
                .iter()
                .any(|finding| finding.code == CORE_X1005);
        let stop = options.plan_only
            || matches!(
                execution.status,
                ExecutionStatus::Refused | ExecutionStatus::Failed | ExecutionStatus::Planned
            )
            || signature_policy_violation;
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

/// Build the single feedback stream consumed by people and iterating agents.
/// Detailed stage reports remain authoritative; this is a lossless-enough,
/// actionable index over the blockers and drift they contain.
fn collect_run_findings(report: &mut CaseRunReport) {
    for document in &report.integrity.documents {
        let (class, state) = match document.state {
            IntegrityCheckState::Missing => (FindingClass::Missing, "missing"),
            IntegrityCheckState::Mismatch => (FindingClass::Inadmissible, "does not match"),
            IntegrityCheckState::Verified
            | IntegrityCheckState::VerifiedCached
            | IntegrityCheckState::NotChecked => continue,
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
            IntegrityCheckState::Verified
            | IntegrityCheckState::VerifiedCached
            | IntegrityCheckState::NotChecked => continue,
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

#[cfg(test)]
mod tests;
