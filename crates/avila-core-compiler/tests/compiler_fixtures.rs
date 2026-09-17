use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::{
    CompilationStatus, FindingClass, RepairApplicability, SourceLocation, compile_documents,
};
use avila_core_kernel::{SEMANTIC_PROFILE, canonicalize_json};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureSuite {
    fixture_set: String,
    version: u64,
    semantic_profile: String,
    registry: RegistryFixture,
    fixtures: Vec<CompilerFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFixture {
    path: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompilerFixture {
    fixture_id: String,
    clause: String,
    contract: String,
    expected: ExpectedReport,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedReport {
    status: String,
    findings: Vec<ExpectedFinding>,
    #[serde(default)]
    compiled: Option<ExpectedCompiled>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedFinding {
    code: String,
    class: FindingClass,
    owner: String,
    primary: SourceLocation,
    repairs: Vec<RepairApplicability>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedCompiled {
    snapshot_sha256: String,
    execution_policy: serde_json::Value,
    step_order: Vec<String>,
    bindings: Vec<ExpectedBinding>,
    #[serde(default)]
    parameters: Vec<ExpectedParameter>,
    reproducibility: Vec<ExpectedReproducibility>,
    #[serde(default)]
    reviews: Vec<ExpectedReview>,
    limits: Vec<ExpectedLimit>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedBinding {
    step_id: String,
    input_slot: String,
    source: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedLimit {
    requirement_id: String,
    purpose: String,
    kind: String,
    value: String,
    unit: String,
    #[serde(default)]
    tolerance: Option<serde_json::Value>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedParameter {
    step_id: String,
    parameter_id: String,
    value: serde_json::Value,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedReproducibility {
    step_id: String,
    value: serde_json::Value,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedReview {
    step_id: String,
    value: serde_json::Value,
}

#[test]
fn compiler_type_fixtures_are_executable() {
    let fixture_root = fixture_root();
    let mut fixture_count = 0;
    for suite_name in [
        "compiler-cases.v1.json",
        "compiler-parameter-cases.v1.json",
        "compiler-reproducibility-cases.v1.json",
        "compiler-review-cases.v1.json",
        "compiler-purpose-cases.v1.json",
    ] {
        fixture_count += execute_suite(&fixture_root, suite_name);
    }
    assert_eq!(fixture_count, 68);
}

// ---------------------------------------------------------------------------
// Real-contract defect corpus: the CASE-001 contract+registry pair verbatim
// with one seeded defect per fixture, applied as JSON-pointer mutations at
// test time. Unlike the minimal type-suite contracts, every finding here is
// produced against a contract that is a committed example case.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefectSuite {
    fixture_set: String,
    version: u64,
    semantic_profile: String,
    contract: RegistryFixture,
    registry: RegistryFixture,
    fixtures: Vec<DefectFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefectFixture {
    fixture_id: String,
    defect: String,
    #[serde(default)]
    contract_mutations: Vec<Mutation>,
    #[serde(default)]
    registry_mutations: Vec<Mutation>,
    expected: DefectExpected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mutation {
    op: String,
    pointer: String,
    #[serde(default)]
    value: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefectExpected {
    status: String,
    findings: Vec<ExpectedFinding>,
    #[serde(default)]
    snapshot_sha256: Option<String>,
}

#[test]
fn compiler_defect_fixtures_are_executable() {
    let defect_root = fixture_root().join("../defects");
    let suite_bytes = fs::read(defect_root.join("defects.v1.json")).unwrap();
    let suite: DefectSuite = serde_json::from_slice(&suite_bytes).unwrap();
    assert_eq!(suite.fixture_set, "compiler-defects");
    assert_eq!(suite.version, 1);
    assert_eq!(suite.semantic_profile, SEMANTIC_PROFILE);

    let contract_bytes = fs::read(defect_root.join(&suite.contract.path)).unwrap();
    let registry_bytes = fs::read(defect_root.join(&suite.registry.path)).unwrap();
    assert_eq!(
        prefixed_sha256(canonicalize_json(&contract_bytes).unwrap()),
        suite.contract.sha256
    );
    assert_eq!(
        prefixed_sha256(canonicalize_json(&registry_bytes).unwrap()),
        suite.registry.sha256
    );
    let base_contract: serde_json::Value = serde_json::from_slice(&contract_bytes).unwrap();
    let base_registry: serde_json::Value = serde_json::from_slice(&registry_bytes).unwrap();

    // The unmutated base pair must compile: every fixture's findings are
    // then attributable to its seeded defect, not the corpus itself.
    let base_report = compile_documents(&contract_bytes, &registry_bytes).unwrap();
    assert_eq!(base_report.status, CompilationStatus::Compiled);
    assert!(
        base_report
            .findings
            .iter()
            .all(|finding| finding.class == FindingClass::Notice),
        "the defect corpus base must carry notices only"
    );

    for fixture in &suite.fixtures {
        assert!(
            !fixture.defect.is_empty(),
            "{} does not name its seeded defect",
            fixture.fixture_id
        );
        let mut contract = base_contract.clone();
        let mut registry = base_registry.clone();
        apply_mutations(&mut contract, &fixture.contract_mutations);
        apply_mutations(&mut registry, &fixture.registry_mutations);
        let report = compile_documents(
            serde_json::to_string_pretty(&contract).unwrap().as_bytes(),
            serde_json::to_string_pretty(&registry).unwrap().as_bytes(),
        )
        .unwrap();

        assert_eq!(
            status_label(report.status),
            fixture.expected.status,
            "{} status",
            fixture.fixture_id
        );
        let actual_findings: Vec<_> = report
            .findings
            .iter()
            .map(|finding| ExpectedFinding {
                code: finding.code.clone(),
                class: finding.class,
                owner: finding.owner.clone(),
                primary: finding.primary.clone(),
                repairs: finding
                    .repairs
                    .iter()
                    .map(|repair| repair.applicability)
                    .collect(),
            })
            .collect();
        assert_eq!(
            actual_findings, fixture.expected.findings,
            "{} findings",
            fixture.fixture_id
        );
        match &fixture.expected.snapshot_sha256 {
            Some(expected) => assert_eq!(
                report.compiled.as_ref().unwrap().snapshot_sha256,
                *expected,
                "{} snapshot identity",
                fixture.fixture_id
            ),
            None => assert!(
                report.compiled.is_none(),
                "{} unexpectedly compiled",
                fixture.fixture_id
            ),
        }
    }
    assert_eq!(suite.fixtures.len(), 23);
}

fn apply_mutations(document: &mut serde_json::Value, mutations: &[Mutation]) {
    for mutation in mutations {
        let (parent_pointer, leaf) = mutation
            .pointer
            .rsplit_once('/')
            .expect("a mutation pointer always has a parent");
        let parent = if parent_pointer.is_empty() {
            &mut *document
        } else {
            document
                .pointer_mut(parent_pointer)
                .unwrap_or_else(|| panic!("no mutation parent at {parent_pointer}"))
        };
        let leaf = leaf.replace("~1", "/").replace("~0", "~");
        match mutation.op.as_str() {
            "set" => match parent {
                serde_json::Value::Array(items) => {
                    items[leaf.parse::<usize>().unwrap()] =
                        mutation.value.clone().expect("set carries a value");
                }
                serde_json::Value::Object(map) => {
                    map.insert(leaf, mutation.value.clone().expect("set carries a value"));
                }
                other => panic!("cannot set inside {other:?}"),
            },
            "remove" => match parent {
                serde_json::Value::Array(items) => {
                    items.remove(leaf.parse::<usize>().unwrap());
                }
                serde_json::Value::Object(map) => {
                    map.remove(&leaf).expect("remove names a present key");
                }
                other => panic!("cannot remove inside {other:?}"),
            },
            other => panic!("unknown mutation op {other:?}"),
        }
    }
}

fn execute_suite(fixture_root: &Path, suite_name: &str) -> usize {
    let suite_bytes = fs::read(fixture_root.join(suite_name)).unwrap();
    let suite: FixtureSuite = serde_json::from_slice(&suite_bytes).unwrap();
    assert!(!suite.fixture_set.is_empty());
    assert_eq!(suite.version, 1);
    assert_eq!(suite.semantic_profile, SEMANTIC_PROFILE);

    let registry_bytes = fs::read(fixture_root.join(&suite.registry.path)).unwrap();
    let registry_canonical = canonicalize_json(&registry_bytes).unwrap();
    assert_eq!(prefixed_sha256(&registry_canonical), suite.registry.sha256);

    let fixture_count = suite.fixtures.len();
    for fixture in suite.fixtures {
        assert!(
            !fixture.clause.is_empty(),
            "{} has no clause",
            fixture.fixture_id
        );
        let contract_bytes = fs::read(fixture_root.join(&fixture.contract)).unwrap();
        let report = compile_documents(&contract_bytes, &registry_bytes).unwrap();

        assert_eq!(
            status_label(report.status),
            fixture.expected.status,
            "{} status",
            fixture.fixture_id
        );
        let actual_findings: Vec<_> = report
            .findings
            .iter()
            .map(|finding| ExpectedFinding {
                code: finding.code.clone(),
                class: finding.class,
                owner: finding.owner.clone(),
                primary: finding.primary.clone(),
                repairs: finding
                    .repairs
                    .iter()
                    .map(|repair| repair.applicability)
                    .collect(),
            })
            .collect();
        assert_eq!(
            actual_findings, fixture.expected.findings,
            "{} findings",
            fixture.fixture_id
        );

        match fixture.expected.compiled {
            Some(expected) => {
                let compiled = report
                    .compiled
                    .as_ref()
                    .unwrap_or_else(|| panic!("{} did not compile", fixture.fixture_id));
                assert_eq!(
                    compiled.snapshot_sha256, expected.snapshot_sha256,
                    "{} snapshot identity",
                    fixture.fixture_id
                );
                assert_eq!(
                    serde_json::to_value(&compiled.execution_policy).unwrap(),
                    expected.execution_policy,
                    "{} execution policy",
                    fixture.fixture_id
                );
                let step_order: Vec<_> = compiled
                    .workflow
                    .iter()
                    .map(|step| step.step_id.clone())
                    .collect();
                assert_eq!(
                    step_order, expected.step_order,
                    "{} order",
                    fixture.fixture_id
                );
                let bindings: Vec<_> = compiled
                    .workflow
                    .iter()
                    .flat_map(|step| {
                        step.bindings.iter().map(|binding| ExpectedBinding {
                            step_id: step.step_id.clone(),
                            input_slot: binding.input_slot.clone(),
                            source: binding.source.label(),
                        })
                    })
                    .collect();
                assert_eq!(
                    bindings, expected.bindings,
                    "{} bindings",
                    fixture.fixture_id
                );
                let parameters: Vec<_> = compiled
                    .workflow
                    .iter()
                    .flat_map(|step| {
                        step.parameters
                            .iter()
                            .map(|(parameter_id, value)| ExpectedParameter {
                                step_id: step.step_id.clone(),
                                parameter_id: parameter_id.clone(),
                                value: serde_json::to_value(value).unwrap(),
                            })
                    })
                    .collect();
                assert_eq!(
                    parameters, expected.parameters,
                    "{} parameters",
                    fixture.fixture_id
                );
                let reproducibility: Vec<_> = compiled
                    .workflow
                    .iter()
                    .map(|step| ExpectedReproducibility {
                        step_id: step.step_id.clone(),
                        value: serde_json::to_value(&step.reproducibility).unwrap(),
                    })
                    .collect();
                assert_eq!(
                    reproducibility, expected.reproducibility,
                    "{} reproducibility",
                    fixture.fixture_id
                );
                let reviews: Vec<_> = compiled
                    .workflow
                    .iter()
                    .filter_map(|step| {
                        step.presentation_gate
                            .as_ref()
                            .map(|review| ExpectedReview {
                                step_id: step.step_id.clone(),
                                value: serde_json::to_value(review).unwrap(),
                            })
                    })
                    .collect();
                assert_eq!(reviews, expected.reviews, "{} reviews", fixture.fixture_id);
                let limits: Vec<_> = compiled
                    .requirements
                    .iter()
                    .map(|requirement| ExpectedLimit {
                        requirement_id: requirement.requirement_id.clone(),
                        purpose: format!(
                            "{}@{}",
                            requirement.purpose.id, requirement.purpose.major
                        ),
                        kind: requirement.limit.kind.clone(),
                        value: requirement.limit.value.clone(),
                        unit: requirement.limit.unit.clone(),
                        tolerance: requirement
                            .tolerance
                            .as_ref()
                            .map(|tolerance| serde_json::to_value(tolerance).unwrap()),
                    })
                    .collect();
                assert_eq!(limits, expected.limits, "{} limits", fixture.fixture_id);
            }
            None => assert!(
                report.compiled.is_none(),
                "{} unexpectedly compiled",
                fixture.fixture_id
            ),
        }
    }
    fixture_count
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/semantic-core/types")
}

const fn status_label(status: CompilationStatus) -> &'static str {
    match status {
        CompilationStatus::Compiled => "compiled",
        CompilationStatus::Rejected => "rejected",
    }
}

fn prefixed_sha256(bytes: impl AsRef<[u8]>) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes.as_ref()))
}
