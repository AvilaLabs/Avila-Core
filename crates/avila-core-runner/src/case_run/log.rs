//! The run-attempt campaign log: one JSON line per run, appended under an
//! exclusive lock, plus the error log for an infrastructure failure that
//! never produced a case report.

use std::error::Error;
use std::fs;
use std::path::Path;

use avila_core_compiler::{CampaignStatus, CompilationStatus, ReviewerRole};
use avila_core_evidence::{PackageIntegrityStatus, sha256_file};
use serde::Serialize;
use serde_json::Value;

use super::{
    BindingStatus, CaseRunOptions, CaseRunReport, CaseRunStatus, ChangeRecord, CoverageStatus,
    ExecutionStatus, OutputReport, PresentationGateReadiness, RUN_ATTEMPT_LOG_SCHEMA_VERSION,
    ReceiptSummary, StepExecutionState, SuppliedInput, VerdictMargin,
};
use crate::attempt::{AttemptComparison, AttemptLineageRequest, AttemptRecord};
use crate::diagnostic::RunFinding;
use crate::execute::rfc3339_now;

pub(crate) fn append_log(
    options: &CaseRunOptions,
    case_or_manifest: &Path,
    report: &CaseRunReport,
) -> Result<(), Box<dyn Error>> {
    let Some(path) = &options.log else {
        return Ok(());
    };
    if let Some(attempt) = &report.attempt {
        let candidate_path = options
            .inputs
            .get(&attempt.candidate_input)
            .ok_or_else(|| {
                format!(
                    "attempt candidate input `{}` disappeared before append",
                    attempt.candidate_input
                )
            })?;
        let (candidate_sha256, _) = sha256_file(candidate_path)?;
        if candidate_sha256 != attempt.candidate_artifact_sha256 {
            return Err(format!(
                "attempt candidate `{}` changed before append: expected {}, observed {candidate_sha256}",
                candidate_path.display(),
                attempt.candidate_artifact_sha256
            )
            .into());
        }
        // Lineage revalidation happens inside `append_log_line`, under the
        // same lock as the write, so a concurrent writer cannot slip a
        // conflicting attempt in between the check and the append.
    }
    #[derive(Serialize)]
    struct LogEntry<'a> {
        schema_version: &'static str,
        recorded_at: String,
        case_path: String,
        case_id: &'a str,
        status: CaseRunStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        attempt_request: Option<&'a AttemptLineageRequest>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attempt: Option<&'a AttemptRecord>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attempt_comparison: Option<&'a AttemptComparison>,
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
        attempt_request: if report.attempt.is_none() {
            options.attempt.as_ref()
        } else {
            None
        },
        attempt: report.attempt.as_ref(),
        attempt_comparison: report.attempt_comparison.as_ref(),
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
    append_log_line(
        path,
        report.attempt.as_ref(),
        &serde_json::to_string(&entry)?,
    )
}

pub(crate) fn append_error_log(
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
        #[serde(skip_serializing_if = "Option::is_none")]
        attempt_request: Option<&'a AttemptLineageRequest>,
        findings: [&'a RunFinding; 1],
    }
    let entry = ErrorLogEntry {
        schema_version: RUN_ATTEMPT_LOG_SCHEMA_VERSION,
        recorded_at: rfc3339_now(),
        case_path: case_or_manifest.display().to_string(),
        status: "error",
        attempt_request: options.attempt.as_ref(),
        findings: [finding],
    };
    // An infrastructure error before a case report exists has no resolved
    // `AttemptRecord` to revalidate; only `attempt_request` (the caller's
    // unvalidated ask) is available, and it is already carried in `entry`.
    append_log_line(path, None, &serde_json::to_string(&entry)?)
}

/// Append one line to the run-attempt log as a single write, holding an
/// exclusive lock across lineage revalidation and the write itself.
///
/// `append_log`'s candidate lineage otherwise reads history to revalidate a
/// parent, releases nothing, and only then appends: two processes racing a
/// campaign can each pass that check against the same parent and both
/// append a child bound to it before either write lands. Locking here makes
/// "revalidate, then append" one critical section instead of two operations
/// with a gap between them.
pub(crate) fn append_log_line(
    path: &Path,
    attempt: Option<&AttemptRecord>,
    line: &str,
) -> Result<(), Box<dyn Error>> {
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
    // A duplicated handle carries the lock so it can be released by a Drop
    // guard without fighting the borrow checker over the original handle,
    // which the write below still needs mutably. Unix `flock`/Windows
    // `LockFileEx` semantics attach to the open file description a `dup`
    // shares, not to either individual handle, so locking one and unlocking
    // the other is exactly one lock over the file's lifetime.
    let lock_handle = file.try_clone()?;
    lock_handle.lock()?;
    let _lock = LogFileLock(lock_handle);

    if let Some(attempt) = attempt {
        crate::attempt::revalidate_before_append(path, attempt)
            .map_err(|issue| format!("attempt lineage changed before append: {issue}"))?;
    }

    let mut buffer = Vec::with_capacity(line.len() + 1);
    buffer.extend_from_slice(line.as_bytes());
    buffer.push(b'\n');
    std::io::Write::write_all(&mut file, &buffer)?;
    Ok(())
}

/// Releases the advisory lock when dropped, including on an early `?`
/// return or a panic unwind, so a failed append never wedges the log for
/// the rest of the process's life.
struct LogFileLock(fs::File);

impl Drop for LogFileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
