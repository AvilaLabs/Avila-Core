//! Adversarial tests for the execution stage over a synthetic case whose
//! capability is a stub shell script. Every test builds its own package,
//! blesses the documents a first honest run generates, then changes one
//! thing and asserts that the workflow fails closed exactly there.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use avila_core_evidence::{
    IntegrityCheckState, PackageIntegrityStatus, ReceiptCheckState, sha256_file,
};
use avila_core_kernel::VerdictStatus;
use serde_json::{Value, json};

use crate::case_run::{
    BindingStatus, CapabilityCheckState, CaseRunOptions, CaseRunReport, CaseRunStatus, ChangeClass,
    ExecutionStatus, StepExecutionState, execute_case, human_summary,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "avila-core-adversarial-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn workspace(&self) -> PathBuf {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        self.0.join(format!("ws-{sequence}"))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const INPUT_IDS: [&str; 14] = [
    "actinv-activation-library",
    "actinv-library-index",
    "actinv-decay-primary",
    "actinv-decay-fallback",
    "actinv-data-notice",
    "r0-builder",
    "actinv-executable",
    "actinv-dump-helper",
    "fns-spectrum",
    "aftermatter-case",
    "aftermatter-federal-rulepack",
    "aftermatter-clive-rulepack",
    "aftermatter-wcs-rulepack",
    "aftermatter-sources-manifest",
];

fn case_000() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-000-actinv-aftermatter")
}

fn digest(path: &Path) -> String {
    sha256_file(path).unwrap().0
}

fn canned_route_result(fraction_1: &str) -> Vec<u8> {
    let mut text = json!({
        "schema": "aftermatter-route-result-2",
        "checkpoints": [{
            "checkpoint_id": "cool-50y",
            "classification": {
                "table_1": { "boundaries": [
                    { "class_if_qualifies": "A", "fraction": fraction_1, "error_bound": "0.0001" }
                ] },
                "table_2": { "boundaries": [
                    { "class_if_qualifies": "A", "fraction": "0.25", "error_bound": "0.00001" }
                ] }
            }
        }]
    })
    .to_string();
    text.push('\n');
    text.into_bytes()
}

/// A stub "capability": copies an external canned file to `--output`. The
/// canned path is baked in because the runner clears the environment, and
/// reading outside the workspace is exactly the leak the drift tests need.
fn write_copy_stub(path: &Path, canned: &Path) {
    let script = format!(
        "#!/bin/sh\nout=\"\"\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = \"--output\" ]; then out=\"$2\"; shift; fi\n  shift\ndone\n: > \"$out\"\nwhile IFS= read -r line || [ -n \"$line\" ]; do printf '%s\\n' \"$line\" >> \"$out\"; done < \"{}\"\nexit 0\n",
        canned.display()
    );
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn write_failing_stub(path: &Path) {
    fs::write(path, "#!/bin/sh\nexit 3\n").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

struct Synthetic {
    case_dir: PathBuf,
    root: PathBuf,
    stub: PathBuf,
    canned: PathBuf,
}

/// Build a synthetic package around the real CASE-000 contract and registry:
/// stand-in input bytes under one source root, a canned expected output, and
/// an execution bound to `stub`.
fn build_package(dir: &Path, stub: &Path) -> Synthetic {
    build_package_with(dir, stub, None)
}

/// A stub for the activation step: ignores the builder arguments and writes
/// the three fixed outputs the R0 builder layout pins.
fn write_activation_stub(path: &Path, inventory: &[u8]) {
    let inventory = String::from_utf8_lossy(inventory).to_string();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' '{{\"spec\":\"actinv-spec-1\"}}' > cases/r0/input/actinv-problem.json\nprintf '%s\\n' '{inventory}' > cases/r0/reference/actinv-result.json\nprintf '%s\\n' '{{\"schema\":\"aftermatter-decay-metadata-1\"}}' > cases/r0/reference/decay-half-lives.json\nexit 0\n"
    );
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

const STUB_INVENTORY: &str = "{\"schema\":\"aftermatter-inventory-1\",\"stub\":true}";

/// Build the synthetic package; with `activation_stub`, the activation step is
/// declared for execution too, so the two-step chain can be reused, rerun, or
/// planned selectively.
fn build_package_with(dir: &Path, stub: &Path, activation_stub: Option<&Path>) -> Synthetic {
    let case_dir = dir.join("case");
    let root = dir.join("root");
    fs::create_dir_all(case_dir.join("receipts")).unwrap();
    fs::create_dir_all(root.join("inputs")).unwrap();
    for name in [
        "contract.json",
        "registry.json",
        "reviewer-eligibility-policy.md",
    ] {
        fs::copy(case_000().join(name), case_dir.join(name)).unwrap();
    }
    let canned = dir.join("canned-route-result.json");
    if !canned.exists() {
        fs::write(&canned, canned_route_result("0.5")).unwrap();
    }
    fs::copy(&canned, root.join("route-result.json")).unwrap();
    // The recorded activation outputs are exactly what the activation stub
    // writes, so a chain with both executions binds one set of identities.
    fs::write(root.join("inventory.json"), format!("{STUB_INVENTORY}\n")).unwrap();
    fs::write(root.join("problem.json"), b"{\"spec\":\"actinv-spec-1\"}\n").unwrap();
    fs::write(
        root.join("decay.json"),
        b"{\"schema\":\"aftermatter-decay-metadata-1\"}\n",
    )
    .unwrap();

    let contract: Value =
        serde_json::from_slice(&fs::read(case_dir.join("contract.json")).unwrap()).unwrap();
    let media_types: BTreeMap<String, String> = contract["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            (
                input["input_id"].as_str().unwrap().to_string(),
                input["media_type"].as_str().unwrap().to_string(),
            )
        })
        .collect();

    let mut artifacts = Vec::new();
    for input_id in INPUT_IDS {
        let path = root.join("inputs").join(input_id);
        fs::write(&path, format!("stand-in bytes for {input_id}\n")).unwrap();
        artifacts.push(json!({
            "artifact_id": input_id,
            "evidence_ids": [format!("input:{input_id}")],
            "source_root": "stub",
            "path": format!("inputs/{input_id}"),
            "sha256": digest(&path),
        }));
    }
    for (artifact_id, claim_id, file) in [
        ("inventory", "actinv-r0-inventory", "inventory.json"),
        ("problem", "actinv-r0-problem", "problem.json"),
        ("decay", "actinv-r0-decay-metadata", "decay.json"),
    ] {
        artifacts.push(json!({
            "artifact_id": artifact_id,
            "evidence_ids": [claim_id],
            "source_root": "stub",
            "path": file,
            "sha256": digest(&root.join(file)),
        }));
    }
    artifacts.push(json!({
        "artifact_id": "route-result",
        "evidence_ids": [
            "aftermatter-r0-table-1-class-a-fraction",
            "aftermatter-r0-table-2-class-a-fraction",
            "aftermatter-r0-route-result"
        ],
        "source_root": "stub",
        "path": "route-result.json",
        "sha256": digest(&root.join("route-result.json")),
    }));

    // The committed claims document before blessing carries only what is
    // never regenerated here: the recorded activation claims.
    let claims = json!({
        "schema_version": "avila.core/evidence-claims/v0.2-draft",
        "semantic_profile": "avila.core/semantic/0.2-draft",
        "compiled_snapshot_sha256": "sha256:c742b277c45deae6ba9935a1729d254eae22fffc8e4311ac5e30bfcd5c975c95",
        "inputs": INPUT_IDS.iter().map(|id| json!({
            "input_id": id,
            "artifact": { "sha256": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "media_type": media_types[*id] }
        })).collect::<Vec<_>>(),
        "claims": [
            {
                "claim_id": "actinv-r0-problem",
                "step_id": "activation",
                "output_slot": "problem",
                "artifact": { "sha256": digest(&root.join("problem.json")), "media_type": "application/vnd.actinv.problem+json" },
                "claim": { "model": "unquantified" }
            },
            {
                "claim_id": "actinv-r0-inventory",
                "step_id": "activation",
                "output_slot": "inventory",
                "artifact": { "sha256": digest(&root.join("inventory.json")), "media_type": "application/vnd.aftermatter.inventory+json" },
                "claim": { "model": "unquantified" }
            },
            {
                "claim_id": "actinv-r0-decay-metadata",
                "step_id": "activation",
                "output_slot": "decay-metadata",
                "artifact": { "sha256": digest(&root.join("decay.json")), "media_type": "application/vnd.aftermatter.decay-metadata+json" },
                "claim": { "model": "unquantified" }
            }
        ],
        "decisions": []
    });
    fs::write(
        case_dir.join("claims.json"),
        serde_json::to_vec_pretty(&claims).unwrap(),
    )
    .unwrap();

    let mut capabilities = vec![json!({
        "capability_id": "stub",
        "package_id": "test/stub@1",
        "executable_sha256": digest(stub),
    })];
    let mut executions = Vec::new();
    if let Some(activation_stub) = activation_stub {
        capabilities.push(json!({
            "capability_id": "python3",
            "package_id": "test/activation-stub@1",
            "executable_sha256": digest(activation_stub),
        }));
        executions.push(json!({
            "step_id": "activation",
            "adapter": "avila-labs.aftermatter/build-r0-case@1",
            "capability_id": "python3",
            "inputs": [
                { "input_slot": "builder", "workspace_path": "tools/build_r0_case.py" },
                { "input_slot": "actinv-executable", "workspace_path": "tools/actinv" },
                { "input_slot": "actinv-dump-helper", "workspace_path": "tools/dump" },
                { "input_slot": "spectrum", "workspace_path": "cases/r0/input/fns-spectrum.json" },
                { "input_slot": "activation-library", "workspace_path": ".data/actinv/v1.0.0/activation/tendl-2025-neutron-709g.npz" },
                { "input_slot": "library-index", "workspace_path": ".data/actinv/v1.0.0/activation/tendl-2025-neutron-709g_index.json" },
                { "input_slot": "decay-primary", "workspace_path": ".data/actinv/v1.0.0/decay/endf-b-viii-0_decay.dat" },
                { "input_slot": "decay-fallback", "workspace_path": ".data/actinv/v1.0.0/decay/jeff-3-3_decay.dat" },
                { "input_slot": "data-notice", "workspace_path": ".data/actinv/v1.0.0/ACTINV-DATA-NOTICE.md" }
            ],
            "outputs": [
                { "output_slot": "problem", "claim_id": "actinv-r0-problem" },
                { "output_slot": "inventory", "claim_id": "actinv-r0-inventory" },
                { "output_slot": "decay-metadata", "claim_id": "actinv-r0-decay-metadata" }
            ]
        }));
    }
    let package = json!({
        "schema_version": "avila.core/case-package/v0.1-draft",
        "case_id": "CASE-STUB",
        "title": "synthetic execution specimen",
        "documents": [
            { "document_id": "contract", "role": "contract", "path": "contract.json", "sha256": digest(&case_dir.join("contract.json")) },
            { "document_id": "registry", "role": "registry", "path": "registry.json", "sha256": digest(&case_dir.join("registry.json")) },
            { "document_id": "claims", "role": "claims", "path": "claims.json", "sha256": digest(&case_dir.join("claims.json")) },
            { "document_id": "policy", "role": "review_policy", "path": "reviewer-eligibility-policy.md", "sha256": digest(&case_dir.join("reviewer-eligibility-policy.md")) }
        ],
        "artifacts": artifacts,
        "capabilities": capabilities,
        "executions": [{
            "step_id": "classification",
            "adapter": "avila-labs.aftermatter/evaluate@1",
            "capability_id": "stub",
            "inputs": [
                { "input_slot": "inventory", "workspace_path": "in/inventory.json" },
                { "input_slot": "case", "workspace_path": "in/case.json" },
                { "input_slot": "decay-metadata", "workspace_path": "in/decay.json" },
                { "input_slot": "federal-rulepack", "workspace_path": "in/federal.json" },
                { "input_slot": "clive-rulepack", "workspace_path": "in/clive.json" },
                { "input_slot": "wcs-rulepack", "workspace_path": "in/wcs.json" },
                { "input_slot": "sources-manifest", "workspace_path": "in/sources.json" }
            ],
            "outputs": [
                { "output_slot": "table-1-class-a-fraction", "claim_id": "aftermatter-r0-table-1-class-a-fraction" },
                { "output_slot": "table-2-class-a-fraction", "claim_id": "aftermatter-r0-table-2-class-a-fraction" },
                { "output_slot": "route-result", "claim_id": "aftermatter-r0-route-result" }
            ]
        }],
        "limitations": ["synthetic test package"]
    });
    let mut package = package;
    let declared = package["executions"].as_array_mut().unwrap();
    for execution in executions.into_iter().rev() {
        declared.insert(0, execution);
    }
    fs::write(
        case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
    Synthetic {
        case_dir,
        root,
        stub: stub.to_path_buf(),
        canned,
    }
}

fn run_options(synthetic: &Synthetic, workspace: PathBuf) -> CaseRunOptions {
    CaseRunOptions {
        source_roots: BTreeMap::from([("stub".to_string(), synthetic.root.clone())]),
        capabilities: BTreeMap::from([("stub".to_string(), synthetic.stub.clone())]),
        workspace: Some(workspace),
        reuse: false,
        plan_only: false,
        inputs: BTreeMap::new(),
        environment: BTreeMap::new(),
        log: None,
    }
}

fn run(synthetic: &Synthetic, workspace: PathBuf) -> CaseRunReport {
    execute_case(&synthetic.case_dir, &run_options(synthetic, workspace)).unwrap()
}

/// Copy the generated documents of an honest run into the package, as the
/// case author does when freezing expectations.
fn bless(synthetic: &Synthetic, workspace: &Path) {
    let case_dir = &synthetic.case_dir;
    fs::copy(workspace.join("claims.json"), case_dir.join("claims.json")).unwrap();
    fs::copy(
        workspace.join("campaign-report.json"),
        case_dir.join("campaign-report.json"),
    )
    .unwrap();
    fs::copy(
        workspace.join("classification/receipt.json"),
        case_dir.join("receipts/classification.json"),
    )
    .unwrap();
    let activation = workspace.join("activation/receipt.json");
    if activation.is_file() {
        fs::copy(&activation, case_dir.join("receipts/activation.json")).unwrap();
    }
    let mut package: Value =
        serde_json::from_slice(&fs::read(case_dir.join("package.json")).unwrap()).unwrap();
    let documents = package["documents"].as_array_mut().unwrap();
    for document in documents.iter_mut() {
        if document["document_id"] == "claims" {
            document["sha256"] = json!(digest(&case_dir.join("claims.json")));
        }
    }
    documents.retain(|document| {
        document["document_id"] != "expected"
            && document["document_id"] != "receipt"
            && document["document_id"] != "activation-receipt"
    });
    if case_dir.join("receipts/activation.json").is_file() {
        documents.push(json!({
            "document_id": "activation-receipt", "role": "execution_receipt", "path": "receipts/activation.json",
            "sha256": digest(&case_dir.join("receipts/activation.json")), "step_id": "activation"
        }));
    }
    documents.push(json!({
        "document_id": "expected", "role": "expected_campaign_report", "path": "campaign-report.json",
        "sha256": digest(&case_dir.join("campaign-report.json"))
    }));
    documents.push(json!({
        "document_id": "receipt", "role": "execution_receipt", "path": "receipts/classification.json",
        "sha256": digest(&case_dir.join("receipts/classification.json")), "step_id": "classification"
    }));
    fs::write(
        case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
}

fn rehash_document(case_dir: &Path, document_id: &str, path: &str) {
    let mut package: Value =
        serde_json::from_slice(&fs::read(case_dir.join("package.json")).unwrap()).unwrap();
    for document in package["documents"].as_array_mut().unwrap() {
        if document["document_id"] == document_id {
            document["sha256"] = json!(digest(&case_dir.join(path)));
        }
    }
    fs::write(
        case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
}

/// A blessed synthetic case: an honest first run has produced the committed
/// claims, campaign report, and receipt.
fn blessed(dir: &TestDir) -> Synthetic {
    let stub = dir.0.join("stub.sh");
    let canned = dir.0.join("canned-route-result.json");
    fs::write(&canned, canned_route_result("0.5")).unwrap();
    write_copy_stub(&stub, &canned);
    let synthetic = build_package(&dir.0, &stub);
    let workspace = dir.workspace();
    let first = run(&synthetic, workspace.clone());
    assert_eq!(
        first.execution.as_ref().unwrap().status,
        ExecutionStatus::Executed,
        "{}",
        human_summary(&first)
    );
    assert!(!first.claims.as_ref().unwrap().matches_committed);
    assert_eq!(first.status, CaseRunStatus::Rejected);
    bless(&synthetic, &workspace);
    synthetic
}

fn step(report: &CaseRunReport) -> &crate::case_run::StepExecutionReport {
    &report.execution.as_ref().unwrap().steps[0]
}

#[test]
fn honest_execution_generates_claims_and_replays() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let report = run(&synthetic, dir.workspace());
    let summary = human_summary(&report);
    assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
    assert_eq!(report.integrity.status, PackageIntegrityStatus::Complete);
    let executed = step(&report);
    assert_eq!(executed.state, StepExecutionState::Executed);
    assert_eq!(
        executed.capability.as_ref().unwrap().state,
        CapabilityCheckState::Verified
    );
    assert_eq!(executed.inputs.len(), 7);
    assert!(
        executed
            .inputs
            .iter()
            .all(|input| input.integrity == IntegrityCheckState::Verified)
    );
    assert_eq!(
        executed.verification.as_ref().unwrap().state,
        ReceiptCheckState::Verified
    );
    assert_eq!(executed.outputs[0].reproduces_bound_artifact, Some(true));
    assert!(executed.replay.as_ref().unwrap().matches);
    let claims = report.claims.as_ref().unwrap();
    assert!(claims.matches_committed);
    assert_eq!(claims.executed_claims, 3);
    assert_eq!(claims.recorded_claims, 3);
    assert_eq!(claims.input_attestations, 14);
    let campaign = report.campaign.as_ref().unwrap();
    assert!(
        campaign
            .verdicts
            .iter()
            .all(|verdict| { verdict.verdict.rule == "not_evaluated.review_pending" })
    );
    assert!(report.replay.as_ref().unwrap().matches);
    assert!(summary.contains("[EXECUTED] classification via stub"));
    assert!(summary.contains("reproduces the bound artifact"));
    assert!(summary.contains("[MATCH] committed receipt"));

    // The committed claims carry the extracted values and the producer.
    let committed: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("claims.json")).unwrap()).unwrap();
    let table_1 = committed["claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|claim| claim["claim_id"] == "aftermatter-r0-table-1-class-a-fraction")
        .unwrap();
    assert_eq!(table_1["claim"]["lower"]["value"], json!("0.4999"));
    assert_eq!(table_1["claim"]["upper"]["value"], json!("0.5001"));
    assert_eq!(table_1["producer"]["package_id"], json!("test/stub@1"));
}

#[test]
fn without_the_capability_the_recorded_claims_are_evaluated_as_attestations() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let mut options = run_options(&synthetic, dir.workspace());
    options.capabilities.clear();
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(report.status, CaseRunStatus::Evaluated);
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::NotRun
    );
    assert_eq!(
        step(&report).capability.as_ref().unwrap().state,
        CapabilityCheckState::NotSupplied
    );
    assert!(report.claims.as_ref().unwrap().matches_committed);
    assert!(human_summary(&report).contains("[NOT RUN] classification"));
}

#[test]
fn modified_input_bytes_stop_before_execution() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    fs::write(
        synthetic.root.join("inputs/aftermatter-case"),
        b"tampered\n",
    )
    .unwrap();
    let report = run(&synthetic, dir.workspace());
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert_eq!(report.integrity.status, PackageIntegrityStatus::Failed);
    assert!(report.execution.is_none());
    assert!(report.campaign.is_none());
    assert!(human_summary(&report).contains("Mismatch: stub:inputs/aftermatter-case"));
}

#[test]
fn unchecked_input_bytes_refuse_execution() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let mut options = run_options(&synthetic, dir.workspace());
    options.source_roots.clear();
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Refused
    );
    assert!(
        step(&report)
            .issues
            .iter()
            .any(|issue| issue.contains("were not verified"))
    );
    assert!(report.claims.is_none());
    assert!(report.campaign.is_none());
}

#[test]
fn a_different_executable_is_refused_before_it_runs() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let other = dir.0.join("other.sh");
    write_copy_stub(&other, &synthetic.canned);
    fs::write(
        &other,
        format!(
            "{}# not the bound bytes\n",
            fs::read_to_string(&other).unwrap()
        ),
    )
    .unwrap();
    fs::set_permissions(&other, fs::Permissions::from_mode(0o755)).unwrap();
    let mut options = run_options(&synthetic, dir.workspace());
    options.capabilities.insert("stub".into(), other);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Refused
    );
    let executed = step(&report);
    assert_eq!(
        executed.capability.as_ref().unwrap().state,
        CapabilityCheckState::Mismatch
    );
    assert!(
        executed.receipt.is_none(),
        "nothing may run under a wrong identity"
    );
    assert!(report.campaign.is_none());

    let mut options = run_options(&synthetic, dir.workspace());
    options
        .capabilities
        .insert("stub".into(), dir.0.join("does-not-exist"));
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(
        step(&report).capability.as_ref().unwrap().state,
        CapabilityCheckState::Missing
    );
    assert_eq!(report.status, CaseRunStatus::Rejected);
}

#[test]
fn a_failing_execution_produces_a_failed_receipt_and_no_verdict() {
    let dir = TestDir::new();
    let stub = dir.0.join("failing.sh");
    write_failing_stub(&stub);
    let synthetic = build_package(&dir.0, &stub);
    let workspace = dir.workspace();
    let report = run(&synthetic, workspace.clone());
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Failed
    );
    let executed = step(&report);
    assert_eq!(executed.state, StepExecutionState::Failed);
    let receipt = executed.receipt.as_ref().unwrap();
    assert_eq!(receipt.exit_status, Some(3));
    assert!(
        executed
            .issues
            .iter()
            .any(|issue| issue.contains("exited with status 3"))
    );
    assert!(
        executed
            .issues
            .iter()
            .any(|issue| issue.contains("was not produced"))
    );
    assert!(report.claims.is_none());
    assert!(report.campaign.is_none());
    assert!(workspace.join("classification/receipt.json").is_file());
}

#[test]
fn drifting_output_fails_binding_and_receipt_replay() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    // The same executable now reads different bytes from outside its declared
    // inputs: the classic undeclared dependency.
    fs::write(&synthetic.canned, canned_route_result("0.6")).unwrap();
    let report = run(&synthetic, dir.workspace());
    let summary = human_summary(&report);
    assert_eq!(report.status, CaseRunStatus::Rejected, "{summary}");
    let executed = step(&report);
    assert_eq!(executed.state, StepExecutionState::Executed);
    assert_eq!(executed.outputs[0].reproduces_bound_artifact, Some(false));
    let replay = executed.replay.as_ref().unwrap();
    assert!(!replay.matches);
    assert!(
        replay
            .differences
            .iter()
            .any(|difference| difference.starts_with("output `route-result`"))
    );
    assert!(!report.claims.as_ref().unwrap().matches_committed);
    assert_eq!(
        report.bindings.as_ref().unwrap().status,
        BindingStatus::Failed
    );
    assert!(
        report.campaign.is_none(),
        "unbound outputs never reach evaluation"
    );
    assert!(summary.contains("DIFFERS from the bound artifact"));
    assert!(summary.contains("[DRIFT] committed receipt"));
}

#[test]
fn missing_or_edited_committed_receipts_are_detected() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let receipt_path = synthetic.case_dir.join("receipts/classification.json");
    let original = fs::read(&receipt_path).unwrap();

    fs::remove_file(&receipt_path).unwrap();
    let report = run(&synthetic, dir.workspace());
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert_eq!(report.integrity.status, PackageIntegrityStatus::Failed);
    assert!(report.execution.is_none());

    // An edited receipt whose package hash was "helpfully" updated still
    // drifts from what the fresh run produces.
    let mut edited: Value = serde_json::from_slice(&original).unwrap();
    edited["outputs"][0]["sha256"] =
        json!("sha256:1111111111111111111111111111111111111111111111111111111111111111");
    fs::write(&receipt_path, serde_json::to_vec_pretty(&edited).unwrap()).unwrap();
    rehash_document(
        &synthetic.case_dir,
        "receipt",
        "receipts/classification.json",
    );
    let report = run(&synthetic, dir.workspace());
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert_eq!(report.integrity.status, PackageIntegrityStatus::Complete);
    let replay = step(&report).replay.as_ref().unwrap();
    assert!(!replay.matches);
    assert!(report.claims.as_ref().unwrap().matches_committed);
    assert!(
        report.campaign.is_some(),
        "the drift is reported, not hidden behind an earlier gate"
    );
}

#[test]
fn an_adapter_bound_to_the_wrong_step_type_is_refused() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let mut package: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("package.json")).unwrap())
            .unwrap();
    package["executions"][0]["step_id"] = json!("activation");
    package["documents"]
        .as_array_mut()
        .unwrap()
        .retain(|document| document["document_id"] != "receipt");
    fs::write(
        synthetic.case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
    let report = run(&synthetic, dir.workspace());
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Refused
    );
    assert!(
        step(&report)
            .issues
            .iter()
            .any(|issue| issue.contains("compiles to `aftermatter.r0-inventory-build@1`"))
    );
    assert!(report.campaign.is_none());
}

fn reuse_options(synthetic: &Synthetic, workspace: PathBuf) -> CaseRunOptions {
    CaseRunOptions {
        reuse: true,
        ..run_options(synthetic, workspace)
    }
}

/// Replace one stand-in input's bytes and rebind its package identity, as a
/// case author does when an input legitimately changes.
fn change_input(synthetic: &Synthetic, artifact_id: &str, file: &str, bytes: &[u8]) {
    fs::write(synthetic.root.join(file), bytes).unwrap();
    let mut package: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("package.json")).unwrap())
            .unwrap();
    for artifact in package["artifacts"].as_array_mut().unwrap() {
        if artifact["artifact_id"] == artifact_id {
            artifact["sha256"] = json!(digest(&synthetic.root.join(file)));
        }
    }
    fs::write(
        synthetic.case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
}

/// A blessed two-step synthetic chain: both executions committed with
/// receipts from an honest fresh run.
fn blessed_chain(dir: &TestDir) -> Synthetic {
    let stub = dir.0.join("stub.sh");
    let canned = dir.0.join("canned-route-result.json");
    fs::write(&canned, canned_route_result("0.5")).unwrap();
    write_copy_stub(&stub, &canned);
    let activation_stub = dir.0.join("activation.sh");
    write_activation_stub(&activation_stub, STUB_INVENTORY.as_bytes());
    let synthetic = build_package_with(&dir.0, &stub, Some(&activation_stub));
    let workspace = dir.workspace();
    let mut options = run_options(&synthetic, workspace.clone());
    options
        .capabilities
        .insert("python3".into(), activation_stub);
    let first = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(
        first.execution.as_ref().unwrap().status,
        ExecutionStatus::Executed,
        "{}",
        human_summary(&first)
    );
    bless(&synthetic, &workspace);
    synthetic
}

fn chain_options(
    dir: &TestDir,
    synthetic: &Synthetic,
    reuse: bool,
    plan_only: bool,
) -> CaseRunOptions {
    let mut options = run_options(synthetic, dir.workspace());
    options
        .capabilities
        .insert("python3".into(), dir.0.join("activation.sh"));
    options.reuse = reuse;
    options.plan_only = plan_only;
    options
}

#[test]
fn an_unchanged_case_is_reused_without_running_anything() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let workspace = dir.workspace();
    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, workspace.clone()),
    )
    .unwrap();
    let summary = human_summary(&report);
    assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
    let execution = report.execution.as_ref().unwrap();
    assert_eq!(execution.status, ExecutionStatus::Reused);
    assert!(execution.workspace.is_none());
    assert!(!workspace.exists(), "reuse must not create a workspace");
    let reused = step(&report);
    assert_eq!(reused.state, StepExecutionState::Reused);
    assert!(reused.changes.is_empty());
    assert_eq!(reused.reused_receipt.as_deref(), Some("receipt"));
    assert_eq!(reused.outputs[0].reproduces_bound_artifact, Some(true));
    let claims = report.claims.as_ref().unwrap();
    assert_eq!(claims.reused_claims, 3);
    assert_eq!(claims.executed_claims, 0);
    assert!(claims.matches_committed);
    assert!(summary.contains("[REUSED] classification"));

    // Reuse never needs the executable: the receipt and the bound bytes do.
    let mut options = reuse_options(&synthetic, dir.workspace());
    options.capabilities.clear();
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Reused
    );
    assert_eq!(report.status, CaseRunStatus::Evaluated);
}

#[test]
fn a_changed_input_reruns_the_step_and_names_the_change() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    change_input(
        &synthetic,
        "aftermatter-case",
        "inputs/aftermatter-case",
        b"revised case\n",
    );

    // Plan first: what would rerun, and why, without running.
    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .map(|_| ())
    .and_then(|_| {
        let mut options = reuse_options(&synthetic, dir.workspace());
        options.plan_only = true;
        execute_case(&synthetic.case_dir, &options)
    })
    .unwrap();
    let summary = human_summary(&report);
    assert_eq!(report.status, CaseRunStatus::Planned, "{summary}");
    let planned = step(&report);
    assert_eq!(planned.state, StepExecutionState::Planned);
    assert!(planned.changes.iter().any(|change| {
        change.class == ChangeClass::InputBytes && change.detail.contains("slot `case`")
    }));
    assert!(report.claims.is_none() && report.campaign.is_none());
    assert!(summary.contains("would rerun because: InputBytes"));

    // Then run: the step executes, the change is recorded, and the committed
    // claims no longer match because the attested input identity moved.
    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    let executed = step(&report);
    assert_eq!(executed.state, StepExecutionState::Executed, "{summary}");
    assert_eq!(executed.changes.len(), 1);
    assert_eq!(executed.changes[0].class, ChangeClass::InputBytes);
    assert!(!report.claims.as_ref().unwrap().matches_committed);
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert!(summary.contains("rerun because: InputBytes"));
}

#[test]
fn a_requirement_change_reuses_evidence_and_recomputes_verdicts() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let contract_path = synthetic.case_dir.join("contract.json");
    let mut contract: Value = serde_json::from_slice(&fs::read(&contract_path).unwrap()).unwrap();
    for requirement in contract["requirements"].as_array_mut().unwrap() {
        requirement["limit"]["value"] = json!("0.2");
    }
    fs::write(
        &contract_path,
        serde_json::to_vec_pretty(&contract).unwrap(),
    )
    .unwrap();
    rehash_document(&synthetic.case_dir, "contract", "contract.json");

    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Reused,
        "{summary}"
    );
    let campaign = report.campaign.as_ref().unwrap();
    assert!(
        campaign
            .verdicts
            .iter()
            .all(|verdict| { verdict.verdict.status == avila_core_kernel::VerdictStatus::Fail }),
        "{summary}"
    );
    assert!(
        !report.claims.as_ref().unwrap().matches_committed,
        "the snapshot identity moved"
    );
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert!(summary.contains("[FAIL] CASE-000-R1"));
}

#[test]
fn an_edited_receipt_cannot_be_reused_and_the_rerun_drifts_from_it() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let receipt_path = synthetic.case_dir.join("receipts/classification.json");
    let mut edited: Value = serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    edited["outputs"][0]["sha256"] =
        json!("sha256:2222222222222222222222222222222222222222222222222222222222222222");
    fs::write(&receipt_path, serde_json::to_vec_pretty(&edited).unwrap()).unwrap();
    rehash_document(
        &synthetic.case_dir,
        "receipt",
        "receipts/classification.json",
    );

    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let executed = step(&report);
    assert_eq!(executed.state, StepExecutionState::Executed);
    assert!(
        executed
            .changes
            .iter()
            .any(|change| change.class == ChangeClass::OutputsUnavailable)
    );
    assert!(!executed.replay.as_ref().unwrap().matches);
    assert_eq!(report.status, CaseRunStatus::Rejected);
}

#[test]
fn a_two_step_chain_reruns_only_what_a_change_reaches() {
    let dir = TestDir::new();
    let synthetic = blessed_chain(&dir);

    // Unchanged: both reused.
    let report = execute_case(
        &synthetic.case_dir,
        &chain_options(&dir, &synthetic, true, false),
    )
    .unwrap();
    let summary = human_summary(&report);
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Reused,
        "{summary}"
    );
    assert_eq!(report.status, CaseRunStatus::Evaluated);
    assert_eq!(report.claims.as_ref().unwrap().reused_claims, 6);

    // A changed classification input reaches only classification.
    change_input(
        &synthetic,
        "aftermatter-wcs-rulepack",
        "inputs/aftermatter-wcs-rulepack",
        b"revised rulepack\n",
    );
    let report = execute_case(
        &synthetic.case_dir,
        &chain_options(&dir, &synthetic, true, true),
    )
    .unwrap();
    let steps = &report.execution.as_ref().unwrap().steps;
    assert_eq!(steps[0].step_id, "activation");
    assert_eq!(steps[0].state, StepExecutionState::Reused);
    assert_eq!(steps[1].step_id, "classification");
    assert_eq!(steps[1].state, StepExecutionState::Planned);
    assert!(
        steps[1]
            .changes
            .iter()
            .any(|change| change.class == ChangeClass::InputBytes)
    );
    let report = execute_case(
        &synthetic.case_dir,
        &chain_options(&dir, &synthetic, true, false),
    )
    .unwrap();
    let steps = &report.execution.as_ref().unwrap().steps;
    assert_eq!(steps[0].state, StepExecutionState::Reused);
    assert_eq!(steps[1].state, StepExecutionState::Executed);
    assert_eq!(report.claims.as_ref().unwrap().reused_claims, 3);
    assert_eq!(report.claims.as_ref().unwrap().executed_claims, 3);

    // A changed activation input reruns activation; classification receives
    // byte-identical outputs and is reused, because dependency follows content.
    bless(
        &synthetic,
        Path::new(
            report
                .execution
                .as_ref()
                .unwrap()
                .workspace
                .as_ref()
                .unwrap(),
        ),
    );
    change_input(
        &synthetic,
        "fns-spectrum",
        "inputs/fns-spectrum",
        b"revised spectrum\n",
    );
    let report = execute_case(
        &synthetic.case_dir,
        &chain_options(&dir, &synthetic, true, false),
    )
    .unwrap();
    let summary = human_summary(&report);
    let steps = &report.execution.as_ref().unwrap().steps;
    assert_eq!(steps[0].state, StepExecutionState::Executed, "{summary}");
    assert!(steps[0].changes.iter().any(|change| {
        change.class == ChangeClass::InputBytes && change.detail.contains("slot `spectrum`")
    }));
    assert_eq!(steps[1].state, StepExecutionState::Reused, "{summary}");
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Executed
    );
}

/// Declare one package input free, as CASE-001 does for its candidate.
fn declare_free_input(synthetic: &Synthetic, input_id: &str) {
    let mut package: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("package.json")).unwrap())
            .unwrap();
    package["free_inputs"] = json!([input_id]);
    fs::write(
        synthetic.case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_supplied_free_input_reruns_what_it_reaches_and_binds_by_receipt() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_free_input(&synthetic, "aftermatter-case");
    let supplied = dir.0.join("other-case.json");
    fs::write(&supplied, b"another candidate case\n").unwrap();
    let log = dir.0.join("campaign-log.jsonl");
    let mut options = reuse_options(&synthetic, dir.workspace());
    options
        .inputs
        .insert("aftermatter-case".into(), supplied.clone());
    options.log = Some(log.clone());

    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    let summary = human_summary(&report);
    assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
    assert!(!report.replay_applicable);
    assert!(report.replay.is_none());
    assert!(
        report
            .invalidated_steps
            .contains(&"classification".to_string())
    );
    assert_eq!(report.supplied_inputs.len(), 1);
    assert_eq!(report.supplied_inputs[0].input_id, "aftermatter-case");
    assert_eq!(report.supplied_inputs[0].sha256, digest(&supplied));

    // The step reached by the supplied input reran, and the receipt names
    // the input change that forced it.
    let executed = step(&report);
    assert_eq!(executed.state, StepExecutionState::Executed, "{summary}");
    assert!(executed.changes.iter().any(|change| {
        change.class == ChangeClass::InputBytes && change.detail.contains("slot `case`")
    }));
    assert!(
        executed
            .outputs
            .iter()
            .all(|output| output.reproduces_bound_artifact.is_none())
    );

    // Its claims carry the receipt's identities, not the package's reference
    // identities, and nothing committed for it was carried.
    let claims = report.claims.as_ref().unwrap();
    assert!(!claims.matches_committed);
    assert_eq!(claims.reused_claims, 0);
    assert!(claims.executed_claims > 0);
    let bindings = report.bindings.as_ref().unwrap();
    assert_eq!(bindings.status, BindingStatus::Verified, "{summary}");
    assert_eq!(bindings.receipted_evidence_records, claims.executed_claims);
    assert!(report.campaign.is_some());
    assert!(
        summary.contains("supplied: input `aftermatter-case`"),
        "{summary}"
    );
    assert!(summary.contains("NOT APPLICABLE"), "{summary}");

    // One campaign-log line records the run.
    let lines: Vec<Value> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["status"], "evaluated");
    assert_eq!(
        lines[0]["supplied_inputs"][0]["input_id"],
        "aftermatter-case"
    );
    assert_eq!(lines[0]["supplied_inputs"][0]["sha256"], digest(&supplied));
}

#[test]
fn without_the_capability_a_supplied_free_input_withholds_the_committed_claims() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_free_input(&synthetic, "aftermatter-case");
    let supplied = dir.0.join("other-case.json");
    fs::write(&supplied, b"another candidate case\n").unwrap();
    let mut options = reuse_options(&synthetic, dir.workspace());
    options.capabilities.clear();
    options.inputs.insert("aftermatter-case".into(), supplied);

    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    let summary = human_summary(&report);
    assert_eq!(step(&report).state, StepExecutionState::NotRun, "{summary}");
    let claims = report.claims.as_ref().unwrap();
    assert_eq!(
        claims.executed_claims + claims.reused_claims,
        0,
        "{summary}"
    );
    assert!(claims.invalidated_claims > 0);
    assert!(summary.contains("are not carried"), "{summary}");
    let bindings = report.bindings.as_ref().unwrap();
    assert_eq!(bindings.status, BindingStatus::Verified, "{summary}");
    assert_eq!(
        bindings.withheld_evidence_records,
        claims.invalidated_claims
    );
    assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
    // With no evidence for the reached step nothing is evaluated: the
    // reference input's committed results cannot speak for another input.
    let campaign = report.campaign.as_ref().unwrap();
    assert!(!campaign.verdicts.is_empty());
    assert!(
        campaign
            .verdicts
            .iter()
            .all(|verdict| verdict.verdict.status == VerdictStatus::NotEvaluated),
        "{summary}"
    );
}

#[test]
fn an_input_the_package_does_not_declare_free_cannot_be_supplied() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let supplied = dir.0.join("other-case.json");
    fs::write(&supplied, b"another candidate case\n").unwrap();
    let mut options = reuse_options(&synthetic, dir.workspace());
    options.inputs.insert("aftermatter-case".into(), supplied);
    let error = execute_case(&synthetic.case_dir, &options)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not a free input"), "{error}");
}
