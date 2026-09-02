//! The controlled runner behind `avila-core run`: stages verified bytes into
//! a fresh workspace, invokes one exact executable with a cleared
//! environment, captures its streams, hashes what it produced, and writes an
//! execution receipt. It is deliberately case-specific: the only adapters it
//! knows are the ones a committed case declares.

#[cfg(test)]
mod adversarial_tests;
pub mod aftermatter;
pub mod claims;

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use avila_core_evidence::{
    CapabilityIdentity, CapabilityTypeRef, EXECUTION_RECEIPT_SCHEMA_VERSION, ExecutionReceipt,
    Invocation, LogRecord, OutputState, ProcessOutcome, RECEIPT_NOTICE, ReceiptInput,
    ReceiptOutput, ReceiptStatus, RunnerIdentity, invocation_identity, sha256_file,
};
use serde_json::Value;

pub const RUNNER_ID: &str = concat!("avila.core/cli-rust@", env!("CARGO_PKG_VERSION"));

/// A declared output file of an adapter.
#[derive(Debug, Clone, Copy)]
pub struct AdapterOutput {
    pub output_id: &'static str,
    pub workspace_path: &'static str,
    pub media_type: &'static str,
}

/// One claim an adapter extracted from a produced output.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedClaim {
    pub output_slot: String,
    pub output_id: String,
    pub claim: Value,
}

/// The case-specific adapters the runner can drive, selected by the adapter
/// identifier a package execution names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adapter {
    AftermatterEvaluate,
}

impl Adapter {
    pub fn by_id(id: &str) -> Option<Self> {
        match id {
            aftermatter::ADAPTER_ID => Some(Self::AftermatterEvaluate),
            _ => None,
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::AftermatterEvaluate => aftermatter::ADAPTER_ID,
        }
    }

    pub fn capability_type(self) -> CapabilityTypeRef {
        match self {
            Self::AftermatterEvaluate => CapabilityTypeRef {
                id: aftermatter::CAPABILITY_TYPE_ID.into(),
                major: aftermatter::CAPABILITY_TYPE_MAJOR,
            },
        }
    }

    pub const fn input_slots(self) -> &'static [&'static str] {
        match self {
            Self::AftermatterEvaluate => aftermatter::INPUT_SLOTS,
        }
    }

    pub const fn outputs(self) -> &'static [AdapterOutput] {
        match self {
            Self::AftermatterEvaluate => aftermatter::OUTPUTS,
        }
    }

    pub const fn output_slots(self) -> &'static [&'static str] {
        match self {
            Self::AftermatterEvaluate => aftermatter::OUTPUT_SLOTS,
        }
    }

    pub const fn timeout(self) -> Duration {
        match self {
            Self::AftermatterEvaluate => aftermatter::TIMEOUT,
        }
    }

    /// The portable argument list: relative workspace paths only.
    pub fn arguments(self, staged: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
        match self {
            Self::AftermatterEvaluate => aftermatter::arguments(staged),
        }
    }

    pub fn extract_claims(
        self,
        outputs: &BTreeMap<String, Vec<u8>>,
        parameters: &BTreeMap<String, Value>,
    ) -> Result<Vec<ExtractedClaim>, String> {
        match self {
            Self::AftermatterEvaluate => aftermatter::extract_claims(outputs, parameters),
        }
    }
}

/// A verified input staged into the step workspace.
#[derive(Debug, Clone)]
pub struct StagedInput {
    pub input_slot: String,
    pub evidence_id: String,
    pub source_path: PathBuf,
    pub workspace_path: String,
    pub media_type: String,
    pub expected_sha256: String,
}

/// Everything the runner needs to execute one step.
#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub case_id: String,
    pub compiled_snapshot_sha256: String,
    pub step_id: String,
    pub adapter: Adapter,
    pub capability: CapabilityIdentity,
    pub executable: PathBuf,
    pub parameters: BTreeMap<String, Value>,
    pub inputs: Vec<StagedInput>,
}

/// Where one execution left its bytes. The receipt is re-read from disk by
/// the caller so verification never trusts in-memory state.
#[derive(Debug)]
pub struct ExecutionOutcome {
    pub step_dir: PathBuf,
    pub receipt_path: PathBuf,
}

/// Stage, run, collect, and receipt one step inside `step_dir`, which must
/// not exist yet.
pub fn execute_step(
    step_dir: &Path,
    request: &ExecutionRequest,
) -> Result<ExecutionOutcome, Box<dyn Error>> {
    if step_dir.exists() {
        return Err(format!(
            "workspace `{}` already exists; every execution needs a fresh directory",
            step_dir.display()
        )
        .into());
    }
    fs::create_dir_all(step_dir)?;
    fs::create_dir_all(step_dir.join("logs"))?;

    // Stage: copy verified bytes to runner-assigned relative paths and re-hash
    // the copies so the receipt describes what the program could read.
    let mut receipt_inputs = Vec::with_capacity(request.inputs.len());
    let mut staged_paths = BTreeMap::new();
    for input in &request.inputs {
        let destination = step_dir.join(&input.workspace_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&input.source_path, &destination)?;
        let (sha256, bytes) = sha256_file(&destination)?;
        if sha256 != input.expected_sha256 {
            return Err(format!(
                "staged copy of `{}` hashes to {sha256}, expected {}",
                input.evidence_id, input.expected_sha256
            )
            .into());
        }
        staged_paths.insert(input.input_slot.clone(), input.workspace_path.clone());
        receipt_inputs.push(ReceiptInput {
            input_slot: input.input_slot.clone(),
            evidence_id: input.evidence_id.clone(),
            workspace_path: input.workspace_path.clone(),
            media_type: input.media_type.clone(),
            sha256,
            bytes,
        });
    }
    for output in request.adapter.outputs() {
        if let Some(parent) = step_dir.join(output.workspace_path).parent() {
            fs::create_dir_all(parent)?;
        }
    }

    let arguments = request.adapter.arguments(&staged_paths)?;
    let timeout = request.adapter.timeout();
    let program = request
        .executable
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| request.capability.capability_id.clone());
    let invocation = Invocation {
        program,
        arguments: arguments.clone(),
        working_directory: ".".into(),
        environment: BTreeMap::new(),
        timeout_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
    };
    let invocation_sha256 = invocation_identity(
        &request.capability,
        &request.parameters,
        &receipt_inputs,
        &invocation,
    )?;

    // Execute with a cleared environment inside the workspace.
    let stdout_path = step_dir.join("logs/stdout.log");
    let stderr_path = step_dir.join("logs/stderr.log");
    let stdout = fs::File::create(&stdout_path)?;
    let stderr = fs::File::create(&stderr_path)?;
    let started_at = rfc3339_now();
    let started = Instant::now();
    let mut child = Command::new(&request.executable)
        .args(&arguments)
        .current_dir(step_dir)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()?;
    let (status, timed_out) = wait_with_timeout(&mut child, timeout)?;
    let duration = started.elapsed();
    let finished_at = rfc3339_now();

    let exit_status = status.code();
    let signal = signal_of(&status);
    let process = ProcessOutcome {
        started_at,
        finished_at,
        duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        exit_status,
        signal,
        timed_out,
    };

    let mut logs = Vec::new();
    for (stream, path) in [("stdout", &stdout_path), ("stderr", &stderr_path)] {
        let (sha256, bytes) = sha256_file(path)?;
        logs.push(LogRecord {
            stream: stream.into(),
            workspace_path: format!("logs/{stream}.log"),
            sha256,
            bytes,
        });
    }

    // Collect only declared outputs.
    let mut outputs = Vec::new();
    let mut all_collected = true;
    for output in request.adapter.outputs() {
        let path = step_dir.join(output.workspace_path);
        let record = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                let (sha256, bytes) = sha256_file(&path)?;
                ReceiptOutput {
                    output_id: output.output_id.into(),
                    workspace_path: output.workspace_path.into(),
                    media_type: output.media_type.into(),
                    state: OutputState::Collected,
                    sha256: Some(sha256),
                    bytes: Some(bytes),
                }
            }
            Ok(_) => {
                all_collected = false;
                missing_output(output)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                all_collected = false;
                missing_output(output)
            }
            Err(error) => return Err(error.into()),
        };
        outputs.push(record);
    }

    let status = if timed_out {
        ReceiptStatus::TimedOut
    } else if exit_status == Some(0) && all_collected {
        ReceiptStatus::Completed
    } else {
        ReceiptStatus::Failed
    };

    let receipt = ExecutionReceipt {
        schema_version: EXECUTION_RECEIPT_SCHEMA_VERSION.into(),
        case_id: request.case_id.clone(),
        compiled_snapshot_sha256: request.compiled_snapshot_sha256.clone(),
        step_id: request.step_id.clone(),
        capability_type: request.adapter.capability_type(),
        adapter: request.adapter.id().into(),
        capability: request.capability.clone(),
        parameters: request.parameters.clone(),
        inputs: receipt_inputs,
        invocation,
        invocation_sha256,
        process,
        logs,
        outputs,
        runner: RunnerIdentity {
            runner: RUNNER_ID.into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
        },
        status,
        limitations: vec![
            "Exit status and output digests are process evidence, not scientific success.".into(),
            "The executable digest identifies the bytes that ran; it does not qualify the method.".into(),
            "No sandbox, resource accounting, or signature was applied; the environment was cleared and the working directory confined to the workspace.".into(),
        ],
        notice: RECEIPT_NOTICE.into(),
    };
    let receipt_path = step_dir.join("receipt.json");
    let mut bytes = serde_json::to_vec_pretty(&receipt)?;
    bytes.push(b'\n');
    fs::write(&receipt_path, bytes)?;
    Ok(ExecutionOutcome {
        step_dir: step_dir.to_path_buf(),
        receipt_path,
    })
}

fn missing_output(output: &AdapterOutput) -> ReceiptOutput {
    ReceiptOutput {
        output_id: output.output_id.into(),
        workspace_path: output.workspace_path.into(),
        media_type: output.media_type.into(),
        state: OutputState::Missing,
        sha256: None,
        bytes: None,
    }
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> io::Result<(std::process::ExitStatus, bool)> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status, false));
        }
        if Instant::now() >= deadline {
            child.kill()?;
            let status = child.wait()?;
            return Ok((status, true));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn signal_of(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt as _;
    status.signal()
}

#[cfg(not(unix))]
fn signal_of(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

/// The current UTC time as RFC 3339 with millisecond precision. Timestamps
/// are observations on the receipt; they never enter an identity.
pub fn rfc3339_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    rfc3339_from_unix_millis(now.as_millis())
}

fn rfc3339_from_unix_millis(millis: u128) -> String {
    let seconds = millis / 1_000;
    let millis_part = millis % 1_000;
    let days = seconds / 86_400;
    let remainder = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis_part:03}Z",
        remainder / 3_600,
        (remainder % 3_600) / 60,
        remainder % 60
    )
}

/// Proleptic Gregorian civil date from days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_rendering_is_civil() {
        assert_eq!(rfc3339_from_unix_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            rfc3339_from_unix_millis(1_788_292_800_123),
            "2026-09-01T20:00:00.123Z"
        );
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }
}
