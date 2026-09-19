//! Every code in the diagnostic catalog must be exercised somewhere a
//! committed check can see it: a fixture suite's expected findings or
//! verdict reasons, or a named non-fixture test. A code that is emitted
//! but never pinned fails here, so a new catalog entry cannot land
//! unexercised.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::DIAGNOSTIC_CATALOG;
use serde_json::Value;

/// Codes pinned by named tests rather than a fixture document — finding
/// classes that only the runner can produce (they need receipts, signed
/// manifests, or a filesystem). Each entry names where the coverage lives
/// so a stale entry is a readable failure, not a dead list.
const TEST_PINNED: &[(&str, &str)] = &[
    (
        "CORE-A4401",
        "avila-core-runner execute::adversarial_tests — evidence outside its qualification envelope",
    ),
    (
        "CORE-A4402",
        "avila-core-runner execute::adversarial_tests — require_qualification with unqualified evidence",
    ),
    (
        "CORE-T2701",
        "avila-core-runner case_run::qualification::tests — qualification-scope attributes checked at record load (the check is compiler-owned; only the runner sees a bound record)",
    ),
    (
        "CORE-T2702",
        "avila-core-compiler compile::tests — input attribute vocabulary checks (undeclared name, outside domain, missing required, metadata overlap/non-scalar)",
    ),
    (
        "CORE-A4801",
        "avila-core-compiler compile::tests::instantiation — record/template resolution and digest pinning (instantiation material is a package input, not a compile fixture)",
    ),
    (
        "CORE-A4802",
        "avila-core-compiler compile::tests::instantiation — parameter binding/domain checks and input coverage",
    ),
    (
        "CORE-A4803",
        "avila-core-compiler compile::tests::instantiation — policy-floor tightening",
    ),
    (
        "CORE-A4804",
        "avila-core-compiler compile::tests::instantiation — eligibility re-derivation, eligible-or-refused",
    ),
    (
        "CORE-A4805",
        "avila-core-compiler compile::tests::instantiation — validation-case materialization/compile",
    ),
    (
        "CORE-A4806",
        "avila-core-compiler compile::tests::instantiation — template_superseded notice",
    ),
];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/semantic-core")
}

/// Every finding code a JSON fixture suite expects — compile-time
/// `expected.findings[].code` plus campaign verdict `reasons[]` entries.
fn collect_fixture_codes() -> BTreeMap<String, Vec<String>> {
    let mut exercised: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut record = |code: &str, source: String| {
        exercised.entry(code.to_string()).or_default().push(source);
    };
    for suite_path in [
        "types/compiler-cases.v1.json",
        "types/compiler-parameter-cases.v1.json",
        "types/compiler-reproducibility-cases.v1.json",
        "types/compiler-review-cases.v1.json",
        "types/compiler-purpose-cases.v1.json",
        "defects/defects.v1.json",
        "campaigns/campaign-cases.v1.json",
    ] {
        let suite: Value =
            serde_json::from_slice(&fs::read(fixture_root().join(suite_path)).unwrap()).unwrap();
        for fixture in suite["fixtures"].as_array().into_iter().flatten() {
            let fixture_id = fixture["fixture_id"].as_str().unwrap_or("?");
            for finding in fixture["expected"]["findings"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(code) = finding["code"].as_str() {
                    record(code, format!("{suite_path}:{fixture_id}"));
                }
            }
            for verdict in fixture["expected"]["verdicts"]
                .as_array()
                .into_iter()
                .flatten()
            {
                for reason in verdict["reasons"].as_array().into_iter().flatten() {
                    if let Some(code) = reason.as_str() {
                        record(code, format!("{suite_path}:{fixture_id}"));
                    }
                }
            }
        }
    }
    exercised
}

#[test]
fn every_catalog_code_is_exercised() {
    let exercised = collect_fixture_codes();
    let mut uncovered = Vec::new();
    let mut stale_pins = Vec::new();
    for entry in DIAGNOSTIC_CATALOG {
        if exercised.contains_key(entry.code) {
            continue;
        }
        match TEST_PINNED.iter().find(|(code, _)| *code == entry.code) {
            Some(_) => {}
            None => uncovered.push(entry.code),
        }
    }
    for (code, _) in TEST_PINNED {
        if exercised.contains_key(*code) {
            stale_pins.push(*code);
        }
    }
    assert!(
        uncovered.is_empty(),
        "catalog codes exercised by no fixture and no named test: {uncovered:?}"
    );
    assert!(
        stale_pins.is_empty(),
        "TEST_PINNED entries now covered by a fixture — remove them: {stale_pins:?}"
    );
    assert_eq!(
        DIAGNOSTIC_CATALOG.len(),
        exercised
            .keys()
            .filter(|code| code.starts_with("CORE-"))
            .count()
            + TEST_PINNED.len(),
        "coverage accounting drifted"
    );
}
