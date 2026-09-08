//! Synthetic transport and admission checks; no scientific model is exercised.

use super::*;
use serde_json::Value;

const DESCRIPTOR: &[u8] =
    include_bytes!("../../../../fixtures/external-checkers/numeric-uncertainty.adapter.json");
const OUTPUT: &[u8] =
    include_bytes!("../../../../fixtures/external-checkers/numeric-uncertainty.output.json");

fn descriptor_value() -> Value {
    serde_json::from_slice(DESCRIPTOR).unwrap()
}

fn adapter(value: &Value) -> ExternalCheckerAdapter {
    ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(value).unwrap()).unwrap()
}

fn extract(adapter: &ExternalCheckerAdapter, output: &[u8]) -> Result<Vec<ExtractedClaim>, String> {
    adapter.extract_claims(
        &BTreeMap::from([("report".into(), output.to_vec())]),
        &StepContext::default(),
    )
}

#[test]
fn numeric_models_preserve_bounds_and_unresolved_uncertainty() {
    let value = descriptor_value();
    let adapter = adapter(&value);
    assert_eq!(serde_json::to_value(&adapter).unwrap(), value);
    let claims = extract(&adapter, OUTPUT).unwrap();
    assert_eq!(
        claims[0].claim,
        json!({
            "model":"interval",
            "lower":{"value":"0.09","unit":"1"},
            "upper":{"value":"0.11","unit":"1"},
            "nominal":{"value":"0.1","unit":"1"}
        })
    );
    assert_eq!(
        claims[1].claim,
        json!({"model":"unquantified","nominal":{"value":"1/3","unit":"1"}})
    );
}

#[test]
fn omitted_interval_nominal_is_not_replaced_with_a_midpoint() {
    let mut descriptor = descriptor_value();
    descriptor["claims"][0]
        .as_object_mut()
        .unwrap()
        .remove("nominal_pointer");
    let mut output: Value = serde_json::from_slice(OUTPUT).unwrap();
    output["measurement"]
        .as_object_mut()
        .unwrap()
        .remove("nominal");
    output["measurement"]["lower"] = json!(0);
    output["measurement"]["upper"] = json!(1);
    let adapter = adapter(&descriptor);
    let claims = extract(&adapter, &serde_json::to_vec(&output).unwrap()).unwrap();
    assert!(claims[0].claim.get("nominal").is_none());
    assert_eq!(claims[0].claim["lower"]["value"], "0");
    assert_eq!(claims[0].claim["upper"]["value"], "1");
    assert_eq!(serde_json::to_value(adapter).unwrap(), descriptor);
}

#[test]
fn numeric_descriptors_refuse_missing_malformed_and_unknown_fields() {
    for field in ["lower_pointer", "upper_pointer", "nominal_pointer"] {
        for invalid in [
            json!(""),
            json!("relative"),
            json!("/bad~2escape"),
            Value::Null,
        ] {
            let mut value = descriptor_value();
            value["claims"][0][field] = invalid;
            assert!(
                ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).is_err(),
                "accepted invalid {field}: {value}"
            );
        }
    }
    for field in ["lower_pointer", "upper_pointer", "unit"] {
        let mut value = descriptor_value();
        value["claims"][0].as_object_mut().unwrap().remove(field);
        assert!(ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    for index in [0, 1] {
        for (field, invalid) in [
            ("unit", json!("")),
            ("output_id", json!("undeclared")),
            ("coverage", json!("0.95")),
        ] {
            let mut value = descriptor_value();
            value["claims"][index][field] = invalid;
            assert!(
                ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).is_err()
            );
        }
    }
    for field in ["pointer", "unit"] {
        let mut value = descriptor_value();
        value["claims"][1].as_object_mut().unwrap().remove(field);
        assert!(ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut value = descriptor_value();
    value["claims"][1]["pointer"] = json!("/bad~3escape");
    assert!(ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn every_numeric_pointer_requires_an_authoritative_number() {
    let adapter = adapter(&descriptor_value());
    for pointer in [
        "/measurement/lower",
        "/measurement/upper",
        "/measurement/nominal",
        "/estimate",
    ] {
        for invalid in [
            json!(true),
            json!("NaN"),
            json!("1e-2"),
            json!("2/4"),
            json!(0.1),
            json!(9007199254740993_u64),
            Value::Null,
        ] {
            let mut output: Value = serde_json::from_slice(OUTPUT).unwrap();
            *output.pointer_mut(pointer).unwrap() = invalid;
            assert!(
                extract(&adapter, &serde_json::to_vec(&output).unwrap()).is_err(),
                "accepted invalid number at {pointer}: {output}"
            );
        }
        let mut output: Value = serde_json::from_slice(OUTPUT).unwrap();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        output
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        let error = extract(&adapter, &serde_json::to_vec(&output).unwrap()).unwrap_err();
        assert!(error.contains(pointer), "{error}");
    }
}

#[cfg(unix)]
mod execution {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use avila_core_compiler::{ClaimQualification, compile_documents};
    use avila_core_evidence::sha256_file;
    use avila_core_kernel::VerdictStatus;

    use crate::case_run::{
        CaseRunOptions, CaseRunReport, CaseRunStatus, ChangeClass, StepExecutionState,
        execute_case, human_summary,
    };

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
        case: PathBuf,
        checker: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn write_json(path: &Path, value: &Value) {
        let mut bytes = serde_json::to_vec_pretty(value).unwrap();
        bytes.push(b'\n');
        fs::write(path, bytes).unwrap();
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    fn document(case: &Path, id: &str, role: &str, path: &str) -> Value {
        json!({"document_id":id,"role":role,"path":path,
            "sha256":sha256_file(&case.join(path)).unwrap().0})
    }

    impl Fixture {
        fn new(output: Value, model: &str, qualification: Option<bool>, basis: &str) -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "avila-core-numeric-checker-{}-{sequence}",
                std::process::id()
            ));
            let case = root.join("case");
            fs::create_dir_all(&case).unwrap();
            let checker = root.join("checker.sh");
            fs::write(&checker, "#!/bin/sh\nin=$1\nout=$2\nwhile IFS= read -r line || [ -n \"$line\" ]; do printf '%s\\n' \"$line\"; done < \"$in\" > \"$out\"\n").unwrap();
            fs::set_permissions(&checker, fs::Permissions::from_mode(0o755)).unwrap();
            let checker_sha = sha256_file(&checker).unwrap().0;
            write_json(&root.join("candidate.json"), &output);
            let output_sha = sha256_file(&root.join("candidate.json")).unwrap().0;

            let mut descriptor = descriptor_value();
            let claim = if model == "interval" {
                descriptor["claims"][0].clone()
            } else {
                let mut claim = descriptor["claims"][1].clone();
                claim["output_slot"] = json!("measurement");
                claim
            };
            descriptor["claims"] = json!([claim]);
            write_json(&case.join("adapter.json"), &descriptor);

            let allowed_models = json!([{"model":"interval"},{"model":"unquantified"}]);
            let contract = json!({
                "schema_version":"avila.core/evidence-contract/v0.2-draft",
                "semantic_profile":"avila.core/semantic/0.2-draft",
                "contract_id":"test.numeric-external-checker", "revision":1, "status":"draft",
                "question":"Synthetic scalar transport only; no physical model.",
                "execution_policy":{"require_qualification":true,"permit_nominal_basis":true},
                "inputs":[{"input_id":"candidate","role":{"id":"test.candidate","major":1},
                    "media_type":"application/json","claim_model":{"model":"unquantified"}}],
                "workflow":[{"step_id":"check","capability_type":{"id":"test.numeric-check","major":1},
                    "bindings":[{"input_slot":"candidate","source":{"source":"contract_input","input_id":"candidate"}}]}],
                "requirements":[{"requirement_id":"SCALAR-LIMIT","statement":"Synthetic scalar <= 0.1.",
                    "purpose":{"id":"test.purpose","major":1},
                    "metric":{"source":"step_output","step_id":"check","output_slot":"measurement"},
                    "comparison":"less_than_or_equal","limit":{"kind":"test.scalar","value":"0.1","unit":"1"},
                    "basis":{"kind":basis}}]
            });
            let registry = json!({
                "schema_version":"avila.core/registry-snapshot/v0.2-draft",
                "semantic_profile":"avila.core/semantic/0.2-draft",
                "registry_id":"test.numeric-external-checker.registry","revision":1,
                "kinds":[{"kind_id":"test.scalar","canonical_unit":"1","unit_class":"test.scalar.units@1",
                    "owner":"test","units":[{"symbol":"1","factor":"1"}]}],
                "purposes":[{"purpose":{"id":"test.purpose","major":1},"owner":"test","description":"synthetic only"}],
                "roles":[
                    {"role":{"id":"test.candidate","major":1},"owner":"test","validator":"test/numeric-check@1",
                     "accepted_media_types":["application/json"],"permitted_claim_models":[{"model":"unquantified"}]},
                    {"role":{"id":"test.measurement","major":1},"owner":"test","validator":"test/numeric-check@1",
                     "quantity_kind":"test.scalar","unit_class":"test.scalar.units@1",
                     "accepted_media_types":["application/json"],"permitted_claim_models":allowed_models}
                ],
                "capability_types":[{"capability_type":{"id":"test.numeric-check","major":1},"owner":"test",
                    "reproducibility":{"determinism":"deterministic"},
                    "inputs":[{"slot_id":"candidate","role":{"id":"test.candidate","major":1},"accepted_media_types":["application/json"]}],
                    "outputs":[{"slot_id":"measurement","role":{"id":"test.measurement","major":1},
                        "media_type":"application/json","permitted_claim_models":allowed_models}]}]
            });
            write_json(&case.join("contract.json"), &contract);
            write_json(&case.join("registry.json"), &registry);
            let compilation = compile_documents(
                &serde_json::to_vec(&contract).unwrap(),
                &serde_json::to_vec(&registry).unwrap(),
            )
            .unwrap();
            let compiled = compilation
                .compiled
                .as_ref()
                .unwrap_or_else(|| panic!("{compilation:?}"));
            let claim = if model == "interval" {
                json!({"model":"interval",
                    "lower":{"value":output["measurement"]["lower"],"unit":"1"},
                    "upper":{"value":output["measurement"]["upper"],"unit":"1"},
                    "nominal":{"value":output["measurement"]["nominal"],"unit":"1"}})
            } else {
                json!({"model":"unquantified","nominal":{"value":output["estimate"],"unit":"1"}})
            };
            write_json(
                &case.join("claims.json"),
                &json!({
                    "schema_version":"avila.core/evidence-claims/v0.2-draft",
                    "semantic_profile":"avila.core/semantic/0.2-draft",
                    "compiled_snapshot_sha256":compiled.snapshot_sha256,
                    "inputs":[{"input_id":"candidate","artifact":{"sha256":output_sha,"media_type":"application/json"}}],
                    "claims":[{"claim_id":"measurement","step_id":"check","output_slot":"measurement",
                        "artifact":{"sha256":output_sha,"media_type":"application/json"},
                        "producer":{"package_id":"test/checker@1","sha256":checker_sha},"claim":claim}]
                }),
            );
            let mut documents = vec![
                document(&case, "contract", "contract", "contract.json"),
                document(&case, "registry", "registry", "registry.json"),
                document(&case, "claims", "claims", "claims.json"),
                document(
                    &case,
                    "test/numeric-check@1",
                    EXTERNAL_CHECKER_DOCUMENT_ROLE,
                    "adapter.json",
                ),
            ];
            if let Some(inside) = qualification {
                write_json(
                    &case.join("qualification.json"),
                    &json!({
                        "schema_version":"avila.core/qualification/v0.1-draft",
                        "qualification_id":"test/synthetic-transport","revision":1,"owner":"test",
                        "adapter":"test/numeric-check@1",
                        "capability":{"capability_id":"checker","executable_sha256":checker_sha},
                        "covered_output_slots":["measurement"],
                        "statement":"Synthetic qualification-gate fixture; no scientific validation.",
                        "scope":{"input_attribute_in":{"slot":"candidate","attribute":"media_type",
                            "values":[if inside {"application/json"} else {"application/octet-stream"}]}},
                        "validation_evidence":[],"limitations":["Synthetic test only; not a qualified physical method."]
                    }),
                );
                documents.push(document(
                    &case,
                    "qualification",
                    "qualification",
                    "qualification.json",
                ));
            }
            write_json(
                &case.join("package.json"),
                &json!({
                    "schema_version":"avila.core/case-package/v0.1-draft",
                    "case_id":"NUMERIC-CHECKER-TEST","title":"Synthetic numeric transport only",
                    "documents":documents,
                    "artifacts":[
                        {"artifact_id":"candidate","evidence_ids":["input:candidate"],"source_root":"fixture","path":"candidate.json","sha256":output_sha},
                        {"artifact_id":"expected","evidence_ids":["measurement"],"source_root":"fixture","path":"candidate.json","sha256":output_sha}
                    ],
                    "capabilities":[{"capability_id":"checker","package_id":"test/checker@1","executable_sha256":checker_sha}],
                    "executions":[{"step_id":"check","adapter":"test/numeric-check@1","capability_id":"checker",
                        "inputs":[{"input_slot":"candidate","workspace_path":"inputs/candidate.json"}],
                        "outputs":[{"output_slot":"measurement","claim_id":"measurement"}]}]
                }),
            );
            let fixture = Self {
                root,
                case,
                checker,
            };
            if qualification.is_some() {
                // Bind the expectation from Core's actual pre-execution
                // qualification assessment. Fresh execution must reproduce it.
                let mut options = fixture.options("qualification-plan");
                options.plan_only = true;
                options.log = None;
                let planned = execute_case(&fixture.case, &options).unwrap();
                let assessment = planned.execution.as_ref().unwrap().steps[0]
                    .qualification
                    .as_ref()
                    .unwrap();
                let mut claims = read_json(&fixture.case.join("claims.json"));
                claims["claims"][0]["qualification"] =
                    serde_json::to_value(ClaimQualification::from(assessment)).unwrap();
                write_json(&fixture.case.join("claims.json"), &claims);
                fixture.bind_document("claims", "claims", "claims.json", None);
            }
            fixture
        }

        fn options(&self, name: &str) -> CaseRunOptions {
            CaseRunOptions {
                source_roots: BTreeMap::from([("fixture".into(), self.root.clone())]),
                capabilities: BTreeMap::from([("checker".into(), self.checker.clone())]),
                workspace: Some(self.root.join(name)),
                log: Some(self.root.join("attempts.jsonl")),
                ..CaseRunOptions::default()
            }
        }

        fn run(&self, name: &str) -> CaseRunReport {
            execute_case(&self.case, &self.options(name)).unwrap()
        }

        fn bind_document(&self, id: &str, role: &str, path: &str, step: Option<&str>) {
            let mut package = read_json(&self.case.join("package.json"));
            let documents = package["documents"].as_array_mut().unwrap();
            documents.retain(|document| document["document_id"] != id);
            let mut bound = document(&self.case, id, role, path);
            if let Some(step) = step {
                bound["step_id"] = json!(step);
            }
            documents.push(bound);
            write_json(&self.case.join("package.json"), &package);
        }

        fn commit_receipt(&self, workspace: &str) {
            fs::copy(
                self.root.join(workspace).join("check/receipt.json"),
                self.case.join("receipt.json"),
            )
            .unwrap();
            fs::copy(
                self.root.join(workspace).join("claims.json"),
                self.case.join("claims.json"),
            )
            .unwrap();
            self.bind_document(
                "receipt",
                "execution_receipt",
                "receipt.json",
                Some("check"),
            );
            self.bind_document("claims", "claims", "claims.json", None);
        }
    }

    fn output(lower: &str, nominal: &str, upper: &str) -> Value {
        json!({"measurement":{"lower":lower,"nominal":nominal,"upper":upper},"estimate":nominal})
    }

    fn assert_verdict(report: &CaseRunReport, expected: VerdictStatus) {
        assert_eq!(
            report.status,
            CaseRunStatus::Evaluated,
            "{}",
            human_summary(report)
        );
        assert_eq!(
            report.margins[0].status,
            expected,
            "{}",
            human_summary(report)
        );
    }

    #[test]
    fn executed_intervals_preserve_verdict_boundaries_and_qualification() {
        for (lower, nominal, upper, qualification, expected) in [
            ("0.08", "0.09", "0.1", Some(true), VerdictStatus::Pass),
            (
                "0.09",
                "0.1",
                "0.11",
                Some(true),
                VerdictStatus::Inconclusive,
            ),
            ("0.11", "0.12", "0.13", Some(true), VerdictStatus::Fail),
            ("0.08", "0.09", "0.1", None, VerdictStatus::NotEvaluated),
            (
                "0.08",
                "0.09",
                "0.1",
                Some(false),
                VerdictStatus::NotEvaluated,
            ),
            (
                "0.11",
                "0.1",
                "0.09",
                Some(true),
                VerdictStatus::NotEvaluated,
            ),
            (
                "0.08",
                "0.12",
                "0.1",
                Some(true),
                VerdictStatus::NotEvaluated,
            ),
        ] {
            let fixture = Fixture::new(
                output(lower, nominal, upper),
                "interval",
                qualification,
                "bounded",
            );
            let report = fixture.run("fresh");
            assert_verdict(&report, expected);
            assert_eq!(
                report.execution.as_ref().unwrap().steps[0].state,
                StepExecutionState::Executed
            );
            let generated = read_json(&fixture.root.join("fresh/claims.json"));
            assert_eq!(generated["claims"][0]["claim"]["lower"]["value"], lower);
            assert_eq!(generated["claims"][0]["claim"]["upper"]["value"], upper);
        }
    }

    #[test]
    fn numeric_unquantified_is_only_a_nominal_guide_even_with_qualification() {
        for (basis, expected) in [
            ("bounded", VerdictStatus::NotEvaluated),
            ("nominal", VerdictStatus::Pass),
        ] {
            let fixture = Fixture::new(
                output("0.08", "0.09", "0.1"),
                "unquantified",
                Some(true),
                basis,
            );
            let report = fixture.run("fresh");
            assert_verdict(&report, expected);
            let generated = read_json(&fixture.root.join("fresh/claims.json"));
            let claim = &generated["claims"][0]["claim"];
            assert_eq!(claim["model"], "unquantified");
            assert!(claim.get("lower").is_none());
            assert!(claim.get("upper").is_none());
            assert_eq!(claim["nominal"]["value"], "0.09");
        }
    }

    #[test]
    fn interval_receipt_reuse_preserves_bounds_and_mapping_changes_invalidate_it() {
        let fixture = Fixture::new(
            output("0.09", "0.1", "0.11"),
            "interval",
            Some(true),
            "bounded",
        );
        let fresh = fixture.run("first");
        assert_verdict(&fresh, VerdictStatus::Inconclusive);
        fixture.commit_receipt("first");

        let mut plan = fixture.options("plan");
        plan.plan_only = true;
        let planned = execute_case(&fixture.case, &plan).unwrap();
        assert_eq!(
            planned.execution.as_ref().unwrap().steps[0].state,
            StepExecutionState::Reused
        );
        assert!(!fixture.root.join("plan").exists());
        let reused = fixture.run("reused");
        assert_verdict(&reused, VerdictStatus::Inconclusive);
        assert_eq!(
            reused.execution.as_ref().unwrap().steps[0].state,
            StepExecutionState::Reused
        );
        assert_eq!(reused.claims.as_ref().unwrap().reused_claims, 1);
        assert!(reused.claims.as_ref().unwrap().matches_committed);
        assert!(!fixture.root.join("reused").exists());

        let mut repeat = fixture.options("deliberately-fresh");
        repeat.reuse = false;
        let repeated = execute_case(&fixture.case, &repeat).unwrap();
        assert_verdict(&repeated, VerdictStatus::Inconclusive);
        assert_eq!(
            repeated.execution.as_ref().unwrap().steps[0].state,
            StepExecutionState::Executed
        );

        let mut descriptor = read_json(&fixture.case.join("adapter.json"));
        descriptor["claims"][0]["upper_pointer"] = json!("/measurement/nominal");
        write_json(&fixture.case.join("adapter.json"), &descriptor);
        // Rebinding the changed descriptor must not keep the old receipt usable.
        fixture.bind_document(
            "test/numeric-check@1",
            EXTERNAL_CHECKER_DOCUMENT_ROLE,
            "adapter.json",
            None,
        );
        let mut options = fixture.options("changed-plan");
        options.plan_only = true;
        let changed = execute_case(&fixture.case, &options).unwrap();
        let step = &changed.execution.as_ref().unwrap().steps[0];
        assert_eq!(step.state, StepExecutionState::Planned);
        assert!(
            step.changes
                .iter()
                .any(|change| change.class == ChangeClass::Invocation)
        );
        assert!(!fixture.root.join("changed-plan").exists());
        let changed = fixture.run("changed");
        assert_eq!(changed.status, CaseRunStatus::Rejected);
        assert!(
            changed
                .findings
                .iter()
                .any(|finding| finding.code == "CORE-X3101")
        );
        assert_eq!(changed.margins[0].status, VerdictStatus::Pass);
        assert_eq!(
            changed.execution.as_ref().unwrap().steps[0].state,
            StepExecutionState::Executed
        );
        let generated = read_json(&fixture.root.join("changed/claims.json"));
        assert_eq!(generated["claims"][0]["claim"]["upper"]["value"], "0.1");
        let receipt = read_json(&fixture.root.join("changed/check/receipt.json"));
        assert_eq!(
            receipt["invocation"]["adapter_sha256"],
            sha256_file(&fixture.case.join("adapter.json")).unwrap().0
        );
        let attempts = fs::read_to_string(fixture.root.join("attempts.jsonl")).unwrap();
        let last: Value = serde_json::from_str(attempts.lines().last().unwrap()).unwrap();
        assert_eq!(last["claims"][0]["claim"]["upper"]["value"], "0.1");
    }
}
