//! The execution half of the language fixtures: `plan` lowers the analysis
//! to an execution plan with per-slot staged digests; `evaluate` replays
//! the derivation against a supplied observation set — digest-bound or
//! foreign — and derives requirement verdicts. These tests build the
//! observation documents by hand against the shared digest recipes; the
//! real-process `execute` path is exercised in the runner crate.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::language::{
    AnalysisOptions, ExecutionPlanDocument, LanguageInvocation, LanguageReceipt,
    OBSERVATIONS_SCHEMA_VERSION, ObservationRecord, ObservationsDocument, ProcessOutcome,
    RECEIPT_SCHEMA_VERSION, ReceiptInput, ReceiptOutput, RunnerIdentity, ValueDecl,
    canonical_json_bytes, evaluate_program, execution_plan, invocation_identity,
};
use sha2::{Digest, Sha256};

fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/language")
}

fn program_bytes(name: &str) -> Vec<u8> {
    fs::read(
        examples_root()
            .join("programs")
            .join(format!("{name}.program.json")),
    )
    .unwrap()
}

fn library_bytes() -> Vec<u8> {
    fs::read(examples_root().join("libraries/thermal-expansion.v1.json")).unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// The canonical output `clearance-pass` expects: `coefficient *
/// temperature_change * length` over the staged operands.
fn honest_output() -> ValueDecl {
    ValueDecl {
        kind: "enclosure".into(),
        value: None,
        lower: Some("1/10".into()),
        upper: Some("1/5".into()),
        unit: "mm".into(),
    }
}

/// Build an observation record the digest binding accepts: input digests
/// copied from the plan (the receipt attests what the plan staged), the
/// output re-hashed, the receipt re-hashable.
fn observation(
    plan: &ExecutionPlanDocument,
    site: usize,
    output: Option<ValueDecl>,
) -> ObservationRecord {
    let invocation = &plan.invocations[site];
    let output_sha256 = output
        .as_ref()
        .map(|decl| sha256(&canonical_json_bytes(decl).unwrap()));
    let mut receipt = LanguageReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION.into(),
        plan_sha256: plan.plan_sha256.clone().unwrap(),
        at: invocation.at.clone(),
        executable: invocation.executable.clone(),
        executable_sha256: format!("sha256:{:x}", Sha256::digest(b"test-executable")),
        inputs: invocation
            .inputs
            .iter()
            .map(|(slot, input)| ReceiptInput {
                slot: slot.clone(),
                workspace_path: format!("inputs/{slot}.json"),
                sha256: input.sha256.clone().unwrap_or_default(),
                bytes: 1,
            })
            .collect(),
        invocation: LanguageInvocation {
            arguments: vec!["inputs".into(), "outputs/output.json".into()],
            working_directory: format!("invocations/body_{site}_"),
            environment: BTreeMap::new(),
            timeout_ms: 30_000,
        },
        invocation_sha256: String::new(),
        process: ProcessOutcome {
            started_at: "2026-01-01T00:00:00.000Z".into(),
            finished_at: "2026-01-01T00:00:00.001Z".into(),
            duration_ms: 1,
            exit_status: Some(0),
            signal: None,
            timed_out: false,
        },
        logs: Vec::new(),
        outputs: vec![ReceiptOutput {
            output_id: "output".into(),
            workspace_path: "outputs/output.json".into(),
            state: "collected".into(),
            sha256: output_sha256.clone(),
            bytes: Some(1),
        }],
        runner: RunnerIdentity {
            runner: "test".into(),
            os: "test".into(),
            arch: "test".into(),
        },
        status: "completed".into(),
        limitations: Vec::new(),
    };
    receipt.invocation_sha256 = invocation_identity(&receipt).unwrap();
    let receipt_sha256 = sha256(&canonical_json_bytes(&receipt).unwrap());
    ObservationRecord {
        at: invocation.at.clone(),
        bind: invocation.bind.clone(),
        executable: invocation.executable.clone(),
        inputs: invocation
            .inputs
            .iter()
            .map(|(slot, input)| (slot.clone(), input.sha256.clone().unwrap_or_default()))
            .collect(),
        output_sha256,
        output,
        receipt_sha256,
        receipt,
    }
}

fn observations_doc(plan_sha256: &str, observations: Vec<ObservationRecord>) -> Vec<u8> {
    let mut doc = ObservationsDocument {
        schema_version: OBSERVATIONS_SCHEMA_VERSION.into(),
        profile: "avila.core/language/0.1-draft".into(),
        plan_sha256: plan_sha256.into(),
        observations,
        observations_sha256: None,
    };
    doc.observations_sha256 = Some(sha256(&canonical_json_bytes(&doc).unwrap()));
    serde_json::to_vec(&doc).unwrap()
}

fn evaluate(
    program: &str,
    observations: &[u8],
) -> avila_core_compiler::language::LanguageEvaluation {
    evaluate_program(
        &program_bytes(program),
        &library_bytes(),
        &AnalysisOptions::default(),
        observations,
    )
}

#[test]
fn plan_binds_staged_inputs_and_obligations() {
    let plan = execution_plan(
        &program_bytes("clearance-pass"),
        &library_bytes(),
        &AnalysisOptions::default(),
    );
    assert_eq!(plan.state, "ready");
    assert_eq!(plan.invocations.len(), 1);
    let invocation = &plan.invocations[0];
    assert_eq!(invocation.at, "body[0]");
    assert_eq!(invocation.executable, "synthetic/linear-expansion@1");
    assert_eq!(invocation.obligations, vec!["body[0].ensures[0]"]);
    for input in invocation.inputs.values() {
        assert_eq!(input.state, "staged");
        assert!(input.sha256.is_some());
    }
    assert!(plan.plan_sha256.is_some());
}

#[test]
fn observation_discharges_and_verdict_derives() {
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    let doc = observations_doc(
        plan.plan_sha256.as_deref().unwrap(),
        vec![observation(&plan, 0, Some(honest_output()))],
    );
    let evaluation = evaluate_program(
        &program,
        &library_bytes(),
        &AnalysisOptions::default(),
        &doc,
    );
    let obligation = evaluation
        .obligations
        .iter()
        .find(|o| o.subject == "body[0].ensures[0]")
        .unwrap();
    assert_eq!(obligation.state, "discharged");
    let verdict = &evaluation.requirements["EL-R1"];
    assert_eq!(verdict.status, "pass");
    assert_eq!(verdict.rule, "bounded.ge");
}

#[test]
fn an_unsatisfied_postcondition_is_refuted() {
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    // The observed output contradicts the declared postcondition — the
    // obligation refutes and the binding cannot be used.
    let mut bad = honest_output();
    bad.lower = Some("7/10".into());
    bad.upper = Some("4/5".into());
    let doc = observations_doc(
        plan.plan_sha256.as_deref().unwrap(),
        vec![observation(&plan, 0, Some(bad))],
    );
    let evaluation = evaluate_program(
        &program,
        &library_bytes(),
        &AnalysisOptions::default(),
        &doc,
    );
    let obligation = evaluation
        .obligations
        .iter()
        .find(|o| o.subject == "body[0].ensures[0]")
        .unwrap();
    assert_eq!(obligation.state, "refuted");
    assert_eq!(evaluation.requirements["EL-R1"].status, "not_evaluated");
}

#[test]
fn absent_observation_leaves_the_obligation_open() {
    let doc = observations_doc("sha256:anything", Vec::new());
    let evaluation = evaluate("clearance-pass", &doc);
    let obligation = evaluation
        .obligations
        .iter()
        .find(|o| o.subject == "body[0].ensures[0]")
        .unwrap();
    assert_eq!(obligation.state, "open");
    assert_eq!(evaluation.requirements["EL-R1"].status, "not_evaluated");
    assert!(
        evaluation
            .observations
            .iter()
            .any(|o| o.state == "absent" && o.at == "body[0]")
    );
}

#[test]
fn a_foreign_plan_digest_rejects_everything() {
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    // A record honestly built — but the document claims another plan.
    let doc = observations_doc(
        "sha256:not-the-plan",
        vec![observation(&plan, 0, Some(honest_output()))],
    );
    let evaluation = evaluate("clearance-pass", &doc);
    assert_eq!(evaluation.requirements["EL-R1"].status, "not_evaluated");
    assert!(
        evaluation
            .observations
            .iter()
            .any(|o| o.state == "rejected")
    );
}

#[test]
fn tampered_digests_keep_the_observation_foreign() {
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    for tamper in ["input", "output", "receipt", "site"] {
        let mut record = observation(&plan, 0, Some(honest_output()));
        match tamper {
            "input" => {
                record
                    .inputs
                    .insert("length".into(), format!("sha256:{}", "0".repeat(64)));
            }
            "output" => {
                record.output_sha256 = Some(format!("sha256:{}", "0".repeat(64)));
            }
            "receipt" => record.receipt_sha256 = format!("sha256:{}", "0".repeat(64)),
            "site" => record.receipt.at = "body[9]".into(),
            _ => {}
        }
        let doc = observations_doc(plan.plan_sha256.as_deref().unwrap(), vec![record]);
        let evaluation = evaluate("clearance-pass", &doc);
        assert_eq!(
            evaluation.requirements["EL-R1"].status, "not_evaluated",
            "{tamper}: a tampered observation must not discharge anything"
        );
        assert!(
            evaluation
                .observations
                .iter()
                .any(|o| o.state == "rejected"),
            "{tamper}: expected a rejected-observation outcome"
        );
        assert!(
            evaluation
                .findings
                .iter()
                .any(|f| f.code == "CORE-E8028" && f.kind == "observation_foreign"),
            "{tamper}: expected the observation_foreign finding"
        );
    }
}

#[test]
fn transplanted_receipts_never_discharge() {
    // Two invocations: swapping the observation records between sites means
    // each receipt's `at` disagrees with the record claiming it — both are
    // foreign; the requirement reports not_evaluated.
    let program = program_bytes("clearance-heuristic-unusable");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    assert_eq!(plan.invocations.len(), 2);
    let output_a = observation(
        &plan,
        0,
        Some(ValueDecl {
            kind: "enclosure".into(),
            value: None,
            lower: Some("3/20".into()),
            upper: Some("9/20".into()),
            unit: "mm".into(),
        }),
    );
    let output_b = observation(
        &plan,
        1,
        Some(ValueDecl {
            kind: "nominal".into(),
            value: Some("19/100".into()),
            lower: None,
            upper: None,
            unit: "mm".into(),
        }),
    );
    let mut swapped_a = output_a.clone();
    swapped_a.at = plan.invocations[1].at.clone();
    let mut swapped_b = output_b;
    swapped_b.at = plan.invocations[0].at.clone();
    let doc = observations_doc(
        plan.plan_sha256.as_deref().unwrap(),
        vec![swapped_a, swapped_b],
    );
    let evaluation = evaluate_program(
        &program,
        &library_bytes(),
        &AnalysisOptions::default(),
        &doc,
    );
    assert_eq!(evaluation.requirements["EL-R1"].status, "not_evaluated");
    assert_eq!(
        evaluation
            .findings
            .iter()
            .filter(|f| f.kind == "observation_foreign")
            .count(),
        2,
        "each transplanted record must be rejected at its claimed site"
    );
}

#[test]
fn records_naming_non_invocation_sites_are_rejected() {
    // clearance-pass's body[1] is a primitive application, not an external
    // invocation — a record supplied for it is foreign, and its site is not
    // counted as "answered" anywhere.
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    let honest = observation(&plan, 0, Some(honest_output()));
    let mut foreign = observation(&plan, 0, Some(honest_output()));
    foreign.at = "body[1]".into();
    foreign.receipt.at = "body[1]".into();
    let doc = observations_doc(plan.plan_sha256.as_deref().unwrap(), vec![honest, foreign]);
    let evaluation = evaluate("clearance-pass", &doc);
    assert_eq!(evaluation.requirements["EL-R1"].status, "pass");
    assert!(
        evaluation
            .observations
            .iter()
            .any(|o| o.at == "body[1]" && o.state == "rejected"),
        "a record naming a non-invocation site must surface as rejected"
    );
}

#[test]
fn duplicate_site_claims_are_ambiguous_and_neither_binds() {
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    let doc = observations_doc(
        plan.plan_sha256.as_deref().unwrap(),
        vec![
            observation(&plan, 0, Some(honest_output())),
            observation(&plan, 0, Some(honest_output())),
        ],
    );
    let evaluation = evaluate("clearance-pass", &doc);
    assert_eq!(evaluation.requirements["EL-R1"].status, "not_evaluated");
    assert!(
        evaluation
            .observations
            .iter()
            .any(|o| o.at == "body[0]" && o.state == "rejected" && o.detail.contains("ambiguous")),
        "duplicate claims must surface as an ambiguity rejection"
    );
}

#[test]
fn verdict_boundaries_hold_under_execution() {
    for (program, status) in [
        ("clearance-pass", "pass"),
        ("clearance-fail", "fail"),
        ("clearance-inconclusive", "inconclusive"),
    ] {
        let bytes = program_bytes(program);
        let plan = execution_plan(&bytes, &library_bytes(), &AnalysisOptions::default());
        let doc = observations_doc(
            plan.plan_sha256.as_deref().unwrap(),
            vec![observation(&plan, 0, Some(honest_output()))],
        );
        let evaluation =
            evaluate_program(&bytes, &library_bytes(), &AnalysisOptions::default(), &doc);
        assert_eq!(
            evaluation.requirements["EL-R1"].status, status,
            "{program}: {status} expected"
        );
    }
}
