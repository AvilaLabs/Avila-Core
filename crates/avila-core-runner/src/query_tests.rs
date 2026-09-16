use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "core-query-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, text: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, text).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn report() -> Value {
    json!({"schema_version":"avila.core/case-run-report/v0.5-draft","case_id":"example","status":"evaluated",
        "integrity":{"status":"complete","manifest_sha256":"recorded-manifest","artifacts":[{"path":"file-that-no-longer-exists","state":"verified"}]},
        "campaign":{"status":"evaluated","verdicts":[
            {"requirement_id":"a","verdict":{"status":"inconclusive"},"evidence_ids":["e1"],"boundary":{"compiled_snapshot_sha256":"bound-question"}},
            {"requirement_id":"b","verdict":{"status":"not_evaluated"},"evidence_ids":[]}],
            "admissions":[{"evidence_id":"e1","state":"admitted"},{"evidence_id":"e2","state":"quarantined"}]},
        "margins":[{"requirement_id":"a","margin":"1/3"}]})
}

#[test]
fn focused_verdict_query_preserves_exact_values_boundary_and_admissions() {
    let fixture = Fixture::new();
    let record = report();
    let bytes = serde_json::to_vec(&record).unwrap();
    let path = fixture.write("report.json", &bytes);
    let result = call_tool(
        &QueryContext::unrestricted(),
        "core_requirements",
        json!({"path":path,"id":"a"}),
    )
    .unwrap();
    let item = &result["result"]["items"][0];
    assert_eq!(
        item["verdict"],
        record["campaign"]["verdicts"][0]["verdict"]
    );
    assert_eq!(
        item["boundary"],
        record["campaign"]["verdicts"][0]["boundary"]
    );
    assert_eq!(item["margin_record"]["margin"], "1/3");
    assert_eq!(item["admissions"].as_array().unwrap().len(), 1);
    assert_eq!(result["verification"], "recorded_only");
    assert_eq!(
        result["source"]["sha256"],
        format!("sha256:{}", sha256_hex(&bytes))
    );
    // The missing artifact must not be silently promoted to a fresh check.
    let artifacts = call_tool(
        &QueryContext::unrestricted(),
        "core_artifacts",
        json!({"path":path}),
    )
    .unwrap();
    assert_eq!(artifacts["verification"], "recorded_only");
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn pagination_and_absent_verdicts_do_not_invent_success() {
    let mut record = report();
    let first = report_view(
        &record,
        "core_requirements",
        &QueryArgs {
            limit: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(first["next_offset"], 1);
    let second = report_view(
        &record,
        "core_requirements",
        &QueryArgs {
            offset: Some(1),
            limit: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(second["items"][0]["verdict"]["status"], "not_evaluated");
    assert!(second["next_offset"].is_null());
    record.as_object_mut().unwrap().remove("campaign");
    record["compile"] = json!({"compiled":{"requirements":[{"requirement_id":"unrun"}]}});
    let unrun = report_view(&record, "core_requirements", &QueryArgs::default()).unwrap();
    assert_eq!(unrun["items"][0]["recorded_verdict_available"], false);
    assert!(unrun["items"][0]["verdict"].is_null());
}

fn log_record(case: &str, state: &str, invocation: &str) -> Value {
    json!({"schema_version":"avila.core/run-attempt/v0.3-draft","case_id":case,"status":"evaluated",
        "steps":[{"step_id":"screen","state":state,"planned_invocation_sha256":invocation}]})
}

#[test]
fn exact_history_search_preserves_planned_failed_and_completed_states() {
    let hash = format!("sha256:{}", "a".repeat(64));
    let records: Vec<_> = ["planned", "failed", "executed", "reused"]
        .into_iter()
        .map(|state| log_record("case", state, &hash))
        .collect();
    let text = records.iter().map(|r| format!("{r}\n")).collect::<String>();
    let args = QueryArgs {
        invocation: Some(hash.clone()),
        ..Default::default()
    };
    let result = history(text.as_bytes(), &args).unwrap();
    assert_eq!(result["total"], 4);
    for (index, state) in ["planned", "failed", "executed", "reused"]
        .iter()
        .enumerate()
    {
        assert_eq!(result["items"][index]["steps"][0]["state"], *state);
        assert_eq!(
            result["items"][index]["record_sha256"],
            format!(
                "sha256:{}",
                sha256_hex(records[index].to_string().as_bytes())
            )
        );
    }
    let absent = history(
        text.as_bytes(),
        &QueryArgs {
            case_id: Some("elsewhere".into()),
            ..args
        },
    )
    .unwrap();
    assert_eq!(absent["match_status"], "no_match_in_record");
    assert!(
        history(
            text.as_bytes(),
            &QueryArgs {
                invocation: Some("a".into()),
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn unavailable_or_malformed_history_is_never_a_negative_answer() {
    let fixture = Fixture::new();
    let context = QueryContext::rooted(&fixture.0).unwrap();
    assert!(call_tool(&context, "core_history", json!({"path":"absent.jsonl"})).is_err());
    fixture.write("empty.jsonl", "");
    assert_eq!(
        call_tool(&context, "core_history", json!({"path":"empty.jsonl"})).unwrap()["result"]["match_status"],
        "no_match_in_record"
    );
    fixture.write(
        "broken.jsonl",
        format!("{}\n{{broken", log_record("case", "executed", "hash")),
    );
    // Even a filter excluding the broken tail must fail closed.
    assert!(
        call_tool(
            &context,
            "core_history",
            json!({"path":"broken.jsonl","case_id":"elsewhere"})
        )
        .is_err()
    );
    fixture.write(
        "unknown.jsonl",
        "{\"schema_version\":\"future\",\"status\":\"evaluated\"}\n",
    );
    assert!(call_tool(&context, "core_history", json!({"path":"unknown.jsonl"})).is_err());
    assert!(
        call_tool(
            &context,
            "core_attempt",
            json!({"path":"unknown.jsonl","id":"missing"})
        )
        .is_err()
    );
    assert!(parse_json(br#"{"status":"failed","status":"evaluated"}"#).is_err());
}

#[test]
fn root_bounds_paths_and_symlinks_and_arguments_match_catalog() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("inside")).unwrap();
    let outside = fixture.write("outside.json", report().to_string());
    let context = QueryContext::rooted(&fixture.0.join("inside")).unwrap();
    assert!(call_tool(&context, "core_inspect", json!({"path":outside})).is_err());
    assert!(call_tool(&context, "core_inspect", json!({"path":"../outside.json"})).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, fixture.0.join("inside/link.json")).unwrap();
        assert!(call_tool(&context, "core_inspect", json!({"path":"link.json"})).is_err());
    }
    for args in [
        json!({"path":"report.json","typo":true}),
        json!({"path":"report.json","limit":0}),
        json!({"path":"report.json","limit":101}),
        json!({"path":"report.json","limit":null}),
        json!({"path":"report.json","offset":-1}),
    ] {
        assert!(call_tool(&context, "core_requirements", args).is_err());
    }
    assert!(
        call_tool(
            &context,
            "core_explain",
            json!({"id":"CORE-R3102","path":outside})
        )
        .is_err()
    );
}

#[test]
fn real_runner_report_and_log_are_queryable_without_mutation() {
    let fixture = Fixture::new();
    let log_path = fixture.0.join("campaign.jsonl");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let case = root.join("examples/cases/case-003-thermal-spreader");
    let report = crate::execute_case(
        &case,
        &crate::CaseRunOptions {
            log: Some(log_path.clone()),
            source_roots: std::collections::BTreeMap::from([
                ("case".into(), case.clone()),
                ("thermal".into(), case.join("../../capabilities/thermal")),
            ]),
            trust_root: Some(root.join("examples/keys/trust-root.json")),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.succeeded(), "{}", crate::human_summary(&report));
    let bytes = serde_json::to_vec(&report).unwrap();
    let path = fixture.write("report.json", &bytes);
    for tool in [
        "core_inspect",
        "core_requirements",
        "core_findings",
        "core_artifacts",
        "core_workflow",
        "core_evidence",
        "core_steps",
    ] {
        let result = call_tool(&QueryContext::unrestricted(), tool, json!({"path":path})).unwrap();
        assert_eq!(result["verification"], "recorded_only");
        assert_eq!(result["result"]["case_id"], report.case_id);
        let memory = call_report_tool(tool, json!({}), &bytes).unwrap();
        assert_eq!(memory["result"], result["result"]);
        assert_eq!(memory["source"]["sha256"], result["source"]["sha256"]);
        assert_eq!(memory["source"]["label"], "Current workbench report");
        assert!(memory["source"].get("path").is_none());
    }
    let history_bytes = fs::read(&log_path).unwrap();
    let result = call_tool(
        &QueryContext::unrestricted(),
        "core_history",
        json!({"path":log_path}),
    )
    .unwrap();
    assert_eq!(result["result"]["total"], 1);
    assert_eq!(result["result"]["items"][0]["case_id"], report.case_id);
    let constellation = call_tool(
        &QueryContext::unrestricted(),
        "core_constellation",
        json!({"path":log_path}),
    )
    .unwrap();
    assert_eq!(constellation["result"]["total"], 1);
    assert_eq!(constellation["result"]["summary"]["untracked_records"], 1);
    assert_eq!(constellation["result"]["lineage_validation"], "consistent");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(&log_path).unwrap(), history_bytes);
    assert!(call_report_tool("core_history", json!({}), &bytes).is_err());
    assert!(call_report_tool("core_constellation", json!({}), &bytes).is_err());
    assert!(call_report_tool("core_inspect", json!({"path":"ignored.json"}), &bytes).is_err());
}

/// CQ-04: a real local checker case driven through plan-then-run lineage:
/// parent selection, a modified candidate, the plan, the run, and the
/// refusal paths the runner owns.
#[test]
fn planned_and_executed_attempts_form_a_queryable_lineage() {
    let fixture = Fixture::new();
    let log_path = fixture.0.join("attempts.jsonl");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let case = root.join("examples/cases/case-003-thermal-spreader");
    let source_roots = || {
        std::collections::BTreeMap::from([
            ("case".into(), case.clone()),
            ("thermal".into(), case.join("../../capabilities/thermal")),
        ])
    };
    let inputs = |candidate: &Path| {
        std::collections::BTreeMap::from([("candidate".into(), candidate.to_path_buf())])
    };
    let request = |attempt_id: &str, parent: Option<&str>| {
        Some(crate::AttemptLineageRequest {
            attempt_id: attempt_id.into(),
            parent_attempt_id: parent.map(str::to_string),
            candidate_input: "candidate".into(),
            revision_id: None,
            amendment_id: None,
        })
    };
    let candidate_a = fixture.write(
        "candidate-a.json",
        fs::read(case.join("candidates/reference.json")).unwrap(),
    );
    let mut changed: Value = serde_json::from_slice(&fs::read(&candidate_a).unwrap()).unwrap();
    changed["candidate_id"] = json!("case-003-child");
    changed["layers"][0]["thickness_mm"] = json!("4");
    let candidate_b = fixture.write(
        "candidate-b.json",
        serde_json::to_vec_pretty(&changed).unwrap(),
    );

    // An attempt without a log is refused before any step runs.
    let no_log = crate::execute_case(
        &case,
        &crate::CaseRunOptions {
            source_roots: source_roots(),
            inputs: inputs(&candidate_a),
            trust_root: Some(root.join("examples/keys/trust-root.json")),
            runner_key: Some(root.join("examples/keys/runner.seed")),
            attempt: request("no-log", None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(no_log.status, crate::CaseRunStatus::Rejected);
    assert!(
        no_log
            .findings
            .iter()
            .any(|finding| finding.message.contains("requires `--log FILE`"))
    );

    // The root attempt runs and records generation 0.
    let root_report = crate::execute_case(
        &case,
        &crate::CaseRunOptions {
            log: Some(log_path.clone()),
            source_roots: source_roots(),
            inputs: inputs(&candidate_a),
            trust_root: Some(root.join("examples/keys/trust-root.json")),
            runner_key: Some(root.join("examples/keys/runner.seed")),
            attempt: request("root-a", None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        root_report.status,
        crate::CaseRunStatus::Evaluated,
        "{}",
        crate::human_summary(&root_report)
    );
    let root_record = root_report.attempt.as_ref().unwrap();
    assert_eq!(root_record.generation, 0);
    assert!(root_record.changes.is_empty());
    assert!(root_report.attempt_comparison.is_none());

    // Planning the child keeps every verdict explicitly unrecorded on the
    // child; the plan neither executes nor invents a child assessment.
    let plan = crate::execute_case(
        &case,
        &crate::CaseRunOptions {
            plan_only: true,
            log: Some(log_path.clone()),
            source_roots: source_roots(),
            inputs: inputs(&candidate_b),
            trust_root: Some(root.join("examples/keys/trust-root.json")),
            runner_key: Some(root.join("examples/keys/runner.seed")),
            attempt: request("plan-b", Some("root-a")),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(plan.status, crate::CaseRunStatus::Planned);
    let plan_attempt = plan.attempt.as_ref().unwrap();
    assert_eq!(plan_attempt.generation, 1);
    assert_eq!(plan_attempt.parent_attempt_id.as_deref(), Some("root-a"));
    assert!(!plan_attempt.changes.is_empty());
    let plan_comparison = plan.attempt_comparison.as_ref().unwrap();
    assert!(plan_comparison.verdict_transitions.is_empty());
    assert!(plan_comparison.exact_margin_comparisons.is_empty());
    assert!(
        !plan_comparison.verdict_comparison_unavailable.is_empty(),
        "a planned child must mark every verdict unrecorded, not fabricate"
    );

    // The executed child records real verdicts and a real comparison.
    let child = crate::execute_case(
        &case,
        &crate::CaseRunOptions {
            log: Some(log_path.clone()),
            source_roots: source_roots(),
            inputs: inputs(&candidate_b),
            trust_root: Some(root.join("examples/keys/trust-root.json")),
            runner_key: Some(root.join("examples/keys/runner.seed")),
            attempt: request("child-b", Some("root-a")),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        child.status,
        crate::CaseRunStatus::Evaluated,
        "{}",
        crate::human_summary(&child)
    );
    let child_record = child.attempt.as_ref().unwrap();
    assert_eq!(child_record.generation, 1);
    assert_eq!(child_record.parent_attempt_id.as_deref(), Some("root-a"));
    assert!(child_record.parent_record_sha256.is_some());
    let comparison = child.attempt_comparison.as_ref().unwrap();
    assert_eq!(comparison.parent_attempt_id, "root-a");
    // No solver executables are configured here: the child's steps do not
    // run, its verdicts are honestly NOT_EVALUATED, and its margins are
    // child_missing — never fabricated or zero.
    assert!(comparison.verdicts_compared > 0);
    assert!(
        comparison
            .verdict_transitions
            .iter()
            .any(|t| t.child_status == avila_core_kernel::VerdictStatus::NotEvaluated)
    );
    assert!(
        comparison
            .margin_comparison_unavailable
            .iter()
            .all(|m| m.reason == crate::AttemptMarginUnavailableReason::ChildMissing)
    );

    // A duplicate attempt ID is refused, not appended.
    let duplicate = crate::execute_case(
        &case,
        &crate::CaseRunOptions {
            log: Some(log_path.clone()),
            source_roots: source_roots(),
            inputs: inputs(&candidate_a),
            trust_root: Some(root.join("examples/keys/trust-root.json")),
            runner_key: Some(root.join("examples/keys/runner.seed")),
            attempt: request("root-a", None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(duplicate.status, crate::CaseRunStatus::Rejected);
    assert!(
        duplicate
            .findings
            .iter()
            .any(|finding| finding.message.contains("already exists"))
    );

    // A parent that does not exist in this log is refused.
    let orphan = crate::execute_case(
        &case,
        &crate::CaseRunOptions {
            log: Some(log_path.clone()),
            source_roots: source_roots(),
            inputs: inputs(&candidate_b),
            trust_root: Some(root.join("examples/keys/trust-root.json")),
            runner_key: Some(root.join("examples/keys/runner.seed")),
            attempt: request("orphan", Some("never-recorded")),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(orphan.status, crate::CaseRunStatus::Rejected);
    assert!(
        orphan
            .findings
            .iter()
            .any(|finding| finding.message.contains("does not exist"))
    );

    // The recorded history shows the root, both children, and the
    // recomputed comparison — including the plan row's honest record.
    let constellation = call_tool(
        &QueryContext::unrestricted(),
        "core_constellation",
        json!({"path":log_path}),
    )
    .unwrap();
    let items = constellation["result"]["items"].as_array().unwrap();
    let attempts: Vec<&str> = items
        .iter()
        .filter_map(|item| item["attempt_id"].as_str())
        .collect();
    assert_eq!(attempts, ["root-a", "plan-b", "child-b"]);
    let detail = call_tool(
        &QueryContext::unrestricted(),
        "core_attempt",
        json!({"path":log_path,"id":"child-b"}),
    )
    .unwrap();
    assert_eq!(detail["result"]["match_status"], "found");
    assert_eq!(
        detail["result"]["comparison"]["parent_attempt_id"],
        "root-a"
    );
}

fn constellation_log() -> String {
    let manifest = format!("sha256:{}", "a".repeat(64));
    let snapshot = format!("sha256:{}", "b".repeat(64));
    let untracked = json!({"schema_version":"avila.core/run-attempt/v0.3-draft","case_id":"case",
        "recorded_at":"2026-09-10T00:00:00Z","status":"evaluated",
        "steps":[{"step_id":"screen","state":"executed"}],
        "supplied_inputs":[{"input_id":"candidate","sha256":"sha256:cc"}],
        "verdicts":[{"requirement_id":"R1","status":"fail","margin":"-1/4"}]});
    let root_state = json!({"candidate_id":"root","ratio":"2"});
    let root_state_sha256 = format!(
        "sha256:{}",
        sha256_hex(
            canonicalize_json(&serde_json::to_vec(&root_state).unwrap())
                .unwrap()
                .as_slice()
        )
    );
    let root = json!({"schema_version":"avila.core/run-attempt/v0.3-draft","case_id":"case",
        "recorded_at":"2026-09-10T00:01:00Z","status":"evaluated",
        "manifest_sha256":manifest,"compiled_snapshot_sha256":snapshot,
        "steps":[{"step_id":"screen","state":"executed"}],
        "verdicts":[{"requirement_id":"R1","status":"pass","margin":"3/8"}],
        "attempt":{"schema_version":"avila.core/attempt-lineage/v0.1-draft",
            "attempt_id":"root-a","generation":0,
            "fixed_manifest_sha256":manifest,"fixed_compiled_snapshot_sha256":snapshot,
            "candidate_input":"candidate","candidate_artifact_sha256":"sha256:aa",
            "candidate_state_sha256":root_state_sha256,"candidate_state":root_state}});
    let root_line = root.to_string();
    let parent_sha256 = format!("sha256:{}", sha256_hex(root_line.as_bytes()));
    let child_state = json!({"candidate_id":"child","ratio":"4"});
    let child_state_sha256 = format!(
        "sha256:{}",
        sha256_hex(
            canonicalize_json(&serde_json::to_vec(&child_state).unwrap())
                .unwrap()
                .as_slice()
        )
    );
    let child = json!({"schema_version":"avila.core/run-attempt/v0.3-draft","case_id":"case",
        "recorded_at":"2026-09-10T00:02:00Z","status":"evaluated",
        "manifest_sha256":manifest,"compiled_snapshot_sha256":snapshot,
        "steps":[{"step_id":"screen","state":"executed"}],
        "verdicts":[{"requirement_id":"R1","status":"fail","margin":"-1/8"}],
        "attempt":{"schema_version":"avila.core/attempt-lineage/v0.1-draft",
            "attempt_id":"child-b","generation":1,
            "parent_attempt_id":"root-a","parent_record_sha256":parent_sha256,
            "fixed_manifest_sha256":manifest,"fixed_compiled_snapshot_sha256":snapshot,
            "candidate_input":"candidate","candidate_artifact_sha256":"sha256:bb",
            "candidate_state_sha256":child_state_sha256,"candidate_state":child_state,
            "changes":[
                {"kind":"replaced","pointer":"/candidate_id","before":"root","after":"child"},
                {"kind":"replaced","pointer":"/ratio","before":"2","after":"4"}]}});
    format!("{untracked}\n{root}\n{child}\n")
}

#[test]
fn constellation_reads_the_whole_recorded_lineage() {
    let text = constellation_log();
    let result = constellation(text.as_bytes(), &QueryArgs::default()).unwrap();
    assert_eq!(result["total"], 3);
    assert_eq!(result["lineage_validation"], "consistent");
    assert_eq!(result["signature_verification"], "not_checked");
    let items = result["items"].as_array().unwrap();
    assert_eq!(items[0]["attempt_id"], Value::Null);
    assert_eq!(items[0]["verdicts"][0]["margin"], "-1/4");
    assert_eq!(items[0]["supplied_inputs"][0]["input_id"], "candidate");
    assert_eq!(items[1]["attempt_id"], "root-a");
    assert_eq!(items[1]["candidate_state"]["ratio"], "2");
    assert!(items[1].get("parent_attempt_id").is_none());
    let child = &items[2];
    assert_eq!(child["attempt_id"], "child-b");
    assert_eq!(child["generation"], 1);
    assert_eq!(child["parent_attempt_id"], "root-a");
    assert_eq!(child["parent_line"], 2);
    assert_eq!(child["changes"].as_array().unwrap().len(), 2);
    let summary = &result["summary"];
    assert_eq!(summary["lines"], 3);
    assert_eq!(summary["attempt_records"], 2);
    assert_eq!(summary["untracked_records"], 1);
    assert_eq!(summary["roots"], json!(["root-a"]));
    assert_eq!(summary["leaves"], json!(["child-b"]));
    assert_eq!(summary["max_generation"], 1);
    assert_eq!(summary["distinct_candidate_states"], 2);
    assert_eq!(summary["requirement_status_counts"]["R1"]["fail"], 2);
    assert_eq!(summary["requirement_status_counts"]["R1"]["pass"], 1);
    let only_child = constellation(
        text.as_bytes(),
        &QueryArgs {
            id: Some("child-b".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(only_child["total"], 1);
    assert_eq!(only_child["items"][0]["attempt_id"], "child-b");
    let elsewhere = constellation(
        text.as_bytes(),
        &QueryArgs {
            case_id: Some("elsewhere".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(elsewhere["match_status"], "no_match_in_record");
}

#[test]
fn constellation_supervision_reads_where_the_campaign_stands() {
    let mut text = constellation_log();
    // A rejected record needs attention; it carries no verdicts and cannot
    // move the latest requirement reading.
    let rejected = json!({"schema_version":"avila.core/run-attempt/v0.3-draft","case_id":"case",
        "recorded_at":"2026-09-10T00:03:00Z","status":"rejected",
        "manifest_sha256":format!("sha256:{}","a".repeat(64)),
        "compiled_snapshot_sha256":format!("sha256:{}","b".repeat(64)),
        "findings":[{"code":"CORE-X9001","message":"unmet"}],
        "steps":[]});
    text.push_str(&format!("{rejected}\n"));
    let result = constellation(text.as_bytes(), &QueryArgs::default()).unwrap();
    let supervision = &result["supervision"];
    assert_eq!(supervision["run_states"]["evaluated"], 3);
    assert_eq!(supervision["run_states"]["rejected"], 1);
    assert_eq!(supervision["step_states"]["executed"], 3);
    assert_eq!(
        supervision["latest_requirements"]["R1"]["status"], "fail",
        "the child's verdict is the latest recorded reading"
    );
    assert_eq!(
        supervision["latest_requirements"]["R1"]["attempt_id"],
        "child-b"
    );
    let attention = supervision["attention"].as_array().unwrap();
    assert_eq!(attention.len(), 1);
    assert_eq!(attention[0]["status"], "rejected");
    assert_eq!(attention[0]["finding_codes"], json!(["CORE-X9001"]));
    // The newest recorded line for the case is the rejection itself — where
    // this case stands now is "rejected", with no attempt behind it.
    assert_eq!(supervision["latest_by_case"]["case"]["status"], "rejected");
    assert_eq!(
        supervision["latest_by_case"]["case"]["attempt_id"],
        Value::Null
    );
    // An attempt filter scopes the roll-up to that record only.
    let one = constellation(
        text.as_bytes(),
        &QueryArgs {
            id: Some("root-a".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(one["supervision"]["run_states"]["evaluated"], 1);
    assert!(
        one["supervision"]["attention"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        one["supervision"]["latest_requirements"]["R1"]["status"],
        "pass"
    );
}

#[test]
fn constellation_refuses_a_tampered_or_incomplete_lineage() {
    let good = constellation_log();
    let mut lines: Vec<String> = good.split('\n').map(str::to_owned).collect();
    // A child naming a parent the log does not carry is an error, not a gap.
    let mut orphan: Value = serde_json::from_str(&lines[2]).unwrap();
    orphan["attempt"]["parent_attempt_id"] = json!("absent");
    lines[2] = orphan.to_string();
    assert!(constellation(lines.join("\n").as_bytes(), &QueryArgs::default()).is_err());
    // A malformed tail fails closed even when the constellation asked for is
    // elsewhere in the file.
    let broken = format!("{good}{{broken");
    assert!(constellation(broken.as_bytes(), &QueryArgs::default()).is_err());
    // A workbench report cannot masquerade as a log.
    assert!(call_report_tool("core_constellation", json!({}), b"{}").is_err());
}

fn legacy_log() -> String {
    let campaign = format!("sha256:{}", "c".repeat(64));
    let first = json!({"case_id":"case","recorded_at":"2026-09-02T00:00:00Z",
        "status":"evaluated","campaign_sha256":campaign,
        "steps":[["screen","executed"],["transport","not_run"]],
        "supplied_inputs":[{"input_id":"candidate","sha256":"sha256:cc"}],
        "verdicts":[{"requirement_id":"R1","status":"fail","margin":"-1/4"}],
        "workspace":"/workspace/case/1"});
    let second = json!({"case_id":"case","recorded_at":"2026-09-02T00:01:00Z",
        "status":"evaluated","campaign_sha256":campaign,
        "steps":[["screen","executed"],["transport","executed"]],
        "supplied_inputs":[],"verdicts":[{"requirement_id":"R1","status":"pass"}],
        "workspace":"/workspace/case/2"});
    format!("{first}\n{second}\n")
}

#[test]
fn pre_schema_campaign_logs_read_as_their_own_profile() {
    let text = legacy_log();
    let result = constellation(text.as_bytes(), &QueryArgs::default()).unwrap();
    assert_eq!(result["total"], 2);
    assert_eq!(result["summary"]["untracked_records"], 2);
    assert_eq!(result["summary"]["attempt_records"], 0);
    assert_eq!(
        result["summary"]["requirement_status_counts"]["R1"]["fail"],
        1
    );
    assert_eq!(
        result["summary"]["requirement_status_counts"]["R1"]["pass"],
        1
    );
    // Legacy `[step_id, state]` pairs project to the same shape schema'd
    // records use; nothing is fabricated — no attempt ids, no lineage.
    assert_eq!(
        result["items"][0]["steps"],
        json!([{"step_id":"screen","state":"executed"},{"step_id":"transport","state":"not_run"}])
    );
    assert_eq!(result["items"][0]["attempt_id"], Value::Null);
    let history_result = history(text.as_bytes(), &QueryArgs::default()).unwrap();
    assert_eq!(history_result["total"], 2);
    assert_eq!(history_result["items"][0]["steps"][0]["step_id"], "screen");
    // A legacy-shaped record with a corrupt campaign identity, an `attempt`
    // member, or a schema_version Core never issued still fails closed.
    for bad in [
        json!({"case_id":"case","recorded_at":"t","status":"evaluated",
            "campaign_sha256":"sha256:deadbeef","steps":[],"verdicts":[],
            "supplied_inputs":[],"workspace":"/w"}),
        json!({"case_id":"case","recorded_at":"t","status":"evaluated",
            "campaign_sha256":format!("sha256:{}","c".repeat(64)),"steps":[],
            "verdicts":[],"supplied_inputs":[],"workspace":"/w",
            "attempt":{"attempt_id":"fake"}}),
        json!({"schema_version":"avila.core/run-attempt/v9.9-draft",
            "case_id":"case","status":"evaluated","steps":[]}),
    ] {
        let bytes = format!("{}{bad}\n", text.clone());
        assert!(
            constellation(bytes.as_bytes(), &QueryArgs::default()).is_err(),
            "{bad}"
        );
        assert!(
            history(bytes.as_bytes(), &QueryArgs::default()).is_err(),
            "{bad}"
        );
    }
}

/// ADR-0019: one log carrying every record kind — explicit revisions,
/// revision-bound runs with their assessment rows, a named reference, a
/// contract amendment, and a legacy revision-less run — is queryable
/// through `core_revision`, `core_assessment`, `core_reference`, and the
/// expanded `core_constellation`.
#[test]
fn design_history_records_are_queryable_with_their_exact_identities() {
    use crate::attempt::{AttemptRecord, candidate_state_identity, digest};
    let fixture = Fixture::new();
    let log = fixture.0.join("campaign.jsonl");

    let write_doc = |name: &str, value: Value| -> (PathBuf, String) {
        let canonical = canonicalize_json(&serde_json::to_vec(&value).unwrap()).unwrap();
        let path = fixture.write(name, &canonical);
        (path, digest(&canonical))
    };
    let (candidate, _) = write_doc("candidate.json", json!({"thickness":"1"}));
    let candidate_state: Value = serde_json::from_slice(&fs::read(&candidate).unwrap()).unwrap();
    let state_sha = candidate_state_identity(&candidate_state).unwrap();
    let (prior_manifest, manifest_old) = write_doc("m1.json", json!({"question":"v1"}));
    let (new_manifest, manifest_new) = write_doc("m2.json", json!({"question":"v2"}));
    let snapshot_old = digest("snapshot-old");
    let snapshot_new = digest("snapshot-new");

    crate::create_revision(
        &log,
        &crate::RevisionRequest {
            revision_id: "rev-001".into(),
            parent_revision_id: None,
            amendment_id: None,
            candidate_input: "candidate".into(),
            candidate: candidate.clone(),
            fixed_manifest_sha256: manifest_old.clone(),
            fixed_compiled_snapshot_sha256: snapshot_old.clone(),
            created_by: "operator".into(),
            intent: Some("first proposal".into()),
        },
        None,
        None,
    )
    .unwrap();

    let run_row = |attempt_id: &str,
                   revision: &str,
                   amendment: Option<&str>,
                   manifest: &str,
                   snapshot: &str,
                   verdicts: Vec<Value>| {
        let record = AttemptRecord {
            schema_version: crate::ATTEMPT_LINEAGE_SCHEMA_VERSION.into(),
            attempt_id: attempt_id.into(),
            generation: 0,
            parent_attempt_id: None,
            parent_record_sha256: None,
            fixed_manifest_sha256: manifest.into(),
            fixed_compiled_snapshot_sha256: snapshot.into(),
            candidate_input: "candidate".into(),
            candidate_artifact_sha256: digest("candidate bytes"),
            candidate_state_sha256: state_sha.clone(),
            candidate_state: candidate_state.clone(),
            changes: Vec::new(),
        };
        let mut row = json!({
            "schema_version": "avila.core/run-attempt/v0.3-draft",
            "recorded_at": "2026-01-01T00:00:00Z",
            "case_path": "case",
            "case_id": "case",
            "status": "evaluated",
            "manifest_sha256": manifest,
            "compiled_snapshot_sha256": snapshot,
            "attempt": record,
            "verdicts": verdicts,
            "steps": [],
            "findings": [],
            "supplied_inputs": [],
            "revision_id": revision,
            "assessment_id": attempt_id,
        });
        if let Some(amendment) = amendment {
            row["amendment_id"] = json!(amendment);
        }
        let request = crate::AttemptLineageRequest {
            attempt_id: attempt_id.into(),
            parent_attempt_id: None,
            candidate_input: "candidate".into(),
            revision_id: Some(revision.into()),
            amendment_id: amendment.map(str::to_owned),
        };
        (row.to_string(), request, record)
    };
    let margin = |value: &str| json!({"requirement_id":"r","status":"pass","rule":"test","unit":"m","margin":value});
    fn append_run(
        log: &Path,
        line: &str,
        request: &crate::AttemptLineageRequest,
        attempt: &AttemptRecord,
    ) {
        let attempt = attempt.clone();
        let request = request.clone();
        crate::case_run::log::append_log_line(log, line, move |content| {
            crate::attempt::revalidate_before_append(log, content, &attempt, &request, "case", None)
        })
        .unwrap()
    }
    fn bind(
        log: &Path,
        attempt: AttemptRecord,
        revision: &str,
        run_sha256: String,
        manifest: &str,
        snapshot: &str,
        verdicts: Vec<Value>,
    ) {
        crate::history::append_assessment(
            log,
            crate::history::AssessmentBinding {
                attempt,
                revision_id: revision.into(),
                run_record_sha256: run_sha256,
                manifest_sha256: manifest.into(),
                compiled_snapshot_sha256: snapshot.into(),
                campaign_sha256: None,
                requirement_set_id: None,
                requirement_set_sha256: None,
                verdicts,
            },
            None,
            None,
        )
        .unwrap()
    }

    let (line, request, attempt) = run_row(
        "run-001",
        "rev-001",
        None,
        &manifest_old,
        &snapshot_old,
        vec![margin("1/3")],
    );
    let run_sha = digest(line.as_bytes());
    append_run(&log, &line, &request, &attempt);
    bind(
        &log,
        attempt,
        "rev-001",
        run_sha,
        &manifest_old,
        &snapshot_old,
        vec![margin("1/3")],
    );

    crate::set_reference(
        &log,
        "baseline",
        "rev-001",
        Some("run-001"),
        "operator",
        "first passing proposal",
        None,
        None,
    )
    .unwrap();

    // A revision-less legacy run derives its revision and assessment ids.
    let (line, request, attempt) = {
        let (line, mut request, record) = run_row(
            "legacy-001",
            "rev-001",
            None,
            &manifest_old,
            &snapshot_old,
            vec![margin("2/3")],
        );
        let mut row: Value = serde_json::from_str(&line).unwrap();
        row.as_object_mut().unwrap().remove("revision_id");
        row.as_object_mut().unwrap().remove("assessment_id");
        request.revision_id = None;
        (row.to_string(), request, record)
    };
    append_run(&log, &line, &request, &attempt);

    // The deliberate question change: an amendment record, then a new
    // root revision and a run citing it across the boundary.
    crate::record_amendment(
        &log,
        "amend-001",
        "rev-001",
        &prior_manifest,
        &new_manifest,
        &snapshot_new,
        "operator",
        "requirement threshold changed deliberately",
        None,
        None,
    )
    .unwrap();
    crate::create_revision(
        &log,
        &crate::RevisionRequest {
            revision_id: "rev-002".into(),
            parent_revision_id: None,
            amendment_id: Some("amend-001".into()),
            candidate_input: "candidate".into(),
            candidate,
            fixed_manifest_sha256: manifest_new.clone(),
            fixed_compiled_snapshot_sha256: snapshot_new.clone(),
            created_by: "operator".into(),
            intent: None,
        },
        None,
        None,
    )
    .unwrap();
    let (line, request, attempt) = run_row(
        "run-002",
        "rev-002",
        Some("amend-001"),
        &manifest_new,
        &snapshot_new,
        vec![margin("1/4")],
    );
    let run_sha = digest(line.as_bytes());
    append_run(&log, &line, &request, &attempt);
    bind(
        &log,
        attempt,
        "rev-002",
        run_sha,
        &manifest_new,
        &snapshot_new,
        vec![margin("1/4")],
    );

    let context = QueryContext::unrestricted();
    let revision = call_tool(
        &context,
        "core_revision",
        json!({"path": log, "id": "rev-001"}),
    )
    .unwrap();
    assert_eq!(revision["result"]["match_status"], "found");
    assert_eq!(revision["result"]["source"], "recorded");
    assert_eq!(revision["result"]["revision"]["intent"], "first proposal");
    assert_eq!(
        revision["result"]["assessments"][0]["assessment_id"],
        "run-001"
    );

    // The legacy attempt reads as a derived revision and a derived
    // assessment, both named by its attempt id.
    let derived = call_tool(
        &context,
        "core_revision",
        json!({"path": log, "id": "legacy-001"}),
    )
    .unwrap();
    assert_eq!(derived["result"]["source"], "derived");
    assert_eq!(derived["result"]["revision"]["revision_id"], "legacy-001");
    let derived_assessment = call_tool(
        &context,
        "core_assessment",
        json!({"path": log, "id": "legacy-001"}),
    )
    .unwrap();
    assert_eq!(derived_assessment["result"]["source"], "derived");
    assert_eq!(derived_assessment["result"]["verdicts"][0]["margin"], "2/3");

    let assessment = call_tool(
        &context,
        "core_assessment",
        json!({"path": log, "id": "run-001"}),
    )
    .unwrap();
    assert_eq!(assessment["result"]["source"], "recorded");
    assert_eq!(assessment["result"]["revision_id"], "rev-001");
    assert_eq!(assessment["result"]["verdicts"][0]["margin"], "1/3");

    // The amended root's assessment compares across the amendment to the
    // superseded root's latest assessment.
    let crossed = call_tool(
        &context,
        "core_assessment",
        json!({"path": log, "id": "run-002"}),
    )
    .unwrap();
    assert_eq!(
        crossed["result"]["crosses_amendment"]["amendment_id"],
        "amend-001"
    );
    assert_eq!(crossed["result"]["comparison"]["basis"], "superseded_root");
    assert_eq!(
        crossed["result"]["comparison"]["parent_assessment_id"],
        "run-001"
    );

    let reference = call_tool(
        &context,
        "core_reference",
        json!({"path": log, "id": "baseline"}),
    )
    .unwrap();
    assert_eq!(reference["result"]["current"]["revision_id"], "rev-001");
    assert_eq!(reference["result"]["current"]["assessment_id"], "run-001");
    assert_eq!(reference["result"]["moves"].as_array().unwrap().len(), 1);

    let constellation = call_tool(&context, "core_constellation", json!({"path": log})).unwrap();
    let summary = &constellation["result"]["summary"];
    assert_eq!(summary["design_revision_records"], 2);
    assert_eq!(summary["assessment_records"], 2);
    assert_eq!(summary["named_reference_records"], 1);
    assert_eq!(summary["contract_amendment_records"], 1);
    assert_eq!(summary["references"]["baseline"]["revision_id"], "rev-001");
    assert_eq!(
        summary["revision_ids"],
        json!(["rev-001", "rev-002", "legacy-001"])
    );
    let kinds: Vec<&str> = constellation["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["record_kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "design_revision",
            "run",
            "assessment",
            "named_reference",
            "run",
            "contract_amendment",
            "design_revision",
            "run",
            "assessment",
        ]
    );
    let history_items = history(fs::read(&log).unwrap().as_slice(), &QueryArgs::default()).unwrap();
    assert_eq!(history_items["total"], 9);
    // An unknown revision or assessment is a scoped no-match, never an error.
    for (tool, id) in [("core_revision", "absent"), ("core_assessment", "absent")] {
        assert_eq!(
            call_tool(&context, tool, json!({"path": log, "id": id})).unwrap()["result"]["match_status"],
            "no_match_in_record"
        );
    }
}
