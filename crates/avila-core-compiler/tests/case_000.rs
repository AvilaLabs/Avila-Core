//! Keeps the first composed internal case pinned to its source identities and
//! technical campaign result.

use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::evaluate_campaign;
use serde_json::{Value, json};

fn case_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-000-actinv-aftermatter")
}

#[test]
fn case_000_is_reproducible_and_technically_evaluated() {
    let root = case_root();
    let contract = fs::read(root.join("contract.json")).unwrap();
    let registry = fs::read(root.join("registry.json")).unwrap();
    let claims = fs::read(root.join("claims.json")).unwrap();

    let report = evaluate_campaign(&contract, &registry, &claims).unwrap();
    assert_eq!(
        report,
        evaluate_campaign(&contract, &registry, &claims).unwrap(),
        "CASE-000 evaluation must be deterministic"
    );

    let actual = serde_json::to_value(report).unwrap();
    let expected: Value =
        serde_json::from_slice(&fs::read(root.join("campaign-report.json")).unwrap()).unwrap();
    assert_eq!(actual, expected, "committed campaign report drifted");

    let admissions = actual["admissions"].as_array().unwrap();
    assert_eq!(admissions.len(), 20);
    assert!(
        admissions
            .iter()
            .all(|record| record["state"] == json!("admitted")),
        "every frozen attestation and claim must remain admitted"
    );

    let verdicts = actual["verdicts"].as_array().unwrap();
    assert_eq!(verdicts.len(), 2);
    assert!(verdicts.iter().all(|record| {
        record["verdict"]["status"] == json!("pass")
            && record["verdict"]["rule"] == json!("bounded.lt.within")
    }));

    let provenance: Value =
        serde_json::from_slice(&fs::read(root.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(provenance["case_id"], json!("CASE-000"));
    assert_eq!(
        provenance["source_repositories"][0]["release"],
        json!("v1.0.1")
    );
    assert_eq!(
        provenance["source_repositories"][1]["commit"],
        json!("70a1c341d478bc37bf1ed0206dad4ee507cf743d")
    );
    assert_eq!(
        provenance["observed_result"]["routes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|route| route["state"] == json!("unresolved"))
            .count(),
        3
    );
}
