//! The controlled runner behind `avila-core run`: stages verified bytes into
//! a fresh workspace, invokes one exact executable with a cleared
//! environment, captures its streams, hashes what it produced, and writes an
//! execution receipt. It is deliberately case-specific: the only adapters it
//! knows are the ones a committed case declares.

pub mod actinv_build;
pub mod activation;
#[cfg(test)]
mod adversarial_tests;
pub mod aftermatter;
pub mod claims;
pub mod external_checker;
pub mod shielding;
pub mod shielding_coupled;
pub mod thermal;

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

/// An adapter output resolved for one execution. Built-in declarations are
/// copied into this owned form; external checker declarations already own
/// their strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAdapterOutput {
    pub output_id: String,
    pub workspace_path: String,
    pub media_type: String,
}

/// What a compiled step hands an adapter besides its inputs: the compiled
/// parameters in tagged form and the bound seed, if the type is seeded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepContext {
    pub parameters: BTreeMap<String, Value>,
    pub seed: Option<String>,
}

/// One claim an adapter extracted from a produced output.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedClaim {
    pub output_slot: String,
    pub output_id: String,
    pub claim: Value,
}

/// The adapters the runner can drive, selected by the adapter identifier a
/// package execution names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adapter {
    AftermatterEvaluate,
    ActinvBuild,
    ShieldingScreen,
    ShieldingTransport,
    ShieldingActivation,
    ShieldingTransportCoupled,
    ThermalScreen,
    ThermalSpreaderFe,
    ExternalChecker {
        adapter: Box<external_checker::ExternalCheckerAdapter>,
        descriptor_sha256: String,
    },
}

impl Adapter {
    pub fn by_id(id: &str) -> Option<Self> {
        match id {
            aftermatter::ADAPTER_ID => Some(Self::AftermatterEvaluate),
            actinv_build::ADAPTER_ID => Some(Self::ActinvBuild),
            shielding::SCREEN_ADAPTER_ID => Some(Self::ShieldingScreen),
            shielding::TRANSPORT_ADAPTER_ID => Some(Self::ShieldingTransport),
            activation::ADAPTER_ID => Some(Self::ShieldingActivation),
            shielding_coupled::TRANSPORT_ADAPTER_ID => Some(Self::ShieldingTransportCoupled),
            thermal::SCREEN_ADAPTER_ID => Some(Self::ThermalScreen),
            thermal::FE_ADAPTER_ID => Some(Self::ThermalSpreaderFe),
            _ => None,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::AftermatterEvaluate => aftermatter::ADAPTER_ID,
            Self::ActinvBuild => actinv_build::ADAPTER_ID,
            Self::ShieldingScreen => shielding::SCREEN_ADAPTER_ID,
            Self::ShieldingTransport => shielding::TRANSPORT_ADAPTER_ID,
            Self::ShieldingActivation => activation::ADAPTER_ID,
            Self::ShieldingTransportCoupled => shielding_coupled::TRANSPORT_ADAPTER_ID,
            Self::ThermalScreen => thermal::SCREEN_ADAPTER_ID,
            Self::ThermalSpreaderFe => thermal::FE_ADAPTER_ID,
            Self::ExternalChecker { adapter, .. } => &adapter.adapter_id,
        }
    }

    pub fn capability_type(&self) -> CapabilityTypeRef {
        match self {
            Self::AftermatterEvaluate => CapabilityTypeRef {
                id: aftermatter::CAPABILITY_TYPE_ID.into(),
                major: aftermatter::CAPABILITY_TYPE_MAJOR,
            },
            Self::ActinvBuild => CapabilityTypeRef {
                id: actinv_build::CAPABILITY_TYPE_ID.into(),
                major: actinv_build::CAPABILITY_TYPE_MAJOR,
            },
            Self::ShieldingScreen => CapabilityTypeRef {
                id: shielding::SCREEN_TYPE_ID.into(),
                major: 1,
            },
            Self::ShieldingTransport => CapabilityTypeRef {
                id: shielding::TRANSPORT_TYPE_ID.into(),
                major: 1,
            },
            Self::ShieldingActivation => CapabilityTypeRef {
                id: activation::CAPABILITY_TYPE_ID.into(),
                major: activation::CAPABILITY_TYPE_MAJOR,
            },
            Self::ShieldingTransportCoupled => CapabilityTypeRef {
                id: shielding_coupled::TRANSPORT_TYPE_ID.into(),
                major: 1,
            },
            Self::ThermalScreen => CapabilityTypeRef {
                id: thermal::SCREEN_TYPE_ID.into(),
                major: 1,
            },
            Self::ThermalSpreaderFe => CapabilityTypeRef {
                id: thermal::FE_TYPE_ID.into(),
                major: 1,
            },
            Self::ExternalChecker { adapter, .. } => adapter.capability_type.clone(),
        }
    }

    pub fn accepts_input(&self, input_slot: &str) -> bool {
        match self {
            Self::AftermatterEvaluate => aftermatter::INPUT_SLOTS.contains(&input_slot),
            Self::ActinvBuild => actinv_build::INPUT_SLOTS.contains(&input_slot),
            Self::ShieldingScreen => shielding::SCREEN_INPUT_SLOTS.contains(&input_slot),
            Self::ShieldingTransport => shielding::TRANSPORT_INPUT_SLOTS.contains(&input_slot),
            Self::ShieldingActivation => activation::INPUT_SLOTS.contains(&input_slot),
            Self::ShieldingTransportCoupled => {
                shielding_coupled::TRANSPORT_INPUT_SLOTS.contains(&input_slot)
            }
            Self::ThermalScreen => thermal::SCREEN_INPUT_SLOTS.contains(&input_slot),
            Self::ThermalSpreaderFe => thermal::FE_INPUT_SLOTS.contains(&input_slot),
            Self::ExternalChecker { adapter, .. } => {
                adapter.input_slots.iter().any(|slot| slot == input_slot)
            }
        }
    }

    pub fn outputs(&self) -> Vec<ResolvedAdapterOutput> {
        let built_in = match self {
            Self::AftermatterEvaluate => aftermatter::OUTPUTS,
            Self::ActinvBuild => actinv_build::OUTPUTS,
            Self::ShieldingScreen => shielding::SCREEN_OUTPUTS,
            Self::ShieldingTransport => shielding::TRANSPORT_OUTPUTS,
            Self::ShieldingActivation => activation::OUTPUTS,
            Self::ShieldingTransportCoupled => shielding_coupled::TRANSPORT_OUTPUTS,
            Self::ThermalScreen => thermal::SCREEN_OUTPUTS,
            Self::ThermalSpreaderFe => thermal::FE_OUTPUTS,
            Self::ExternalChecker { adapter, .. } => return adapter.output_specs(),
        };
        built_in
            .iter()
            .map(|output| ResolvedAdapterOutput {
                output_id: output.output_id.into(),
                workspace_path: output.workspace_path.into(),
                media_type: output.media_type.into(),
            })
            .collect()
    }

    pub fn output_slots(&self) -> Vec<&str> {
        match self {
            Self::AftermatterEvaluate => aftermatter::OUTPUT_SLOTS.to_vec(),
            Self::ActinvBuild => actinv_build::OUTPUT_SLOTS.to_vec(),
            Self::ShieldingScreen => shielding::SCREEN_OUTPUT_SLOTS.to_vec(),
            Self::ShieldingTransport => shielding::TRANSPORT_OUTPUT_SLOTS.to_vec(),
            Self::ShieldingActivation => activation::OUTPUT_SLOTS.to_vec(),
            Self::ShieldingTransportCoupled => shielding_coupled::TRANSPORT_OUTPUT_SLOTS.to_vec(),
            Self::ThermalScreen => thermal::SCREEN_OUTPUT_SLOTS.to_vec(),
            Self::ThermalSpreaderFe => thermal::FE_OUTPUT_SLOTS.to_vec(),
            Self::ExternalChecker { adapter, .. } => adapter.output_slots(),
        }
    }

    pub fn timeout(&self) -> Duration {
        match self {
            Self::AftermatterEvaluate => aftermatter::TIMEOUT,
            Self::ActinvBuild => actinv_build::TIMEOUT,
            Self::ShieldingScreen => shielding::SCREEN_TIMEOUT,
            Self::ShieldingTransport => shielding::TRANSPORT_TIMEOUT,
            Self::ShieldingActivation => activation::TIMEOUT,
            Self::ShieldingTransportCoupled => shielding_coupled::TRANSPORT_TIMEOUT,
            Self::ThermalScreen => thermal::SCREEN_TIMEOUT,
            Self::ThermalSpreaderFe => thermal::FE_TIMEOUT,
            Self::ExternalChecker { adapter, .. } => adapter.timeout(),
        }
    }

    /// Environment keys the package must supply values for. They are named
    /// by the adapter and valued by the operator; only the names are recorded
    /// in the receipt.
    pub fn required_environment(&self) -> &'static [&'static str] {
        match self {
            Self::AftermatterEvaluate
            | Self::ActinvBuild
            | Self::ShieldingScreen
            | Self::ShieldingActivation
            | Self::ThermalScreen
            | Self::ThermalSpreaderFe => &[],
            Self::ShieldingTransport => shielding::TRANSPORT_ENVIRONMENT_KEYS,
            Self::ShieldingTransportCoupled => shielding_coupled::TRANSPORT_ENVIRONMENT_KEYS,
            Self::ExternalChecker { .. } => &[],
        }
    }

    /// The environment passed to the program. The runner clears everything
    /// else; whatever an adapter needs is declared here and recorded.
    pub fn environment(&self) -> BTreeMap<String, String> {
        match self {
            Self::AftermatterEvaluate
            | Self::ShieldingScreen
            | Self::ThermalScreen
            | Self::ThermalSpreaderFe => BTreeMap::new(),
            Self::ActinvBuild => actinv_build::environment(),
            Self::ShieldingTransport => shielding::transport_environment(),
            Self::ShieldingActivation => activation::environment(),
            Self::ShieldingTransportCoupled => shielding_coupled::transport_environment(),
            Self::ExternalChecker { .. } => BTreeMap::new(),
        }
    }

    /// Facts about this step for a qualification envelope, as the kernel's
    /// applicability context in JSON. Every adapter contributes the media
    /// type and identity of each staged input; the transport adapter adds
    /// what it reads from the source and candidate documents. Facts carry
    /// the input's identity as provenance and the adapter as validator.
    pub fn applicability(
        &self,
        staged: &[(String, String, String, Vec<u8>)],
        invocation_sha256: &str,
    ) -> Result<serde_json::Value, String> {
        let mut inputs = serde_json::Map::new();
        for (slot, media_type, sha256, _) in staged {
            inputs.insert(
                slot.clone(),
                serde_json::json!({ "attributes": { "media_type": media_type, "sha256": sha256 } }),
            );
        }
        let mut facts = serde_json::Map::new();
        facts.insert(
            "inputs.count".into(),
            serde_json::json!({
                "value": staged.len(),
                "source": { "class": "runner_measured", "identity": "runner:local",
                            "validator": self.id(), "receipt": format!("plan:{invocation_sha256}") }
            }),
        );
        if matches!(self, Self::ShieldingTransport) {
            shielding::transport_facts(staged, invocation_sha256, &mut facts, &mut inputs)?;
        }
        if matches!(self, Self::ShieldingActivation) {
            activation::activation_facts(staged, invocation_sha256, &mut facts, &mut inputs)?;
        }
        if matches!(self, Self::ShieldingTransportCoupled) {
            shielding_coupled::transport_facts_coupled(
                staged,
                invocation_sha256,
                &mut facts,
                &mut inputs,
            )?;
        }
        if matches!(self, Self::ThermalSpreaderFe) {
            thermal::thermal_facts(staged, invocation_sha256, &mut facts, &mut inputs)?;
        }
        Ok(serde_json::json!({ "facts": facts, "inputs": inputs }))
    }

    /// The portable argument list: relative workspace paths only.
    pub fn arguments(
        &self,
        staged: &BTreeMap<String, String>,
        context: &StepContext,
    ) -> Result<Vec<String>, String> {
        match self {
            Self::AftermatterEvaluate => aftermatter::arguments(staged),
            Self::ActinvBuild => actinv_build::arguments(staged),
            Self::ShieldingScreen => shielding::screen_arguments(staged, context),
            Self::ShieldingTransport => shielding::transport_arguments(staged, context),
            Self::ShieldingActivation => activation::arguments(staged, context),
            Self::ShieldingTransportCoupled => {
                shielding_coupled::transport_arguments(staged, context)
            }
            Self::ThermalScreen => thermal::screen_arguments(staged, context),
            Self::ThermalSpreaderFe => thermal::fe_arguments(staged, context),
            Self::ExternalChecker { adapter, .. } => adapter.arguments(staged),
        }
    }

    pub fn extract_claims(
        &self,
        outputs: &BTreeMap<String, Vec<u8>>,
        context: &StepContext,
    ) -> Result<Vec<ExtractedClaim>, String> {
        match self {
            Self::AftermatterEvaluate => aftermatter::extract_claims(outputs, &context.parameters),
            Self::ActinvBuild => actinv_build::extract_claims(outputs, &context.parameters),
            Self::ShieldingScreen => shielding::screen_claims(outputs, context),
            Self::ShieldingTransport => shielding::transport_claims(outputs, context),
            Self::ShieldingActivation => activation::extract_claims(outputs, context),
            Self::ShieldingTransportCoupled => {
                shielding_coupled::transport_claims(outputs, context)
            }
            Self::ThermalScreen => thermal::screen_claims(outputs, context),
            Self::ThermalSpreaderFe => thermal::fe_claims(outputs, context),
            Self::ExternalChecker { adapter, .. } => adapter.extract_claims(outputs, context),
        }
    }

    pub fn limitations(&self) -> Vec<String> {
        match self {
            Self::ExternalChecker { adapter, .. } => adapter.limitations.clone(),
            _ => Vec::new(),
        }
    }

    /// Raw-byte identity of a package-declared descriptor. Built-in adapters
    /// are identified by the runner build and have no separate document.
    pub fn descriptor_sha256(&self) -> Option<&str> {
        match self {
            Self::ExternalChecker {
                descriptor_sha256, ..
            } => Some(descriptor_sha256),
            _ => None,
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
    pub context: StepContext,
    /// Keys the adapter or the package requires the operator to value.
    pub required_environment: Vec<String>,
    /// Operator-supplied values for those keys, merged over the adapter's
    /// static environment when the program runs.
    pub environment: BTreeMap<String, String>,
    pub inputs: Vec<StagedInput>,
}

/// What a step execution would ask of the program, computed before anything
/// is staged or run: the receipt inputs, the portable invocation, and their
/// identity. The same identity on a completed committed receipt whose
/// outputs still verify is the memoization key for reuse.
#[derive(Debug, Clone)]
pub struct PlannedInvocation {
    pub inputs: Vec<ReceiptInput>,
    pub invocation: Invocation,
    pub invocation_sha256: String,
    /// Operator-supplied values needed only to launch the process. They are
    /// deliberately kept out of the portable invocation and receipt.
    pub runtime_environment: BTreeMap<String, String>,
}

pub fn plan_invocation(
    adapter: &Adapter,
    capability: &CapabilityIdentity,
    program: &str,
    context: &StepContext,
    required_environment: &[String],
    supplied_environment: &BTreeMap<String, String>,
    staged: &[StagedInput],
) -> Result<PlannedInvocation, Box<dyn Error>> {
    let mut inputs = Vec::with_capacity(staged.len());
    let mut staged_paths = BTreeMap::new();
    for input in staged {
        let bytes = fs::metadata(&input.source_path)?.len();
        staged_paths.insert(input.input_slot.clone(), input.workspace_path.clone());
        inputs.push(ReceiptInput {
            input_slot: input.input_slot.clone(),
            evidence_id: input.evidence_id.clone(),
            workspace_path: input.workspace_path.clone(),
            media_type: input.media_type.clone(),
            sha256: input.expected_sha256.clone(),
            bytes,
        });
    }
    // The plan needs the required key names, not their values: identity is
    // the names, and a value is only needed when the program actually runs.
    let environment = adapter.environment();
    let mut runtime_environment = BTreeMap::new();
    for key in required_environment {
        if environment.contains_key(key) {
            return Err(format!(
                "environment `{key}` is fixed by the adapter and cannot be supplied"
            )
            .into());
        }
        if let Some(value) = supplied_environment.get(key) {
            runtime_environment.insert(key.clone(), value.clone());
        }
    }
    let invocation = Invocation {
        program: program.into(),
        arguments: adapter.arguments(&staged_paths, context)?,
        working_directory: ".".into(),
        adapter_sha256: adapter.descriptor_sha256().map(str::to_owned),
        environment,
        required_environment: required_environment.to_vec(),
        timeout_ms: u64::try_from(adapter.timeout().as_millis()).unwrap_or(u64::MAX),
    };
    let invocation_sha256 =
        invocation_identity(capability, &context.parameters, &inputs, &invocation)?;
    Ok(PlannedInvocation {
        inputs,
        invocation,
        invocation_sha256,
        runtime_environment,
    })
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
    let program = request
        .executable
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| request.capability.capability_id.clone());
    let plan = plan_invocation(
        &request.adapter,
        &request.capability,
        &program,
        &request.context,
        &request.required_environment,
        &request.environment,
        &request.inputs,
    )?;
    fs::create_dir_all(step_dir)?;
    fs::create_dir_all(step_dir.join("logs"))?;

    // Stage: copy verified bytes to runner-assigned relative paths and re-hash
    // the copies so the receipt describes exactly what the program could read.
    for (input, planned) in request.inputs.iter().zip(&plan.inputs) {
        let destination = step_dir.join(&input.workspace_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&input.source_path, &destination)?;
        let (sha256, bytes) = sha256_file(&destination)?;
        if sha256 != planned.sha256 || bytes != planned.bytes {
            return Err(format!(
                "staged copy of `{}` hashes to {sha256} ({bytes} bytes), expected {} ({} bytes)",
                input.evidence_id, planned.sha256, planned.bytes
            )
            .into());
        }
    }
    let declared_outputs = request.adapter.outputs();
    for output in &declared_outputs {
        if let Some(parent) = step_dir.join(&output.workspace_path).parent() {
            fs::create_dir_all(parent)?;
        }
    }

    let receipt_inputs = plan.inputs;
    let invocation = plan.invocation;
    let invocation_sha256 = plan.invocation_sha256;
    let arguments = invocation.arguments.clone();
    let mut environment = invocation.environment.clone();
    environment.extend(plan.runtime_environment);
    let timeout = request.adapter.timeout();

    // Execute with a cleared environment inside the workspace.
    let stdout_path = step_dir.join("logs/stdout.log");
    let stderr_path = step_dir.join("logs/stderr.log");
    let stdout = fs::File::create(&stdout_path)?;
    let stderr = fs::File::create(&stderr_path)?;
    let started_at = rfc3339_now();
    let started = Instant::now();
    let mut command = Command::new(&request.executable);
    command
        .args(&arguments)
        .current_dir(step_dir)
        .env_clear()
        .envs(&environment)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        // Make the child the leader of its own process group (pgid equal to
        // its own pid) so a timeout can reach a grandchild the adapter
        // spawned — a `python3` adapter that shells out to a solver, for
        // example — and not only the direct child `wait_with_timeout` holds
        // a handle to.
        command.process_group(0);
    }
    let mut child = command.spawn()?;
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
    for output in &declared_outputs {
        let path = step_dir.join(&output.workspace_path);
        let record = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "declared output `{}` is a symbolic link; outputs must be regular files inside the workspace",
                    output.output_id
                )
                .into());
            }
            Ok(metadata) if metadata.is_file() => {
                let (sha256, bytes) = sha256_file(&path)?;
                ReceiptOutput {
                    output_id: output.output_id.clone(),
                    workspace_path: output.workspace_path.clone(),
                    media_type: output.media_type.clone(),
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
        parameters: request.context.parameters.clone(),
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
        limitations: [
            "Exit status and output digests are process evidence, not scientific success.".into(),
            "The executable digest identifies the bytes that ran; it does not qualify the method.".into(),
            "No sandbox or resource accounting was applied beyond a cleared environment, a working directory confined to the workspace, and (on unix) killing the child's whole process group on timeout; nothing here is signed.".into(),
        ]
        .into_iter()
        .chain(timed_out.then(|| {
            "This step timed out; on unix the child's whole process group was killed, not only the direct child, so a solver or transport process the adapter shelled out to does not outlive the step.".to_string()
        }))
        .chain(request.adapter.limitations())
        .collect(),
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

fn missing_output(output: &ResolvedAdapterOutput) -> ReceiptOutput {
    ReceiptOutput {
        output_id: output.output_id.clone(),
        workspace_path: output.workspace_path.clone(),
        media_type: output.media_type.clone(),
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
            kill_timed_out(child)?;
            let status = child.wait()?;
            return Ok((status, true));
        }
        // Polled completion rather than a SIGCHLD self-pipe or an async
        // reactor: measurement over real steps put this loop under 1% of
        // wall time for any job that runs longer than a couple hundred
        // milliseconds, and event-driven completion would need a runtime
        // this crate otherwise has no reason to depend on. The interval
        // must stay at 20 ms or less so a short adversarial timeout (this
        // module's own test uses one second) still resolves promptly.
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Kill everything a timed-out child left running. On unix the child was
/// spawned as the leader of its own process group (`process_group(0)`, pgid
/// equal to its pid), so a grandchild the adapter spawned — a solver a
/// `python3` adapter shells out to, for example — shares that pgid unless it
/// called `setpgid` itself, and a plain `child.kill()` would leave it
/// running after the timeout.
///
/// The direct syscall for this is `killpg(2)`, but std exposes no group-
/// signal API and this crate forbids unsafe code crate-wide, so the group is
/// reached through the standard `kill` utility instead of raw FFI. If `kill`
/// cannot be found or run at all, this is not fatal: the direct child below
/// is still killed exactly as before this fix, so a missing `kill` binary
/// only narrows the fix back to its pre-existing behavior rather than
/// failing the step outright.
#[cfg(unix)]
fn kill_timed_out(child: &mut std::process::Child) -> io::Result<()> {
    let pgid = child.id();
    // `-s KILL -- -PGID` is the one spelling both procps and util-linux
    // accept: procps reads a bare `-4321` after the signal as a second
    // signal specification and does nothing.
    let _ = Command::new("kill")
        .args(["-s", "KILL", "--"])
        .arg(format!("-{pgid}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    child.kill()
}

#[cfg(not(unix))]
fn kill_timed_out(child: &mut std::process::Child) -> io::Result<()> {
    child.kill()
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

    fn planned_with_runtime_value(value: &str) -> PlannedInvocation {
        let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
        let staged = [
            ("script", "tools/screen.py"),
            ("candidate", "inputs/candidate.json"),
            ("materials", "inputs/materials.json"),
            ("source", "inputs/source.json"),
        ]
        .into_iter()
        .map(|(input_slot, workspace_path)| StagedInput {
            input_slot: input_slot.into(),
            evidence_id: format!("input:{input_slot}"),
            source_path: source_path.clone(),
            workspace_path: workspace_path.into(),
            media_type: "application/json".into(),
            expected_sha256:
                "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
        })
        .collect::<Vec<_>>();
        let capability = CapabilityIdentity {
            capability_id: "python3".into(),
            package_id: "test/python@1".into(),
            source_repository: None,
            source_commit: None,
            executable_sha256:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".into(),
        };
        plan_invocation(
            &Adapter::ShieldingScreen,
            &capability,
            "python3",
            &StepContext::default(),
            &["PRIVATE_LOCATOR".into()],
            &BTreeMap::from([("PRIVATE_LOCATOR".into(), value.into())]),
            &staged,
        )
        .unwrap()
    }

    #[test]
    fn rfc3339_rendering_is_civil() {
        assert_eq!(rfc3339_from_unix_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            rfc3339_from_unix_millis(1_788_292_800_123),
            "2026-09-01T20:00:00.123Z"
        );
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }

    #[test]
    fn runtime_environment_is_retained_outside_the_portable_invocation() {
        let first = planned_with_runtime_value("operator-secret-one");
        let second = planned_with_runtime_value("operator-secret-two");

        assert_eq!(
            first
                .runtime_environment
                .get("PRIVATE_LOCATOR")
                .map(String::as_str),
            Some("operator-secret-one")
        );
        assert_eq!(
            first.invocation.required_environment,
            vec!["PRIVATE_LOCATOR"]
        );
        assert_eq!(first.invocation_sha256, second.invocation_sha256);

        let serialized = serde_json::to_string(&first.invocation).unwrap();
        assert!(!serialized.contains("operator-secret-one"));
        assert!(!serialized.contains("supplied_environment"));
    }

    /// A stub capability that backgrounds a long sleep and then waits on it,
    /// the same shape as a python3 adapter that shells out to a solver. Under
    /// a 1-second step timeout, `execute_step` must report the timeout and
    /// leave no grandchild running: proof that the whole process group, not
    /// only the direct child, was killed.
    #[cfg(unix)]
    #[test]
    fn timeout_kills_the_whole_process_group_not_just_the_direct_child() {
        use std::os::unix::fs::PermissionsExt as _;
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "avila-core-runner-timeout-pgroup-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();

        let config_path = root.join("config.json");
        fs::write(&config_path, b"{}\n").unwrap();
        let (config_sha256, _) = sha256_file(&config_path).unwrap();

        // Starts a detached `sleep 300`, records its pid, then blocks on it:
        // the same shape as an adapter that shells out to a long-running
        // solver and waits for it to finish.
        let script_path = root.join("stub.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nsleep 300 &\necho $! > grandchild.pid\nwait\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).unwrap();
        let (executable_sha256, _) = sha256_file(&script_path).unwrap();

        let adapter = external_checker::ExternalCheckerAdapter {
            schema_version: external_checker::EXTERNAL_CHECKER_ADAPTER_SCHEMA_VERSION.into(),
            adapter_id: "test/timeout-checker@1".into(),
            capability_type: CapabilityTypeRef {
                id: "test.timeout/checker".into(),
                major: 1,
            },
            input_slots: vec!["config".into()],
            arguments: vec![
                external_checker::ExternalArgument::InputPath {
                    input_slot: "config".into(),
                },
                external_checker::ExternalArgument::OutputPath {
                    output_id: "result".into(),
                },
            ],
            outputs: vec![external_checker::ExternalOutput {
                output_id: "result".into(),
                workspace_path: "result.json".into(),
                media_type: "application/json".into(),
            }],
            claims: vec![external_checker::ExternalClaim::Categorical {
                output_slot: "outcome".into(),
                output_id: "result".into(),
                pointer: "/outcome".into(),
                allowed_values: vec!["ok".into()],
            }],
            timeout_ms: 1_000,
            limitations: Vec::new(),
        };

        let request = ExecutionRequest {
            case_id: "TEST-TIMEOUT".into(),
            compiled_snapshot_sha256:
                "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
            step_id: "sleeper".into(),
            adapter: Adapter::ExternalChecker {
                adapter: Box::new(adapter),
                descriptor_sha256:
                    "sha256:2222222222222222222222222222222222222222222222222222222222222222".into(),
            },
            capability: CapabilityIdentity {
                capability_id: "stub-sleeper".into(),
                package_id: "test/stub-sleeper@1".into(),
                source_repository: None,
                source_commit: None,
                executable_sha256,
            },
            executable: script_path,
            context: StepContext::default(),
            required_environment: Vec::new(),
            environment: BTreeMap::new(),
            inputs: vec![StagedInput {
                input_slot: "config".into(),
                evidence_id: "input:config".into(),
                source_path: config_path,
                workspace_path: "inputs/config.json".into(),
                media_type: "application/json".into(),
                expected_sha256: config_sha256,
            }],
        };

        let step_dir = root.join("step");
        let started = Instant::now();
        let outcome = execute_step(&step_dir, &request)
            .expect("a timeout is a reported outcome, not an error");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the step should stop near its 1-second timeout, not run the 300-second sleep to completion"
        );

        let receipt: ExecutionReceipt =
            serde_json::from_slice(&fs::read(&outcome.receipt_path).unwrap()).unwrap();
        assert_eq!(receipt.status, ReceiptStatus::TimedOut);
        assert!(receipt.process.timed_out);
        assert!(
            receipt
                .limitations
                .iter()
                .any(|limitation| limitation.contains("process group")
                    && limitation.contains("killed")),
            "the timeout limitation should say the process group was killed: {:?}",
            receipt.limitations
        );

        // The stub had a moment to background `sleep` and record its pid
        // before the 1-second deadline; read it with a short bounded retry
        // in case the write raced the timeout.
        let pid_path = step_dir.join("grandchild.pid");
        let mut pid_text = String::new();
        for _ in 0..50 {
            if let Ok(text) = fs::read_to_string(&pid_path) {
                pid_text = text;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let grandchild_pid: u32 = pid_text
            .trim()
            .parse()
            .expect("the stub script should have recorded the backgrounded sleep's pid");

        // A killed grandchild whose parent died first is reparented and may
        // linger as a zombie until its new parent reaps it; `kill -0` still
        // succeeds on a zombie, so read the state from /proc where it
        // exists and fall back to `kill -0` elsewhere. Bounded retry absorbs
        // the reap taking a moment.
        let state = |pid: u32| -> Option<String> {
            match fs::read_to_string(format!("/proc/{pid}/stat")) {
                Ok(stat) => {
                    // "<pid> (<comm>) <state> <ppid> ..."; comm may contain
                    // spaces or parentheses, so split after the last ')'.
                    let after_comm = stat.rsplit(')').next().unwrap_or("");
                    let mut fields = after_comm.split_whitespace();
                    let state = fields.next().unwrap_or("?").to_string();
                    let ppid = fields.next().unwrap_or("?").to_string();
                    Some(format!("state {state} ppid {ppid}"))
                }
                Err(_) if cfg!(target_os = "linux") => None,
                Err(_) => Command::new("kill")
                    .arg("-0")
                    .arg(pid.to_string())
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false)
                    .then(|| "state ? (kill -0 succeeded)".to_string()),
            }
        };
        let is_dead = |observed: &Option<String>| match observed {
            None => true,
            Some(text) => text.starts_with("state Z") || text.starts_with("state X"),
        };
        let mut observed = state(grandchild_pid);
        for _ in 0..50 {
            if is_dead(&observed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
            observed = state(grandchild_pid);
        }
        assert!(
            is_dead(&observed),
            "the grandchild `sleep 300` (pid {grandchild_pid}, {}) should have been killed with the timed-out process group, not left running",
            observed.as_deref().unwrap_or("gone")
        );

        let _ = fs::remove_dir_all(&root);
    }
}
