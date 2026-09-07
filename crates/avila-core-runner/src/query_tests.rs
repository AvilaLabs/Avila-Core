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
    let case = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/cases/case-003-thermal-spreader");
    let report = crate::execute_case(
        &case,
        &crate::CaseRunOptions {
            log: Some(log_path.clone()),
            source_roots: std::collections::BTreeMap::from([
                ("case".into(), case.clone()),
                ("thermal".into(), case.join("../../capabilities/thermal")),
            ]),
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
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(&log_path).unwrap(), history_bytes);
}
