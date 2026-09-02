//! Execution receipts: what one controlled step execution actually did.
//!
//! A receipt binds the exact capability, the staged input bytes, the
//! invocation, the process outcome, the captured logs, and the produced
//! output bytes of one step. It is process evidence only. A zero exit status
//! and matching digests establish that a declared program ran over declared
//! bytes and wrote these bytes; they do not establish scientific correctness,
//! qualification, professional review, or regulatory suitability.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use avila_core_kernel::canonicalize_json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::package::{IntegrityCheckState, PackageError, hash_confined_file};

pub const EXECUTION_RECEIPT_SCHEMA_VERSION: &str = "avila.core/execution-receipt/v0.1-draft";
pub const RECEIPT_NOTICE: &str = "An execution receipt is process evidence: it binds the exact capability, staged input bytes, invocation, process outcome, logs, and produced output bytes of one step. It does not establish scientific correctness, qualification, professional review, or regulatory suitability.";

/// One step execution, as recorded by the runner that performed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReceipt {
    pub schema_version: String,
    pub case_id: String,
    pub compiled_snapshot_sha256: String,
    pub step_id: String,
    pub capability_type: CapabilityTypeRef,
    pub adapter: String,
    pub capability: CapabilityIdentity,
    #[serde(default)]
    pub parameters: BTreeMap<String, serde_json::Value>,
    pub inputs: Vec<ReceiptInput>,
    pub invocation: Invocation,
    /// Identity of the capability, parameters, staged inputs, and invocation:
    /// what was asked of the program, before it ran.
    pub invocation_sha256: String,
    pub process: ProcessOutcome,
    #[serde(default)]
    pub logs: Vec<LogRecord>,
    pub outputs: Vec<ReceiptOutput>,
    pub runner: RunnerIdentity,
    pub status: ReceiptStatus,
    #[serde(default)]
    pub limitations: Vec<String>,
    pub notice: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityTypeRef {
    pub id: String,
    pub major: u64,
}

/// The exact implementation that ran: a named package and the digest of the
/// executable bytes. Source coordinates are annotations; the digest is the
/// identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityIdentity {
    pub capability_id: String,
    pub package_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    pub executable_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptInput {
    pub input_slot: String,
    pub evidence_id: String,
    pub workspace_path: String,
    pub media_type: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub program: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessOutcome {
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogRecord {
    pub stream: String,
    pub workspace_path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptOutput {
    pub output_id: String,
    pub workspace_path: String,
    pub media_type: String,
    pub state: OutputState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputState {
    Collected,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerIdentity {
    pub runner: String,
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Completed,
    Failed,
    TimedOut,
}

#[derive(Debug, Error)]
pub enum ReceiptError {
    #[error("execution receipt is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("cannot serialize receipt identity: {0}")]
    Serialization(String),
    #[error("receipt identity is outside the canonical profile: {0}")]
    Canonicalization(String),
    #[error(transparent)]
    Package(#[from] PackageError),
}

/// The identity of what was asked of the program: capability, parameters,
/// staged inputs, and invocation. Process outcome and outputs are results
/// and deliberately excluded, so a rerun of the same request has the same
/// invocation identity whatever it produced. This is the memoization key of
/// SC-12: a completed receipt with the same identity, whose outputs are still
/// verifiable, stands for a rerun of a deterministic capability.
pub fn invocation_identity(
    capability: &CapabilityIdentity,
    parameters: &BTreeMap<String, serde_json::Value>,
    inputs: &[ReceiptInput],
    invocation: &Invocation,
) -> Result<String, ReceiptError> {
    /// The program name is a display annotation of the executable, whose
    /// identity is its digest; it does not enter the invocation identity.
    #[derive(Serialize)]
    struct InvocationBody<'a> {
        arguments: &'a [String],
        working_directory: &'a str,
        environment: &'a BTreeMap<String, String>,
        timeout_ms: u64,
    }
    #[derive(Serialize)]
    struct IdentityBody<'a> {
        schema_version: &'a str,
        capability: &'a CapabilityIdentity,
        parameters: &'a BTreeMap<String, serde_json::Value>,
        inputs: &'a [ReceiptInput],
        invocation: InvocationBody<'a>,
    }
    let body = IdentityBody {
        schema_version: EXECUTION_RECEIPT_SCHEMA_VERSION,
        capability,
        parameters,
        inputs,
        invocation: InvocationBody {
            arguments: &invocation.arguments,
            working_directory: &invocation.working_directory,
            environment: &invocation.environment,
            timeout_ms: invocation.timeout_ms,
        },
    };
    let bytes = serde_json::to_vec(&body)
        .map_err(|error| ReceiptError::Serialization(error.to_string()))?;
    let canonical = canonicalize_json(&bytes)
        .map_err(|error| ReceiptError::Canonicalization(error.to_string()))?;
    Ok(format!("sha256:{:x}", Sha256::digest(&canonical)))
}

/// Parse receipt bytes strictly.
pub fn parse_receipt(bytes: &[u8]) -> Result<ExecutionReceipt, ReceiptError> {
    Ok(serde_json::from_slice(bytes)?)
}

/// What the case package and compiled snapshot say the execution must have
/// been. Verification compares the receipt against these values and against
/// the bytes still present in the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptExpectations {
    pub case_id: String,
    pub compiled_snapshot_sha256: String,
    pub step_id: String,
    pub capability_type: CapabilityTypeRef,
    pub adapter: String,
    pub capability: CapabilityIdentity,
    /// Expected staged inputs by input slot.
    pub inputs: BTreeMap<String, ExpectedInput>,
    /// Expected output identifiers.
    pub outputs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedInput {
    pub evidence_id: String,
    pub sha256: String,
    pub media_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptCheckState {
    Verified,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptFileCheck {
    pub role: String,
    pub workspace_path: String,
    pub expected_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_sha256: Option<String>,
    pub state: IntegrityCheckState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptCheck {
    pub state: ReceiptCheckState,
    pub invocation_sha256: String,
    pub invocation_identity_reproduced: bool,
    pub files: Vec<ReceiptFileCheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

/// Verify a receipt against the expectations and against the workspace bytes.
///
/// Every input, log, and output file named by the receipt is re-hashed from
/// the workspace; the invocation identity is recomputed; the process outcome
/// must be a completed run with exit status zero; and every identity the
/// package and compiled snapshot fixed must match. Any deviation fails the
/// receipt. A verified receipt establishes process provenance only.
pub fn verify_receipt(
    receipt: &ExecutionReceipt,
    workspace: &Path,
    expected: &ReceiptExpectations,
) -> Result<ReceiptCheck, ReceiptError> {
    let mut issues = Vec::new();
    let mut files = Vec::new();

    if receipt.schema_version != EXECUTION_RECEIPT_SCHEMA_VERSION {
        issues.push(format!(
            "receipt schema `{}` is not `{EXECUTION_RECEIPT_SCHEMA_VERSION}`",
            receipt.schema_version
        ));
    }
    expect_equal(&mut issues, "case_id", &receipt.case_id, &expected.case_id);
    expect_equal(
        &mut issues,
        "compiled_snapshot_sha256",
        &receipt.compiled_snapshot_sha256,
        &expected.compiled_snapshot_sha256,
    );
    expect_equal(&mut issues, "step_id", &receipt.step_id, &expected.step_id);
    if receipt.capability_type != expected.capability_type {
        issues.push(format!(
            "receipt capability type `{}@{}` differs from the compiled step's `{}@{}`",
            receipt.capability_type.id,
            receipt.capability_type.major,
            expected.capability_type.id,
            expected.capability_type.major
        ));
    }
    expect_equal(&mut issues, "adapter", &receipt.adapter, &expected.adapter);
    if receipt.capability != expected.capability {
        issues.push(format!(
            "receipt capability `{}` ({}, {}) differs from the package's `{}` ({}, {})",
            receipt.capability.capability_id,
            receipt.capability.package_id,
            receipt.capability.executable_sha256,
            expected.capability.capability_id,
            expected.capability.package_id,
            expected.capability.executable_sha256
        ));
    }

    match receipt.status {
        ReceiptStatus::Completed => {}
        ReceiptStatus::Failed => issues.push("receipt records a failed execution".into()),
        ReceiptStatus::TimedOut => issues.push("receipt records a timed-out execution".into()),
    }
    if receipt.process.timed_out {
        issues.push("process timed out".into());
    }
    match receipt.process.exit_status {
        Some(0) => {}
        Some(code) => issues.push(format!("process exited with status {code}")),
        None => issues.push("process recorded no exit status".into()),
    }
    if let Some(signal) = receipt.process.signal {
        issues.push(format!("process was terminated by signal {signal}"));
    }
    if receipt.invocation.working_directory != "." {
        issues.push(format!(
            "invocation working directory `{}` is not the workspace root",
            receipt.invocation.working_directory
        ));
    }
    for argument in &receipt.invocation.arguments {
        if Path::new(argument).is_absolute() {
            issues.push(format!(
                "invocation argument `{argument}` is an absolute host path; receipts must be portable"
            ));
        }
    }

    let mut seen_slots = BTreeSet::new();
    for input in &receipt.inputs {
        if !seen_slots.insert(input.input_slot.as_str()) {
            issues.push(format!("input slot `{}` is staged twice", input.input_slot));
        }
        match expected.inputs.get(&input.input_slot) {
            Some(expectation) => {
                if input.evidence_id != expectation.evidence_id {
                    issues.push(format!(
                        "input slot `{}` was staged from `{}`, expected `{}`",
                        input.input_slot, input.evidence_id, expectation.evidence_id
                    ));
                }
                if input.sha256 != expectation.sha256 {
                    issues.push(format!(
                        "input slot `{}` was staged with digest {}, expected {}",
                        input.input_slot, input.sha256, expectation.sha256
                    ));
                }
                if input.media_type != expectation.media_type {
                    issues.push(format!(
                        "input slot `{}` was staged as `{}`, expected `{}`",
                        input.input_slot, input.media_type, expectation.media_type
                    ));
                }
            }
            None => issues.push(format!(
                "input slot `{}` is not bound in the compiled step",
                input.input_slot
            )),
        }
        files.push(check_workspace_file(
            workspace,
            "input",
            &input.workspace_path,
            &input.sha256,
            Some(input.bytes),
            &mut issues,
        )?);
    }
    for slot in expected.inputs.keys() {
        if !seen_slots.contains(slot.as_str()) {
            issues.push(format!("bound input slot `{slot}` was not staged"));
        }
    }

    for log in &receipt.logs {
        files.push(check_workspace_file(
            workspace,
            "log",
            &log.workspace_path,
            &log.sha256,
            Some(log.bytes),
            &mut issues,
        )?);
    }

    let mut seen_outputs = BTreeSet::new();
    for output in &receipt.outputs {
        if !seen_outputs.insert(output.output_id.as_str()) {
            issues.push(format!("output `{}` is recorded twice", output.output_id));
        }
        if !expected.outputs.contains(&output.output_id) {
            issues.push(format!(
                "output `{}` is not declared by the adapter",
                output.output_id
            ));
        }
        match (output.state, &output.sha256) {
            (OutputState::Collected, Some(sha256)) => {
                files.push(check_workspace_file(
                    workspace,
                    "output",
                    &output.workspace_path,
                    sha256,
                    output.bytes,
                    &mut issues,
                )?);
            }
            (OutputState::Collected, None) => issues.push(format!(
                "output `{}` is marked collected without a digest",
                output.output_id
            )),
            (OutputState::Missing, _) => {
                issues.push(format!("output `{}` was not produced", output.output_id));
            }
        }
    }
    for output_id in &expected.outputs {
        if !seen_outputs.contains(output_id.as_str()) {
            issues.push(format!(
                "declared output `{output_id}` is absent from the receipt"
            ));
        }
    }

    let recomputed = invocation_identity(
        &receipt.capability,
        &receipt.parameters,
        &receipt.inputs,
        &receipt.invocation,
    )?;
    let invocation_identity_reproduced = recomputed == receipt.invocation_sha256;
    if !invocation_identity_reproduced {
        issues.push(format!(
            "recorded invocation identity {} does not match the recomputed {recomputed}",
            receipt.invocation_sha256
        ));
    }

    Ok(ReceiptCheck {
        state: if issues.is_empty() {
            ReceiptCheckState::Verified
        } else {
            ReceiptCheckState::Failed
        },
        invocation_sha256: recomputed,
        invocation_identity_reproduced,
        files,
        issues,
    })
}

fn expect_equal(issues: &mut Vec<String>, field: &str, actual: &str, expected: &str) {
    if actual != expected {
        issues.push(format!(
            "receipt {field} `{actual}` differs from the expected `{expected}`"
        ));
    }
}

fn check_workspace_file(
    workspace: &Path,
    role: &str,
    workspace_path: &str,
    expected_sha256: &str,
    expected_bytes: Option<u64>,
    issues: &mut Vec<String>,
) -> Result<ReceiptFileCheck, ReceiptError> {
    let (actual_sha256, state) = match hash_confined_file(workspace, workspace_path)? {
        Some((digest, length)) => {
            let state = if digest == expected_sha256 {
                IntegrityCheckState::Verified
            } else {
                IntegrityCheckState::Mismatch
            };
            if let Some(expected) = expected_bytes
                && expected != length
            {
                issues.push(format!(
                    "{role} `{workspace_path}` is {length} bytes; the receipt recorded {expected}"
                ));
            }
            (Some(digest), state)
        }
        None => (None, IntegrityCheckState::Missing),
    };
    match state {
        IntegrityCheckState::Verified => {}
        IntegrityCheckState::Mismatch => issues.push(format!(
            "{role} `{workspace_path}` no longer matches the receipt digest"
        )),
        IntegrityCheckState::Missing => {
            issues.push(format!(
                "{role} `{workspace_path}` is missing from the workspace"
            ));
        }
        IntegrityCheckState::NotChecked => {}
    }
    Ok(ReceiptFileCheck {
        role: role.into(),
        workspace_path: workspace_path.into(),
        expected_sha256: expected_sha256.into(),
        actual_sha256,
        state,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::sha256_hex;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "avila-core-receipt-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn digest(bytes: &[u8]) -> String {
        format!("sha256:{}", sha256_hex(bytes))
    }

    fn capability() -> CapabilityIdentity {
        CapabilityIdentity {
            capability_id: "stub".into(),
            package_id: "test/stub@1".into(),
            source_repository: None,
            source_commit: None,
            executable_sha256: digest(b"stub-binary"),
        }
    }

    fn fixture(root: &Path) -> (ExecutionReceipt, ReceiptExpectations) {
        fs::create_dir_all(root.join("inputs")).unwrap();
        fs::create_dir_all(root.join("outputs")).unwrap();
        fs::create_dir_all(root.join("logs")).unwrap();
        fs::write(root.join("inputs/a.json"), b"input-a").unwrap();
        fs::write(root.join("outputs/result.json"), b"result").unwrap();
        fs::write(root.join("logs/stdout.log"), b"").unwrap();
        let inputs = vec![ReceiptInput {
            input_slot: "a".into(),
            evidence_id: "input:a".into(),
            workspace_path: "inputs/a.json".into(),
            media_type: "application/json".into(),
            sha256: digest(b"input-a"),
            bytes: 7,
        }];
        let invocation = Invocation {
            program: "stub".into(),
            arguments: vec!["--output".into(), "outputs/result.json".into()],
            working_directory: ".".into(),
            environment: BTreeMap::new(),
            timeout_ms: 1_000,
        };
        let parameters = BTreeMap::new();
        let invocation_sha256 =
            invocation_identity(&capability(), &parameters, &inputs, &invocation).unwrap();
        let receipt = ExecutionReceipt {
            schema_version: EXECUTION_RECEIPT_SCHEMA_VERSION.into(),
            case_id: "CASE-TEST".into(),
            compiled_snapshot_sha256: digest(b"snapshot"),
            step_id: "step".into(),
            capability_type: CapabilityTypeRef {
                id: "test.type".into(),
                major: 1,
            },
            adapter: "test/adapter@1".into(),
            capability: capability(),
            parameters,
            inputs,
            invocation,
            invocation_sha256,
            process: ProcessOutcome {
                started_at: "2026-01-01T00:00:00Z".into(),
                finished_at: "2026-01-01T00:00:01Z".into(),
                duration_ms: 1_000,
                exit_status: Some(0),
                signal: None,
                timed_out: false,
            },
            logs: vec![LogRecord {
                stream: "stdout".into(),
                workspace_path: "logs/stdout.log".into(),
                sha256: digest(b""),
                bytes: 0,
            }],
            outputs: vec![ReceiptOutput {
                output_id: "result".into(),
                workspace_path: "outputs/result.json".into(),
                media_type: "application/json".into(),
                state: OutputState::Collected,
                sha256: Some(digest(b"result")),
                bytes: Some(6),
            }],
            runner: RunnerIdentity {
                runner: "test".into(),
                os: "linux".into(),
                arch: "x86_64".into(),
            },
            status: ReceiptStatus::Completed,
            limitations: Vec::new(),
            notice: RECEIPT_NOTICE.into(),
        };
        let expected = ReceiptExpectations {
            case_id: "CASE-TEST".into(),
            compiled_snapshot_sha256: digest(b"snapshot"),
            step_id: "step".into(),
            capability_type: CapabilityTypeRef {
                id: "test.type".into(),
                major: 1,
            },
            adapter: "test/adapter@1".into(),
            capability: capability(),
            inputs: BTreeMap::from([(
                "a".to_string(),
                ExpectedInput {
                    evidence_id: "input:a".into(),
                    sha256: digest(b"input-a"),
                    media_type: "application/json".into(),
                },
            )]),
            outputs: BTreeSet::from(["result".to_string()]),
        };
        (receipt, expected)
    }

    #[test]
    fn intact_receipt_verifies_and_round_trips() {
        let root = TestDir::new();
        let (receipt, expected) = fixture(&root.0);
        let check = verify_receipt(&receipt, &root.0, &expected).unwrap();
        assert_eq!(
            check.state,
            ReceiptCheckState::Verified,
            "{:?}",
            check.issues
        );
        assert!(check.invocation_identity_reproduced);
        assert_eq!(check.files.len(), 3);

        let bytes = serde_json::to_vec(&receipt).unwrap();
        assert_eq!(parse_receipt(&bytes).unwrap(), receipt);
    }

    #[test]
    fn altered_output_bytes_fail_the_receipt() {
        let root = TestDir::new();
        let (receipt, expected) = fixture(&root.0);
        fs::write(root.0.join("outputs/result.json"), b"altered").unwrap();
        let check = verify_receipt(&receipt, &root.0, &expected).unwrap();
        assert_eq!(check.state, ReceiptCheckState::Failed);
        assert!(
            check
                .issues
                .iter()
                .any(|issue| issue.contains("no longer matches"))
        );
    }

    #[test]
    fn missing_output_and_nonzero_exit_fail_closed() {
        let root = TestDir::new();
        let (mut receipt, expected) = fixture(&root.0);
        fs::remove_file(root.0.join("outputs/result.json")).unwrap();
        receipt.process.exit_status = Some(3);
        receipt.status = ReceiptStatus::Failed;
        let check = verify_receipt(&receipt, &root.0, &expected).unwrap();
        assert_eq!(check.state, ReceiptCheckState::Failed);
        assert!(
            check
                .issues
                .iter()
                .any(|issue| issue.contains("exited with status 3"))
        );
        assert!(
            check
                .issues
                .iter()
                .any(|issue| issue.contains("is missing from the workspace"))
        );
    }

    #[test]
    fn wrong_capability_and_edited_invocation_are_detected() {
        let root = TestDir::new();
        let (mut receipt, expected) = fixture(&root.0);
        receipt.capability.executable_sha256 = digest(b"other-binary");
        receipt.invocation.arguments.push("--extra".into());
        let check = verify_receipt(&receipt, &root.0, &expected).unwrap();
        assert_eq!(check.state, ReceiptCheckState::Failed);
        assert!(!check.invocation_identity_reproduced);
        assert!(
            check
                .issues
                .iter()
                .any(|issue| issue.contains("differs from the package's"))
        );
    }

    #[test]
    fn substituted_input_is_detected() {
        let root = TestDir::new();
        let (receipt, mut expected) = fixture(&root.0);
        expected.inputs.get_mut("a").unwrap().sha256 = digest(b"the-bound-plan-said-this");
        let check = verify_receipt(&receipt, &root.0, &expected).unwrap();
        assert_eq!(check.state, ReceiptCheckState::Failed);
        assert!(
            check
                .issues
                .iter()
                .any(|issue| issue.contains("was staged with digest"))
        );
    }
}
