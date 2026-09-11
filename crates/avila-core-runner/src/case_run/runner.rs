//! The execution engine: staging, planning, reuse, receipts, and promotion.
//!
//! `Runner` carries the verified package, the compiled contract, the resolved
//! adapter, and the mutable run state (staged inputs, fresh outputs, committed
//! receipts). `run_step` is the controlled boundary: it stages each declared
//! input from a trusted root or a previous step's output, plans the
//! invocation, classifies the committed receipt's differences by SC-12 change
//! class and reuses it when nothing changed, or executes fresh and promotes
//! outputs only after the receipt verifies. Every state transition is
//! recorded on the step report; a later step never runs over the output of a
//! failed or invalidated earlier one.

use super::replay::changes_since;
use super::stderr::{DiagnosticStderrFeedback, read_diagnostic_stderr};
use super::*;

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

pub(super) struct Runner<'a> {
    package: &'a VerifiedCasePackage,
    compiled: &'a CompiledContract,
    options: &'a CaseRunOptions,
    committed_claims: &'a Value,
    artifact_checks: BTreeMap<String, &'a ArtifactCheck>,
    canonical_roots: BTreeMap<String, PathBuf>,
    fresh_outputs: BTreeMap<(String, String), FreshOutput>,
    pub(super) workspace: Option<PathBuf>,
    pub(super) claims: Vec<GeneratedClaim>,
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
    pub(super) fn new(
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

    pub(super) fn run_all(&mut self) -> Result<ExecutionReport, Box<dyn Error>> {
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
                    absent_slots: Vec::new(),
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
            absent_slots: Vec::new(),
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
                    let required_slots: BTreeSet<&str> =
                        adapter.required_output_slots().into_iter().collect();
                    if !required_slots.is_subset(&extracted_slots)
                        || !extracted_slots.is_subset(&adapter_slots)
                    {
                        report.add_finding(
                            CORE_X2801,
                            FindingClass::Invalid,
                            RunStage::ClaimGeneration,
                            "adapter_owner",
                            SourceLocation::new("case-run", "/execution"),
                            format!(
                                "adapter extracted claims for {:?}, but requires {:?} and declares {:?}",
                                extracted_slots, required_slots, adapter_slots
                            ),
                        );
                        report.state = StepExecutionState::Failed;
                        return Ok(report);
                    }
                    report.absent_slots = adapter
                        .optional_output_slots()
                        .into_iter()
                        .filter(|slot| !extracted_slots.contains(slot))
                        .map(str::to_string)
                        .collect();
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
            let required_slots: BTreeSet<&str> =
                adapter.required_output_slots().into_iter().collect();
            if !extracted.is_empty()
                && (!required_slots.is_subset(&extracted_slots)
                    || !extracted_slots.is_subset(&adapter_slots))
            {
                report.add_finding(
                    CORE_X2801,
                    FindingClass::Invalid,
                    RunStage::ClaimGeneration,
                    "adapter_owner",
                    SourceLocation::new("case-run", "/execution"),
                    format!(
                        "adapter extracted claims for {:?}, but requires {:?} and declares {:?}",
                        extracted_slots, required_slots, adapter_slots
                    ),
                );
            }
            report.absent_slots = adapter
                .optional_output_slots()
                .into_iter()
                .filter(|slot| !extracted_slots.contains(slot))
                .map(str::to_string)
                .collect();
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
