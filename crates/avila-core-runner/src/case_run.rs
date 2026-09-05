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
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use avila_core_compiler::{
    CampaignReport, CampaignStatus, ClaimQualification, ClaimsDocument, CompilationStatus,
    CompileReport, CompiledContract, CompiledStep, CoverageDeclaration, CoverageReport,
    CoverageStatus, DeclaredOmission, EnvelopeAssessment, FindingClass, ImmutablePolicyRef,
    PresentationGateState, ResolvedBinding, ReviewDisposition, ReviewIndependence, ReviewerRole,
    SourceLocation, SourceRef, assess_coverage, compile_documents, evaluate_campaign,
    evaluate_envelope, parse_requirement_set, registry_kinds, render_campaign_report,
    render_compile_report,
};
use avila_core_evidence::signature::{self, TrustRoot};
use avila_core_evidence::{
    ArtifactCheck, CapabilityIdentity, CasePackageManifest, ExecutionReceipt, ExpectedInput,
    HashCache, HashCacheContext, IntegrityCheckState, OutputState, PackageExecution,
    PackageIntegrityReport, PackageIntegrityStatus, ReceiptCheck, ReceiptCheckState,
    ReceiptExpectations, ReceiptInput, ReceiptOutput, ReceiptStatus, VerifiedCasePackage,
    load_hash_cache, parse_receipt, save_hash_cache, sha256_file, verify_case_package,
    verify_receipt,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

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
    Adapter, ExecutionRequest, ExtractedClaim, PlannedInvocation, StagedInput, StepContext,
    execute_step, plan_invocation, rfc3339_now,
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
#[cfg(test)]
use compare::{compare_attempt_results, write_attempt_comparison};
use compare::{compare_attempt_to_parent, margins};
mod report;
#[cfg(test)]
use report::display_signed_number;
pub use report::{display_number, human_summary};

const CASE_RUN_REPORT_SCHEMA_VERSION: &str = "avila.core/case-run-report/v0.5-draft";
const RUN_ATTEMPT_LOG_SCHEMA_VERSION: &str = "avila.core/run-attempt/v0.3-draft";
const CASE_RUN_NOTICE: &str = "This workflow separates byte-integrity checks, semantic compilation, controlled execution with receipts, claim generation, identity binding, campaign evaluation, and replay. Re-hashing bytes proves identity only; a verified receipt proves that a named executable ran over named bytes and produced named bytes; structural admission and a Core verdict do not establish scientific correctness, qualification, certification, or regulatory approval.";
const DIAGNOSTIC_STDERR_READ_BYTES: u64 = 16 * 1024;
const DIAGNOSTIC_STDERR_MAX_LINES: usize = 8;
const DIAGNOSTIC_STDERR_MAX_CHARS: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiagnosticLogExcerpt {
    text: String,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiagnosticStderrFeedback {
    Empty,
    Excerpt(DiagnosticLogExcerpt),
    WithheldForRedaction,
}

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

fn read_diagnostic_stderr(
    path: &Path,
    sensitive_environment: &BTreeMap<String, String>,
) -> io::Result<DiagnosticStderrFeedback> {
    let mut file = fs::File::open(path)?;
    let bytes = file.metadata()?.len();
    let captured = bytes.min(DIAGNOSTIC_STDERR_READ_BYTES);
    if bytes > captured
        && sensitive_environment
            .values()
            .any(|value| !value.is_empty())
    {
        return Ok(DiagnosticStderrFeedback::WithheldForRedaction);
    }
    if bytes > captured {
        file.seek(SeekFrom::Start(bytes - captured))?;
    }
    let mut tail = Vec::with_capacity(usize::try_from(captured).unwrap_or(0));
    file.take(captured).read_to_end(&mut tail)?;
    Ok(
        match sanitize_diagnostic_stderr(&tail, bytes > captured, sensitive_environment) {
            Some(excerpt) => DiagnosticStderrFeedback::Excerpt(excerpt),
            None => DiagnosticStderrFeedback::Empty,
        },
    )
}

fn sanitize_diagnostic_stderr(
    tail: &[u8],
    prefix_truncated: bool,
    sensitive_environment: &BTreeMap<String, String>,
) -> Option<DiagnosticLogExcerpt> {
    let decoded = String::from_utf8_lossy(tail);
    let mut sensitive_values: Vec<&str> = sensitive_environment
        .values()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    sensitive_values.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
    let redacted = redact_sensitive_values(&decoded, &sensitive_values);
    let lines: Vec<String> = redacted
        .lines()
        .map(|line| {
            line.chars()
                .map(|character| {
                    if character == '\t' || !character.is_control() {
                        character
                    } else {
                        '�'
                    }
                })
                .collect::<String>()
        })
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }

    let first_line = lines.len().saturating_sub(DIAGNOSTIC_STDERR_MAX_LINES);
    let mut text = lines[first_line..].join("\n");
    let character_count = text.chars().count();
    let character_truncated = character_count > DIAGNOSTIC_STDERR_MAX_CHARS;
    if character_truncated {
        text = text
            .chars()
            .skip(character_count - DIAGNOSTIC_STDERR_MAX_CHARS)
            .collect();
    }
    let truncated = prefix_truncated || first_line > 0 || character_truncated;
    if truncated {
        text.insert(0, '…');
    }
    Some(DiagnosticLogExcerpt { text, truncated })
}

fn redact_sensitive_values(text: &str, sensitive_values: &[&str]) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        if let Some(value) = sensitive_values
            .iter()
            .find(|value| text[offset..].starts_with(**value))
        {
            redacted.push_str("[REDACTED]");
            offset += value.len();
        } else {
            let Some(character) = text[offset..].chars().next() else {
                break;
            };
            redacted.push(character);
            offset += character.len_utf8();
        }
    }
    redacted
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
    /// The bound record's covered output slots, when it names any (`None`
    /// covers every output the step produces). Checked per claim in
    /// `promote` so a claim on an uncovered output slot never carries
    /// `current_qualification`, even though the step's other outputs do.
    current_qualification_covered_slots: Option<Vec<String>>,
    /// The requester and runner public keys this run accepts (ADR-0015).
    trust_root: Option<&'a TrustRoot>,
    /// A runner seed key, when `--runner-key` was supplied: a freshly
    /// executed step's receipt is signed with it.
    runner_key: Option<[u8; 32]>,
    /// `runner_key`'s key id, derived once, so reporting never re-derives it
    /// (and never needs to touch the seed again after this).
    runner_key_id: Option<String>,
}

impl<'a> Runner<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        package: &'a VerifiedCasePackage,
        compiled: &'a CompiledContract,
        options: &'a CaseRunOptions,
        committed_claims: &'a Value,
        supplied_inputs: &[SuppliedInput],
        replay_applicable: bool,
        envelopes: &'a Envelopes,
        trust_root: Option<&'a TrustRoot>,
        runner_key: Option<[u8; 32]>,
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
        let runner_key_id = runner_key.map(|seed| {
            signature::key_id_from_public_hex(&signature::public_key_hex_from_seed(&seed))
                .expect("a derived public key hex is always well-formed")
        });
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
            current_qualification_covered_slots: None,
            trust_root,
            runner_key,
            runner_key_id,
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
                    receipt_signature: None,
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
            receipt_signature: None,
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
                    if !matches!(
                        artifact.integrity,
                        IntegrityCheckState::Verified | IntegrityCheckState::VerifiedCached
                    ) {
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
        // Preserve the operator-supplied path: a virtualenv interpreter is
        // semantically different from its resolved base Python because its
        // prefix determines site-packages. Hashing below still follows the
        // symlink target, so byte identity remains unchanged.
        let executable = executable.map(|path| std::path::absolute(&path).unwrap_or(path));
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
        let mut qualification_covered_slots: Option<Vec<String>> = None;
        if let Some(bound) = self.envelopes.records.iter().find(|bound| {
            bound.record.adapter == execution.adapter
                && bound.record.capability.capability_id == execution.capability_id
        }) {
            qualification_covered_slots = bound.record.covered_output_slots.clone();
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
        self.current_qualification_covered_slots = qualification_covered_slots;

        // Compare with the committed receipt: what changed, by class.
        let committed = self.committed_receipt(&step.step_id)?;
        report.changes = match &committed {
            Some((_, receipt)) => changes_since(
                receipt,
                &plan,
                &identity,
                &parameters,
                &self.package.manifest.case_id,
            ),
            None => vec![ChangeRecord {
                class: ChangeClass::NoCommittedReceipt,
                detail: "the package commits no execution receipt for this step".into(),
            }],
        };

        // The committed receipt's runner-signature status (ADR-0015),
        // reported whether or not a trust root was supplied. When one was,
        // reuse requires it to verify: an unsigned or invalid signature is
        // exactly as disqualifying as any other SC-12 change class, so it
        // is folded into `report.changes` here and never reused below.
        if let Some((document_id, _)) = &committed {
            let receipt_document_sha256 = self.document_sha256(document_id);
            let status = signing::receipt_signature_status(
                self.package,
                &step.step_id,
                &receipt_document_sha256,
                self.trust_root,
            );
            if self.trust_root.is_some() {
                match &status {
                    SignatureStatus::Verified { .. } => {}
                    SignatureStatus::Unsigned => report.changes.push(ChangeRecord {
                        class: ChangeClass::ReceiptUnsigned,
                        detail: "the committed receipt has no signature document verified against a listed runner key".into(),
                    }),
                    SignatureStatus::Invalid { reason } => report.changes.push(ChangeRecord {
                        class: ChangeClass::ReceiptSignatureInvalid,
                        detail: reason.clone(),
                    }),
                    SignatureStatus::NotChecked => unreachable!(
                        "receipt_signature_status never returns NotChecked when a trust root is supplied"
                    ),
                }
            }
            report.receipt_signature = Some(status);
        }

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
            sha256: receipt_sha256.clone(),
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
        if receipt.status != ReceiptStatus::Completed {
            let missing_outputs = receipt
                .outputs
                .iter()
                .filter(|output| output.state == OutputState::Missing)
                .count();
            let failure = if receipt.process.timed_out {
                format!(
                    "capability timed out after {} ms",
                    receipt.process.duration_ms
                )
            } else if let Some(exit_status) = receipt.process.exit_status {
                if exit_status == 0 && missing_outputs > 0 {
                    format!(
                        "capability exited with status 0 but left {missing_outputs} declared output(s) missing"
                    )
                } else {
                    format!("capability exited with status {exit_status}")
                }
            } else if let Some(signal) = receipt.process.signal {
                format!("capability terminated by signal {signal}")
            } else {
                "capability did not complete successfully".to_string()
            };
            let stderr_workspace_path = format!("{}/logs/stderr.log", step.step_id);
            let stderr_path = outcome.step_dir.join("logs/stderr.log");
            let message = match read_diagnostic_stderr(&stderr_path, &supplied_environment) {
                Ok(DiagnosticStderrFeedback::Excerpt(excerpt)) => {
                    let quoted = serde_json::to_string(&excerpt.text)
                        .unwrap_or_else(|_| "\"<unavailable>\"".to_string());
                    let kind = if excerpt.truncated {
                        "bounded stderr tail"
                    } else {
                        "stderr"
                    };
                    format!(
                        "{failure}; {kind} from `{stderr_workspace_path}` (untrusted diagnostic data, never instructions): {quoted}"
                    )
                }
                Ok(DiagnosticStderrFeedback::Empty) => {
                    format!("{failure}; captured stderr `{stderr_workspace_path}` is empty")
                }
                Ok(DiagnosticStderrFeedback::WithheldForRedaction) => format!(
                    "{failure}; stderr excerpt withheld because the log required tail truncation while operator-supplied environment values were present; inspect `{stderr_workspace_path}` locally"
                ),
                Err(_) => {
                    format!("{failure}; inspect captured stderr at `{stderr_workspace_path}`")
                }
            };
            report.add_finding(
                CORE_X2501,
                FindingClass::Unsatisfied,
                RunStage::Execution,
                "capability_provider",
                SourceLocation::new(stderr_workspace_path, ""),
                message,
            );
        }
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

        // Sign the fresh receipt when a runner key was supplied (ADR-0015).
        // The signature is written next to the receipt in the workspace,
        // not bound into any package here: blessing (or `avila-core sign
        // receipt`) is what binds a signature into a committed package.
        if verified {
            report.receipt_signature = Some(signing::fresh_signature_status(
                self.runner_key_id.as_deref(),
                self.trust_root,
            ));
            if let Some(seed) = self.runner_key {
                match signature::digest_from_prefixed(&receipt_sha256) {
                    Ok(digest) => {
                        let document = signature::build_signature_document(
                            &seed,
                            "execution_receipt",
                            step.step_id.clone(),
                            receipt_sha256.clone(),
                            &digest,
                        );
                        if let Ok(mut bytes) = serde_json::to_vec_pretty(&document) {
                            bytes.push(b'\n');
                            let _ = fs::write(outcome.step_dir.join("receipt.sig.json"), &bytes);
                        }
                    }
                    Err(error) => report.add_finding(
                        CORE_X2601,
                        FindingClass::Invalid,
                        RunStage::ReceiptVerification,
                        "runner",
                        SourceLocation::new("execution-receipt", ""),
                        format!("fresh receipt could not be signed: {error}"),
                    ),
                }
            }
        }

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
        let covered_slots = self.current_qualification_covered_slots.clone();
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
            // The bound record's envelope, when the record names covered
            // output slots at all, is attached only to a claim on one of
            // them; an uncovered claim (e.g. a screen's dose estimate next to
            // its qualified geometry claims) carries none, exactly as if no
            // record had been bound for this capability.
            let claim_qualification = qualification.clone().filter(|_| {
                covered_slots
                    .as_ref()
                    .is_none_or(|slots| slots.iter().any(|slot| slot == &claim.output_slot))
            });
            self.claims.push(GeneratedClaim {
                claim_id: (*claim_id).to_string(),
                step_id: step.step_id.clone(),
                output_slot: claim.output_slot.clone(),
                artifact_sha256: sha256,
                media_type: output.media_type.clone(),
                producer_package_id: identity.package_id.clone(),
                producer_sha256: identity.executable_sha256.clone(),
                claim: claim.claim.clone(),
                qualification: claim_qualification,
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
                matches!(
                    check.state,
                    IntegrityCheckState::Verified | IntegrityCheckState::VerifiedCached
                ) && check.expected_sha256 == sha256
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
    case_id: &str,
) -> Vec<ChangeRecord> {
    let mut changes = Vec::new();
    // A receipt whose case_id differs was produced for a different case,
    // even if every other field of the plan happens to coincide (a donor
    // receipt copied from another package with the same capability,
    // parameters, and input identities). Checked before anything else so
    // such a receipt is never mistaken for a merely-unchanged one.
    //
    // `compiled_snapshot_sha256` is deliberately not compared: SC-12
    // execution memoization is about what a capability ran over, not about
    // the requirement or policy logic later applied to its outputs, so a
    // requirement, registry, or review edit that changes the compiled
    // snapshot identity must not by itself invalidate a step's receipt.
    if committed.case_id != case_id {
        changes.push(ChangeRecord {
            class: ChangeClass::DifferentCase,
            detail: format!(
                "receipt was produced for case `{}`, not `{case_id}`",
                committed.case_id
            ),
        });
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn comparison_attempt() -> AttemptRecord {
        AttemptRecord {
            schema_version: crate::ATTEMPT_LINEAGE_SCHEMA_VERSION.into(),
            attempt_id: "child".into(),
            generation: 1,
            parent_attempt_id: Some("parent".into()),
            parent_record_sha256: Some(format!("sha256:{}", "a".repeat(64))),
            fixed_manifest_sha256: format!("sha256:{}", "b".repeat(64)),
            fixed_compiled_snapshot_sha256: format!("sha256:{}", "c".repeat(64)),
            candidate_input: "candidate".into(),
            candidate_artifact_sha256: format!("sha256:{}", "d".repeat(64)),
            candidate_state_sha256: format!("sha256:{}", "e".repeat(64)),
            candidate_state: Value::Null,
            changes: Vec::new(),
        }
    }

    fn comparison_margin(
        requirement_id: &str,
        status: VerdictStatus,
        unit: Option<&str>,
        limit: Option<&str>,
        margin: Option<&str>,
    ) -> VerdictMargin {
        VerdictMargin {
            requirement_id: requirement_id.into(),
            status,
            rule: "test.rule".into(),
            unit: unit.map(str::to_owned),
            limit: limit.map(str::to_owned),
            lower: None,
            upper: None,
            nominal: None,
            observed_category: None,
            accepted_categories: None,
            margin: margin.map(str::to_owned),
        }
    }

    fn case_000() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-000-actinv-aftermatter")
    }

    fn case_001() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-001-shield-search")
    }

    #[test]
    fn attempt_comparison_derives_exact_deltas_and_renders_only_meaningful_changes() {
        let parent = vec![
            comparison_margin(
                "R1",
                VerdictStatus::Fail,
                Some("1"),
                Some("1"),
                Some("-1/4"),
            ),
            comparison_margin("R2", VerdictStatus::Pass, Some("1"), Some("1"), Some("1/2")),
            VerdictMargin {
                observed_category: Some("valid".into()),
                accepted_categories: Some(vec!["valid".into()]),
                ..comparison_margin("R3", VerdictStatus::Pass, None, None, None)
            },
            comparison_margin("R4", VerdictStatus::Pass, Some("m"), Some("1"), Some("1")),
            comparison_margin(
                "R-parent-only",
                VerdictStatus::Pass,
                Some("1"),
                Some("1"),
                Some("1"),
            ),
        ];
        let child = vec![
            comparison_margin("R1", VerdictStatus::Pass, Some("1"), Some("1"), Some("1/8")),
            comparison_margin("R2", VerdictStatus::Pass, Some("1"), Some("1"), Some("1/4")),
            VerdictMargin {
                observed_category: Some("valid".into()),
                accepted_categories: Some(vec!["valid".into()]),
                ..comparison_margin("R3", VerdictStatus::Pass, None, None, None)
            },
            comparison_margin("R4", VerdictStatus::Pass, Some("s"), Some("1"), Some("2")),
            comparison_margin(
                "R-child-only",
                VerdictStatus::Pass,
                Some("1"),
                Some("1"),
                Some("1"),
            ),
        ];

        let comparison = compare_attempt_results(&comparison_attempt(), &parent, &child).unwrap();
        assert_eq!(comparison.schema_version, ATTEMPT_COMPARISON_SCHEMA_VERSION);
        assert_eq!(comparison.verdicts_compared, 4);
        assert_eq!(comparison.unchanged_verdicts, 3);
        assert_eq!(
            comparison.verdict_transitions,
            vec![AttemptVerdictTransition {
                requirement_id: "R1".into(),
                parent_status: VerdictStatus::Fail,
                child_status: VerdictStatus::Pass,
            }]
        );
        assert_eq!(comparison.verdict_comparison_unavailable.len(), 2);
        assert_eq!(comparison.exact_margin_comparisons.len(), 2);
        assert_eq!(comparison.exact_margin_comparisons[0].delta, "0.375");
        assert_eq!(comparison.exact_margin_comparisons[1].delta, "-0.25");
        assert_eq!(comparison.margin_comparison_unavailable.len(), 4);
        assert_eq!(
            comparison
                .margin_comparison_unavailable
                .iter()
                .find(|unavailable| unavailable.requirement_id == "R3")
                .unwrap()
                .reason,
            AttemptMarginUnavailableReason::NotNumeric
        );

        let mut rendered = String::new();
        write_attempt_comparison(&mut rendered, &comparison);
        assert!(rendered.contains("4 verdict(s), 1 transition(s); 2 exact numeric margin(s)"));
        assert!(
            rendered.contains("result R1: FAIL -> PASS; margin -0.25 -> 0.125 1 (delta +0.375)")
        );
        assert!(rendered.contains("margin R2: 0.5 -> 0.25 1 (delta -0.25)"));
        assert!(rendered.contains("no numeric margin: 1 categorical requirement(s)"));
        assert!(rendered.contains("unavailable: 2 verdict comparison(s), 3 margin comparison(s)"));
        assert_eq!(display_signed_number("0.000000000069"), "+0.000000000069");
        assert_eq!(display_signed_number("-0.000006849895"), "-0.000006849895");
    }

    #[test]
    fn stderr_feedback_is_bounded_redacted_and_control_safe() {
        let environment = BTreeMap::from([
            ("ACCESS_TOKEN".to_string(), "secret-value".to_string()),
            ("SHORT_VALUE".to_string(), "A".to_string()),
        ]);
        let stderr = format!(
            "{}\nmarker=A token=secret-value\nfinal diagnostic\u{0007}\n",
            "x".repeat(DIAGNOSTIC_STDERR_MAX_CHARS + 200)
        );
        let excerpt = sanitize_diagnostic_stderr(stderr.as_bytes(), false, &environment).unwrap();
        assert!(excerpt.truncated);
        assert!(excerpt.text.starts_with('…'));
        assert!(excerpt.text.contains("marker=[REDACTED] token=[REDACTED]"));
        assert!(excerpt.text.contains("final diagnostic�"));
        assert!(!excerpt.text.contains("secret-value"));
        assert!(!excerpt.text.contains('\u{0007}'));
        assert!(excerpt.text.chars().count() <= DIAGNOSTIC_STDERR_MAX_CHARS + 1);
    }

    #[test]
    fn stderr_feedback_keeps_only_the_last_nonempty_lines() {
        let stderr = (0..=DIAGNOSTIC_STDERR_MAX_LINES)
            .map(|line| format!("diagnostic {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let excerpt =
            sanitize_diagnostic_stderr(stderr.as_bytes(), false, &BTreeMap::new()).unwrap();
        assert!(excerpt.truncated);
        assert!(!excerpt.text.contains("diagnostic 0"));
        assert!(excerpt.text.contains("diagnostic 1"));
        assert!(
            excerpt
                .text
                .contains(&format!("diagnostic {}", DIAGNOSTIC_STDERR_MAX_LINES))
        );
    }

    #[test]
    fn truncated_stderr_with_environment_values_is_not_embedded() {
        let path = std::env::temp_dir().join(format!(
            "avila-core-stderr-feedback-{}.log",
            std::process::id()
        ));
        fs::write(
            &path,
            vec![b'x'; usize::try_from(DIAGNOSTIC_STDERR_READ_BYTES).unwrap() + 1],
        )
        .unwrap();
        let environment = BTreeMap::from([("ACCESS_TOKEN".to_string(), "secret".to_string())]);
        let feedback = read_diagnostic_stderr(&path, &environment).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(feedback, DiagnosticStderrFeedback::WithheldForRedaction);
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
            "sha256:d21d5702aa3f7431d6ccea7d7b5f529687b1143547db1fc07bc89db2ef78449e"
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

    fn shielding_root() -> BTreeMap<String, PathBuf> {
        BTreeMap::from([(
            "shielding".to_string(),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/capabilities/shielding"),
        )])
    }

    /// S-038: `--hash-cache` is opt-in, never changes a verdict, and reports
    /// a hit as a state distinct from a fresh hash.
    #[test]
    fn hash_cache_cold_run_populates_and_warm_run_reports_the_cached_state() {
        let cache_path = std::env::temp_dir().join(format!(
            "avila-core-hash-cache-runner-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&cache_path);

        let options = CaseRunOptions {
            source_roots: shielding_root(),
            hash_cache: Some(cache_path.clone()),
            ..CaseRunOptions::default()
        };

        let cold = execute_case(&case_001(), &options).unwrap();
        assert_eq!(cold.status, CaseRunStatus::Evaluated, "{cold:?}");
        assert!(cold.replay.as_ref().unwrap().matches);
        let shielding_checks: Vec<_> = cold
            .integrity
            .artifacts
            .iter()
            .filter(|check| check.source_root == "shielding")
            .collect();
        assert_eq!(
            shielding_checks.len(),
            4,
            "the shielding root has four artifacts"
        );
        assert!(
            shielding_checks
                .iter()
                .all(|check| check.state == IntegrityCheckState::Verified),
            "a cold cache must not fabricate a hit: {shielding_checks:?}"
        );
        assert!(
            cold.findings
                .iter()
                .all(|finding| finding.code != CORE_X1003),
            "a cold run with no prior file must not report a cache problem"
        );
        assert!(
            cache_path.is_file(),
            "the cold run must create the cache file"
        );

        let warm = execute_case(&case_001(), &options).unwrap();
        assert_eq!(warm.status, CaseRunStatus::Evaluated, "{warm:?}");
        assert!(warm.replay.as_ref().unwrap().matches);
        let warm_shielding: Vec<_> = warm
            .integrity
            .artifacts
            .iter()
            .filter(|check| check.source_root == "shielding")
            .collect();
        assert!(
            warm_shielding
                .iter()
                .all(|check| check.state == IntegrityCheckState::VerifiedCached),
            "a warm run must report the cached state, never plain verified: {warm_shielding:?}"
        );
        for (cold_check, warm_check) in shielding_checks.iter().zip(&warm_shielding) {
            assert_eq!(cold_check.actual_sha256, warm_check.actual_sha256);
        }
        // The cache changes only which state an already-passing artifact
        // reports; it never touches a verdict.
        assert_eq!(cold.margins, warm.margins);

        let _ = fs::remove_file(&cache_path);
    }

    /// A corrupt cache file is a notice, not a crash, and the run heals it.
    #[test]
    fn hash_cache_corrupt_file_is_ignored_with_a_finding_and_self_heals() {
        let cache_path = std::env::temp_dir().join(format!(
            "avila-core-hash-cache-corrupt-{}.json",
            std::process::id()
        ));
        fs::write(&cache_path, b"{ this is not json").unwrap();

        let options = CaseRunOptions {
            source_roots: shielding_root(),
            hash_cache: Some(cache_path.clone()),
            ..CaseRunOptions::default()
        };
        let report = execute_case(&case_001(), &options).unwrap();
        assert_eq!(report.status, CaseRunStatus::Evaluated, "{report:?}");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == CORE_X1003 && finding.class == FindingClass::Notice),
            "a corrupt cache file must be a visible notice: {:?}",
            report.findings
        );
        assert!(
            report
                .integrity
                .artifacts
                .iter()
                .filter(|check| check.source_root == "shielding")
                .all(|check| check.state == IntegrityCheckState::Verified),
            "every artifact must still be hashed fresh when the cache is ignored"
        );

        // The run heals the file: it is valid and populated afterward.
        let healed = load_hash_cache(&cache_path).unwrap();
        assert_eq!(healed.entries.len(), 4);

        let _ = fs::remove_file(&cache_path);
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
        assert_eq!(report.campaign.as_ref().unwrap().verdicts.len(), 3);
        assert!(report.replay.as_ref().unwrap().matches);

        let summary = human_summary(&report);
        assert!(summary.contains("activation [aftermatter.r0-inventory-build@1]"));
        assert!(summary.contains("[NOT RUN] activation"));
        assert!(summary.contains("[NOT RUN] classification"));
        assert!(summary.contains("CASE-000-R1 — bounded.lt.within"));
        assert!(summary.contains(
            "CASE-000-R3 — categorical.equals.mismatch (observed unresolved; accepted feasible)"
        ));
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
            attempt: None,
            hash_cache: None,
            trust_root: None,
            runner_key: None,
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

    fn log_test_dir(label: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "avila-core-runner-log-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn append_log_line_writes_the_row_and_newline_in_one_call() {
        let dir = log_test_dir("single-write");
        let path = dir.join("attempts.jsonl");
        append_log_line(&path, None, r#"{"a":1}"#, None).unwrap();
        append_log_line(&path, None, r#"{"b":2}"#, None).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes, b"{\"a\":1}\n{\"b\":2}\n".to_vec());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Eight threads race to claim the same root attempt id on one shared
    /// log file. Locking the revalidate-before-append check together with
    /// the write (rather than reading unlocked and appending separately,
    /// the pre-fix sequence) must admit exactly one of them and reject the
    /// rest with the log showing the duplicate, never two racers both
    /// believing they claimed it.
    #[test]
    fn concurrent_appends_racing_one_attempt_id_admit_exactly_one() {
        let dir = log_test_dir("race-one-id");
        let log_path = dir.join("attempts.jsonl");
        let candidate_path = dir.join("candidate.json");
        fs::write(&candidate_path, b"{}\n").unwrap();
        let (candidate_sha256, _) = sha256_file(&candidate_path).unwrap();
        let manifest_sha256 = format!("sha256:{}", "a".repeat(64));
        let snapshot_sha256 = format!("sha256:{}", "b".repeat(64));

        const RACERS: usize = 8;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(RACERS));
        let handles: Vec<_> = (0..RACERS)
            .map(|_| {
                let barrier = std::sync::Arc::clone(&barrier);
                let log_path = log_path.clone();
                let candidate_path = candidate_path.clone();
                let candidate_sha256 = candidate_sha256.clone();
                let manifest_sha256 = manifest_sha256.clone();
                let snapshot_sha256 = snapshot_sha256.clone();
                std::thread::spawn(move || -> Result<(), String> {
                    let request = AttemptLineageRequest {
                        attempt_id: "contested-root".into(),
                        parent_attempt_id: None,
                        candidate_input: "candidate".into(),
                    };
                    barrier.wait();
                    let attempt = prepare_attempt(
                        &request,
                        Some(&log_path),
                        Some(&candidate_path),
                        Some(&candidate_sha256),
                        &manifest_sha256,
                        &snapshot_sha256,
                        None,
                    )?;
                    let line = serde_json::json!({
                        "attempt": &attempt,
                        "manifest_sha256": manifest_sha256,
                        "compiled_snapshot_sha256": snapshot_sha256,
                    })
                    .to_string();
                    append_log_line(&log_path, Some(&attempt), &line, None)
                        .map_err(|error| error.to_string())
                })
            })
            .collect();

        let mut successes = 0;
        let mut rejections = 0;
        for handle in handles {
            match handle.join().unwrap() {
                Ok(()) => successes += 1,
                Err(message) => {
                    assert!(
                        message.contains("already exists") || message.contains("appeared in"),
                        "a racer should fail only on the duplicate id, not some other error: {message}"
                    );
                    rejections += 1;
                }
            }
        }
        assert_eq!(
            successes, 1,
            "exactly one racer should claim the contested attempt id"
        );
        assert_eq!(rejections, RACERS - 1);

        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert_eq!(
            lines.len(),
            1,
            "the log must carry exactly the one winner's line"
        );
        let value: Value = serde_json::from_str(lines[0])
            .expect("the single appended line must be intact, unsplit JSON");
        assert_eq!(value["attempt"]["attempt_id"], "contested-root");

        let _ = fs::remove_dir_all(&dir);
    }
}
