use super::gates::build_presentation_gates;
use super::stderr::{
    DIAGNOSTIC_STDERR_MAX_CHARS, DIAGNOSTIC_STDERR_MAX_LINES, DIAGNOSTIC_STDERR_READ_BYTES,
    DiagnosticStderrFeedback, read_diagnostic_stderr, sanitize_diagnostic_stderr,
};
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-000-actinv-aftermatter")
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
    assert!(rendered.contains("result R1: FAIL -> PASS; margin -0.25 -> 0.125 1 (delta +0.375)"));
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
    let excerpt = sanitize_diagnostic_stderr(stderr.as_bytes(), false, &BTreeMap::new()).unwrap();
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

/// Exercises `build_presentation_gates` directly against CASE-001's
/// committed contract, registry, and claims, rather than through
/// `execute_case`: CASE-001's `transport` step needs an external
/// `nuclear-data` root this workspace does not carry, and since CASE-001
/// now sets `execution_policy.require_signatures` (ADR-0015), a step
/// left unreached for lack of that root is unsigned and the whole run
/// refuses before a campaign is ever evaluated. Package integrity,
/// compilation, and presentation-gate construction do not depend on
/// execution at all, so reading the committed documents directly keeps
/// this test exercising the real fixture without needing that root or a
/// trust root.
#[test]
fn case_001_materializes_an_exact_optional_practical_review_request() {
    let contract = fs::read(case_001().join("contract.json")).unwrap();
    let registry = fs::read(case_001().join("registry.json")).unwrap();
    let compile = compile_documents(&contract, &registry).unwrap();
    let compiled = compile.compiled.as_ref().unwrap();
    let claims_bytes = fs::read(case_001().join("claims.json")).unwrap();
    let claims: ClaimsDocument = serde_json::from_slice(&claims_bytes).unwrap();
    let campaign = evaluate_campaign(&contract, &registry, &claims_bytes).unwrap();

    let transport = campaign
        .verdicts
        .iter()
        .find(|verdict| verdict.requirement_id == "SHIELD-R2-transport")
        .unwrap();
    assert_eq!(transport.verdict.status, VerdictStatus::Fail);

    let stages = build_presentation_gates(compiled, &claims, &campaign).unwrap();
    let [stage] = stages.as_slice() else {
        panic!("CASE-001 must materialize exactly one staged review request");
    };
    assert_eq!(stage.step_id, "practical-review");
    assert_eq!(stage.reviewer_role, ReviewerRole::Agent);
    assert_eq!(stage.gate_state, PresentationGateState::AwaitingAgent);
    assert_eq!(stage.readiness, PresentationGateReadiness::ReadyForAgent);
    assert_eq!(
        stage.request_sha256,
        "sha256:a9bbd8680910f6fcc9c642eaba748a233b4b94155d8cf2b6b3863bf13a13a7cd"
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

    // Withholding `transport-result`, as a supplied free input would,
    // leaves the gate awaiting that one piece of evidence.
    let mut without_transport = claims.clone();
    without_transport
        .claims
        .retain(|claim| claim.claim_id != "transport-result");
    let without_transport_bytes = serde_json::to_vec(&without_transport).unwrap();
    let campaign_without_transport =
        evaluate_campaign(&contract, &registry, &without_transport_bytes).unwrap();
    let stages =
        build_presentation_gates(compiled, &without_transport, &campaign_without_transport)
            .unwrap();
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
// Both hash-cache tests below supply only the `shielding` root: enough
// for `screen`'s inputs, but not for `transport`'s external
// `nuclear-data` root, which this workspace does not carry. CASE-001 now
// sets `execution_policy.require_signatures` (ADR-0015), and a step left
// unreached for want of an unsupplied root is unsigned, so the run
// refuses before a campaign is ever evaluated regardless. Package
// integrity and the hash cache it is opt-in to are both resolved before
// that point, which is all these two tests exercise.
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
    assert_eq!(cold.status, CaseRunStatus::Rejected, "{cold:?}");
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
    assert_eq!(warm.status, CaseRunStatus::Rejected, "{warm:?}");
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
    assert_eq!(report.status, CaseRunStatus::Rejected, "{report:?}");
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

/// ADR-0018: every qualification-carrying claim in a committed claims
/// document persists the applicability context its assessment was
/// evaluated over, so re-evaluating the bound record's scope against
/// that context reproduces the recorded per-term results and state.
/// CASE-001 is the committed fixture; CASE-002's claims predate
/// ADR-0018 until its pinned ACTINV binary resolves again.
#[test]
fn committed_qualification_contexts_re_derive_the_recorded_assessment() {
    let case = case_001();
    let claims_bytes = fs::read(case.join("claims.json")).unwrap();
    let claims: ClaimsDocument = serde_json::from_slice(&claims_bytes).unwrap();
    let registry = fs::read(case.join("registry.json")).unwrap();
    let kinds = registry_kinds(&registry).unwrap();
    let records = [
        avila_core_compiler::parse_qualification(
            &fs::read(case.join("qualification.json")).unwrap(),
        )
        .unwrap(),
        avila_core_compiler::parse_qualification(
            &fs::read(case.join("qualification-screen.json")).unwrap(),
        )
        .unwrap(),
    ];

    let mut checked = 0;
    for claim in &claims.claims {
        let Some(qualification) = &claim.qualification else {
            continue;
        };
        let record = records
            .iter()
            .find(|record| {
                record.qualification_id == qualification.qualification_id
                    && record.revision == qualification.revision
            })
            .expect("claim's qualification must name a bound record");
        let derived = evaluate_envelope(
            record,
            &qualification.sha256,
            &kinds,
            &qualification.context,
        );
        assert_eq!(
            derived.state, qualification.state,
            "claim {} state must re-derive from its persisted context",
            claim.claim_id
        );
        assert_eq!(
            derived
                .terms
                .iter()
                .map(|term| term.result)
                .collect::<Vec<_>>(),
            qualification
                .terms
                .iter()
                .map(|term| term.result)
                .collect::<Vec<_>>(),
            "claim {} per-term results must re-derive from its persisted context",
            claim.claim_id
        );
        // The persisted context names the step's own staged inputs by
        // digest, bound through the step's committed receipt.
        let receipt: ExecutionReceipt = serde_json::from_slice(
            &fs::read(
                case.join("receipts")
                    .join(format!("{}.json", claim.step_id)),
            )
            .unwrap(),
        )
        .unwrap();
        for (slot, input) in qualification.context["inputs"]
            .as_object()
            .expect("context inputs is an object")
        {
            let staged = receipt
                .inputs
                .iter()
                .find(|input| input.input_slot == *slot)
                .expect("context input slot must be staged in the receipt");
            assert_eq!(
                input["attributes"]["sha256"].as_str().unwrap(),
                staged.sha256
            );
        }
        for (name, fact) in qualification.context["facts"]
            .as_object()
            .expect("context facts is an object")
        {
            assert_eq!(
                fact["source"]["receipt"].as_str().unwrap(),
                format!("plan:{}", receipt.invocation_sha256),
                "fact {name} must cite this step's planned invocation"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 4, "CASE-001 carries four qualified claims");
}
