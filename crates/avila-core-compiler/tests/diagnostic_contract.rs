//! The diagnostic contract: every finding must be actionable by a tool that
//! has never seen the document before.
//!
//! The harness takes every compiled fixture, applies one of a fixed set of
//! authoring mistakes with a deterministic generator, and checks each report:
//!
//! 1. compiling twice yields identical reports;
//! 2. every pointer anchors in the mutated document, exactly, as a documented
//!    logical slot location, or as a missing property under an existing parent;
//! 3. every `mechanically_safe` repair has one candidate and removes its
//!    finding when applied verbatim;
//! 4. every `constrained_choice` on a substitutable value removes its finding
//!    when its first candidate is applied, and every feeding candidate on an
//!    unfed slot names something the snapshot actually defines; and
//! 5. a fixer that applies only the compiler's own repairs reaches a compiled
//!    snapshot within two rounds for every mistake that is mechanically
//!    repairable at all.
//!
//! The summary it prints is the first measurement of the thesis that compiler
//! feedback lets an agent repair a contract without re-reading everything.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::{
    CompilationStatus, CompileReport, CoreDiagnostic, FindingClass, RepairApplicability,
    compile_documents,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct Manifest {
    registry: RegistryRef,
    fixtures: Vec<Fixture>,
}

#[derive(Debug, Deserialize)]
struct RegistryRef {
    path: String,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    fixture_id: String,
    contract: String,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
struct Expected {
    status: String,
}

struct Base {
    fixture_id: String,
    contract: Value,
    registry: Value,
    registry_bytes: Vec<u8>,
}

/// Deterministic generator so every run mutates the corpus identically.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mistake {
    UnknownField,
    FloatParameter,
    NoncanonicalLimit,
    WrongComparison,
    MissingStatement,
    UnknownUnit,
    UnknownCapabilityType,
    DanglingBinding,
    EqualWithoutTolerance,
    BadCoverage,
    DroppedInput,
    DuplicateStepId,
    UnknownPurpose,
    NullValue,
}

impl Mistake {
    const ALL: [Self; 14] = [
        Self::UnknownField,
        Self::FloatParameter,
        Self::NoncanonicalLimit,
        Self::WrongComparison,
        Self::MissingStatement,
        Self::UnknownUnit,
        Self::UnknownCapabilityType,
        Self::DanglingBinding,
        Self::EqualWithoutTolerance,
        Self::BadCoverage,
        Self::DroppedInput,
        Self::DuplicateStepId,
        Self::UnknownPurpose,
        Self::NullValue,
    ];

    /// Mistakes whose only correct fix is one the compiler states outright:
    /// remove the key, use the canonical form, or pick a listed value.
    const fn mechanically_repairable(self) -> bool {
        matches!(
            self,
            Self::UnknownField
                | Self::NoncanonicalLimit
                | Self::WrongComparison
                | Self::UnknownUnit
        )
    }

    /// Applies the mistake; returns false when the document offers no target.
    fn apply(self, doc: &mut Value, rng: &mut XorShift) -> bool {
        let steps = doc["workflow"].as_array().map_or(0, Vec::len);
        let requirements = doc["requirements"].as_array().map_or(0, Vec::len);
        let inputs = doc["inputs"].as_array().map_or(0, Vec::len);
        match self {
            Self::UnknownField => {
                let target = match rng.below(3) {
                    0 => "".to_owned(),
                    1 => format!("/workflow/{}", rng.below(steps)),
                    _ => format!("/requirements/{}", rng.below(requirements)),
                };
                let Some(object) = doc.pointer_mut(&target).and_then(Value::as_object_mut) else {
                    return false;
                };
                object.insert("unexpected_field".into(), json!(1));
                true
            }
            Self::FloatParameter => {
                let step = &mut doc["workflow"][rng.below(steps)];
                if !step["parameters"].is_object() {
                    step["parameters"] = json!({});
                }
                step["parameters"]["unexpected_ratio"] = json!(1.5);
                true
            }
            Self::NoncanonicalLimit => {
                let value = &mut doc["requirements"][rng.below(requirements)]["limit"]["value"];
                let Some(text) = value.as_str() else {
                    return false;
                };
                let widened = if text.contains('.') {
                    format!("{text}0")
                } else if text.contains('/') {
                    return false;
                } else {
                    format!("{text}.0")
                };
                *value = json!(widened);
                true
            }
            Self::WrongComparison => {
                doc["requirements"][rng.below(requirements)]["comparison"] = json!("lessthan");
                true
            }
            Self::MissingStatement => doc["requirements"][rng.below(requirements)]
                .as_object_mut()
                .is_some_and(|requirement| requirement.remove("statement").is_some()),
            Self::UnknownUnit => {
                doc["requirements"][rng.below(requirements)]["limit"]["unit"] = json!("bogus_unit");
                true
            }
            Self::UnknownCapabilityType => {
                doc["workflow"][rng.below(steps)]["capability_type"]["id"] =
                    json!("fixture.does_not_exist");
                true
            }
            Self::DanglingBinding => {
                let step = &mut doc["workflow"][rng.below(steps)];
                if !step["bindings"].is_array() {
                    step["bindings"] = json!([]);
                }
                step["bindings"].as_array_mut().unwrap().push(json!({
                    "input_slot": "dangling_slot",
                    "source": {"source": "step_output", "step_id": "no_such_step", "output_slot": "x"}
                }));
                true
            }
            Self::EqualWithoutTolerance => {
                let requirement = &mut doc["requirements"][rng.below(requirements)];
                requirement["comparison"] = json!("equal");
                requirement.as_object_mut().unwrap().remove("tolerance");
                true
            }
            Self::BadCoverage => {
                doc["requirements"][rng.below(requirements)]["basis"] =
                    json!({"kind": "bounded", "coverage": "1.5"});
                true
            }
            Self::DroppedInput => {
                if inputs == 0 {
                    return false;
                }
                let index = rng.below(inputs);
                doc["inputs"].as_array_mut().unwrap().remove(index);
                true
            }
            Self::DuplicateStepId => {
                if steps < 2 {
                    return false;
                }
                let first = doc["workflow"][0]["step_id"].clone();
                doc["workflow"][1]["step_id"] = first;
                true
            }
            Self::UnknownPurpose => {
                doc["requirements"][rng.below(requirements)]["purpose"]["id"] =
                    json!("fixture.no_such_purpose");
                true
            }
            Self::NullValue => {
                doc["requirements"][rng.below(requirements)]["statement"] = Value::Null;
                true
            }
        }
    }
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/semantic-core/types")
}

fn compiled_bases() -> Vec<Base> {
    let root = fixture_root();
    let mut bases = Vec::new();
    for manifest_name in [
        "compiler-cases.v1.json",
        "compiler-parameter-cases.v1.json",
        "compiler-reproducibility-cases.v1.json",
        "compiler-review-cases.v1.json",
        "compiler-purpose-cases.v1.json",
    ] {
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(root.join(manifest_name)).unwrap()).unwrap();
        let registry_bytes = fs::read(root.join(&manifest.registry.path)).unwrap();
        let registry: Value = serde_json::from_slice(&registry_bytes).unwrap();
        for fixture in manifest.fixtures {
            if fixture.expected.status != "compiled" {
                continue;
            }
            let contract: Value =
                serde_json::from_slice(&fs::read(root.join(&fixture.contract)).unwrap()).unwrap();
            bases.push(Base {
                fixture_id: fixture.fixture_id,
                contract,
                registry: registry.clone(),
                registry_bytes: registry_bytes.clone(),
            });
        }
    }
    assert!(
        bases.len() >= 10,
        "the corpus should have many compiled bases"
    );
    bases
}

fn compile(doc: &Value, registry: &[u8]) -> CompileReport {
    compile_documents(&serde_json::to_vec(doc).unwrap(), registry).unwrap()
}

/// A pointer anchors when it resolves, names a documented logical slot under
/// an existing step whose type declares that slot, or is a `missing` finding
/// whose parent or grandparent resolves.
fn anchored(doc: &Value, registry: &Value, finding: &CoreDiagnostic) -> Result<(), String> {
    let pointer = finding.primary.pointer.as_str();
    let target = if finding.primary.document == "registry" {
        registry
    } else {
        doc
    };
    if pointer.is_empty() || target.pointer(pointer).is_some() {
        return Ok(());
    }
    let tokens: Vec<&str> = pointer.split('/').collect();
    if let ["", "workflow", index, "inputs", slot] = tokens.as_slice()
        && let Some(step) = doc.pointer(&format!("/workflow/{index}"))
        && registry["capability_types"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|capability| {
                capability["capability_type"] == step["capability_type"]
                    && capability["inputs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|input| input["slot_id"] == json!(slot))
            })
    {
        return Ok(());
    }
    if finding.class == FindingClass::Missing {
        let mut ancestor = pointer.to_owned();
        for _ in 0..2 {
            ancestor = ancestor[..ancestor.rfind('/').unwrap_or(0)].to_owned();
            if target.pointer(&ancestor).is_some() {
                return Ok(());
            }
        }
    }
    Err(format!(
        "{} at {}:{} does not anchor in the document",
        finding.code, finding.primary.document, pointer
    ))
}

/// Applies the compiler's own repair for one finding when it is mechanical:
/// the canonical form, the first listed value, a listed source, or removal of
/// an undeclared key. Returns whether anything changed.
fn apply_repair(doc: &mut Value, finding: &CoreDiagnostic) -> bool {
    let pointer = finding.primary.pointer.as_str();
    if finding.code == "CORE-S1101" {
        let Some(split) = pointer.rfind('/') else {
            return false;
        };
        let (parent, key) = (&pointer[..split], &pointer[split + 1..]);
        return doc
            .pointer_mut(parent)
            .and_then(Value::as_object_mut)
            .is_some_and(|object| object.remove(key).is_some());
    }
    let Some(repair) = finding.repairs.first() else {
        return false;
    };
    let Some(candidate) = repair.candidates.first() else {
        return false;
    };
    match repair.applicability {
        RepairApplicability::MechanicallySafe => {
            assert_eq!(
                repair.candidates.len(),
                1,
                "a mechanically safe repair has exactly one candidate: {finding:?}"
            );
            let Some(slot) = doc.pointer_mut(pointer) else {
                return false;
            };
            *slot = json!(candidate);
            true
        }
        RepairApplicability::ConstrainedChoice => match finding.code.as_str() {
            "CORE-T2001" | "CORE-T2402" | "CORE-S1102" => {
                let Some(slot) = doc.pointer_mut(pointer) else {
                    return false;
                };
                *slot = json!(candidate);
                true
            }
            "CORE-R3102" => {
                let tokens: Vec<&str> = pointer.split('/').collect();
                let ["", "workflow", index, "inputs", slot] = tokens.as_slice() else {
                    return false;
                };
                let source = if let Some(input) = candidate.strip_prefix("input:") {
                    json!({"source": "contract_input", "input_id": input})
                } else if let Some((step, output)) = candidate
                    .strip_prefix("step:")
                    .and_then(|rest| rest.split_once('/'))
                {
                    json!({"source": "step_output", "step_id": step, "output_slot": output})
                } else {
                    return false;
                };
                let step = &mut doc["workflow"][index.parse::<usize>().unwrap()];
                if !step["bindings"].is_array() {
                    step["bindings"] = json!([]);
                }
                step["bindings"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"input_slot": slot, "source": source}));
                true
            }
            _ => false,
        },
        RepairApplicability::MethodOwnerJudgment => false,
    }
}

fn same_finding(report: &CompileReport, finding: &CoreDiagnostic) -> bool {
    report
        .findings
        .iter()
        .any(|candidate| candidate.code == finding.code && candidate.primary == finding.primary)
}

fn feeding_candidates_exist(registry: &Value, finding: &CoreDiagnostic) {
    let Some(repair) = finding.repairs.first() else {
        return;
    };
    for candidate in &repair.candidates {
        if let Some(role) = candidate.strip_prefix("declare_input:") {
            let (id, major) = role.rsplit_once('@').unwrap();
            let exists = registry["roles"].as_array().unwrap().iter().any(|entry| {
                entry["role"]["id"] == json!(id)
                    && entry["role"]["major"] == json!(major.parse::<u64>().unwrap())
            });
            assert!(
                exists,
                "feeding candidate names an unknown role: {candidate}"
            );
        } else if let Some(rest) = candidate.strip_prefix("add_step:") {
            let (type_ref, output) = rest.rsplit_once('/').unwrap();
            let (id, major) = type_ref.rsplit_once('@').unwrap();
            let exists = registry["capability_types"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| {
                    entry["capability_type"]["id"] == json!(id)
                        && entry["capability_type"]["major"] == json!(major.parse::<u64>().unwrap())
                        && entry["outputs"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|slot| slot["slot_id"] == json!(output))
                });
            assert!(
                exists,
                "feeding candidate names an unknown producer: {candidate}"
            );
        } else {
            panic!("unrecognized feeding candidate label: {candidate}");
        }
    }
}

#[derive(Default, Debug)]
struct Tally {
    cases: usize,
    findings: usize,
    fixed: usize,
    rounds: usize,
}

#[test]
fn every_finding_honors_the_diagnostic_contract() {
    let bases = compiled_bases();
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut tallies: BTreeMap<Mistake, Tally> = BTreeMap::new();

    for base in &bases {
        assert_eq!(
            compile(&base.contract, &base.registry_bytes).status,
            CompilationStatus::Compiled,
            "{} must compile before mutation",
            base.fixture_id
        );
        for mistake in Mistake::ALL {
            let mut doc = base.contract.clone();
            if !mistake.apply(&mut doc, &mut rng) {
                continue;
            }
            let label = format!("{} + {mistake:?}", base.fixture_id);
            let report = compile(&doc, &base.registry_bytes);
            assert_eq!(
                report,
                compile(&doc, &base.registry_bytes),
                "{label}: nondeterministic"
            );

            let tally = tallies.entry(mistake).or_default();
            tally.cases += 1;
            tally.findings += report.findings.len();
            if mistake != Mistake::DroppedInput {
                assert!(
                    !report.findings.is_empty(),
                    "{label}: the mistake went unreported"
                );
            }

            for finding in &report.findings {
                anchored(&doc, &base.registry, finding)
                    .unwrap_or_else(|error| panic!("{label}: {error}"));
                if finding.code == "CORE-R3101" && finding.primary.pointer.contains("/inputs/") {
                    feeding_candidates_exist(&base.registry, finding);
                }
                let substitutable =
                    finding
                        .repairs
                        .first()
                        .is_some_and(|repair| match repair.applicability {
                            RepairApplicability::MechanicallySafe => true,
                            RepairApplicability::ConstrainedChoice => {
                                matches!(
                                    finding.code.as_str(),
                                    "CORE-T2001" | "CORE-T2402" | "CORE-S1102" | "CORE-R3102"
                                )
                            }
                            RepairApplicability::MethodOwnerJudgment => false,
                        });
                if substitutable {
                    let mut repaired = doc.clone();
                    assert!(
                        apply_repair(&mut repaired, finding),
                        "{label}: repair did not apply for {finding:?}"
                    );
                    let after = compile(&repaired, &base.registry_bytes);
                    assert!(
                        !same_finding(&after, finding),
                        "{label}: applying the compiler's own repair left the finding in place: {finding:?}"
                    );
                }
            }

            // A fixer that only applies the compiler's own repairs.
            let mut working = doc.clone();
            let mut rounds = 0;
            let mut current = report;
            while current.status == CompilationStatus::Rejected && rounds < 6 {
                let mut applied = false;
                for finding in current.findings.clone() {
                    if finding.blocks_compilation() && apply_repair(&mut working, &finding) {
                        applied = true;
                    }
                }
                if !applied {
                    break;
                }
                rounds += 1;
                current = compile(&working, &base.registry_bytes);
            }
            if current.status == CompilationStatus::Compiled {
                tally.fixed += 1;
                tally.rounds += rounds;
            }
            // A listed comparison is a mechanical choice, but choosing an
            // inequality for a requirement that carried an equality tolerance
            // leaves the tolerance misplaced, and a repair that means "remove
            // this" is not expressible as a value candidate yet.
            let repairable = mistake.mechanically_repairable()
                && !(mistake == Mistake::WrongComparison
                    && doc["requirements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|requirement| requirement.get("tolerance").is_some()));
            if repairable {
                assert_eq!(
                    current.status,
                    CompilationStatus::Compiled,
                    "{label}: a mechanically repairable mistake was not repaired by the compiler's own repairs"
                );
                assert!(rounds <= 2, "{label}: took {rounds} rounds");
            }
        }
    }

    eprintln!("\nmistake                 cases  findings/case  fixed  rounds/fixed");
    for (mistake, tally) in &tallies {
        eprintln!(
            "{:<22} {:>6} {:>13.2} {:>6} {:>12.2}",
            format!("{mistake:?}"),
            tally.cases,
            tally.findings as f64 / tally.cases as f64,
            tally.fixed,
            if tally.fixed == 0 {
                0.0
            } else {
                tally.rounds as f64 / tally.fixed as f64
            }
        );
    }
}
