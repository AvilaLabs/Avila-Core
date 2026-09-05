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

use avila_core_evidence::signature::{
    self as signature, GeneratedKeyPair, KeyRole, SignatureDocument, SignedDocumentRef, TrustRoot,
    TrustRootEntry,
};
use avila_core_evidence::{
    CasePackageManifest, IntegrityCheckState, PackageDocument, PackageIntegrityStatus,
    ReceiptCheckState, sha256_file, sha256_hex,
};
use avila_core_kernel::VerdictStatus;
use serde_json::{Value, json};

use crate::AttemptLineageRequest;
use crate::case_run::{
    BindingStatus, CapabilityCheckState, CaseRunOptions, CaseRunReport, CaseRunStatus, ChangeClass,
    ExecutionStatus, StepExecutionState, execute_case, human_summary,
};
use crate::diagnostic::{
    CORE_X1001, CORE_X1004, CORE_X1005, CORE_X1201, CORE_X1301, CORE_X2501, CORE_X2601, CORE_X9001,
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
            },
            "routes": [
                { "route_id": "clive-bwf", "state": "unresolved" }
            ]
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
    fs::write(
        path,
        "#!/bin/sh\nprintf '%s\\n' 'model keys differ: missing=[force_model]' >&2\nexit 3\n",
    )
    .unwrap();
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
    for name in ["contract.json", "registry.json"] {
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
            "aftermatter-r0-clive-route-state"
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
        ]
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
            { "document_id": "claims", "role": "claims", "path": "claims.json", "sha256": digest(&case_dir.join("claims.json")) }
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
                { "output_slot": "route-state", "claim_id": "aftermatter-r0-clive-route-state" }
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
        expected_manifest_sha256: None,
        attempt: None,
        hash_cache: None,
        trust_root: None,
        runner_key: None,
    }
}

fn enable_free_input(synthetic: &Synthetic, input_id: &str) {
    let path = synthetic.case_dir.join("package.json");
    let mut package: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    package["free_inputs"] = json!([input_id]);
    fs::write(path, serde_json::to_vec_pretty(&package).unwrap()).unwrap();
}

fn lineage_options(
    synthetic: &Synthetic,
    workspace: PathBuf,
    log: PathBuf,
    candidate: PathBuf,
    attempt_id: &str,
    parent_attempt_id: Option<&str>,
) -> CaseRunOptions {
    let mut options = run_options(synthetic, workspace);
    options.inputs.insert("aftermatter-case".into(), candidate);
    options.log = Some(log);
    options.attempt = Some(AttemptLineageRequest {
        attempt_id: attempt_id.into(),
        parent_attempt_id: parent_attempt_id.map(str::to_owned),
        candidate_input: "aftermatter-case".into(),
    });
    options
}

fn write_lineage_candidate(path: &Path, candidate_id: &str, thickness: &str, note: bool) {
    let mut candidate = json!({
        "schema": "test/design-candidate/v1",
        "candidate_id": candidate_id,
        "design": {
            "material": "steel",
            "thickness": thickness
        }
    });
    if note {
        candidate["note"] = json!("force-balanced repair");
    }
    fs::write(path, serde_json::to_vec_pretty(&candidate).unwrap()).unwrap();
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
            .take(2)
            .all(|verdict| { verdict.verdict.rule == "bounded.lt.within" })
    );
    assert_eq!(
        campaign.verdicts[2].verdict.rule,
        "categorical.equals.mismatch"
    );
    assert_eq!(
        campaign.verdicts[2].verdict.observed_category.as_deref(),
        Some("unresolved")
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
    let route_state = committed["claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|claim| claim["claim_id"] == "aftermatter-r0-clive-route-state")
        .unwrap();
    assert_eq!(route_state["claim"]["value"], json!("unresolved"));
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
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1001)
    );
}

#[test]
fn early_rejections_and_infrastructure_errors_are_both_logged() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    fs::write(
        synthetic.root.join("inputs/aftermatter-case"),
        b"tampered\n",
    )
    .unwrap();
    let log = dir.0.join("attempts/campaign-log.jsonl");
    let mut options = run_options(&synthetic, dir.workspace());
    options.log = Some(log.clone());

    let rejected = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(rejected.status, CaseRunStatus::Rejected);
    assert!(
        rejected
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1001)
    );

    let error = execute_case(&dir.0.join("missing-case"), &options).unwrap_err();
    assert!(error.to_string().contains("No such file"));

    let lines: Vec<Value> = fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0]["schema_version"],
        "avila.core/run-attempt/v0.3-draft"
    );
    assert_eq!(lines[0]["status"], "rejected");
    assert!(
        lines[0]["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == CORE_X1001)
    );
    assert_eq!(lines[1]["status"], "error");
    assert_eq!(lines[1]["findings"][0]["code"], CORE_X9001);
}

#[test]
fn attempt_lineage_derives_typed_changes_and_binds_the_exact_parent_record() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    enable_free_input(&synthetic, "aftermatter-case");
    let root_candidate = dir.0.join("root-candidate.json");
    let child_candidate = dir.0.join("child-candidate.json");
    write_lineage_candidate(&root_candidate, "root-design", "4", false);
    write_lineage_candidate(&child_candidate, "child-design", "6", true);
    let log = dir.0.join("lineage.jsonl");

    let root = execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log.clone(),
            root_candidate,
            "try-001",
            None,
        ),
    )
    .unwrap();
    assert_eq!(
        root.status,
        CaseRunStatus::Evaluated,
        "{}",
        human_summary(&root)
    );
    let root_attempt = root.attempt.as_ref().unwrap();
    assert_eq!(root_attempt.generation, 0);
    assert!(root_attempt.changes.is_empty());

    // A current child remains comparable with a parent written under the
    // preceding run-log envelope. The identity link binds fields and bytes,
    // not an arbitrary minimum envelope version.
    let mut legacy_root: Value =
        serde_json::from_str(fs::read_to_string(&log).unwrap().trim()).unwrap();
    legacy_root["schema_version"] = json!("avila.core/run-attempt/v0.2-draft");
    fs::write(
        &log,
        format!("{}\n", serde_json::to_string(&legacy_root).unwrap()),
    )
    .unwrap();

    let child = execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log.clone(),
            child_candidate,
            "try-002",
            Some("try-001"),
        ),
    )
    .unwrap();
    assert_eq!(
        child.status,
        CaseRunStatus::Evaluated,
        "{}",
        human_summary(&child)
    );
    let child_attempt = child.attempt.as_ref().unwrap();
    assert_eq!(child_attempt.generation, 1);
    assert_eq!(child_attempt.parent_attempt_id.as_deref(), Some("try-001"));
    assert_eq!(
        child_attempt
            .changes
            .iter()
            .map(|change| change.pointer())
            .collect::<Vec<_>>(),
        vec!["/candidate_id", "/design/thickness", "/note"]
    );
    let summary = human_summary(&child);
    assert!(summary.contains("Attempt `try-002` — generation 1, parent `try-001`"));
    assert!(summary.contains("change /design/thickness: \"4\" -> \"6\""));

    let raw = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = raw.lines().collect();
    assert_eq!(lines.len(), 2);
    let entries: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        entries[0]["schema_version"],
        "avila.core/run-attempt/v0.2-draft"
    );
    assert_eq!(
        entries[1]["schema_version"],
        "avila.core/run-attempt/v0.3-draft"
    );
    assert_eq!(
        child.schema_version,
        "avila.core/case-run-report/v0.5-draft"
    );
    let comparison = child.attempt_comparison.as_ref().unwrap();
    assert_eq!(comparison.parent_attempt_id, "try-001");
    assert_eq!(comparison.verdicts_compared, child.margins.len());
    assert_eq!(comparison.unchanged_verdicts, child.margins.len());
    assert!(comparison.verdict_transitions.is_empty());
    let report_json = serde_json::to_value(&child).unwrap();
    assert_eq!(
        report_json["attempt_comparison"]["parent_attempt_id"],
        "try-001"
    );
    assert_eq!(
        entries[1]["attempt_comparison"]["schema_version"],
        "avila.core/attempt-comparison/v0.1-draft"
    );
    assert_eq!(
        entries[1]["attempt_comparison"]["parent_record_sha256"],
        entries[1]["attempt"]["parent_record_sha256"]
    );
    assert_eq!(
        entries[1]["attempt"]["parent_record_sha256"],
        format!("sha256:{}", sha256_hex(lines[0].as_bytes()))
    );
    assert_eq!(entries[1]["attempt"]["changes"][1]["kind"], "replaced");
    assert_eq!(entries[1]["attempt"]["changes"][1]["before"], "4");
    assert_eq!(entries[1]["attempt"]["changes"][1]["after"], "6");
    assert_eq!(
        entries[1]["attempt"]["fixed_manifest_sha256"],
        entries[1]["manifest_sha256"]
    );
    assert_eq!(
        entries[1]["attempt"]["fixed_compiled_snapshot_sha256"],
        entries[1]["compiled_snapshot_sha256"]
    );
}

#[test]
fn attempt_lineage_refuses_changed_goalposts_before_execution() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    enable_free_input(&synthetic, "aftermatter-case");
    let root_candidate = dir.0.join("root-candidate.json");
    let child_candidate = dir.0.join("child-candidate.json");
    write_lineage_candidate(&root_candidate, "root-design", "4", false);
    write_lineage_candidate(&child_candidate, "child-design", "6", false);
    let log = dir.0.join("lineage.jsonl");
    execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log.clone(),
            root_candidate,
            "try-001",
            None,
        ),
    )
    .unwrap();

    let package_path = synthetic.case_dir.join("package.json");
    let mut package: Value = serde_json::from_slice(&fs::read(&package_path).unwrap()).unwrap();
    package["title"] = json!("rewritten goalposts");
    fs::write(&package_path, serde_json::to_vec_pretty(&package).unwrap()).unwrap();

    let child = execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log.clone(),
            child_candidate,
            "try-002",
            Some("try-001"),
        ),
    )
    .unwrap();
    assert_eq!(child.status, CaseRunStatus::Rejected);
    assert!(child.attempt.is_none());
    assert!(
        child.execution.is_none(),
        "lineage fails before any capability runs"
    );
    assert!(child.findings.iter().any(|finding| {
        finding.code == CORE_X1201 && finding.message.contains("changed goalposts")
    }));
    let entries: Vec<Value> = fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1]["attempt_request"]["attempt_id"], "try-002");
    assert!(entries[1].get("attempt").is_none());
}

#[test]
fn attempt_lineage_refuses_a_tampered_candidate_history() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    enable_free_input(&synthetic, "aftermatter-case");
    let root_candidate = dir.0.join("root-candidate.json");
    let child_candidate = dir.0.join("child-candidate.json");
    write_lineage_candidate(&root_candidate, "root-design", "4", false);
    write_lineage_candidate(&child_candidate, "child-design", "6", false);
    let log = dir.0.join("lineage.jsonl");
    execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log.clone(),
            root_candidate,
            "try-001",
            None,
        ),
    )
    .unwrap();

    let mut root_entry: Value =
        serde_json::from_str(fs::read_to_string(&log).unwrap().trim()).unwrap();
    root_entry["attempt"]["candidate_state"]["design"]["thickness"] = json!("999");
    fs::write(
        &log,
        format!("{}\n", serde_json::to_string(&root_entry).unwrap()),
    )
    .unwrap();

    let child = execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log,
            child_candidate,
            "try-002",
            Some("try-001"),
        ),
    )
    .unwrap();
    assert_eq!(child.status, CaseRunStatus::Rejected);
    assert!(child.execution.is_none());
    assert!(child.findings.iter().any(|finding| {
        finding.code == CORE_X1201 && finding.message.contains("candidate state hashes to")
    }));
}

#[test]
fn attempt_lineage_detects_changes_to_a_parent_result_record() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    enable_free_input(&synthetic, "aftermatter-case");
    let root_candidate = dir.0.join("root-candidate.json");
    let child_candidate = dir.0.join("child-candidate.json");
    let grandchild_candidate = dir.0.join("grandchild-candidate.json");
    write_lineage_candidate(&root_candidate, "root-design", "4", false);
    write_lineage_candidate(&child_candidate, "child-design", "6", false);
    write_lineage_candidate(&grandchild_candidate, "grandchild-design", "8", false);
    let log = dir.0.join("lineage.jsonl");
    execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log.clone(),
            root_candidate,
            "try-001",
            None,
        ),
    )
    .unwrap();
    execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log.clone(),
            child_candidate,
            "try-002",
            Some("try-001"),
        ),
    )
    .unwrap();

    let raw = fs::read_to_string(&log).unwrap();
    let mut lines = raw.lines();
    let mut root_entry: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    let child_line = lines.next().unwrap();
    root_entry["status"] = json!("rejected");
    fs::write(
        &log,
        format!(
            "{}\n{child_line}\n",
            serde_json::to_string(&root_entry).unwrap()
        ),
    )
    .unwrap();

    let grandchild = execute_case(
        &synthetic.case_dir,
        &lineage_options(
            &synthetic,
            dir.workspace(),
            log,
            grandchild_candidate,
            "try-003",
            Some("try-002"),
        ),
    )
    .unwrap();
    assert_eq!(grandchild.status, CaseRunStatus::Rejected);
    assert!(grandchild.execution.is_none());
    assert!(grandchild.findings.iter().any(|finding| {
        finding.code == CORE_X1201 && finding.message.contains("log record hashes to")
    }));
}

#[test]
fn attempt_lineage_requires_a_log_before_execution() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    enable_free_input(&synthetic, "aftermatter-case");
    let candidate = dir.0.join("candidate.json");
    write_lineage_candidate(&candidate, "root-design", "4", false);
    let mut options = run_options(&synthetic, dir.workspace());
    options.inputs.insert("aftermatter-case".into(), candidate);
    options.attempt = Some(AttemptLineageRequest {
        attempt_id: "try-001".into(),
        parent_attempt_id: None,
        candidate_input: "aftermatter-case".into(),
    });

    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert!(report.execution.is_none());
    assert!(report.findings.iter().any(|finding| {
        finding.code == CORE_X1201 && finding.message.contains("requires `--log FILE`")
    }));
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
            .findings
            .iter()
            .any(|finding| finding.message.contains("were not verified"))
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
    let log = dir.0.join("campaign-log.jsonl");
    let mut options = run_options(&synthetic, workspace.clone());
    options.log = Some(log.clone());
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
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
            .findings
            .iter()
            .any(|finding| finding.message.contains("exited with status 3"))
    );
    assert!(
        executed
            .findings
            .iter()
            .any(|finding| finding.message.contains("was not produced"))
    );
    let process_finding = executed
        .findings
        .iter()
        .find(|finding| finding.code == CORE_X2501)
        .expect("a failed process must expose its bounded diagnostic feedback");
    assert_eq!(
        process_finding.primary.document,
        "classification/logs/stderr.log"
    );
    assert!(
        process_finding
            .message
            .contains("model keys differ: missing=[force_model]")
    );
    assert!(
        process_finding
            .message
            .contains("untrusted diagnostic data")
    );
    let summary = human_summary(&report);
    assert!(summary.contains("model keys differ: missing=[force_model]"));
    assert!(report.claims.is_none());
    assert!(report.campaign.is_none());
    assert!(workspace.join("classification/receipt.json").is_file());
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X2601)
    );
    let logged: Value = serde_json::from_str(fs::read_to_string(log).unwrap().trim()).unwrap();
    assert_eq!(logged["steps"][0]["receipt"]["exit_status"], 3);
    assert!(
        logged["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == CORE_X2601)
    );
    assert!(
        logged["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| {
                finding["code"] == CORE_X2501
                    && finding["message"]
                        .as_str()
                        .is_some_and(|message| message.contains("model keys differ"))
            })
    );
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
    assert!(step(&report).findings.iter().any(|finding| {
        finding
            .message
            .contains("compiles to `aftermatter.r0-inventory-build@1`")
    }));
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

/// Declares an `input_schema` on the named role in the synthetic registry
/// copy, so a free input filling that role is validated against it before
/// anything is staged or executed.
fn declare_role_input_schema(synthetic: &Synthetic, role_id: &str, schema: Value) {
    let mut registry: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("registry.json")).unwrap())
            .unwrap();
    for role in registry["roles"].as_array_mut().unwrap() {
        if role["role"]["id"] == role_id {
            role["input_schema"] = schema.clone();
        }
    }
    fs::write(
        synthetic.case_dir.join("registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    let mut package: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("package.json")).unwrap())
            .unwrap();
    for document in package["documents"].as_array_mut().unwrap() {
        if document["document_id"] == "registry" {
            document["sha256"] = json!(digest(&synthetic.case_dir.join("registry.json")));
        }
    }
    fs::write(
        synthetic.case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
}

/// The role `aftermatter-case` fills, so its free-input tests below can
/// declare a schema for it without touching the real CASE-000 registry.
const CANDIDATE_ROLE: &str = "aftermatter.project";

fn candidate_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["schema", "layers"],
        "properties": {
            "schema": { "const": "test/candidate/v1" },
            "layers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["material", "thickness_cm"],
                    "properties": {
                        "material": { "type": "string" },
                        "thickness_cm": {
                            "type": "string",
                            "pattern": r"^(?:(?:0|-?[1-9][0-9]*)(?:\.[0-9]*[1-9])?|-?[1-9][0-9]*/[1-9][0-9]*)$"
                        }
                    }
                }
            }
        }
    })
}

#[test]
fn a_malformed_free_input_is_rejected_before_staging() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_free_input(&synthetic, "aftermatter-case");
    declare_role_input_schema(&synthetic, CANDIDATE_ROLE, candidate_schema());
    let supplied = dir.0.join("candidate.json");

    let cases: [(&str, &[u8], &str); 4] = [
        (
            "wrong type",
            br#"{"schema":"test/candidate/v1","layers":"not-an-array"}"#,
            "/layers",
        ),
        (
            "missing layers",
            br#"{"schema":"test/candidate/v1"}"#,
            "/layers",
        ),
        (
            "unknown key",
            br#"{"schema":"test/candidate/v1","layers":[],"extra":true}"#,
            "/extra",
        ),
        (
            "non-canonical number",
            br#"{"schema":"test/candidate/v1","layers":[{"material":"lead","thickness_cm":"01"}]}"#,
            "/layers/0/thickness_cm",
        ),
    ];
    for (label, bytes, pointer) in cases {
        fs::write(&supplied, bytes).unwrap();
        let mut options = reuse_options(&synthetic, dir.workspace());
        options
            .inputs
            .insert("aftermatter-case".into(), supplied.clone());
        let report = execute_case(&synthetic.case_dir, &options).unwrap();
        let summary = human_summary(&report);
        assert_eq!(report.status, CaseRunStatus::Rejected, "{label}: {summary}");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == CORE_X1301 && finding.primary.pointer == pointer),
            "{label}: expected CORE-X1301 at {pointer}; findings: {:?}",
            report.findings
        );
        assert!(
            report.execution.is_none(),
            "{label}: nothing should have been staged or executed; {summary}"
        );
    }
}

#[test]
fn a_well_formed_free_input_satisfying_its_schema_proceeds_unchanged() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_free_input(&synthetic, "aftermatter-case");
    declare_role_input_schema(&synthetic, CANDIDATE_ROLE, candidate_schema());
    let supplied = dir.0.join("candidate.json");
    fs::write(
        &supplied,
        br#"{"schema":"test/candidate/v1","layers":[{"material":"lead","thickness_cm":"5"}]}"#,
    )
    .unwrap();
    let mut options = reuse_options(&synthetic, dir.workspace());
    options
        .inputs
        .insert("aftermatter-case".into(), supplied.clone());
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    let summary = human_summary(&report);
    assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1301),
        "{summary}"
    );
}

/// Give the synthetic package a two-entry requirement set: one entry the
/// contract covers, one it does not.
fn declare_requirement_set(synthetic: &Synthetic, omission: Option<(&str, &str)>) {
    let set = json!({
        "schema_version": "avila.core/requirement-set/v0.1-draft",
        "set_id": "test/activated-metal", "revision": 1, "owner": "test", "title": "Test set",
        "requirements": [
            { "set_requirement_id": "class-a-fraction", "statement": "fraction below one", "kind": "core.dimensionless-ratio",
              "comparison": "less_than", "minimum_basis": "bounded", "omission": "must_state" },
            { "set_requirement_id": "surface-dose-rate", "statement": "contact dose rate", "kind": "nuclear.ambient-dose-equivalent-rate",
              "comparison": "less_than_or_equal", "minimum_basis": "bounded", "omission": "must_state" }
        ]
    });
    fs::write(
        synthetic.case_dir.join("requirement-set.json"),
        serde_json::to_vec_pretty(&set).unwrap(),
    )
    .unwrap();
    let mut package: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("package.json")).unwrap())
            .unwrap();
    package["documents"].as_array_mut().unwrap().push(json!({
        "document_id": "requirement-set", "role": "requirement_set", "path": "requirement-set.json",
        "sha256": digest(&synthetic.case_dir.join("requirement-set.json"))
    }));
    let mut coverage = json!({
        "requirement_set": "requirement-set",
        "mapping": { "class-a-fraction": ["CASE-000-R1", "CASE-000-R2"] }
    });
    if let Some((reason, accepted_by)) = omission {
        coverage["omissions"] = json!([
            { "set_requirement_id": "surface-dose-rate", "reason": reason, "accepted_by": accepted_by }
        ]);
    }
    package["coverage"] = coverage;
    fs::write(
        synthetic.case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_stated_omission_keeps_coverage_complete_and_the_run_evaluates() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_requirement_set(
        &synthetic,
        Some(("no dose capability is bound", "the requirement owner")),
    );
    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    let coverage = report.coverage.as_ref().expect("coverage assessed");
    assert_eq!(
        coverage.status,
        avila_core_compiler::CoverageStatus::Complete,
        "{summary}"
    );
    assert_eq!(
        coverage.count(avila_core_compiler::CoverageState::Covered),
        1
    );
    assert_eq!(
        coverage.count(avila_core_compiler::CoverageState::OmittedStated),
        1
    );
    assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
    assert!(summary.contains("[COMPLETE] 1 covered, 1 omitted with a stated reason"));
    assert!(summary.contains("accepted by the requirement owner"));
}

#[test]
fn an_unstated_omission_stops_the_run_before_anything_executes() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_requirement_set(&synthetic, None);
    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    let coverage = report.coverage.as_ref().expect("coverage assessed");
    assert_eq!(
        coverage.status,
        avila_core_compiler::CoverageStatus::Incomplete,
        "{summary}"
    );
    assert_eq!(
        coverage.count(avila_core_compiler::CoverageState::OmittedUnstated),
        1
    );
    assert_eq!(report.status, CaseRunStatus::Rejected, "{summary}");
    assert!(report.execution.is_none(), "{summary}");
    assert!(report.campaign.is_none());
    assert!(
        summary.contains("[UNSTATED] surface-dose-rate"),
        "{summary}"
    );
}

#[test]
fn a_requirement_set_document_without_a_coverage_declaration_is_refused() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_requirement_set(&synthetic, None);
    let mut package: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("package.json")).unwrap())
            .unwrap();
    package.as_object_mut().unwrap().remove("coverage");
    fs::write(
        synthetic.case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
    let error = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("declares no `coverage`"), "{error}");
}

#[test]
fn case_001_carries_the_library_requirement_set_byte_for_byte() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let library = fs::read(root.join("libraries/shielding/requirement-set.json")).unwrap();
    let case = fs::read(root.join("cases/case-001-shield-search/requirement-set.json")).unwrap();
    assert_eq!(library, case, "the case's copy must be the library's bytes");
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join("cases/case-001-shield-search/package.json")).unwrap(),
    )
    .unwrap();
    let document = manifest["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|document| document["role"] == "requirement_set")
        .expect("requirement_set document");
    assert_eq!(
        document["sha256"],
        json!(digest(
            &root.join("cases/case-001-shield-search/requirement-set.json")
        ))
    );
}

/// Bind a qualification for the synthetic `stub` capability whose envelope
/// is written over the generic facts every adapter reports. `covered_output_slots`
/// mirrors the record field of the same name: `None` covers every output the
/// capability produces.
fn declare_qualification(
    synthetic: &Synthetic,
    media_types: &[&str],
    executable_sha256: Option<&str>,
    covered_output_slots: Option<&[&str]>,
) {
    let package_value: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("package.json")).unwrap())
            .unwrap();
    let bound_sha256 = package_value["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|capability| capability["capability_id"] == "stub")
        .map(|capability| {
            capability["executable_sha256"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .unwrap();
    let mut record = json!({
        "schema_version": "avila.core/qualification/v0.1-draft",
        "qualification_id": "test/stub-classification", "revision": 1, "owner": "test",
        "adapter": "avila-labs.aftermatter/evaluate@1",
        "capability": { "capability_id": "stub", "executable_sha256": executable_sha256.unwrap_or(&bound_sha256) },
        "statement": "the stub is claimed applicable to JSON cases only",
        "scope": { "all": [
            { "input_attribute_in": { "slot": "case", "attribute": "media_type", "values": media_types } },
            { "fact": { "name": "inputs.count", "op": "ge", "value": 1,
                        "source_requirement": { "class": "runner_measured", "validator": "avila-labs.aftermatter/evaluate@1" } } }
        ] }
    });
    if let Some(slots) = covered_output_slots {
        record["covered_output_slots"] = json!(slots);
    }
    fs::write(
        synthetic.case_dir.join("qualification.json"),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();
    let mut package = package_value;
    let documents = package["documents"].as_array_mut().unwrap();
    documents.retain(|document| document["document_id"] != "qualification");
    documents.push(json!({
        "document_id": "qualification", "role": "qualification", "path": "qualification.json",
        "sha256": digest(&synthetic.case_dir.join("qualification.json"))
    }));
    fs::write(
        synthetic.case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
}

#[test]
fn a_run_inside_the_envelope_carries_the_qualification_on_its_claims() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    // The envelope admits exactly the media type the contract declares for
    // the case input; the adapter reports it as an attribute of that slot.
    let contract: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("contract.json")).unwrap())
            .unwrap();
    let media_type = contract["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["input_id"] == "aftermatter-case")
        .map(|input| input["media_type"].as_str().unwrap().to_string())
        .unwrap();
    declare_qualification(&synthetic, &[media_type.as_str()], None, None);
    let workspace = dir.workspace();
    let report = execute_case(
        &synthetic.case_dir,
        &run_options(&synthetic, workspace.clone()),
    )
    .unwrap();
    let summary = human_summary(&report);
    let assessment = step(&report)
        .qualification
        .as_ref()
        .expect("envelope assessed");
    assert_eq!(
        assessment.state,
        avila_core_compiler::EnvelopeState::Inside,
        "{summary}"
    );
    assert!(summary.contains("[INSIDE] 2/2 terms hold"), "{summary}");
    // Every generated claim of the step carries the envelope; the committed
    // claims, written before the qualification was bound, no longer match.
    let claims: Value =
        serde_json::from_slice(&fs::read(workspace.join("claims.json")).unwrap()).unwrap();
    let step_claims: Vec<&Value> = claims["claims"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|claim| claim["step_id"] == "classification")
        .collect();
    assert!(!step_claims.is_empty());
    for claim in step_claims {
        assert_eq!(claim["qualification"]["state"], "inside", "{claim}");
        assert_eq!(
            claim["qualification"]["qualification_id"],
            "test/stub-classification"
        );
    }
    assert!(!report.claims.as_ref().unwrap().matches_committed);
}

#[test]
fn a_run_outside_the_envelope_cannot_establish_a_bounded_requirement() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_qualification(&synthetic, &["text/plain"], None, None);
    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    let assessment = step(&report)
        .qualification
        .as_ref()
        .expect("envelope assessed");
    assert_eq!(
        assessment.state,
        avila_core_compiler::EnvelopeState::Outside,
        "{summary}"
    );
    assert!(summary.contains("[OUTSIDE] 1/2 terms hold"), "{summary}");
    // The qualification failure is carried on every bounded verdict.
    let campaign = report.campaign.as_ref().expect("campaign evaluated");
    for verdict in &campaign.verdicts {
        assert_eq!(
            verdict.verdict.status,
            VerdictStatus::NotEvaluated,
            "{summary}"
        );
        let reasons = serde_json::to_string(&verdict.verdict.reasons).unwrap();
        assert!(reasons.contains("CORE-A4401"), "{reasons}");
        assert!(reasons.contains("outside_qualification"), "{reasons}");
    }
    assert!(summary.contains("because:"), "{summary}");
}

#[test]
fn a_qualification_for_a_different_executable_is_refused() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    declare_qualification(
        &synthetic,
        &["text/plain"],
        Some("sha256:0000000000000000000000000000000000000000000000000000000000000000"),
        None,
    );
    let error = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("covers executable"), "{error}");
}

#[test]
fn a_qualification_scoped_to_one_output_slot_leaves_the_others_unqualified() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);

    // Require a qualification behind every bounded requirement's evidence,
    // so an uncovered claim is visibly refused rather than merely footnoted.
    let contract_path = synthetic.case_dir.join("contract.json");
    let mut contract: Value = serde_json::from_slice(&fs::read(&contract_path).unwrap()).unwrap();
    contract["execution_policy"]["require_qualification"] = json!(true);
    let media_type = contract["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["input_id"] == "aftermatter-case")
        .map(|input| input["media_type"].as_str().unwrap().to_string())
        .unwrap();
    fs::write(
        &contract_path,
        serde_json::to_vec_pretty(&contract).unwrap(),
    )
    .unwrap();
    rehash_document(&synthetic.case_dir, "contract", "contract.json");

    // The classification step produces two bounded claims from one output
    // document, table-1 and table-2; the record covers only table-1.
    declare_qualification(
        &synthetic,
        &[media_type.as_str()],
        None,
        Some(&["table-1-class-a-fraction"]),
    );

    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    let assessment = step(&report)
        .qualification
        .as_ref()
        .expect("envelope assessed");
    assert_eq!(
        assessment.state,
        avila_core_compiler::EnvelopeState::Inside,
        "{summary}"
    );

    // The covered claim carries the envelope; the uncovered one, from the
    // very same step and the very same executed run, carries none at all.
    let evidence_claims = &report.claims.as_ref().unwrap().evidence_claims;
    let claim = |claim_id: &str| -> Value {
        evidence_claims
            .iter()
            .find(|claim| claim["claim_id"] == claim_id)
            .unwrap()
            .clone()
    };
    let table_1 = claim("aftermatter-r0-table-1-class-a-fraction");
    assert_eq!(table_1["qualification"]["state"], "inside", "{table_1}");
    let table_2 = claim("aftermatter-r0-table-2-class-a-fraction");
    assert!(
        table_2.get("qualification").is_none(),
        "an uncovered claim must carry no qualification at all: {table_2}"
    );

    // CASE-000-R1 (table-1) evaluates on the qualified claim; CASE-000-R2
    // (table-2) is refused under `require_qualification` exactly as if no
    // record had been bound for its output at all.
    let campaign = report.campaign.as_ref().expect("campaign evaluated");
    let verdict = |requirement_id: &str| {
        campaign
            .verdicts
            .iter()
            .find(|verdict| verdict.requirement_id == requirement_id)
            .unwrap()
    };
    let r1 = verdict("CASE-000-R1");
    assert_ne!(r1.verdict.status, VerdictStatus::NotEvaluated, "{summary}");
    let r1_reasons = serde_json::to_string(&r1.verdict.reasons).unwrap();
    assert!(!r1_reasons.contains("CORE-A4402"), "{r1_reasons}");

    let r2 = verdict("CASE-000-R2");
    assert_eq!(r2.verdict.status, VerdictStatus::NotEvaluated, "{summary}");
    assert_eq!(r2.verdict.rule, "not_evaluated.unqualified", "{summary}");
    let r2_reasons = serde_json::to_string(&r2.verdict.reasons).unwrap();
    assert!(r2_reasons.contains("CORE-A4402"), "{r2_reasons}");
}

#[test]
fn a_pinned_manifest_refuses_a_rewritten_package_and_the_log_names_identities() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let log = dir.0.join("campaign-log.jsonl");
    let mut options = reuse_options(&synthetic, dir.workspace());
    options.log = Some(log.clone());

    // An honest run: the log line carries the identities it was evaluated under.
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(
        report.status,
        CaseRunStatus::Evaluated,
        "{}",
        human_summary(&report)
    );
    let manifest = report.integrity.manifest_sha256.clone();
    let read_log = || -> Vec<Value> {
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    let lines = read_log();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["manifest_sha256"], manifest);
    assert!(
        lines[0]["compiled_snapshot_sha256"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(
        lines[0]["documents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|document| document["role"] == "contract")
    );

    // Pinned to a different digest: refused before compilation, and logged.
    options.expected_manifest_sha256 = Some(format!("sha256:{}", "0".repeat(64)));
    let refused = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(refused.status, CaseRunStatus::Rejected);
    assert!(refused.compile.is_none());
    assert!(refused.notice.contains("pinned"), "{}", refused.notice);
    let lines = read_log();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["status"], "rejected");
    assert_eq!(lines[1]["manifest_sha256"], manifest);

    // Pinned to the true digest: the run proceeds.
    options.expected_manifest_sha256 = Some(manifest);
    let pinned = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(
        pinned.status,
        CaseRunStatus::Evaluated,
        "{}",
        human_summary(&pinned)
    );
}

// --- SC-12 reuse does not mask a registry, presentation, or qualification edit ---
//
// These three tests share one question: when a step is reused under SC-12
// (its committed receipt still matches the planned invocation, so nothing
// runs), is the reused evidence re-evaluated against whatever the registry,
// contract, and qualification documents say *now*, or does reuse silently
// carry forward the verdict a stale registry or qualification would have
// produced? Reuse only skips re-execution; compilation and campaign
// evaluation always run fresh over the current documents.

/// Narrow the registry so the classification step's two fraction outputs no
/// longer permit the `interval` model its already-admitted claims use.
/// Applied to both the role definition and the two capability-type output
/// slots that reference it, since the compiler requires an output's
/// permitted models to be a subset of its role's.
fn narrow_classification_fraction_to_exact(synthetic: &Synthetic) {
    let registry_path = synthetic.case_dir.join("registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    let exact_only = json!([{ "model": "exact" }]);
    for role in registry["roles"].as_array_mut().unwrap() {
        if role["role"]["id"] == "aftermatter.classification-fraction" {
            role["permitted_claim_models"] = exact_only.clone();
        }
    }
    for capability_type in registry["capability_types"].as_array_mut().unwrap() {
        if let Some(outputs) = capability_type["outputs"].as_array_mut() {
            for output in outputs {
                if output["role"]["id"] == "aftermatter.classification-fraction" {
                    output["permitted_claim_models"] = exact_only.clone();
                }
            }
        }
    }
    fs::write(
        &registry_path,
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    rehash_document(&synthetic.case_dir, "registry", "registry.json");
}

#[test]
fn a_registry_edit_narrowing_a_claim_model_is_not_masked_by_reuse() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    narrow_classification_fraction_to_exact(&synthetic);

    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    // The step itself is reused: a registry edit does not touch invocation
    // identity (capability digest, parameters, staged inputs), so the
    // committed receipt still matches the plan and nothing runs.
    assert_eq!(step(&report).state, StepExecutionState::Reused, "{summary}");
    assert!(summary.contains("[REUSED] classification"), "{summary}");

    // But compilation and campaign evaluation are always fresh: the
    // `interval` claims the reused receipt still carries are no longer
    // permitted by the edited output, so admission quarantines them
    // (CORE-E7201) and both bounded requirements retreat to NOT_EVALUATED.
    let campaign = report.campaign.as_ref().expect("campaign evaluated");
    let bounded: Vec<_> = campaign
        .verdicts
        .iter()
        .filter(|verdict| {
            verdict.requirement_id == "CASE-000-R1" || verdict.requirement_id == "CASE-000-R2"
        })
        .collect();
    assert_eq!(bounded.len(), 2, "{summary}");
    for verdict in bounded {
        assert_eq!(
            verdict.verdict.status,
            VerdictStatus::NotEvaluated,
            "{summary}"
        );
        assert_eq!(
            verdict.verdict.rule, "not_evaluated.quarantined",
            "{summary}"
        );
    }
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "CORE-E7201"
                && finding.message.contains("is not permitted by output")),
        "{:?}",
        report.findings
    );
}

/// Add an optional agent-review step to the synthetic contract and registry,
/// wired the way CASE-001's `practical-review` step is: it presents an
/// upstream output to a connected agent after Core's technical evaluation,
/// is never itself executed, and cannot bind to any requirement. The
/// package also needs a matching `review_policy` document, since the
/// runner's evidence binding refuses a compiled presentation gate whose
/// declared policy digest the package does not contain.
fn add_practical_review_step(synthetic: &Synthetic, instructions: &[&str]) {
    let registry_path = synthetic.case_dir.join("registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    registry["roles"].as_array_mut().unwrap().push(json!({
        "role": { "id": "test.review-decision", "major": 1 },
        "owner": "test",
        "validator": "test.validate.review-decision@1",
        "accepted_media_types": ["application/vnd.test.review-decision+json"],
        "permitted_claim_models": [{ "model": "unquantified" }]
    }));
    registry["capability_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "capability_type": { "id": "test.review", "major": 1 },
            "owner": "test",
            "reproducibility": { "determinism": "deterministic" },
            "inputs": [{
                "slot_id": "result",
                "role": { "id": "aftermatter.classification-fraction", "major": 1 },
                "accepted_media_types": ["application/vnd.aftermatter.route-result+json"]
            }],
            "outputs": [{
                "slot_id": "decision",
                "role": { "id": "test.review-decision", "major": 1 },
                "media_type": "application/vnd.test.review-decision+json",
                "permitted_claim_models": [{ "model": "unquantified" }]
            }],
            "review": {
                "reviewer_role": "agent",
                "presented_input_slots": ["result"],
                "decision_output_slot": "decision",
                "allowed_dispositions": ["present_to_user", "request_changes", "abstain"]
            }
        }));
    fs::write(
        &registry_path,
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    rehash_document(&synthetic.case_dir, "registry", "registry.json");

    let policy_path = synthetic.case_dir.join("review-policy.json");
    fs::write(&policy_path, b"{\"policy\":\"presentation only\"}\n").unwrap();
    let policy_sha256 = digest(&policy_path);

    let contract_path = synthetic.case_dir.join("contract.json");
    let mut contract: Value = serde_json::from_slice(&fs::read(&contract_path).unwrap()).unwrap();
    let workflow = contract["workflow"].as_array_mut().unwrap();
    workflow.retain(|step| step["step_id"] != "review");
    workflow.push(json!({
        "step_id": "review",
        "capability_type": { "id": "test.review", "major": 1 },
        "bindings": [{
            "input_slot": "result",
            "source": {
                "source": "step_output",
                "step_id": "classification",
                "output_slot": "table-1-class-a-fraction"
            }
        }],
        "review": {
            "reviewer_eligibility_policy": {
                "policy_id": "test.org/reviewer-eligibility",
                "revision": 1,
                "sha256": policy_sha256
            },
            "independence": { "mode": "none" },
            "instructions": instructions
        }
    }));
    fs::write(
        &contract_path,
        serde_json::to_vec_pretty(&contract).unwrap(),
    )
    .unwrap();
    rehash_document(&synthetic.case_dir, "contract", "contract.json");

    let mut package: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("package.json")).unwrap())
            .unwrap();
    let documents = package["documents"].as_array_mut().unwrap();
    documents.retain(|document| document["document_id"] != "review-policy");
    documents.push(json!({
        "document_id": "review-policy",
        "role": "review_policy",
        "path": "review-policy.json",
        "sha256": digest(&policy_path)
    }));
    fs::write(
        synthetic.case_dir.join("package.json"),
        serde_json::to_vec_pretty(&package).unwrap(),
    )
    .unwrap();
}

/// Change only the practical instructions shown to the connected agent, the
/// way a case author edits practicality guidance after the fact.
fn edit_review_instructions(synthetic: &Synthetic, instructions: &[&str]) {
    let contract_path = synthetic.case_dir.join("contract.json");
    let mut contract: Value = serde_json::from_slice(&fs::read(&contract_path).unwrap()).unwrap();
    for step in contract["workflow"].as_array_mut().unwrap() {
        if step["step_id"] == "review" {
            step["review"]["instructions"] = json!(instructions);
        }
    }
    fs::write(
        &contract_path,
        serde_json::to_vec_pretty(&contract).unwrap(),
    )
    .unwrap();
    rehash_document(&synthetic.case_dir, "contract", "contract.json");
}

#[test]
fn an_optional_review_edit_changes_nothing_in_technical_verdicts() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    add_practical_review_step(&synthetic, &["baseline: request changes unless every PASS"]);

    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    assert_eq!(step(&report).state, StepExecutionState::Reused, "{summary}");
    let [gate] = report.presentation_gates.as_slice() else {
        panic!("exactly one presentation gate expected: {summary}");
    };
    assert_eq!(
        gate.instructions,
        vec!["baseline: request changes unless every PASS".to_string()]
    );
    let baseline_verdicts: Vec<(String, VerdictStatus)> = report
        .campaign
        .as_ref()
        .unwrap()
        .verdicts
        .iter()
        .map(|verdict| (verdict.requirement_id.clone(), verdict.verdict.status))
        .collect();
    assert!(
        baseline_verdicts
            .iter()
            .any(|(id, status)| id == "CASE-000-R1" && *status == VerdictStatus::Pass),
        "{summary}"
    );

    // Edit only the practical instructions text: a presentation/optional-
    // review concern with no technical content.
    edit_review_instructions(
        &synthetic,
        &["revised: also flag adjacent identical layers"],
    );
    let report = execute_case(
        &synthetic.case_dir,
        &reuse_options(&synthetic, dir.workspace()),
    )
    .unwrap();
    let summary = human_summary(&report);
    // The classification step is still reused: the edit lives entirely in
    // the review step's presentation policy, never touching an execution's
    // invocation identity.
    assert_eq!(step(&report).state, StepExecutionState::Reused, "{summary}");
    let [gate] = report.presentation_gates.as_slice() else {
        panic!("exactly one presentation gate expected: {summary}");
    };
    // The edit is visible exactly where it belongs...
    assert_eq!(
        gate.instructions,
        vec!["revised: also flag adjacent identical layers".to_string()]
    );
    // ...and nowhere else: every technical verdict is byte-for-byte the
    // same as before the edit.
    let edited_verdicts: Vec<(String, VerdictStatus)> = report
        .campaign
        .as_ref()
        .unwrap()
        .verdicts
        .iter()
        .map(|verdict| (verdict.requirement_id.clone(), verdict.verdict.status))
        .collect();
    assert_eq!(baseline_verdicts, edited_verdicts, "{summary}");
}

#[test]
fn a_qualification_edit_narrowing_the_envelope_leaves_a_reused_step_not_evaluated() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let contract: Value =
        serde_json::from_slice(&fs::read(synthetic.case_dir.join("contract.json")).unwrap())
            .unwrap();
    let media_type = contract["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["input_id"] == "aftermatter-case")
        .map(|input| input["media_type"].as_str().unwrap().to_string())
        .unwrap();

    // Bind a qualification whose scope admits the case's actual media type:
    // the reused step's evidence sits inside the envelope, and both bounded
    // requirements pass.
    declare_qualification(&synthetic, &[media_type.as_str()], None, None);
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
    assert_eq!(
        step(&report).qualification.as_ref().unwrap().state,
        avila_core_compiler::EnvelopeState::Inside,
        "{summary}"
    );
    let bounded_before: Vec<_> = report
        .campaign
        .as_ref()
        .unwrap()
        .verdicts
        .iter()
        .filter(|verdict| {
            verdict.requirement_id == "CASE-000-R1" || verdict.requirement_id == "CASE-000-R2"
        })
        .cloned()
        .collect();
    assert_eq!(bounded_before.len(), 2, "{summary}");
    for verdict in &bounded_before {
        assert_eq!(verdict.verdict.status, VerdictStatus::Pass, "{summary}");
    }

    // The method owner narrows the scope so this case's evidence no longer
    // qualifies. The same committed receipt is still reused -- nothing
    // about a qualification record touches invocation identity -- but the
    // envelope now excludes it and both bounded requirements retreat to
    // NOT_EVALUATED.
    declare_qualification(&synthetic, &["text/plain"], None, None);
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
    assert_eq!(
        step(&report).qualification.as_ref().unwrap().state,
        avila_core_compiler::EnvelopeState::Outside,
        "{summary}"
    );
    let campaign = report.campaign.as_ref().unwrap();
    let bounded_after: Vec<_> = campaign
        .verdicts
        .iter()
        .filter(|verdict| {
            verdict.requirement_id == "CASE-000-R1" || verdict.requirement_id == "CASE-000-R2"
        })
        .collect();
    assert_eq!(bounded_after.len(), 2, "{summary}");
    for verdict in bounded_after {
        assert_eq!(
            verdict.verdict.status,
            VerdictStatus::NotEvaluated,
            "{summary}"
        );
        let reasons = serde_json::to_string(&verdict.verdict.reasons).unwrap();
        assert!(reasons.contains("CORE-A4401"), "{reasons}");
        assert!(reasons.contains("outside_qualification"), "{reasons}");
    }
}

// --- ADR-0015: signed manifests and receipts ------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_manifest(case_dir: &Path) -> CasePackageManifest {
    serde_json::from_slice(&fs::read(case_dir.join("package.json")).unwrap()).unwrap()
}

fn write_manifest(case_dir: &Path, manifest: &CasePackageManifest) {
    let mut bytes = serde_json::to_vec_pretty(manifest).unwrap();
    bytes.push(b'\n');
    fs::write(case_dir.join("package.json"), bytes).unwrap();
}

fn trust_root(entries: &[(&GeneratedKeyPair, KeyRole)]) -> TrustRoot {
    TrustRoot {
        schema_version: signature::TRUST_ROOT_SCHEMA_VERSION.into(),
        keys: entries
            .iter()
            .map(|(pair, role)| TrustRootEntry {
                key_id: pair.key_id.clone(),
                public_key_hex: pair.public_key_hex.clone(),
                role: *role,
            })
            .collect(),
    }
}

fn write_trust_root(path: &Path, root: &TrustRoot) {
    fs::write(path, serde_json::to_vec_pretty(root).unwrap()).unwrap();
}

fn write_runner_key_file(path: &Path, pair: &GeneratedKeyPair) {
    fs::write(path, pair.seed).unwrap();
}

fn write_signature_document(case_dir: &Path, relative_path: &str, document: &SignatureDocument) {
    let full_path = case_dir.join(relative_path);
    fs::create_dir_all(full_path.parent().unwrap()).unwrap();
    let mut bytes = serde_json::to_vec_pretty(document).unwrap();
    bytes.push(b'\n');
    fs::write(full_path, bytes).unwrap();
}

/// Bind a `signature` document (built or hand-forged) into the manifest at
/// `relative_path`, rehashing its own entry to match the bytes just
/// written, replacing any prior entry with the same `document_id`.
fn bind_signature_document(
    case_dir: &Path,
    document_id: &str,
    relative_path: &str,
    document: &SignatureDocument,
) {
    write_signature_document(case_dir, relative_path, document);
    let mut manifest = read_manifest(case_dir);
    let sha256 = format!(
        "sha256:{}",
        sha256_hex(fs::read(case_dir.join(relative_path)).unwrap())
    );
    manifest
        .documents
        .retain(|existing| existing.document_id != document_id);
    manifest.documents.push(PackageDocument {
        document_id: document_id.into(),
        role: "signature".into(),
        path: relative_path.into(),
        sha256,
        step_id: None,
    });
    write_manifest(case_dir, &manifest);
}

/// Sign a case package's manifest with `seed`, exactly as `avila-core sign
/// manifest` does: digest the manifest with the not-yet-bound signature
/// entry excluded, sign it, write `signatures/manifest.sig.json`, and bind
/// it into `package.json`. Idempotent: signs whatever is currently on disk,
/// including any other signature already bound (such as a receipt's).
fn sign_manifest(case_dir: &Path, seed: &[u8; 32]) {
    const DOCUMENT_ID: &str = "signature-manifest";
    // Digest the manifest as its struct-based serialization will actually
    // render it, not the raw file bytes, which may predate any struct
    // round-trip and so omit fields the struct always writes (an empty
    // `free_inputs`, for one). Verification always reads struct-normalized
    // bytes back from disk, so signing must match that shape.
    let manifest = read_manifest(case_dir);
    let normalized_bytes = serde_json::to_vec(&manifest).unwrap();
    let digest = signature::manifest_signing_digest(&normalized_bytes, DOCUMENT_ID).unwrap();
    let signed_document_sha256 = format!("sha256:{}", hex_encode(&digest));
    let document = signature::build_signature_document(
        seed,
        "manifest",
        manifest.case_id,
        signed_document_sha256,
        &digest,
    );
    bind_signature_document(
        case_dir,
        DOCUMENT_ID,
        "signatures/manifest.sig.json",
        &document,
    );
}

/// Sign a case package's already-committed receipt for `step_id` with
/// `seed`, exactly as `avila-core sign receipt` does.
fn sign_receipt(case_dir: &Path, step_id: &str, seed: &[u8; 32]) {
    let manifest = read_manifest(case_dir);
    let receipt_document = manifest
        .documents
        .iter()
        .find(|document| {
            document.role == "execution_receipt" && document.step_id.as_deref() == Some(step_id)
        })
        .unwrap()
        .clone();
    let receipt_bytes = fs::read(case_dir.join(&receipt_document.path)).unwrap();
    let actual_sha256 = format!("sha256:{}", sha256_hex(&receipt_bytes));
    assert_eq!(
        actual_sha256, receipt_document.sha256,
        "receipt bytes must already match the bound identity before signing"
    );
    let digest = signature::digest_from_prefixed(&actual_sha256).unwrap();
    let document = signature::build_signature_document(
        seed,
        "execution_receipt",
        step_id.to_string(),
        actual_sha256,
        &digest,
    );
    let document_id = format!("signature-receipt-{step_id}");
    let relative_path = format!("signatures/{step_id}-receipt.sig.json");
    bind_signature_document(case_dir, &document_id, &relative_path, &document);
}

fn set_require_signatures(synthetic: &Synthetic) {
    let contract_path = synthetic.case_dir.join("contract.json");
    let mut contract: Value = serde_json::from_slice(&fs::read(&contract_path).unwrap()).unwrap();
    contract["execution_policy"]["require_signatures"] = json!(true);
    fs::write(
        &contract_path,
        serde_json::to_vec_pretty(&contract).unwrap(),
    )
    .unwrap();
    rehash_document(&synthetic.case_dir, "contract", "contract.json");
}

#[test]
fn signed_manifest_and_receipt_verify_and_reuse_under_a_trust_root() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let requester = signature::generate_keypair(KeyRole::Requester).unwrap();
    let runner = signature::generate_keypair(KeyRole::Runner).unwrap();
    sign_receipt(&synthetic.case_dir, "classification", &runner.seed);
    sign_manifest(&synthetic.case_dir, &requester.seed);

    let root = trust_root(&[(&requester, KeyRole::Requester), (&runner, KeyRole::Runner)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);

    let mut options = reuse_options(&synthetic, dir.workspace());
    options.trust_root = Some(trust_root_path);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    let summary = human_summary(&report);
    assert_eq!(report.status, CaseRunStatus::Evaluated, "{summary}");
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Reused,
        "{summary}"
    );
    assert!(
        report.manifest_signature.as_ref().unwrap().is_verified(),
        "{:?}",
        report.manifest_signature
    );
    let executed = step(&report);
    assert!(
        executed.receipt_signature.as_ref().unwrap().is_verified(),
        "{:?}",
        executed.receipt_signature
    );
}

#[test]
fn without_a_trust_root_signatures_are_reported_but_never_verified() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let requester = signature::generate_keypair(KeyRole::Requester).unwrap();
    let runner = signature::generate_keypair(KeyRole::Runner).unwrap();
    sign_receipt(&synthetic.case_dir, "classification", &runner.seed);
    sign_manifest(&synthetic.case_dir, &requester.seed);

    let options = reuse_options(&synthetic, dir.workspace());
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(
        report.status,
        CaseRunStatus::Evaluated,
        "{}",
        human_summary(&report)
    );
    assert_eq!(
        report.manifest_signature.as_ref().unwrap().describe(),
        "signature not checked (no trust root supplied)"
    );
    assert_eq!(
        step(&report).receipt_signature.as_ref().unwrap().describe(),
        "signature not checked (no trust root supplied)"
    );

    // An entirely unsigned package is reported unsigned, not merely unchecked.
    let plain_dir = TestDir::new();
    let plain = blessed(&plain_dir);
    let plain_report = execute_case(
        &plain.case_dir,
        &reuse_options(&plain, plain_dir.workspace()),
    )
    .unwrap();
    assert_eq!(
        plain_report.manifest_signature.as_ref().unwrap().describe(),
        "unsigned"
    );
    assert_eq!(
        step(&plain_report)
            .receipt_signature
            .as_ref()
            .unwrap()
            .describe(),
        "unsigned"
    );
}

#[test]
fn a_rewritten_manifest_with_the_old_signature_is_refused() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let requester = signature::generate_keypair(KeyRole::Requester).unwrap();
    sign_manifest(&synthetic.case_dir, &requester.seed);
    let root = trust_root(&[(&requester, KeyRole::Requester)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);

    // Rewrite the manifest after signing, without re-signing it.
    let mut manifest = read_manifest(&synthetic.case_dir);
    manifest.title = "a rewritten title".into();
    write_manifest(&synthetic.case_dir, &manifest);

    let mut options = reuse_options(&synthetic, dir.workspace());
    options.trust_root = Some(trust_root_path);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert!(report.compile.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1004),
        "{:?}",
        report.findings
    );
}

#[test]
fn a_hand_forged_receipt_signature_does_not_verify() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let requester = signature::generate_keypair(KeyRole::Requester).unwrap();
    let runner = signature::generate_keypair(KeyRole::Runner).unwrap();

    // The forged signature's target is the receipt's real, current digest
    // (internally consistent); only the signature bytes themselves are
    // fabricated, as an attacker without the runner's private key would do.
    let manifest = read_manifest(&synthetic.case_dir);
    let receipt_document = manifest
        .documents
        .iter()
        .find(|document| document.step_id.as_deref() == Some("classification"))
        .unwrap()
        .clone();
    let forged = SignatureDocument {
        schema_version: signature::SIGNATURE_SCHEMA_VERSION.into(),
        signed_document: SignedDocumentRef {
            role: "execution_receipt".into(),
            document_id: "classification".into(),
            sha256: receipt_document.sha256.clone(),
        },
        key_id: runner.key_id.clone(),
        algorithm: signature::ALGORITHM_ED25519.into(),
        signature_hex: "00".repeat(64),
        notice: signature::SIGNATURE_NOTICE.into(),
    };
    bind_signature_document(
        &synthetic.case_dir,
        "signature-receipt-classification",
        "signatures/classification-receipt.sig.json",
        &forged,
    );
    // The manifest is signed last, over the state including the forged
    // receipt-signature entry, so only the receipt signature is under test.
    sign_manifest(&synthetic.case_dir, &requester.seed);
    let root = trust_root(&[(&requester, KeyRole::Requester), (&runner, KeyRole::Runner)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);

    let mut options = reuse_options(&synthetic, dir.workspace());
    options.trust_root = Some(trust_root_path);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    let executed = step(&report);
    assert_ne!(
        executed.state,
        StepExecutionState::Reused,
        "{}",
        human_summary(&report)
    );
    assert!(
        executed
            .changes
            .iter()
            .any(|change| change.class == ChangeClass::ReceiptSignatureInvalid),
        "{:?}",
        executed.changes
    );
}

#[test]
fn a_receipt_copied_from_a_donor_package_is_refused() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let requester = signature::generate_keypair(KeyRole::Requester).unwrap();
    let runner = signature::generate_keypair(KeyRole::Runner).unwrap();

    // A "donor" receipt: byte-identical to the real committed one except for
    // its case_id, as if it had been copied in from a different package that
    // happened to run the same capability over the same input identities.
    let mut donor_receipt: Value = serde_json::from_slice(
        &fs::read(synthetic.case_dir.join("receipts/classification.json")).unwrap(),
    )
    .unwrap();
    donor_receipt["case_id"] = json!("CASE-DONOR");
    let donor_bytes = serde_json::to_vec_pretty(&donor_receipt).unwrap();
    fs::write(
        synthetic.case_dir.join("receipts/classification.json"),
        &donor_bytes,
    )
    .unwrap();
    let mut manifest = read_manifest(&synthetic.case_dir);
    for document in &mut manifest.documents {
        if document.step_id.as_deref() == Some("classification") {
            document.sha256 = format!("sha256:{}", sha256_hex(&donor_bytes));
        }
    }
    write_manifest(&synthetic.case_dir, &manifest);
    sign_receipt(&synthetic.case_dir, "classification", &runner.seed);
    sign_manifest(&synthetic.case_dir, &requester.seed);
    let root = trust_root(&[(&requester, KeyRole::Requester), (&runner, KeyRole::Runner)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);
    // The donor receipt's own signature is genuine and would verify; case
    // identity is what refuses reuse here, not the signature. Supplying the
    // same runner key for this run lets the step rerun signed too, so the
    // final receipt_signature stays `verified` throughout and the assertion
    // below isolates case identity as the cause.
    let runner_key_path = dir.0.join("runner.seed");
    write_runner_key_file(&runner_key_path, &runner);

    let mut options = reuse_options(&synthetic, dir.workspace());
    options.trust_root = Some(trust_root_path);
    options.runner_key = Some(runner_key_path);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    let executed = step(&report);
    assert_ne!(
        executed.state,
        StepExecutionState::Reused,
        "{}",
        human_summary(&report)
    );
    assert!(
        executed
            .changes
            .iter()
            .any(|change| change.class == ChangeClass::DifferentCase),
        "{:?}",
        executed.changes
    );
    assert!(
        executed.receipt_signature.as_ref().unwrap().is_verified(),
        "{:?}",
        executed.receipt_signature
    );
}

#[test]
fn a_signature_made_with_an_unlisted_key_is_refused() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let signer = signature::generate_keypair(KeyRole::Requester).unwrap();
    sign_manifest(&synthetic.case_dir, &signer.seed);

    // The trust root lists a different requester key, never the one that
    // actually signed this manifest.
    let listed = signature::generate_keypair(KeyRole::Requester).unwrap();
    let root = trust_root(&[(&listed, KeyRole::Requester)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);

    let mut options = reuse_options(&synthetic, dir.workspace());
    options.trust_root = Some(trust_root_path);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1004),
        "{:?}",
        report.findings
    );
}

#[test]
fn a_manifest_signed_by_the_runner_key_instead_of_the_requester_key_is_refused() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    let runner = signature::generate_keypair(KeyRole::Runner).unwrap();
    // The requester never signs this manifest; the runner key does instead.
    sign_manifest(&synthetic.case_dir, &runner.seed);
    let root = trust_root(&[(&runner, KeyRole::Runner)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);

    let mut options = reuse_options(&synthetic, dir.workspace());
    options.trust_root = Some(trust_root_path);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1004),
        "{:?}",
        report.findings
    );
}

#[test]
fn a_log_line_edited_after_signing_fails_lineage_revalidation() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    enable_free_input(&synthetic, "aftermatter-case");
    let requester = signature::generate_keypair(KeyRole::Requester).unwrap();
    let runner = signature::generate_keypair(KeyRole::Runner).unwrap();
    sign_manifest(&synthetic.case_dir, &requester.seed);
    let root = trust_root(&[(&requester, KeyRole::Requester), (&runner, KeyRole::Runner)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);
    let runner_key_path = dir.0.join("runner.seed");
    write_runner_key_file(&runner_key_path, &runner);

    let root_candidate = dir.0.join("root-candidate.json");
    let child_candidate = dir.0.join("child-candidate.json");
    write_lineage_candidate(&root_candidate, "root-design", "4", false);
    write_lineage_candidate(&child_candidate, "child-design", "6", false);
    let log = dir.0.join("lineage.jsonl");

    let mut root_options = lineage_options(
        &synthetic,
        dir.workspace(),
        log.clone(),
        root_candidate,
        "try-001",
        None,
    );
    root_options.runner_key = Some(runner_key_path);
    execute_case(&synthetic.case_dir, &root_options).unwrap();
    let raw = fs::read_to_string(&log).unwrap();
    assert!(raw.contains("\"signature\""), "{raw}");

    // Edit the line's visible content after signing, before any child ever
    // references it, leaving the stale signature untouched.
    let mut root_entry: Value = serde_json::from_str(raw.trim()).unwrap();
    root_entry["status"] = json!("evaluated_but_actually_tampered");
    fs::write(
        &log,
        format!("{}\n", serde_json::to_string(&root_entry).unwrap()),
    )
    .unwrap();

    let mut child_options = lineage_options(
        &synthetic,
        dir.workspace(),
        log,
        child_candidate,
        "try-002",
        Some("try-001"),
    );
    child_options.trust_root = Some(trust_root_path);
    let child = execute_case(&synthetic.case_dir, &child_options).unwrap();
    assert_eq!(child.status, CaseRunStatus::Rejected);
    assert!(child.execution.is_none());
    assert!(
        child.findings.iter().any(|finding| {
            finding.code == CORE_X1201 && finding.message.contains("does not verify")
        }),
        "{:?}",
        child.findings
    );
}

#[test]
fn require_signatures_refuses_a_run_without_a_trust_root() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    set_require_signatures(&synthetic);

    let options = reuse_options(&synthetic, dir.workspace());
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(report.status, CaseRunStatus::Rejected);
    assert!(report.execution.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1005),
        "{:?}",
        report.findings
    );
}

#[test]
fn require_signatures_refuses_unsigned_execution_even_with_a_verified_manifest() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    set_require_signatures(&synthetic);
    let requester = signature::generate_keypair(KeyRole::Requester).unwrap();
    // The receipt itself is never signed.
    sign_manifest(&synthetic.case_dir, &requester.seed);
    let root = trust_root(&[(&requester, KeyRole::Requester)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);

    let mut options = reuse_options(&synthetic, dir.workspace());
    options.trust_root = Some(trust_root_path);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    assert_eq!(
        report.status,
        CaseRunStatus::Rejected,
        "{}",
        human_summary(&report)
    );
    assert!(
        report.manifest_signature.as_ref().unwrap().is_verified(),
        "{}",
        report.manifest_signature.as_ref().unwrap().describe()
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1005),
        "{:?}",
        report.findings
    );
}

#[test]
fn require_signatures_evaluates_a_fully_signed_case() {
    let dir = TestDir::new();
    let synthetic = blessed(&dir);
    set_require_signatures(&synthetic);
    let requester = signature::generate_keypair(KeyRole::Requester).unwrap();
    let runner = signature::generate_keypair(KeyRole::Runner).unwrap();
    sign_receipt(&synthetic.case_dir, "classification", &runner.seed);
    sign_manifest(&synthetic.case_dir, &requester.seed);
    let root = trust_root(&[(&requester, KeyRole::Requester), (&runner, KeyRole::Runner)]);
    let trust_root_path = dir.0.join("trust-root.json");
    write_trust_root(&trust_root_path, &root);

    let mut options = reuse_options(&synthetic, dir.workspace());
    options.trust_root = Some(trust_root_path);
    let report = execute_case(&synthetic.case_dir, &options).unwrap();
    let summary = human_summary(&report);
    // Adding require_signatures to the contract legitimately changes the
    // compiled snapshot identity, so the committed claims and campaign
    // report (frozen before that edit, by `blessed`) no longer replay; that
    // is an honest, unrelated consequence of editing the contract, not
    // something this test re-blesses away. What this test establishes is
    // narrower: fully signed evidence is not itself refused by the policy.
    assert_eq!(
        report.execution.as_ref().unwrap().status,
        ExecutionStatus::Reused,
        "{summary}"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.code == CORE_X1005),
        "{:?}",
        report.findings
    );
    assert!(
        step(&report)
            .receipt_signature
            .as_ref()
            .unwrap()
            .is_verified(),
        "{summary}"
    );
}
