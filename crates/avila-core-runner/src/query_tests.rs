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
