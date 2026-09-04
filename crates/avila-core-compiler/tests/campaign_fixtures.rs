//! Executes the campaign-evaluation fixtures: a contract, a registry, and a
//! claims document in, and the expected status, findings, admission states,
//! verdicts, and campaign identity out.

use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::{compile_documents, evaluate_campaign};
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
    assert_eq!(suite.fixtures.len(), 15);

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

#[test]
fn optional_agent_review_is_not_a_technical_verdict_input() {
    let root = fixture_root();
    let contract: Value = serde_json::from_slice(
        &fs::read(root.join("../types/types.R9.review-bound.pass.contract.json")).unwrap(),
    )
    .unwrap();
    let registry: Value = serde_json::from_slice(
        &fs::read(root.join("../types/compiler.review.registry.v1.json")).unwrap(),
    )
    .unwrap();

    let contract = serde_json::to_vec(&contract).unwrap();
    let registry = serde_json::to_vec(&registry).unwrap();
    let compile = compile_documents(&contract, &registry).unwrap();
    let snapshot = compile.compiled.unwrap().snapshot_sha256;
    let mut claims: Value = serde_json::from_slice(
        &fs::read(root.join("campaign.practical-review.pass.claims.json")).unwrap(),
    )
    .unwrap();
    claims["compiled_snapshot_sha256"] = json!(snapshot);
    let report =
        evaluate_campaign(&contract, &registry, &serde_json::to_vec(&claims).unwrap()).unwrap();

    assert!(report.findings.is_empty());
    assert_eq!(report.verdicts.len(), 1);
    assert_eq!(
        report.verdicts[0].verdict.status,
        avila_core_kernel::VerdictStatus::Pass
    );
}

#[test]
fn categorical_claims_produce_closed_set_pass_and_fail_verdicts() {
    let root = fixture_root();
    let mut contract: Value = serde_json::from_slice(
        &fs::read(root.join("../types/types.R1.resolved.pass.contract.json")).unwrap(),
    )
    .unwrap();
    let mut registry: Value =
        serde_json::from_slice(&fs::read(root.join("../types/compiler.registry.v1.json")).unwrap())
            .unwrap();
    registry["roles"].as_array_mut().unwrap().push(json!({
        "role": {"id": "fixture.classification", "major": 1},
        "owner": "fixture.method_owner",
        "validator": "fixture.validate.classification@1",
        "accepted_media_types": ["application/vnd.fixture.category+json"],
        "permitted_claim_models": [{"model": "unquantified"}],
        "categorical_values": ["clear", "rejected"]
    }));
    registry["capability_types"][0]["outputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "slot_id": "classification",
            "role": {"id": "fixture.classification", "major": 1},
            "media_type": "application/vnd.fixture.category+json",
            "permitted_claim_models": [{"model": "unquantified"}]
        }));
    contract["categorical_requirements"] = json!([{
        "requirement_id": "R-CATEGORY",
        "statement": "The classification must be clear.",
        "purpose": {"id": "fixture.requirement_evaluation", "major": 1},
        "metric": {"source": "step_output", "step_id": "calculate", "output_slot": "classification"},
        "predicate": {"operator": "equals", "value": "clear"}
    }]);

    let contract = serde_json::to_vec(&contract).unwrap();
    let registry = serde_json::to_vec(&registry).unwrap();
    let snapshot = compile_documents(&contract, &registry)
        .unwrap()
        .compiled
        .unwrap()
        .snapshot_sha256;
    let mut claims: Value = serde_json::from_slice(
        &fs::read(root.join("campaign.le.within.pass.claims.json")).unwrap(),
    )
    .unwrap();
    claims["compiled_snapshot_sha256"] = json!(snapshot);
    claims["claims"].as_array_mut().unwrap().push(json!({
        "claim_id": "classification",
        "step_id": "calculate",
        "output_slot": "classification",
        "artifact": {
            "sha256": "sha256:abababababababababababababababababababababababababababababababab",
            "media_type": "application/vnd.fixture.category+json"
        },
        "claim": {"model": "unquantified", "value": "clear"}
    }));

    let report =
        evaluate_campaign(&contract, &registry, &serde_json::to_vec(&claims).unwrap()).unwrap();
    let categorical = report
        .verdicts
        .iter()
        .find(|verdict| verdict.requirement_id == "R-CATEGORY")
        .unwrap();
    assert_eq!(
        categorical.verdict.status,
        avila_core_kernel::VerdictStatus::Pass
    );
    assert_eq!(categorical.verdict.rule, "categorical.equals.match");
    assert_eq!(
        categorical.verdict.observed_category.as_deref(),
        Some("clear")
    );

    let category_claim = claims["claims"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|claim| claim["claim_id"] == "classification")
        .unwrap();
    category_claim["claim"]["value"] = json!("rejected");
    let report =
        evaluate_campaign(&contract, &registry, &serde_json::to_vec(&claims).unwrap()).unwrap();
    let categorical = report
        .verdicts
        .iter()
        .find(|verdict| verdict.requirement_id == "R-CATEGORY")
        .unwrap();
    assert_eq!(
        categorical.verdict.status,
        avila_core_kernel::VerdictStatus::Fail
    );
    assert_eq!(categorical.verdict.rule, "categorical.equals.mismatch");
}
