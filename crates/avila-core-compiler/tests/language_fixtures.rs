//! Executes the engineering-language fixtures: a program document and its
//! pinned method library in, and the expected analysis status, findings,
//! obligations, goal candidates, residual assumptions, and plan state out.
//! These tests exercise the shared `analyze_program` boundary — the same
//! operation the CLI calls — not a test-only evaluator.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::language::{
    AnalysisOptions, LanguageDocument, LifecycleEntry, analyze_program, project_document,
};
use avila_core_kernel::{CanonicalJsonValue, canonicalize_json, read_authoritative_json};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/language")
}

fn library_bytes(name: &str, revision: i64) -> Vec<u8> {
    let root = examples_root().join("libraries");
    for entry in fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let header = &value["library"];
        if header["name"] == name && header["revision"] == revision {
            return fs::read(&path).unwrap();
        }
    }
    panic!("no library {name}@{revision} in {}", root.display());
}

fn analysis_json(program: &Path) -> Value {
    let program_bytes = fs::read(program).unwrap();
    let header: Value = serde_json::from_slice(&program_bytes).unwrap();
    let library = library_bytes(
        header["library"]["name"].as_str().unwrap(),
        header["library"]["revision"].as_i64().unwrap(),
    );
    let analysis = analyze_program(&program_bytes, &library, &AnalysisOptions::default());
    serde_json::to_value(&analysis).unwrap()
}

fn finding_key(finding: &Value) -> Value {
    let mut key = json!({
        "code": finding["code"],
        "kind": finding["kind"],
        "at": finding["at"],
    });
    if let Some(candidates) = finding.get("candidates") {
        key["candidates"] = candidates.clone();
    }
    key
}

/// Every program in `examples/language/programs/` must correspond to exactly
/// one expectations row and reproduce it.
#[test]
fn all_programs_match_expectations() {
    let root = examples_root();
    let expectations: Value =
        serde_json::from_slice(&fs::read(root.join("expectations.json")).unwrap()).unwrap();
    let programs = expectations["programs"].as_object().unwrap();
    let programs_dir = root.join("programs");

    let mut files: Vec<String> = fs::read_dir(&programs_dir)
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().to_string_lossy().to_string();
            name.ends_with(".json").then_some(name)
        })
        .collect();
    files.sort();
    assert_eq!(
        files.len(),
        programs.len(),
        "fixture programs and expectations rows must be 1:1"
    );

    for file in &files {
        let row = &programs[file];
        let analysis = analysis_json(&programs_dir.join(file));

        assert_eq!(
            analysis["status"].as_str().unwrap(),
            row["analysis"].as_str().unwrap(),
            "{file}: analysis status"
        );
        assert_eq!(
            analysis["plan"]["state"].as_str().unwrap(),
            row["plan"].as_str().unwrap(),
            "{file}: plan state"
        );

        let expected_findings: Vec<Value> = row
            .get("findings")
            .and_then(|f| f.as_array())
            .map(|a| {
                a.iter()
                    .map(|f| {
                        let mut key = json!({"code": f["code"], "kind": f["kind"], "at": f["at"]});
                        if let Some(c) = f.get("candidates") {
                            key["candidates"] = c.clone();
                        }
                        key
                    })
                    .collect()
            })
            .unwrap_or_default();
        let actual_findings: Vec<Value> = analysis["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .map(finding_key)
            .collect();
        assert_eq!(actual_findings, expected_findings, "{file}: findings");

        // Findings carry stable codes consumers can match.
        for finding in analysis["findings"].as_array().into_iter().flatten() {
            let code = finding["code"].as_str().unwrap_or_default();
            assert!(
                code.starts_with("CORE-E8"),
                "{file}: finding {code} must be a catalogued language code"
            );
        }

        // Rows pin runtime obligations directionally: a "none — …" note means
        // the analysis discharges everything statically; any other entry means
        // at least one obligation awaits execution.
        if let Some(obligations) = row["runtime_obligations"].as_array() {
            let runtime = analysis["plan"]["runtime_obligations"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0);
            let pins_none = obligations
                .first()
                .and_then(|o| o.as_str())
                .is_some_and(|s| s.starts_with("none"));
            if pins_none {
                assert_eq!(runtime, 0, "{file}: expected no runtime obligations");
            } else {
                assert!(runtime > 0, "{file}: expected runtime obligations");
            }
        }

        // Rows pinning `verdicts` to "none — <reason>" mean no requirement
        // may carry an evaluated verdict at analysis time; rows pinning
        // `expected_on_execution.verdicts` pin the analyzer's verdict for
        // statically-decidable refusal (not_evaluated.*) and the pending
        // state for verdicts that need a runtime observation.
        match row.get("verdicts") {
            Some(Value::String(note)) if note.starts_with("none") => {
                for (id, report) in analysis["requirements"].as_object().into_iter().flatten() {
                    let status = report["verdict"]["status"].as_str().unwrap_or_default();
                    assert_eq!(
                        status, "not_evaluated",
                        "{file}: requirement {id} must not evaluate — {note}"
                    );
                }
            }
            _ => {}
        }
        if let Some(verdicts) = row
            .pointer("/expected_on_execution/verdicts")
            .and_then(|v| v.as_object())
        {
            for (id, expected) in verdicts {
                let report = &analysis["requirements"][id];
                assert!(!report.is_null(), "{file}: requirement {id} missing");
                if expected["status"] == "not_evaluated" {
                    // Statically decidable refusals are emitted now.
                    assert_eq!(
                        report["verdict"]["status"].as_str().unwrap(),
                        "not_evaluated",
                        "{file}: requirement {id}"
                    );
                    assert_eq!(
                        report["verdict"]["rule"].as_str().unwrap(),
                        expected["rule"].as_str().unwrap(),
                        "{file}: requirement {id} rule"
                    );
                } else {
                    // PASS/FAIL/INCONCLUSIVE await a runtime observation —
                    // the requirement stays pending at analysis time.
                    assert_eq!(
                        report["state"].as_str().unwrap(),
                        "pending",
                        "{file}: requirement {id} must be pending before execution"
                    );
                }
            }
        }

        // Residual assumptions on the final clearance binding — pinned for the
        // discharge fixtures.
        if let Some(expected) = row
            .pointer("/expected_on_execution/residual_assumptions_on_remaining_clearance")
            .and_then(|r| r.as_array())
        {
            let actual: Vec<String> =
                analysis["bindings"]["remaining_clearance"]["residual_assumptions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(render_scoped)
                    .collect();
            let expected: Vec<String> = expected
                .iter()
                .map(|e| e.as_str().unwrap().to_string())
                .collect();
            assert_eq!(actual, expected, "{file}: residual assumptions");
        }
    }
}

fn render_scoped(a: &Value) -> String {
    let at = a["at"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| format!("{k}: {}", v.as_str().unwrap()))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!("{}@({{{}}})", a["proposition"].as_str().unwrap(), at)
}

/// Admission and identity are enforced before analysis: malformed bytes,
/// pin mismatches, and undeclared references are findings, not panics.
#[test]
fn admission_and_identity_are_checked() {
    let root = examples_root();
    let program_bytes = fs::read(root.join("programs/clearance-pass.program.json")).unwrap();
    let thermal = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();
    let measurement = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    let kinds = |analysis: &avila_core_compiler::language::LanguageAnalysis| -> Vec<String> {
        analysis.findings.iter().map(|f| f.kind.clone()).collect()
    };

    // Malformed: non-canonical bytes never reach semantic analysis.
    let analysis = analyze_program(b"{\"schema_version\": \"0.1\", }", &thermal, &options);
    assert!(kinds(&analysis).contains(&"malformed".to_string()));

    // Malformed: an undeclared field — including an annotation-named field at
    // an undeclared position — is a finding, not silent removal.
    let mut mutated: Value = serde_json::from_slice(&program_bytes).unwrap();
    mutated["body"][0]["arguments"]["coefficient"]["note"] =
        json!("annotation name at an undeclared position");
    let bytes = canonicalize_json(&serde_json::to_vec(&mutated).unwrap()).unwrap();
    let analysis = analyze_program(&bytes, &thermal, &options);
    assert!(
        kinds(&analysis).contains(&"malformed".to_string()),
        "undeclared annotation-position field must refuse at admission"
    );

    // Pin mismatch: the measurement library is a valid document but not the
    // pinned identity.
    let analysis = analyze_program(&program_bytes, &measurement, &options);
    assert!(kinds(&analysis).contains(&"library_pin_mismatch".to_string()));
    assert!(
        !analysis.library.pin_match,
        "a foreign library must not be bound silently"
    );

    // Undeclared proposition, entity, method, and missing argument — each a
    // named finding, never a resolution.
    let mutate = |edit: &dyn Fn(&mut Value)| -> avila_core_compiler::language::LanguageAnalysis {
        let mut program: Value = serde_json::from_slice(&program_bytes).unwrap();
        edit(&mut program);
        let bytes = canonicalize_json(&serde_json::to_vec(&program).unwrap()).unwrap();
        analyze_program(&bytes, &thermal, &options)
    };

    let analysis = mutate(&|p| {
        p["assumptions"] = json!([{
            "id": "a-ghost",
            "asserts": "no-such-proposition",
            "at": {"geometry": "bracket@2"},
        }]);
    });
    assert!(kinds(&analysis).contains(&"undeclared_proposition".to_string()));

    let analysis = mutate(&|p| {
        p["body"][0]["apply"] = json!("no-such-method");
    });
    assert!(kinds(&analysis).contains(&"undeclared_method".to_string()));

    let analysis = mutate(&|p| {
        p["inputs"][0]["type"]["geometry"] = json!("ghost-geometry");
    });
    assert!(kinds(&analysis).contains(&"undeclared_entity".to_string()));

    let analysis = mutate(&|p| {
        p["body"][0]["arguments"]
            .as_object_mut()
            .unwrap()
            .remove("coefficient");
    });
    assert!(kinds(&analysis).contains(&"missing_argument".to_string()));

    // Undeclared projection: the output signature drops `material` while an
    // operand carries it and `projects` does not list it — the library is
    // mutated, which also produces a pin mismatch, but the projection rule
    // itself must fire.
    let mut lib: Value = serde_json::from_slice(&thermal).unwrap();
    for method in lib["methods"].as_array_mut().unwrap() {
        if method["id"] == "clearance-difference" {
            method["output"].as_object_mut().unwrap().remove("material");
        }
    }
    let lib_bytes = canonicalize_json(&serde_json::to_vec(&lib).unwrap()).unwrap();
    let analysis = analyze_program(&program_bytes, &lib_bytes, &options);
    let kinds = kinds(&analysis);
    assert!(
        kinds.contains(&"undeclared_projection".to_string()),
        "an output dropping a carried relation without `projects` must refuse: {kinds:?}"
    );
}

/// Determinism: the same documents analyze identically twice.
#[test]
fn analysis_is_deterministic() {
    let program = examples_root().join("programs/clearance-pass.program.json");
    assert_eq!(analysis_json(&program), analysis_json(&program));
}

/// Reviewed counterexample (r2 finding 1): operand order is semantic. The
/// r2-projection collision that hashed `sub(a,b)` and `sub(b,a)` identically
/// must stay closed.
#[test]
fn operand_order_changes_semantic_identity() {
    let path = examples_root().join("programs/invalid-product-kind.program.json");
    let bytes = fs::read(&path).unwrap();
    let mut program: Value = serde_json::from_slice(&bytes).unwrap();

    let canonical = read_authoritative_json(&bytes).unwrap();
    let original = project_document(&canonical, LanguageDocument::Program).unwrap();
    let original_digest = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&original).unwrap())
    );

    // Swap the infer arguments — the typed meaning changes.
    let args = program["body"][0]["infer"]["arguments"]
        .as_array_mut()
        .unwrap();
    args.swap(0, 1);
    let swapped_bytes = canonicalize_json(&serde_json::to_vec(&program).unwrap()).unwrap();
    let swapped = read_authoritative_json(&swapped_bytes).unwrap();
    let swapped = project_document(&swapped, LanguageDocument::Program).unwrap();
    let swapped_digest = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&swapped).unwrap())
    );

    assert_ne!(
        original_digest, swapped_digest,
        "swapped subtraction operands must produce different semantic identity"
    );
}

/// Reviewed counterexample (r2 finding 2): annotation names are not global.
/// A proposition named `note` keeps its declaration; changing its declared
/// `params` changes the semantic identity.
#[test]
fn annotation_named_identifiers_carry_semantics() {
    let path = examples_root().join("libraries/thermal-expansion.v1.json");
    let bytes = fs::read(&path).unwrap();

    // Inject a proposition literally named `note` and confirm its params are
    // semantic content.
    let mut doc: Value = serde_json::from_slice(&bytes).unwrap();
    doc["propositions"]["note"] = json!({
        "params": ["geometry", "scenario"],
        "gloss": "a proposition coincidentally named after an annotation field"
    });
    let a_bytes = canonicalize_json(&serde_json::to_vec(&doc).unwrap()).unwrap();
    let a = read_authoritative_json(&a_bytes).unwrap();
    let a_projected = project_document(&a, LanguageDocument::Library).unwrap();
    // The map key `note` and its params survive projection; only the gloss goes.
    let params = a_projected.pointer("/propositions/note/params").unwrap();
    let params: Vec<&str> = match params {
        CanonicalJsonValue::Array(items) => items
            .iter()
            .map(|v| match v {
                CanonicalJsonValue::String(s) => s.as_str(),
                _ => panic!("param must be a string"),
            })
            .collect(),
        _ => panic!("params must be an array"),
    };
    assert_eq!(params, ["geometry", "scenario"]);
    assert!(
        a_projected.pointer("/propositions/note/gloss").is_none(),
        "gloss is a declared annotation field and must be projected away"
    );

    doc["propositions"]["note"]["params"] = json!(["material", "scenario"]);
    let b_bytes = canonicalize_json(&serde_json::to_vec(&doc).unwrap()).unwrap();
    let b = read_authoritative_json(&b_bytes).unwrap();
    let b_projected = project_document(&b, LanguageDocument::Library).unwrap();

    let digest_a = Sha256::digest(serde_json::to_vec(&a_projected).unwrap());
    let digest_b = Sha256::digest(serde_json::to_vec(&b_projected).unwrap());
    assert_ne!(
        digest_a, digest_b,
        "changing a proposition's declared scope must change semantic identity"
    );
}

/// §10.2: lifecycle entries combine by union-of-refusals across library and
/// method scope; same-key conflicting states refuse at bind.
#[test]
fn lifecycle_combines_by_refusal_union() {
    let root = examples_root();
    let program = fs::read(root.join("programs/clearance-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();

    let run = |entries: Vec<(&str, &str)>| {
        let options = AnalysisOptions {
            lifecycle: entries
                .into_iter()
                .map(|(key, state)| LifecycleEntry {
                    key: key.to_string(),
                    state: state.to_string(),
                })
                .collect(),
        };
        serde_json::to_value(analyze_program(&program, &library, &options)).unwrap()
    };

    // No lifecycle material → usable, not silently active.
    let analysis = run(vec![]);
    assert!(
        analysis["lifecycle"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["state"] == "absent"),
        "absent lifecycle material must be visible"
    );

    // Method active + library withdrawn → refused; both reasons visible.
    let analysis = run(vec![
        ("method:thermal-expansion/linear-expansion@1", "active"),
        ("library:thermal-expansion@1", "withdrawn"),
    ]);
    assert!(
        analysis["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["kind"] == "lifecycle_refused"),
        "a method-level active never overrides a library-level withdrawn"
    );
    let refused = analysis["lifecycle"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["state"] == "refused")
        .unwrap();
    assert!(
        refused["refusal_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.as_str().unwrap().contains("withdrawn"))
    );

    // Expired at either scope refuses.
    let analysis = run(vec![("library:thermal-expansion@1", "expired")]);
    assert!(
        analysis["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["kind"] == "lifecycle_refused")
    );

    // Same-key conflicting states → lifecycle_conflict, order-independent.
    for order in [("active", "withdrawn"), ("withdrawn", "active")] {
        let analysis = run(vec![
            ("library:thermal-expansion@1", order.0),
            ("library:thermal-expansion@1", order.1),
        ]);
        assert!(
            analysis["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["kind"] == "lifecycle_conflict"),
            "conflicting same-key lifecycle states refuse regardless of order"
        );
    }
}

/// Reviewed counterexample (r2 finding 3): `clearance-difference` must carry
/// `material` through the union — an output that silently drops a declared
/// relation is an `undeclared_projection`.
#[test]
fn outputs_carry_declared_relations_only() {
    let root = examples_root();
    let program = fs::read(root.join("programs/clearance-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();
    let analysis = analyze_program(&program, &library, &AnalysisOptions::default());
    let json = serde_json::to_value(&analysis).unwrap();

    let remaining = &json["bindings"]["remaining_clearance"];
    let relations: BTreeMap<_, _> = remaining["type"]["relations"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert!(
        relations.contains_key("material"),
        "remaining_clearance must carry the material it was computed over"
    );
    assert_eq!(
        relations.keys().cloned().collect::<Vec<_>>(),
        vec!["geometry", "material", "scenario"]
    );
}

/// r2 finding 4: shared recorded provenance refutes `provenance_disjoint` but
/// never `independent` — the Z-mod-2 counterexample is a regression test.
#[test]
fn shared_provenance_never_refutes_independence() {
    let root = examples_root();
    let program =
        fs::read(root.join("programs/invalid-shared-source-independence.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let analysis = analyze_program(&program, &library, &AnalysisOptions::default());
    let json = serde_json::to_value(&analysis).unwrap();

    let kinds: Vec<&str> = json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"obligation_refuted"),
        "shared source edge refutes the disjointness admission policy"
    );
    assert!(
        kinds.contains(&"obligation_unmet"),
        "independence is open, not refuted, under shared provenance"
    );
    assert!(
        !kinds.contains(&"premise_conflict"),
        "no premise asserts what the record refutes in this program"
    );

    let states: Vec<(&str, &str)> = json["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| (o["kind"].as_str().unwrap(), o["state"].as_str().unwrap()))
        .collect();
    assert!(states.contains(&("provenance_disjoint", "refuted")));
    assert!(states.contains(&("independence", "open")));
}

/// r2 finding 5 / EL-02 rule: nominal operands make the arithmetic rule
/// inapplicable — `unsupported`, never an invented bound.
#[test]
fn nominal_operands_are_unsupported() {
    let root = examples_root();
    let program = fs::read(root.join("programs/invalid-nominal-arithmetic.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let analysis = analyze_program(&program, &library, &AnalysisOptions::default());
    let json = serde_json::to_value(&analysis).unwrap();

    assert!(
        json["findings"].as_array().unwrap().iter().any(
            |f| f["kind"] == "unsupported" && f["detail"].as_str().unwrap().contains("nominal")
        )
    );
    // No binding is established from an inapplicable rule.
    assert!(
        json["bindings"].get("scaled").is_none()
            || json["bindings"]["scaled"]["value_state"] == "unestablished"
    );
}

/// Requirement verdicts belong to `evaluate`; analysis reports the subject's
/// state and statically-decidable `not_evaluated` refusals only. The exact
/// interval arithmetic still lands in the emitted binding values.
#[test]
fn verdicts_are_computed_from_exact_arithmetic() {
    let root = examples_root();
    for (file, requirement, expected) in [
        ("clearance-pass.program.json", "EL-R1", "pending"),
        ("clearance-inconclusive.program.json", "EL-R1", "pending"),
        ("clearance-fail.program.json", "EL-R1", "pending"),
        (
            "clearance-not-evaluated.program.json",
            "EL-R1",
            "not_evaluated",
        ),
        ("positive-measurement-pass.program.json", "EL-R2", "pending"),
    ] {
        let program = fs::read(root.join("programs").join(file)).unwrap();
        let header: Value = serde_json::from_slice(&program).unwrap();
        let library = library_bytes(
            header["library"]["name"].as_str().unwrap(),
            header["library"]["revision"].as_i64().unwrap(),
        );
        let analysis = analyze_program(&program, &library, &AnalysisOptions::default());
        let json = serde_json::to_value(&analysis).unwrap();
        assert_eq!(
            json["requirements"][requirement]["state"].as_str().unwrap(),
            expected,
            "{file} {requirement} state"
        );
    }
    // The primitive composition computed the scaled rate exactly — the value
    // is in the binding, but the verdict itself awaits `evaluate`.
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let analysis = analyze_program(&program, &library, &AnalysisOptions::default());
    let json = serde_json::to_value(&analysis).unwrap();
    assert_eq!(
        json["bindings"]["combined_rate"]["value"],
        "[187/100, 231/100]"
    );
    assert!(json["requirements"]["EL-R2"]["verdict"].is_null());
}

// ---------------------------------------------------------------------------
// EL-02 revision-review counterexamples — exercised at the shared API
// boundary, with the expected outcome derived from the specification.
// ---------------------------------------------------------------------------

fn canonical_bytes(value: &Value) -> Vec<u8> {
    canonicalize_json(&serde_json::to_vec(value).unwrap()).unwrap()
}

fn edited(bytes: &[u8], edit: impl Fn(&mut Value)) -> Vec<u8> {
    let mut doc: Value = serde_json::from_slice(bytes).unwrap();
    edit(&mut doc);
    canonical_bytes(&doc)
}

/// Re-run the program against a mutated library after repinning the
/// program's declared identity — mirroring the review's repin step, so a
/// valid mutated library is judged on its own findings, not pin mismatch.
fn repinned(program_bytes: &[u8], library_bytes: &[u8]) -> Vec<u8> {
    let first = analyze_program(program_bytes, library_bytes, &AnalysisOptions::default());
    let mut program: Value = serde_json::from_slice(program_bytes).unwrap();
    if let Some(hash) = &first.library.identity.semantic_sha256 {
        program["library"]["semantic_sha256"] = json!(hash);
    }
    canonical_bytes(&program)
}

fn kinds_of(analysis: &avila_core_compiler::language::LanguageAnalysis) -> Vec<String> {
    analysis.findings.iter().map(|f| f.kind.clone()).collect()
}

/// F1: postconditions are replayed against the actual operand values — a
/// false postcondition is `refuted`, a mistyped body is refused, and an
/// unsupported check can never report `discharged`.
#[test]
fn postconditions_are_replayed_not_discharged() {
    let root = examples_root();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    // The body computes `sub` while the postcondition asserts `add` — the
    // replayed check refutes it, and no value is established.
    let lib = edited(&library, |l| {
        l["methods"][0]["implementation"]["body"] = json!("interval.sub(reading_a, reading_b)");
    });
    let repinned_bytes = repinned(&program, &lib);
    let analysis = analyze_program(&repinned_bytes, &lib, &options);
    assert_eq!(analysis.plan.state, "refused");
    assert!(
        analysis
            .obligations
            .iter()
            .any(|o| o.kind == "postcondition" && o.state == "refuted"),
        "a false postcondition must refute, not discharge"
    );
    assert_eq!(
        analysis.bindings["combined_rate"].value, None,
        "a refuted postcondition establishes nothing"
    );

    // A body typed differently from the declared output is refused; the
    // signature may not relabel the dimensionless value as a dose rate.
    let lib = edited(&library, |l| {
        l["methods"][0]["implementation"]["body"] = json!("calibration");
    });
    let repinned_bytes = repinned(&program, &lib);
    let analysis = analyze_program(&repinned_bytes, &lib, &options);
    let kinds = kinds_of(&analysis);
    assert!(kinds.contains(&"type_mismatch".to_string()), "{kinds:?}");

    // An unrecognized function and an unsupported check name can only stay
    // open or unsupported — never "discharged".
    let lib = edited(&library, |l| {
        l["methods"][0]["ensures"][0]["expression"] =
            json!("output = nonexistent_function(reading_a)");
        l["methods"][0]["ensures"][0]["check"] = json!("nonexistent_checker");
    });
    let repinned_bytes = repinned(&program, &lib);
    let analysis = analyze_program(&repinned_bytes, &lib, &options);
    let kinds = kinds_of(&analysis);
    assert!(kinds.contains(&"unsupported".to_string()), "{kinds:?}");
    assert!(
        !analysis
            .obligations
            .iter()
            .any(|o| o.state == "discharged" && o.kind == "postcondition"),
        "an unreplayable check must not report discharged"
    );
}

/// F3: a payload must establish its declared claim — a nominal payload under
/// an enclosure type is refused, and a wrong unit is refused rather than
/// silently reinterpreted.
#[test]
fn payloads_must_establish_their_declared_claim() {
    let root = examples_root();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    let bytes = edited(&program, |p| {
        p["inputs"][0]["binding"]["value"] =
            json!({"kind": "nominal", "value": "1", "unit": "mSv/h"});
    });
    let analysis = analyze_program(&bytes, &library, &options);
    let kinds = kinds_of(&analysis);
    assert!(
        kinds.contains(&"type_mismatch".to_string()),
        "a nominal payload cannot satisfy an enclosure claim: {kinds:?}"
    );
    assert_eq!(analysis.plan.state, "refused");

    let bytes = edited(&program, |p| {
        p["inputs"][0]["binding"]["value"]["unit"] = json!("kg");
    });
    let analysis = analyze_program(&bytes, &library, &options);
    let kinds = kinds_of(&analysis);
    assert!(
        kinds.contains(&"malformed".to_string()),
        "a non-canonical unit must refuse admission, not reinterpret: {kinds:?}"
    );
}

/// F2: declared sets are order-independent — a duplicate id is refused in
/// both source orders under the same identity, and a reordered `ensures` set
/// cannot change the derived value under the same library identity.
#[test]
fn declared_sets_are_order_invariant() {
    let root = examples_root();
    let measurement = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let thermal = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    // Two conflicting `reading_a` inputs — refused both ways, with identical
    // findings and values regardless of source order.
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let dup = edited(&program, |p| {
        let mut second = p["inputs"][0].clone();
        second["binding"]["value"]["lower"] = json!("20");
        second["binding"]["value"]["upper"] = json!("21");
        p["inputs"].as_array_mut().unwrap().push(second);
    });
    let reversed = edited(&dup, |p| {
        p["inputs"].as_array_mut().unwrap().reverse();
    });
    let a = analyze_program(&dup, &measurement, &options);
    let b = analyze_program(&reversed, &measurement, &options);
    assert_eq!(a.plan.state, "refused");
    assert_eq!(b.plan.state, "refused");
    assert!(kinds_of(&a).contains(&"malformed".to_string()));
    assert!(kinds_of(&b).contains(&"malformed".to_string()));
    assert_eq!(
        a.bindings["combined_rate"].value, b.bindings["combined_rate"].value,
        "equal inputs must not produce order-dependent values"
    );

    // A reordered `ensures` set holds the same library semantic identity and
    // cannot change what `displacement` is computed or declared to be.
    let thermal_program = fs::read(root.join("programs/clearance-pass.program.json")).unwrap();
    let lib_a = edited(&thermal, |l| {
        l["methods"][0]["ensures"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "kind": "relation",
                "expression": "output = length",
                "check": "interval_arithmetic",
            }));
    });
    let lib_b = edited(&thermal, |l| {
        let second = json!({
            "kind": "relation",
            "expression": "output = length",
            "check": "interval_arithmetic",
        });
        let first = l["methods"][0]["ensures"][0].clone();
        l["methods"][0]["ensures"] = json!([second, first]);
    });
    let pa = repinned(&thermal_program, &lib_a);
    let pb = repinned(&thermal_program, &lib_b);
    let a = analyze_program(&pa, &lib_a, &options);
    let b = analyze_program(&pb, &lib_b, &options);
    assert_eq!(
        a.library.identity.semantic_sha256, b.library.identity.semantic_sha256,
        "reordered declared set must keep semantic identity"
    );
    assert_eq!(
        a.bindings["displacement"].value, b.bindings["displacement"].value,
        "same identity must produce the same value"
    );
    assert_eq!(
        kinds_of(&a),
        kinds_of(&b),
        "same identity must produce the same findings"
    );
}

/// F5/F6: a premise without attribution cannot discharge anything, a
/// certificate without a replayable check cannot establish, and a discharging
/// witness contributes its own assumptions and provenance to the result.
#[test]
fn witnesses_must_carry_attribution_and_support() {
    let root = examples_root();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let thermal = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    // Deleting `established_by` leaves the obligation unmet — nothing may
    // discharge on an unattributed premise.
    let bytes = edited(&program, |p| {
        p["premises"][0]
            .as_object_mut()
            .unwrap()
            .remove("established_by");
    });
    let analysis = analyze_program(&bytes, &library, &options);
    let kinds = kinds_of(&analysis);
    assert!(kinds.contains(&"malformed".to_string()), "{kinds:?}");
    assert!(
        analysis
            .obligations
            .iter()
            .any(|o| o.kind == "independence" && o.state != "discharged"),
        "unattributed premises never discharge"
    );

    // A certificate with a well-formed digest but an unknown check is
    // refused — no established value, no silent admission.
    let thermal_program = fs::read(root.join("programs/clearance-pass.program.json")).unwrap();
    let bytes = edited(&thermal_program, |p| {
        p["body"] = json!([{
            "bind": "remaining_clearance",
            "import": {
                "type": {
                    "quantity_kind": "length",
                    "claim": "enclosure",
                    "geometry": "bracket@2",
                    "scenario": "thermal-soak-steady",
                    "material": "al-6061-t6",
                },
                "value": {"kind": "enclosure", "lower": "1", "upper": "1", "unit": "mm"},
                "assumptions": [],
                "source": {
                    "kind": "certificate",
                    "digest": format!("sha256:{}", "0".repeat(64)),
                    "check": "made-up",
                },
            },
        }]);
    });
    let analysis = analyze_program(&bytes, &thermal, &options);
    let kinds = kinds_of(&analysis);
    assert!(kinds.contains(&"unsupported".to_string()), "{kinds:?}");
    assert_eq!(
        serde_json::to_value(&analysis.bindings["remaining_clearance"].value_state).unwrap(),
        json!("unestablished")
    );

    // The unmodified measurement program's independence witness contributes
    // its own assumptions and provenance edge to `combined_rate`.
    let analysis = analyze_program(&program, &library, &options);
    let support = &analysis.bindings["combined_rate"];
    let edges = support.source_edges.iter().cloned().collect::<Vec<_>>();
    assert!(
        edges.contains(&"indep-memo-7".to_string()),
        "the witness's provenance edge must reach the result: {edges:?}"
    );
    assert!(
        support
            .residual_assumptions
            .iter()
            .any(|a| a.proposition == "separate-instrumentation"),
        "the witness's assumptions must reach the result"
    );
    assert!(
        support
            .dependencies
            .iter()
            .any(|d| d.kind == "premise" && d.target == "p-independence"),
        "the witness premise link must appear in dependencies"
    );
}

/// F4/F8: domain containment requires admitted intervals and resolvable
/// field paths; a blocking cause propagates through dependents while an
/// unreachable hole stays inspectable without refusing the plan.
#[test]
fn context_checks_and_reachability_are_enforced() {
    let root = examples_root();
    let thermal = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();
    let program = fs::read(root.join("programs/clearance-pass.program.json")).unwrap();
    let options = AnalysisOptions::default();

    // A kilogram operating domain against a Kelvin applicability is a
    // checked type failure, not a discharged containment.
    let bytes = edited(&program, |p| {
        p["entities"]["scenarios"]["thermal-soak-steady"]["operating_domain"]["unit"] = json!("kg");
    });
    let analysis = analyze_program(&bytes, &thermal, &options);
    let kinds = kinds_of(&analysis);
    assert!(kinds.contains(&"type_mismatch".to_string()), "{kinds:?}");

    // An inverted domain is malformed, not contained.
    let bytes = edited(&program, |p| {
        p["entities"]["scenarios"]["thermal-soak-steady"]["operating_domain"]["lower"] =
            json!("400");
        p["entities"]["scenarios"]["thermal-soak-steady"]["operating_domain"]["upper"] =
            json!("300");
    });
    let analysis = analyze_program(&bytes, &thermal, &options);
    let kinds = kinds_of(&analysis);
    assert!(kinds.contains(&"malformed".to_string()), "{kinds:?}");

    // A second material identity on `length` collides with `coefficient`'s —
    // the operand union is a conflict, not a silent pick.
    let bytes = edited(&program, |p| {
        p["entities"]["materials"]["other-material"] =
            p["entities"]["materials"]["al-6061-t6"].clone();
        p["inputs"][0]["type"]["material"] = json!("other-material");
    });
    let analysis = analyze_program(&bytes, &thermal, &options);
    let kinds = kinds_of(&analysis);
    assert!(kinds.contains(&"type_mismatch".to_string()), "{kinds:?}");

    // An unmet precondition on `combined_rate` propagates to the dependent
    // requirement — the causal rule is kept, not a bare pending state.
    let measurement = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let measurement_program =
        fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let bytes = edited(&measurement_program, |p| {
        p["premises"] = json!([]);
        p["body"].as_array_mut().unwrap().push(json!({
            "bind": "dependent",
            "infer": {
                "rule": "interval.add",
                "arguments": [{"ref": "combined_rate"}, {"ref": "reading_a"}],
            },
        }));
        p["requirements"][0]["subject"]["ref"] = json!("dependent");
    });
    let analysis = analyze_program(&bytes, &measurement, &options);
    assert_eq!(
        analysis.requirements["EL-R2"]
            .verdict
            .as_ref()
            .unwrap()
            .rule,
        "not_evaluated.obligation_unmet",
        "the blocking cause must reach the dependent's requirement"
    );

    // An unused hole remains inspectable without refusing the plan.
    let bytes = edited(&program, |p| {
        p["body"].as_array_mut().unwrap().push(json!({
            "bind": "unused",
            "hole": {"quantity_kind": "length", "claim": "enclosure"},
        }));
    });
    let analysis = analyze_program(&bytes, &thermal, &options);
    assert!(
        kinds_of(&analysis).contains(&"hole".to_string()),
        "the hole stays visible"
    );
    assert_eq!(
        analysis.plan.state, "ready",
        "an unreachable hole must not refuse the plan"
    );
}

/// F9/F10: requirement admission validates the comparison vocabulary, the
/// limit shape, scope kinds, and scenario identity; scope refinement is the
/// lattice — `any` accepts every scope. Lifecycle material is validated
/// against the closed grammar.
#[test]
fn requirement_and_lifecycle_admission_are_checked() {
    let root = examples_root();
    let measurement = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let thermal = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let options = AnalysisOptions::default();

    let bytes = edited(&program, |p| {
        p["requirements"][0]["comparison"] = json!("potato");
        p["requirements"][0]["limit"]["value"] = json!("not-a-number");
    });
    let analysis = analyze_program(&bytes, &measurement, &options);
    assert!(
        kinds_of(&analysis).contains(&"malformed".to_string()),
        "an unsupported comparison and a non-rational limit are refused"
    );

    // `any` is the top of the scope lattice — a steady-state subject serves
    // it. A matching scenario identity is admitted.
    for edit in [
        Box::new(|p: &mut Value| {
            p["requirements"][0]["scope"] = json!("any");
        }) as Box<dyn Fn(&mut Value)>,
        Box::new(|p: &mut Value| {
            p["requirements"][0]["scenario"] = json!("field-survey-1");
        }),
    ] {
        let bytes = edited(&program, edit);
        let analysis = analyze_program(&bytes, &measurement, &options);
        assert_eq!(
            analysis.status,
            "clean",
            "a supported scope/scenario form must be admitted: {:?}",
            kinds_of(&analysis)
        );
    }

    // An unknown scenario name is an undeclared entity, not a refinement.
    let bytes = edited(&program, |p| {
        p["requirements"][0]["scenario"] = json!("ghost-survey");
    });
    let analysis = analyze_program(&bytes, &measurement, &options);
    assert!(
        kinds_of(&analysis).contains(&"undeclared_entity".to_string()),
        "a scenario identity must resolve to a declared entity"
    );

    // An unknown lifecycle state is malformed — only the closed vocabulary
    // plays a part in usability.
    let options = AnalysisOptions {
        lifecycle: vec![LifecycleEntry {
            key: "library:thermal-expansion@1".into(),
            state: "withdrawnn".into(),
        }],
    };
    let thermal_program = fs::read(root.join("programs/clearance-pass.program.json")).unwrap();
    let analysis = analyze_program(&thermal_program, &thermal, &options);
    assert!(
        kinds_of(&analysis).contains(&"malformed".to_string()),
        "an unknown lifecycle state is refused"
    );
}

/// F2/F7: a document with a forward reference publishes no semantic
/// identity; an expression past the term/depth budget returns a bounded
/// `budget` finding instead of exhausting the evaluator.
#[test]
fn identity_is_withheld_and_expressions_are_bounded() {
    let root = examples_root();
    let program = fs::read(root.join("programs/clearance-pass.program.json")).unwrap();
    let thermal = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    // Reordering the sequence produces a forward reference — the document is
    // not admitted, so no semantic identity is published for it.
    let bytes = edited(&program, |p| {
        p["body"].as_array_mut().unwrap().reverse();
    });
    let analysis = analyze_program(&bytes, &thermal, &options);
    assert!(
        !analysis.program.identity.admitted,
        "a forward reference is inadmissible"
    );
    assert!(
        analysis.program.identity.semantic_sha256.is_none(),
        "an inadmitted document publishes no semantic identity"
    );
    assert!(
        analysis
            .program
            .identity
            .document_sha256
            .starts_with("sha256:"),
        "the bytes-read identity is still published"
    );

    // A deeply nested expression returns a bounded finding — the analysis
    // completes in bounded time and memory rather than recursing.
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let deep = edited(&library, |l| {
        l["methods"][0]["implementation"]["body"] = json!(format!(
            "{}reading_a{}",
            "(".repeat(10000),
            ")".repeat(10000)
        ));
    });
    let measurement_program =
        fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let analysis = analyze_program(&measurement_program, &deep, &options);
    let kinds = kinds_of(&analysis);
    assert!(
        kinds.contains(&"budget".to_string()),
        "a deep expression is a bounded refusal, not a crash: {kinds:?}"
    );
    assert!(
        !analysis.library.identity.admitted,
        "a document that cannot be checked is not admitted"
    );
}

/// A decodable program's own findings must not be masked when the library
/// cannot be decoded — the vocabulary-free shape checks still run, while
/// vocabulary-dependent checks are undecidable (not failed) without it.
#[test]
fn partial_decode_reports_each_documents_own_findings() {
    let root = examples_root();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let options = AnalysisOptions::default();

    // A duplicate input id is a vocabulary-free program defect; the library
    // is bytes that cannot decode at all.
    let program = edited(&program, |p| {
        let dup = p["inputs"].as_array().unwrap()[0].clone();
        p["inputs"].as_array_mut().unwrap().push(dup);
    });
    let analysis = analyze_program(&program, b"{ not json", &options);
    let kinds = kinds_of(&analysis);
    assert!(
        kinds.contains(&"malformed".to_string()),
        "the program's own defect must surface: {kinds:?}"
    );
    assert!(
        analysis
            .findings
            .iter()
            .any(|f| f.document == "program" && f.at == "inputs[reading_a]"),
        "the duplicate id finding names the program position: {:?}",
        analysis.findings
    );
    assert!(
        analysis
            .findings
            .iter()
            .any(|f| f.document == "library" && f.kind == "malformed"),
        "the undecodable library is reported"
    );
    assert_eq!(analysis.plan.state, "refused");
    assert!(!analysis.library.identity.admitted);
    assert!(analysis.library.identity.semantic_sha256.is_none());
    assert!(
        analysis
            .program
            .identity
            .document_sha256
            .starts_with("sha256:"),
        "the bytes-read identity is still published"
    );
}

/// `scope_check` obligations evaluate the subject's declared scenario scope
/// against the required scope through the scope lattice — refuted when the
/// scenario's scope cannot serve, discharged when it can.
#[test]
fn scope_check_is_evaluated_through_the_scope_lattice() {
    let root = examples_root();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    // `reading_a` is scoped to field-survey-1, declared `steady-state`. A
    // `transient` requirement refutes; `steady-state` discharges; `any`
    // discharges for every scope.
    for (required, expect_refuted) in [("transient", true), ("steady-state", false), ("any", false)]
    {
        let lib = edited(&library, |l| {
            l["methods"][0]["requires"]
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "kind": "scope_check",
                    "subject": "reading_a",
                    "scope": required,
                }));
        });
        let repinned_bytes = repinned(&program, &lib);
        let analysis = analyze_program(&repinned_bytes, &lib, &options);
        let obligation = analysis
            .obligations
            .iter()
            .find(|o| o.kind == "scope_check")
            .expect("the scope_check obligation is generated");
        if expect_refuted {
            assert_eq!(obligation.state, "refuted", "required={required}");
            assert!(
                kinds_of(&analysis).contains(&"precondition_refuted".to_string()),
                "required={required}"
            );
        } else {
            assert_eq!(obligation.state, "discharged", "required={required}");
            assert_eq!(analysis.plan.state, "ready", "required={required}");
        }
    }
}

/// `provenance_disjoint` is open — not refuted — when an operand carries no
/// recorded source edges: missing provenance cannot establish disjointness.
#[test]
fn provenance_disjoint_is_open_when_edges_are_absent() {
    let root = examples_root();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    let bytes = edited(&program, |p| {
        p["inputs"][0]["binding"]["source"]
            .as_object_mut()
            .unwrap()
            .remove("edge");
    });
    let analysis = analyze_program(&bytes, &library, &options);
    let obligation = analysis
        .obligations
        .iter()
        .find(|o| o.kind == "provenance_disjoint")
        .expect("the disjointness obligation is generated");
    assert_eq!(obligation.state, "open");
    assert!(
        kinds_of(&analysis).contains(&"obligation_unmet".to_string()),
        "missing edges cannot discharge disjointness: {:?}",
        kinds_of(&analysis)
    );
    assert_eq!(analysis.plan.state, "refused");
}

/// Witness provenance flows through the full support chain: an assumption
/// discharged through a premise makes the premise's provenance — and its
/// own witnesses' — the binding's own (§8.3). A disjointness operand whose
/// transitive cone shares a recorded edge must refute, not falsely
/// discharge on static edges alone.
#[test]
fn transitive_witness_provenance_flows_into_disjointness() {
    let root = examples_root();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    let bytes = edited(&program, |p| {
        // p-sep witnesses `separate-instrumentation` — the support
        // p-independence rests on — under edge `iso-cert-3`, so the edge
        // joins combined_rate's provenance two hops deep.
        p["premises"].as_array_mut().unwrap().push(json!({
            "id": "p-sep",
            "proposition": "separate-instrumentation",
            "at": {},
            "established_by": {
                "kind": "assertion",
                "party": "qa-4",
                "edge": "iso-cert-3"
            }
        }));
        // A second independence witness lets the chained apply's own
        // independence obligation discharge.
        p["premises"].as_array_mut().unwrap().push(json!({
            "id": "p-independence-2",
            "proposition": "independent",
            "arguments": {"over": ["combined_rate", "iso_probe"]},
            "at": {"scenario": "field-survey-1"},
            "established_by": {
                "kind": "assertion",
                "party": "lab-9-independence-review",
                "edge": "indep-memo-8"
            }
        }));
        // The only shared edge is two hops deep in combined_rate's cone.
        p["inputs"].as_array_mut().unwrap().push(json!({
            "id": "iso_probe",
            "type": {
                "quantity_kind": "dose_rate",
                "claim": "enclosure",
                "scenario": "field-survey-1"
            },
            "binding": {
                "state": "bound",
                "value": {"kind": "enclosure", "lower": "1/2", "upper": "3/5", "unit": "mSv/h"},
                "source": {"kind": "assertion", "party": "lab-7", "edge": "iso-cert-3"}
            }
        }));
        p["body"].as_array_mut().unwrap().push(json!({
            "bind": "final",
            "apply": "scaled-sum",
            "arguments": {
                "reading_a": {"ref": "combined_rate"},
                "reading_b": {"ref": "iso_probe"},
                "calibration": {"ref": "calibration"}
            }
        }));
        // `final` must be reachable for its refutation to refuse the plan.
        p["requirements"].as_array_mut().unwrap().push(json!({
            "id": "EL-R3",
            "subject": {"ref": "final"},
            "comparison": "le",
            "quantity_kind": "dose_rate",
            "limit": {"kind": "exact", "value": "9/2", "unit": "mSv/h"},
            "scope": "steady-state"
        }));
    });

    let analysis = analyze_program(&bytes, &library, &options);
    let obligation = analysis
        .obligations
        .iter()
        .rev()
        .find(|o| o.kind == "provenance_disjoint")
        .expect("the second apply generates the obligation");
    assert_eq!(
        obligation.state, "refuted",
        "combined_rate's transitive witness cone shares `iso-cert-3` with iso_probe"
    );
    assert!(kinds_of(&analysis).contains(&"obligation_refuted".to_string()));
    assert_eq!(analysis.plan.state, "refused");

    // Without the shared edge the transitive support still folds: the
    // residual empties and the deep witness edge joins provenance.
    let clean = edited(&bytes, |p| {
        for input in p["inputs"].as_array_mut().unwrap() {
            if input["id"] == "iso_probe" {
                input["binding"]["source"]["edge"] = json!("iso-cert-9");
            }
        }
    });
    let analysis = analyze_program(&clean, &library, &options);
    assert_eq!(analysis.plan.state, "ready");
    let combined = &analysis.bindings["combined_rate"];
    assert!(
        !combined
            .residual_assumptions
            .iter()
            .any(|a| a.proposition == "separate-instrumentation"),
        "separate-instrumentation discharges transitively: {:?}",
        combined.residual_assumptions
    );
    assert!(
        combined.source_edges.contains(&"iso-cert-3".to_string()),
        "the deep witness edge joins provenance: {:?}",
        combined.source_edges
    );
}

/// A `provenance_disjoint` premise is admissible at declaration time but
/// verified again at use: if its `over` operands share an edge only through
/// a witness cone — invisible to static admission — the premise is a
/// conflict and never discharges the assumption claiming it.
#[test]
fn disjointness_premise_is_rechecked_at_use_against_witness_cones() {
    let root = examples_root();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let options = AnalysisOptions::default();

    let bytes = edited(&program, |p| {
        // combined_rate's cone gains `iso-cert-3` through p-sep.
        p["premises"].as_array_mut().unwrap().push(json!({
            "id": "p-sep",
            "proposition": "separate-instrumentation",
            "at": {},
            "established_by": {"kind": "assertion", "party": "qa-4", "edge": "iso-cert-3"}
        }));
        // iso_probe shares that deep edge.
        p["inputs"].as_array_mut().unwrap().push(json!({
            "id": "iso_probe",
            "type": {
                "quantity_kind": "dose_rate",
                "claim": "enclosure",
                "scenario": "field-survey-1"
            },
            "binding": {
                "state": "bound",
                "value": {"kind": "enclosure", "lower": "1/2", "upper": "3/5", "unit": "mSv/h"},
                "source": {"kind": "assertion", "party": "lab-7", "edge": "iso-cert-3"}
            }
        }));
        // The premise asserts disjointness over operands whose cone
        // overlaps — statically invisible, false at use.
        p["premises"].as_array_mut().unwrap().push(json!({
            "id": "p-pd",
            "proposition": "provenance_disjoint",
            "arguments": {"over": ["combined_rate", "iso_probe"]},
            "at": {"scenario": "field-survey-1"},
            "established_by": {"kind": "assertion", "party": "qa-1", "edge": "pd-memo-1"}
        }));
        // An import carries the matching assumption — its discharge is the
        // use site.
        p["body"].as_array_mut().unwrap().push(json!({
            "bind": "probe2",
            "import": {
                "type": {
                    "quantity_kind": "dose_rate",
                    "claim": "enclosure",
                    "scenario": "field-survey-1"
                },
                "value": {"kind": "enclosure", "lower": "1/4", "upper": "2/5", "unit": "mSv/h"},
                "source": {"kind": "assertion", "party": "lab-7", "edge": "edge-probe2"},
                "assumptions": [
                    {"proposition": "provenance_disjoint", "at": {"scenario": "field-survey-1"}}
                ]
            }
        }));
    });
    let analysis = analyze_program(&bytes, &library, &options);
    assert!(
        analysis
            .findings
            .iter()
            .any(|f| f.kind == "premise_conflict" && f.at == "premises[p-pd]"),
        "the false premise is flagged at use: {:?}",
        analysis
            .findings
            .iter()
            .map(|f| (f.kind.as_str(), f.at.as_str()))
            .collect::<Vec<_>>()
    );
    let probe2 = &analysis.bindings["probe2"];
    assert!(
        probe2
            .residual_assumptions
            .iter()
            .any(|a| a.proposition == "provenance_disjoint"),
        "a conflicted witness does not discharge: {:?}",
        probe2.residual_assumptions
    );
}

/// Identifiers are interpolated into report paths (`premises[a, b]`),
/// `arguments.{slot}` segments, and lifecycle keys — a declared identifier
/// carrying a path delimiter or whitespace is refused at admission.
#[test]
fn identifiers_carry_no_path_delimiters() {
    let root = examples_root();
    let library = fs::read(root.join("libraries/measurement-scaling.v1.json")).unwrap();
    let program = fs::read(root.join("programs/positive-measurement-pass.program.json")).unwrap();
    let options = AnalysisOptions::default();

    // `a, b` inside a premise id parses as a two-member group — ambiguous.
    let bytes = edited(&program, |p| {
        p["premises"].as_array_mut().unwrap().push(json!({
            "id": "p-2, p-3",
            "proposition": "separate-instrumentation",
            "at": {},
            "established_by": {"kind": "assertion", "party": "qa-1"}
        }));
    });
    let analysis = analyze_program(&bytes, &library, &options);
    assert!(
        analysis
            .findings
            .iter()
            .any(|f| f.kind == "malformed" && f.detail.contains("delimiter")),
        "delimiter-bearing id refused: {:?}",
        analysis
            .findings
            .iter()
            .map(|f| f.at.clone())
            .collect::<Vec<_>>()
    );

    // A `.` in a binding name embeds a false segment in report paths.
    let bytes = edited(&program, |p| {
        p["body"].as_array_mut().unwrap().push(json!({
            "bind": "probe.two",
            "hole": {"quantity_kind": "dose_rate", "claim": "enclosure"}
        }));
    });
    let analysis = analyze_program(&bytes, &library, &options);
    assert!(
        analysis
            .findings
            .iter()
            .any(|f| f.kind == "malformed" && f.detail.contains("delimiter"))
    );

    // And the library side: a method named with `/` can be addressed by a
    // lifecycle key that collides with another (library, method) pair.
    let lib = edited(&library, |l| {
        l["methods"].as_array_mut().unwrap().push(json!({
            "id": "a/b",
            "inputs": {},
            "output": {"quantity_kind": "dose_rate", "claim": "enclosure"},
            "implementation": {"kind": "external"}
        }));
    });
    let analysis = analyze_program(&program, &lib, &options);
    assert!(
        analysis.findings.iter().any(|f| {
            f.document == "library" && f.kind == "malformed" && f.detail.contains("delimiter")
        }),
        "library identifier delimiters refused too: {:?}",
        analysis
            .findings
            .iter()
            .map(|f| (f.document.as_str(), f.at.as_str()))
            .collect::<Vec<_>>()
    );
}

/// F2 hardened: reorder every declared set AND perturb annotation fields —
/// semantic identity holds, and the whole analysis (findings order,
/// obligation positions, residual order, plan) must be byte-identical.
#[test]
fn annotation_and_order_changes_preserve_the_whole_analysis() {
    let root = examples_root();
    let library = fs::read(root.join("libraries/thermal-expansion.v1.json")).unwrap();
    let program =
        fs::read(root.join("programs/positive-assumption-discharge.program.json")).unwrap();
    let options = AnalysisOptions::default();

    let scrambled_lib = edited(&library, |l| {
        // Sets: methods, per-method requires/ensures/assumes — reverse all.
        l["methods"].as_array_mut().unwrap().reverse();
        for m in l["methods"].as_array_mut().unwrap() {
            for field in ["requires", "ensures", "assumes", "projects"] {
                if let Some(arr) = m.get_mut(field).and_then(|v| v.as_array_mut()) {
                    arr.reverse();
                }
            }
        }
        l["kind_products"].as_array_mut().unwrap().reverse();
        // Annotations never enter identity — change one anyway.
        l["methods"][0]["label"] = json!("presentation-only label");
        l["title"] = json!("a different title");
    });
    let scrambled_program = edited(&program, |p| {
        p["inputs"].as_array_mut().unwrap().reverse();
        p["premises"].as_array_mut().unwrap().reverse();
        p["requirements"].as_array_mut().unwrap().reverse();
        p["inputs"][0]["label"] = json!("annotation noise");
    });

    let a = analyze_program(&program, &library, &options);
    let b = analyze_program(&scrambled_program, &scrambled_lib, &options);
    assert_eq!(
        a.library.identity.semantic_sha256, b.library.identity.semantic_sha256,
        "annotation+order changes must preserve semantic identity"
    );
    assert_eq!(
        a.program.identity.semantic_sha256,
        b.program.identity.semantic_sha256
    );
    let mut av = serde_json::to_value(&a).unwrap();
    let mut bv = serde_json::to_value(&b).unwrap();
    for v in [&mut av, &mut bv] {
        v["program"]["document_sha256"] = json!("elided");
        v["library"]["document_sha256"] = json!("elided");
    }
    assert_eq!(
        av, bv,
        "equal identities must produce an equal analysis — findings order, \
         obligation positions, residual order included"
    );
}
