//! Executes the campaign-evaluation fixtures: a contract, a registry, and a
//! claims document in, and the expected status, findings, admission states,
//! verdicts, and campaign identity out.

use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::evaluate_campaign;
use avila_core_kernel::SEMANTIC_PROFILE;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    fixture_set: String,
    version: u64,
    semantic_profile: String,
    fixtures: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    fixture_id: String,
    clause: String,
    contract: String,
    registry: String,
    claims: String,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    status: String,
    findings: Vec<Value>,
    admissions: Vec<Value>,
    verdicts: Vec<Value>,
    #[serde(default)]
    campaign_sha256: Option<String>,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/semantic-core/campaigns")
}

#[test]
fn campaign_fixtures_are_executable() {
    let root = fixture_root();
    let suite: Suite =
        serde_json::from_slice(&fs::read(root.join("campaign-cases.v1.json")).unwrap()).unwrap();
    assert_eq!(suite.fixture_set, "campaign-evaluation");
    assert_eq!(suite.version, 1);
    assert_eq!(suite.semantic_profile, SEMANTIC_PROFILE);
    assert_eq!(suite.fixtures.len(), 12);

    for case in suite.fixtures {
        assert!(!case.clause.is_empty(), "{} has no clause", case.fixture_id);
        let contract = fs::read(root.join(&case.contract)).unwrap();
        let registry = fs::read(root.join(&case.registry)).unwrap();
        let claims = fs::read(root.join(&case.claims)).unwrap();
        let report = evaluate_campaign(&contract, &registry, &claims).unwrap();
        assert_eq!(
            report,
            evaluate_campaign(&contract, &registry, &claims).unwrap(),
            "{}: nondeterministic",
            case.fixture_id
        );
        let report = serde_json::to_value(&report).unwrap();
        assert_eq!(
            report["status"],
            json!(case.expected.status),
            "{} status",
            case.fixture_id
        );

        let findings: Vec<Value> = report["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|finding| {
                json!({
                    "code": finding["code"],
                    "class": finding["class"],
                    "owner": finding["owner"],
                    "primary": finding["primary"],
                })
            })
            .collect();
        assert_eq!(
            findings, case.expected.findings,
            "{} findings",
            case.fixture_id
        );

        let admissions: Vec<Value> = report["admissions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|record| {
                json!({
                    "evidence_id": record["evidence_id"],
                    "state": record["state"],
                    "reasons": record["reasons"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|reason| reason["code"].clone())
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        assert_eq!(
            admissions, case.expected.admissions,
            "{} admissions",
            case.fixture_id
        );

        let verdicts: Vec<Value> = report["verdicts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|record| {
                json!({
                    "requirement_id": record["requirement_id"],
                    "status": record["verdict"]["status"],
                    "rule": record["verdict"]["rule"],
                    "reviews_outstanding": record["verdict"]["reviews_outstanding"].as_array().cloned().unwrap_or_default(),
                })
            })
            .collect();
        assert_eq!(
            verdicts, case.expected.verdicts,
            "{} verdicts",
            case.fixture_id
        );

        match &case.expected.campaign_sha256 {
            Some(expected) => assert_eq!(
                report["campaign_sha256"],
                json!(expected),
                "{} campaign identity",
                case.fixture_id
            ),
            None => assert!(
                report.get("campaign_sha256").is_none(),
                "{} unexpectedly has an identity",
                case.fixture_id
            ),
        }
    }
}
