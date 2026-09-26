//! Language-plan execution (EL-03): the `execute` stage of spec §1.
//!
//! For each invocation in an `avila.core/execution-plan/v0.1-draft` plan the
//! runner resolves the declared executable through a caller-supplied map,
//! stages the planned inputs as canonical bytes, spawns the process under a
//! cleared environment with a timeout, collects the declared output, and
//! emits the `avila.core/language-observations/v0.1-draft` document —
//! per-site input/output digests and the receipt — that `evaluate` binds
//! by re-derivation. The runner establishes process facts only; the
//! declared postcondition check and the requirement verdict belong to
//! `avila_core_compiler::language::evaluate_program`.
//!
//! Executable contract: `<executable> <inputs_dir> <output_path>` — the
//! process reads the staged input documents and writes the output document
//! (`avila.core` `ValueDecl`) as canonical JSON. A non-canonical or absent
//! output file is a missing observation, never a staged claim.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use avila_core_compiler::language::{
    ExecutionPlanDocument, LanguageInvocation, LanguageReceipt, OBSERVATIONS_SCHEMA_VERSION,
    ObservationRecord, ObservationsDocument, PlannedInvocation, ProcessOutcome,
    RECEIPT_SCHEMA_VERSION, ReceiptInput, ReceiptLog, ReceiptOutput, RunnerIdentity, ValueDecl,
    invocation_identity,
};
use avila_core_evidence::sha256_file;
use sha2::{Digest, Sha256};

use crate::execute::{rfc3339_now, signal_of, spawn_step, wait_with_timeout};

/// How a `synthetic/<name>@<rev>` (or any declared) executable resolves.
pub struct LanguageExecuteOptions {
    /// Declared executable identity → the path to spawn.
    pub executables: BTreeMap<String, PathBuf>,
    /// Per-invocation spawn timeout.
    pub timeout: Duration,
}

/// Execute every runnable invocation of a `ready` plan. A site whose inputs
/// cannot all be staged produces no observation — the honest absence that
/// leaves its obligations open at `evaluate`.
pub fn execute_plan(
    plan: &ExecutionPlanDocument,
    workdir: &Path,
    options: &LanguageExecuteOptions,
) -> Result<ObservationsDocument, Box<dyn Error>> {
    let plan_sha256 = plan.plan_sha256.clone().unwrap_or_default();
    let mut observations = Vec::new();
    // binding name → the observed output bytes + digest, for chained
    // invocations whose operand is an earlier invocation's result.
    let mut observed_outputs: BTreeMap<String, (Vec<u8>, String)> = BTreeMap::new();
    for invocation in &plan.invocations {
        if let Some(record) = execute_invocation(
            plan_sha256.as_str(),
            invocation,
            workdir,
            options,
            &observed_outputs,
        )? {
            if let (Some(value), Some(digest)) = (&record.output, record.output_sha256.clone()) {
                let bytes = canonical_bytes_of(value)?;
                observed_outputs.insert(record.bind.clone(), (bytes, digest));
            }
            observations.push(record);
        }
    }
    let mut doc = ObservationsDocument {
        schema_version: OBSERVATIONS_SCHEMA_VERSION.into(),
        profile: "avila.core/language/0.1-draft".into(),
        plan_sha256,
        observations,
        observations_sha256: None,
    };
    doc.observations_sha256 = Some(sha256_text(&canonical_bytes_of(&doc)?));
    Ok(doc)
}

/// Canonical bytes of a serializable document — the bytes on disk and the
/// bytes digests cover are the same canonical form. Absent optional fields
/// serialize as `null`; the canonical grammar carries only present fields,
/// so nulls are stripped first.
fn canonical_bytes_of<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, Box<dyn Error>> {
    fn strip_nulls(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.retain(|_, v| !v.is_null());
                map.values_mut().for_each(strip_nulls);
            }
            serde_json::Value::Array(items) => {
                items.iter_mut().for_each(strip_nulls);
            }
            _ => {}
        }
    }
    let mut json = serde_json::to_value(value)?;
    strip_nulls(&mut json);
    let bytes = serde_json::to_vec(&json)?;
    Ok(avila_core_kernel::canonicalize_json(&bytes)?)
}

fn sha256_text(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Execute one invocation. Returns `None` when a planned input is
/// `unstaged` (the operand has no value) or the executable has no mapping —
/// no observation is fabricated for a process that cannot lawfully run.
fn execute_invocation(
    plan_sha256: &str,
    invocation: &PlannedInvocation,
    workdir: &Path,
    options: &LanguageExecuteOptions,
    observed_outputs: &BTreeMap<String, (Vec<u8>, String)>,
) -> Result<Option<ObservationRecord>, Box<dyn Error>> {
    let site = invocation.at.replace(['[', ']'], "_").replace('/', "_");
    let step_dir = workdir.join("invocations").join(&site);
    let inputs_dir = step_dir.join("inputs");
    fs::create_dir_all(&inputs_dir)?;

    // Stage inputs: declared bytes for authored operands, the observed
    // upstream bytes for a chained operand — the digest must cover what
    // the process actually read.
    let mut staged_digests: BTreeMap<String, String> = BTreeMap::new();
    let mut receipt_inputs = Vec::new();
    for (slot, input) in &invocation.inputs {
        if input.state != "staged" {
            // The plan itself declares this operand unstageable — the
            // invocation cannot run.
            return Ok(None);
        }
        let (bytes, _) = match observed_outputs.get(&input.binding) {
            // A chained operand stages the observed upstream output.
            Some((bytes, digest)) => (bytes.clone(), digest.clone()),
            None => {
                let Some(value) = &input.value else {
                    return Ok(None);
                };
                (canonical_bytes_of(value)?, String::new())
            }
        };
        // The staged bytes must match the plan's declared digest — either
        // the declared operand bytes or a bound upstream observation.
        let declared = input.sha256.clone().unwrap_or_default();
        let digest = sha256_text(&bytes);
        if !declared.is_empty()
            && digest != declared
            && !observed_outputs.contains_key(&input.binding)
        {
            return Err(format!(
                "plan input `{slot}` of {} declares {declared} but stages to {digest}",
                invocation.at
            )
            .into());
        }
        let workspace_path = format!("inputs/{slot}.json");
        fs::write(inputs_dir.join(format!("{slot}.json")), &bytes)?;
        staged_digests.insert(slot.clone(), digest.clone());
        receipt_inputs.push(ReceiptInput {
            slot: slot.clone(),
            workspace_path,
            sha256: digest,
            bytes: bytes.len() as u64,
        });
    }

    let Some(executable_path) = options.executables.get(&invocation.executable) else {
        // No supplied executable for this method — the observation is
        // honestly absent rather than synthesized.
        return Ok(None);
    };
    // The child's working directory is the step workspace — the executable
    // path must be absolute so it resolves outside it.
    let executable_path = executable_path
        .canonicalize()
        .unwrap_or_else(|_| executable_path.clone());
    let (executable_sha256, _) = sha256_file(&executable_path)?;

    let outputs_dir = step_dir.join("outputs");
    let logs_dir = step_dir.join("logs");
    fs::create_dir_all(&outputs_dir)?;
    fs::create_dir_all(&logs_dir)?;
    let output_path = outputs_dir.join("output.json");

    let arguments = vec!["inputs".to_string(), "outputs/output.json".to_string()];
    let started_at = rfc3339_now();
    let started = Instant::now();
    let stdout = fs::File::create(logs_dir.join("stdout.log"))?;
    let stderr = fs::File::create(logs_dir.join("stderr.log"))?;
    // Cleared environment plus the minimal `PATH` a shebang interpreter
    // (`#!/usr/bin/env …`) resolves through — deterministic, not inherited.
    let environment = BTreeMap::from([(
        "PATH".to_string(),
        "/usr/local/bin:/usr/bin:/bin".to_string(),
    )]);
    let mut command = Command::new(executable_path);
    command
        .args(&arguments)
        .current_dir(step_dir)
        .env_clear()
        .envs(&environment)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let mut child = spawn_step(command)?;
    let (status, timed_out) = wait_with_timeout(child.as_mut(), options.timeout)?;
    drop(child);
    let duration = started.elapsed();
    let finished_at = rfc3339_now();
    let process = ProcessOutcome {
        started_at,
        finished_at,
        duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        exit_status: status.code(),
        signal: signal_of(&status),
        timed_out,
    };

    let mut logs = Vec::new();
    for stream in ["stdout", "stderr"] {
        let path = logs_dir.join(format!("{stream}.log"));
        let (sha256, bytes) = sha256_file(&path)?;
        logs.push(ReceiptLog {
            stream: stream.into(),
            workspace_path: format!("logs/{stream}.log"),
            sha256,
            bytes,
        });
    }

    // Collect the declared output: present, a regular file, and a valid
    // ValueDecl — the digest covers its canonical content. Anything else
    // is a missing observation.
    let mut output_decl: Option<ValueDecl> = None;
    let mut output_digest = None;
    let output_state = match fs::symlink_metadata(&output_path) {
        Ok(metadata) if metadata.is_file() => {
            let bytes = fs::read(&output_path)?;
            match serde_json::from_slice::<ValueDecl>(&bytes)
                .ok()
                .and_then(|decl| canonical_bytes_of(&decl).ok().map(|c| (decl, c)))
            {
                Some((decl, canonical)) => {
                    output_digest = Some(sha256_text(&canonical));
                    output_decl = Some(decl);
                    "collected"
                }
                _ => "missing",
            }
        }
        _ => "missing",
    };
    let receipt_outputs = vec![ReceiptOutput {
        output_id: "output".into(),
        workspace_path: "outputs/output.json".into(),
        state: output_state.into(),
        sha256: output_digest.clone(),
        bytes: if output_state == "collected" {
            fs::metadata(&output_path).map(|m| m.len()).ok()
        } else {
            None
        },
    }];

    let status = if timed_out {
        "timed_out"
    } else if status.code() == Some(0) && output_state == "collected" {
        "completed"
    } else {
        "failed"
    };

    let mut receipt = LanguageReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION.into(),
        plan_sha256: plan_sha256.into(),
        at: invocation.at.clone(),
        executable: invocation.executable.clone(),
        executable_sha256: executable_sha256.clone(),
        inputs: receipt_inputs,
        invocation: LanguageInvocation {
            arguments: arguments.clone(),
            working_directory: format!("invocations/{site}"),
            environment: environment.clone(),
            timeout_ms: u64::try_from(options.timeout.as_millis()).unwrap_or(u64::MAX),
        },
        invocation_sha256: String::new(),
        process,
        logs,
        outputs: receipt_outputs,
        runner: RunnerIdentity {
            runner: "avila-core-runner/language".into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
        },
        status: status.into(),
        limitations: [
            "Exit status and output digests are process evidence, not scientific success.".into(),
            "The executable digest identifies the bytes that ran; it does not qualify the method.".into(),
            "No sandbox or resource accounting was applied beyond a cleared environment, a working directory confined to the workspace, and terminating the child's process group or Job Object on timeout.".into(),
        ]
        .into(),
    };
    receipt.invocation_sha256 = invocation_identity(&receipt).unwrap_or_default();
    let receipt_bytes = canonical_bytes_of(&receipt)?;
    Ok(Some(ObservationRecord {
        at: invocation.at.clone(),
        bind: invocation.bind.clone(),
        executable: invocation.executable.clone(),
        inputs: staged_digests,
        output_sha256: output_digest,
        output: output_decl,
        receipt_sha256: sha256_text(&receipt_bytes),
        receipt,
    }))
}
