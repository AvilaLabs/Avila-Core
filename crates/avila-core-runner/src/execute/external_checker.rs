//! A package-declared adapter for content-bound command-line checkers.
//!
//! The descriptor fixes a capability type, input/output boundary, argument
//! vector, timeout, and JSON-pointer claim extraction. It never invokes a
//! shell, discovers files, or delegates scientific interpretation to Core.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use avila_core_evidence::CapabilityTypeRef;
use avila_core_kernel::{CanonicalJsonValue, ExactNumber, read_authoritative_json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{ExtractedClaim, ResolvedAdapterOutput, StepContext};

pub const EXTERNAL_CHECKER_ADAPTER_SCHEMA_VERSION: &str =
    "avila.core/external-checker-adapter/v0.1-draft";
pub const EXTERNAL_CHECKER_DOCUMENT_ROLE: &str = "external_checker_adapter";
const MAX_TIMEOUT_MS: u64 = 24 * 60 * 60 * 1_000;

/// A hash-bound command-line checker's complete adapter boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalCheckerAdapter {
    pub schema_version: String,
    pub adapter_id: String,
    pub capability_type: CapabilityTypeRef,
    pub input_slots: Vec<String>,
    pub arguments: Vec<ExternalArgument>,
    pub outputs: Vec<ExternalOutput>,
    pub claims: Vec<ExternalClaim>,
    pub timeout_ms: u64,
    #[serde(default)]
    pub limitations: Vec<String>,
}

/// One argument in the exact argv passed directly to the executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExternalArgument {
    Literal { value: String },
    InputPath { input_slot: String },
    OutputPath { output_id: String },
}

/// One file the checker must leave in the runner-owned workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalOutput {
    pub output_id: String,
    pub workspace_path: String,
    pub media_type: String,
}

/// A claim extracted from an authoritative JSON output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExternalClaim {
    Exact {
        output_slot: String,
        output_id: String,
        pointer: String,
        unit: String,
    },
    Categorical {
        output_slot: String,
        output_id: String,
        pointer: String,
        allowed_values: Vec<String>,
    },
}

impl ExternalClaim {
    fn output_slot(&self) -> &str {
        match self {
            Self::Exact { output_slot, .. } | Self::Categorical { output_slot, .. } => output_slot,
        }
    }

    fn output_id(&self) -> &str {
        match self {
            Self::Exact { output_id, .. } | Self::Categorical { output_id, .. } => output_id,
        }
    }

    fn pointer(&self) -> &str {
        match self {
            Self::Exact { pointer, .. } | Self::Categorical { pointer, .. } => pointer,
        }
    }
}

impl ExternalCheckerAdapter {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        read_authoritative_json(bytes).map_err(|error| {
            format!(
                "adapter descriptor is not authoritative JSON at `{}`: {}",
                error.pointer().unwrap_or(""),
                error.detail()
            )
        })?;
        let adapter: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("adapter descriptor does not match its schema: {error}"))?;
        adapter.validate()?;
        Ok(adapter)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != EXTERNAL_CHECKER_ADAPTER_SCHEMA_VERSION {
            return Err(format!(
                "unsupported adapter schema `{}`; expected `{EXTERNAL_CHECKER_ADAPTER_SCHEMA_VERSION}`",
                self.schema_version
            ));
        }
        require_nonempty("adapter_id", &self.adapter_id)?;
        require_nonempty("capability_type.id", &self.capability_type.id)?;
        if self.capability_type.major == 0 {
            return Err("capability_type.major must be at least 1".into());
        }
        if self.timeout_ms == 0 || self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(format!(
                "timeout_ms must lie in 1..={MAX_TIMEOUT_MS}, observed {}",
                self.timeout_ms
            ));
        }
        if self.input_slots.is_empty() {
            return Err("an external checker must declare at least one input slot".into());
        }
        if self.outputs.is_empty() {
            return Err("an external checker must declare at least one output".into());
        }
        if self.claims.is_empty() {
            return Err("an external checker must extract at least one claim".into());
        }

        let mut input_slots = BTreeSet::new();
        for slot in &self.input_slots {
            require_nonempty("input slot", slot)?;
            if !input_slots.insert(slot.as_str()) {
                return Err(format!("input slot `{slot}` is declared twice"));
            }
        }

        let mut output_ids = BTreeSet::new();
        let mut output_paths = BTreeSet::new();
        for output in &self.outputs {
            require_nonempty("output_id", &output.output_id)?;
            require_nonempty("output media_type", &output.media_type)?;
            validate_workspace_path(&output.workspace_path)?;
            if !output_ids.insert(output.output_id.as_str()) {
                return Err(format!("output `{}` is declared twice", output.output_id));
            }
            if !output_paths.insert(output.workspace_path.as_str()) {
                return Err(format!(
                    "two outputs use workspace path `{}`",
                    output.workspace_path
                ));
            }
        }

        let mut referenced_inputs = BTreeSet::new();
        let mut referenced_outputs = BTreeSet::new();
        for argument in &self.arguments {
            match argument {
                ExternalArgument::Literal { value } => require_nonempty("literal argument", value)?,
                ExternalArgument::InputPath { input_slot } => {
                    if !input_slots.contains(input_slot.as_str()) {
                        return Err(format!(
                            "argument references undeclared input slot `{input_slot}`"
                        ));
                    }
                    referenced_inputs.insert(input_slot.as_str());
                }
                ExternalArgument::OutputPath { output_id } => {
                    if !output_ids.contains(output_id.as_str()) {
                        return Err(format!(
                            "argument references undeclared output `{output_id}`"
                        ));
                    }
                    referenced_outputs.insert(output_id.as_str());
                }
            }
        }
        if referenced_inputs != input_slots {
            return Err(format!(
                "argument vector references input slots {referenced_inputs:?}, expected every declared slot {input_slots:?}"
            ));
        }
        if referenced_outputs != output_ids {
            return Err(format!(
                "argument vector references outputs {referenced_outputs:?}, expected every declared output {output_ids:?}"
            ));
        }

        let mut output_slots = BTreeSet::new();
        for claim in &self.claims {
            require_nonempty("claim output_slot", claim.output_slot())?;
            if !output_slots.insert(claim.output_slot()) {
                return Err(format!(
                    "output slot `{}` is extracted twice",
                    claim.output_slot()
                ));
            }
            if !output_ids.contains(claim.output_id()) {
                return Err(format!(
                    "claim for output slot `{}` references undeclared output `{}`",
                    claim.output_slot(),
                    claim.output_id()
                ));
            }
            validate_pointer(claim.pointer())?;
            match claim {
                ExternalClaim::Exact { unit, .. } => require_nonempty("exact claim unit", unit)?,
                ExternalClaim::Categorical { allowed_values, .. } => {
                    if allowed_values.is_empty() {
                        return Err(format!(
                            "categorical output slot `{}` has no allowed values",
                            claim.output_slot()
                        ));
                    }
                    let mut unique = BTreeSet::new();
                    for value in allowed_values {
                        require_nonempty("allowed categorical value", value)?;
                        if !unique.insert(value.as_str()) {
                            return Err(format!(
                                "categorical output slot `{}` repeats allowed value `{value}`",
                                claim.output_slot()
                            ));
                        }
                    }
                }
            }
        }
        for limitation in &self.limitations {
            require_nonempty("limitation", limitation)?;
        }
        Ok(())
    }

    pub fn output_specs(&self) -> Vec<ResolvedAdapterOutput> {
        self.outputs
            .iter()
            .map(|output| ResolvedAdapterOutput {
                output_id: output.output_id.clone(),
                workspace_path: output.workspace_path.clone(),
                media_type: output.media_type.clone(),
            })
            .collect()
    }

    pub fn output_slots(&self) -> Vec<&str> {
        self.claims.iter().map(ExternalClaim::output_slot).collect()
    }

    pub fn arguments(&self, staged: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
        let supplied: BTreeSet<&str> = staged.keys().map(String::as_str).collect();
        let expected: BTreeSet<&str> = self.input_slots.iter().map(String::as_str).collect();
        if supplied != expected {
            return Err(format!(
                "staged input slots {supplied:?} differ from descriptor slots {expected:?}"
            ));
        }
        let output_paths: BTreeMap<&str, &str> = self
            .outputs
            .iter()
            .map(|output| (output.output_id.as_str(), output.workspace_path.as_str()))
            .collect();
        self.arguments
            .iter()
            .map(|argument| match argument {
                ExternalArgument::Literal { value } => Ok(value.clone()),
                ExternalArgument::InputPath { input_slot } => staged
                    .get(input_slot)
                    .cloned()
                    .ok_or_else(|| format!("input slot `{input_slot}` is not staged")),
                ExternalArgument::OutputPath { output_id } => output_paths
                    .get(output_id.as_str())
                    .map(|path| (*path).to_string())
                    .ok_or_else(|| format!("output `{output_id}` has no workspace path")),
            })
            .collect()
    }

    pub fn extract_claims(
        &self,
        outputs: &BTreeMap<String, Vec<u8>>,
        _context: &StepContext,
    ) -> Result<Vec<ExtractedClaim>, String> {
        let expected: BTreeSet<&str> = self
            .outputs
            .iter()
            .map(|output| output.output_id.as_str())
            .collect();
        for supplied in outputs.keys() {
            if !expected.contains(supplied.as_str()) {
                return Err(format!("checker returned undeclared output `{supplied}`"));
            }
        }

        let mut documents = BTreeMap::new();
        for claim in &self.claims {
            if documents.contains_key(claim.output_id()) {
                continue;
            }
            let bytes = outputs.get(claim.output_id()).ok_or_else(|| {
                format!("checker output `{}` was not collected", claim.output_id())
            })?;
            let document = read_authoritative_json(bytes).map_err(|error| {
                format!(
                    "checker output `{}` is not authoritative JSON at `{}`: {}",
                    claim.output_id(),
                    error.pointer().unwrap_or(""),
                    error.detail()
                )
            })?;
            documents.insert(claim.output_id().to_string(), document);
        }

        self.claims
            .iter()
            .map(|claim| {
                let document = documents
                    .get(claim.output_id())
                    .expect("every referenced output was parsed");
                let value = document.pointer(claim.pointer()).ok_or_else(|| {
                    format!(
                        "checker output `{}` has no value at JSON Pointer `{}`",
                        claim.output_id(),
                        claim.pointer()
                    )
                })?;
                let claim_value = match claim {
                    ExternalClaim::Exact { unit, .. } => {
                        let exact = exact_value(value, claim.pointer())?;
                        json!({
                            "model": "exact",
                            "nominal": { "value": exact, "unit": unit },
                        })
                    }
                    ExternalClaim::Categorical { allowed_values, .. } => {
                        let CanonicalJsonValue::String(category) = value else {
                            return Err(format!(
                                "categorical value at `{}` must be a string",
                                claim.pointer()
                            ));
                        };
                        if !allowed_values.contains(category) {
                            return Err(format!(
                                "categorical value `{category}` at `{}` is outside the descriptor's closed set {:?}",
                                claim.pointer(), allowed_values
                            ));
                        }
                        json!({ "model": "unquantified", "value": category })
                    }
                };
                Ok(ExtractedClaim {
                    output_slot: claim.output_slot().to_string(),
                    output_id: claim.output_id().to_string(),
                    claim: claim_value,
                })
            })
            .collect()
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

fn exact_value(value: &CanonicalJsonValue, pointer: &str) -> Result<String, String> {
    match value {
        CanonicalJsonValue::Integer(value) => Ok(value.to_string()),
        CanonicalJsonValue::String(value) => ExactNumber::from_canonical(value)
            .map(|_| value.clone())
            .map_err(|error| format!("exact value at `{pointer}`: {}", error.detail())),
        _ => Err(format!(
            "exact value at `{pointer}` must be a safe integer or canonical exact-number string"
        )),
    }
}

fn validate_pointer(pointer: &str) -> Result<(), String> {
    if pointer.is_empty() || !pointer.starts_with('/') {
        return Err(format!(
            "JSON Pointer `{pointer}` must be nonempty and begin with `/`"
        ));
    }
    // RFC 6901 otherwise permits any escaped token. Explicitly reject
    // malformed tilde escapes here.
    let bytes = pointer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            if !matches!(bytes.get(index + 1), Some(b'0' | b'1')) {
                return Err(format!(
                    "JSON Pointer `{pointer}` has an invalid `~` escape"
                ));
            }
            index += 1;
        }
        index += 1;
    }
    Ok(())
}

fn validate_workspace_path(value: &str) -> Result<(), String> {
    require_nonempty("output workspace_path", value)?;
    let path = Path::new(value);
    if !path.is_relative()
        || value.contains('\\')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(format!(
            "output workspace path `{value}` must be normalized, portable, and relative with `/`-separated components"
        ));
    }
    if value == "receipt.json" || path.starts_with("logs") {
        return Err(format!(
            "output workspace path `{value}` collides with runner-owned records"
        ));
    }
    Ok(())
}

fn require_nonempty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;
    #[cfg(unix)]
    use std::path::{Path, PathBuf};
    #[cfg(unix)]
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(unix)]
    use avila_core_compiler::compile_documents;
    #[cfg(unix)]
    use avila_core_evidence::sha256_file;
    #[cfg(unix)]
    use avila_core_kernel::VerdictStatus;
    #[cfg(unix)]
    use serde_json::Value;

    #[cfg(unix)]
    use crate::case_run::{
        CaseRunOptions, CaseRunStatus, StepExecutionState, execute_case, human_summary,
    };

    #[cfg(unix)]
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    #[cfg(unix)]
    struct TestDir(PathBuf);

    #[cfg(unix)]
    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "avila-core-external-checker-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    #[cfg(unix)]
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn descriptor() -> ExternalCheckerAdapter {
        ExternalCheckerAdapter::from_bytes(
            br#"{
              "schema_version":"avila.core/external-checker-adapter/v0.1-draft",
              "adapter_id":"example/check@1",
              "capability_type":{"id":"example.check","major":1},
              "input_slots":["candidate"],
              "arguments":[
                {"kind":"literal","value":"check"},
                {"kind":"input_path","input_slot":"candidate"},
                {"kind":"literal","value":"--output"},
                {"kind":"output_path","output_id":"report"}
              ],
              "outputs":[{"output_id":"report","workspace_path":"outputs/report.json","media_type":"application/json"}],
              "claims":[
                {"model":"exact","output_slot":"remaining","output_id":"report","pointer":"/remaining","unit":"1"},
                {"model":"categorical","output_slot":"outcome","output_id":"report","pointer":"/outcome","allowed_values":["clear","rejected"]}
              ],
              "timeout_ms":30000,
              "limitations":["fixture"]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn descriptor_maps_only_declared_paths_into_argv() {
        let adapter = descriptor();
        let arguments = adapter
            .arguments(&BTreeMap::from([(
                "candidate".into(),
                "inputs/candidate.json".into(),
            )]))
            .unwrap();
        assert_eq!(
            arguments,
            [
                "check",
                "inputs/candidate.json",
                "--output",
                "outputs/report.json"
            ]
        );
        assert_eq!(adapter.output_slots(), ["remaining", "outcome"]);
    }

    #[test]
    fn extraction_preserves_number_and_category_without_conflating_them() {
        let claims = descriptor()
            .extract_claims(
                &BTreeMap::from([(
                    "report".into(),
                    br#"{"remaining":102,"outcome":"rejected"}"#.to_vec(),
                )]),
                &StepContext::default(),
            )
            .unwrap();
        assert_eq!(claims[0].claim["nominal"]["value"], "102");
        assert_eq!(
            claims[1].claim,
            json!({"model":"unquantified","value":"rejected"})
        );
    }

    #[test]
    fn extraction_refuses_unknown_categories_and_non_authoritative_numbers() {
        let adapter = descriptor();
        let unknown = adapter
            .extract_claims(
                &BTreeMap::from([(
                    "report".into(),
                    br#"{"remaining":0,"outcome":"maybe"}"#.to_vec(),
                )]),
                &StepContext::default(),
            )
            .unwrap_err();
        assert!(unknown.contains("outside the descriptor's closed set"));

        let floating = adapter
            .extract_claims(
                &BTreeMap::from([(
                    "report".into(),
                    br#"{"remaining":0.0,"outcome":"clear"}"#.to_vec(),
                )]),
                &StepContext::default(),
            )
            .unwrap_err();
        assert!(floating.contains("not authoritative JSON"));
    }

    #[test]
    fn descriptor_refuses_unbounded_or_escaping_execution_surfaces() {
        let mut value = serde_json::to_value(descriptor()).unwrap();
        value["outputs"][0]["workspace_path"] = json!("../report.json");
        let error =
            ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.contains("normalized, portable, and relative"));

        value["outputs"][0]["workspace_path"] = json!("outputs//report.json");
        let error =
            ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.contains("normalized, portable, and relative"));

        value["outputs"][0]["workspace_path"] = json!("outputs/report.json");
        value["timeout_ms"] = json!(MAX_TIMEOUT_MS + 1);
        let error =
            ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.contains("timeout_ms"));
    }

    #[test]
    fn descriptor_requires_every_staged_and_collected_path_in_the_argument_vector() {
        let mut value = serde_json::to_value(descriptor()).unwrap();
        value["arguments"] = json!([
            {"kind":"literal","value":"check"},
            {"kind":"literal","value":"--output"},
            {"kind":"output_path","output_id":"report"}
        ]);
        let error =
            ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.contains("expected every declared slot"));

        value["arguments"] = json!([
            {"kind":"literal","value":"check"},
            {"kind":"input_path","input_slot":"candidate"}
        ]);
        let error =
            ExternalCheckerAdapter::from_bytes(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.contains("expected every declared output"));
    }

    #[cfg(unix)]
    #[test]
    fn package_declared_checker_executes_and_logs_categorical_evidence() {
        let temp = TestDir::new();
        let case = temp.0.join("case");
        let root = temp.0.join("artifacts");
        fs::create_dir_all(&case).unwrap();
        fs::create_dir_all(&root).unwrap();

        let checker = temp.0.join("checker.sh");
        fs::write(
            &checker,
            "#!/bin/sh\nin=$1\nout=$2\nwhile IFS= read -r line || [ -n \"$line\" ]; do printf '%s\\n' \"$line\"; done < \"$in\" > \"$out\"\n",
        )
        .unwrap();
        fs::set_permissions(&checker, fs::Permissions::from_mode(0o755)).unwrap();
        let checker_sha = sha256_file(&checker).unwrap().0;

        let result = b"{\"remaining\":2,\"outcome\":\"rejected\"}\n";
        fs::write(root.join("candidate.json"), result).unwrap();
        fs::write(root.join("expected.json"), result).unwrap();
        let result_sha = sha256_file(&root.join("candidate.json")).unwrap().0;

        let contract = json!({
            "schema_version": "avila.core/evidence-contract/v0.2-draft",
            "semantic_profile": "avila.core/semantic/0.2-draft",
            "contract_id": "test.external-checker",
            "revision": 1,
            "status": "draft",
            "question": "Are no findings left?",
            "inputs": [{
                "input_id": "candidate",
                "role": {"id":"test.candidate","major":1},
                "media_type": "application/json",
                "claim_model": {"model":"unquantified"}
            }],
            "workflow": [{
                "step_id": "check",
                "capability_type": {"id":"test.check","major":1},
                "bindings": [{
                    "input_slot": "candidate",
                    "source": {"source":"contract_input","input_id":"candidate"}
                }]
            }],
            "requirements": [{
                "requirement_id": "NO-FINDINGS",
                "statement": "No findings remain.",
                "purpose": {"id":"test.purpose","major":1},
                "metric": {"source":"step_output","step_id":"check","output_slot":"remaining"},
                "comparison": "less_than_or_equal",
                "limit": {"kind":"test.count","value":"0","unit":"1"},
                "basis": {"kind":"bounded"}
            }]
        });
        let registry = json!({
            "schema_version": "avila.core/registry-snapshot/v0.2-draft",
            "semantic_profile": "avila.core/semantic/0.2-draft",
            "registry_id": "test.external-checker.registry",
            "revision": 1,
            "kinds": [{
                "kind_id":"test.count", "canonical_unit":"1",
                "unit_class":"test.count.units@1", "owner":"test",
                "units":[{"symbol":"1","factor":"1"}]
            }],
            "purposes": [{
                "purpose":{"id":"test.purpose","major":1},
                "owner":"test", "description":"test only"
            }],
            "roles": [
                {
                    "role":{"id":"test.candidate","major":1}, "owner":"test",
                    "validator":"test/check@1", "accepted_media_types":["application/json"],
                    "permitted_claim_models":[{"model":"unquantified"}]
                },
                {
                    "role":{"id":"test.remaining","major":1}, "owner":"test",
                    "validator":"test/check@1", "quantity_kind":"test.count",
                    "unit_class":"test.count.units@1", "accepted_media_types":["application/json"],
                    "permitted_claim_models":[{"model":"exact"}]
                },
                {
                    "role":{"id":"test.outcome","major":1}, "owner":"test",
                    "validator":"test/check@1", "accepted_media_types":["application/json"],
                    "permitted_claim_models":[{"model":"unquantified"}]
                }
            ],
            "capability_types": [{
                "capability_type":{"id":"test.check","major":1}, "owner":"test",
                "reproducibility":{"determinism":"deterministic"},
                "inputs":[{
                    "slot_id":"candidate", "role":{"id":"test.candidate","major":1},
                    "accepted_media_types":["application/json"]
                }],
                "outputs":[
                    {
                        "slot_id":"remaining", "role":{"id":"test.remaining","major":1},
                        "media_type":"application/json", "permitted_claim_models":[{"model":"exact"}]
                    },
                    {
                        "slot_id":"outcome", "role":{"id":"test.outcome","major":1},
                        "media_type":"application/json", "permitted_claim_models":[{"model":"unquantified"}]
                    }
                ]
            }]
        });
        write_json(&case.join("contract.json"), &contract);
        write_json(&case.join("registry.json"), &registry);

        let compiled = compile_documents(
            &fs::read(case.join("contract.json")).unwrap(),
            &fs::read(case.join("registry.json")).unwrap(),
        )
        .unwrap()
        .compiled
        .unwrap();
        let claims = json!({
            "schema_version":"avila.core/evidence-claims/v0.2-draft",
            "semantic_profile":"avila.core/semantic/0.2-draft",
            "compiled_snapshot_sha256":compiled.snapshot_sha256,
            "inputs":[{
                "input_id":"candidate",
                "artifact":{"sha256":result_sha,"media_type":"application/json"}
            }],
            "claims":[
                {
                    "claim_id":"remaining", "step_id":"check", "output_slot":"remaining",
                    "artifact":{"sha256":result_sha,"media_type":"application/json"},
                    "producer":{"package_id":"test/checker@1","sha256":checker_sha},
                    "claim":{"model":"exact","nominal":{"value":"2","unit":"1"}}
                },
                {
                    "claim_id":"outcome", "step_id":"check", "output_slot":"outcome",
                    "artifact":{"sha256":result_sha,"media_type":"application/json"},
                    "producer":{"package_id":"test/checker@1","sha256":checker_sha},
                    "claim":{"model":"unquantified","value":"rejected"}
                }
            ]
        });
        write_json(&case.join("claims.json"), &claims);

        let adapter = json!({
            "schema_version":EXTERNAL_CHECKER_ADAPTER_SCHEMA_VERSION,
            "adapter_id":"test/check@1",
            "capability_type":{"id":"test.check","major":1},
            "input_slots":["candidate"],
            "arguments":[
                {"kind":"input_path","input_slot":"candidate"},
                {"kind":"output_path","output_id":"report"}
            ],
            "outputs":[{
                "output_id":"report", "workspace_path":"outputs/report.json",
                "media_type":"application/json"
            }],
            "claims":[
                {"model":"exact","output_slot":"remaining","output_id":"report","pointer":"/remaining","unit":"1"},
                {"model":"categorical","output_slot":"outcome","output_id":"report","pointer":"/outcome","allowed_values":["clear","rejected"]}
            ],
            "timeout_ms":30000
        });
        write_json(&case.join("adapter.json"), &adapter);

        let package = json!({
            "schema_version":"avila.core/case-package/v0.1-draft",
            "case_id":"EXTERNAL-CHECKER-TEST", "title":"external checker test",
            "documents":[
                document(&case, "contract", "contract", "contract.json"),
                document(&case, "registry", "registry", "registry.json"),
                document(&case, "claims", "claims", "claims.json"),
                document(&case, "test/check@1", EXTERNAL_CHECKER_DOCUMENT_ROLE, "adapter.json")
            ],
            "artifacts":[
                {
                    "artifact_id":"candidate", "evidence_ids":["input:candidate"],
                    "source_root":"fixture", "path":"candidate.json", "sha256":result_sha
                },
                {
                    "artifact_id":"expected", "evidence_ids":["remaining","outcome"],
                    "source_root":"fixture", "path":"expected.json", "sha256":result_sha
                }
            ],
            "capabilities":[{
                "capability_id":"checker", "package_id":"test/checker@1",
                "executable_sha256":checker_sha
            }],
            "executions":[{
                "step_id":"check", "adapter":"test/check@1", "capability_id":"checker",
                "inputs":[{"input_slot":"candidate","workspace_path":"inputs/candidate.json"}],
                "outputs":[
                    {"output_slot":"remaining","claim_id":"remaining"},
                    {"output_slot":"outcome","claim_id":"outcome"}
                ]
            }]
        });
        write_json(&case.join("package.json"), &package);

        let log = temp.0.join("attempts.jsonl");
        let report = execute_case(
            &case,
            &CaseRunOptions {
                source_roots: BTreeMap::from([("fixture".into(), root)]),
                capabilities: BTreeMap::from([("checker".into(), checker)]),
                workspace: Some(temp.0.join("workspace")),
                reuse: false,
                plan_only: false,
                inputs: BTreeMap::new(),
                environment: BTreeMap::new(),
                log: Some(log.clone()),
                expected_manifest_sha256: None,
                attempt: None,
                hash_cache: None,
                trust_root: None,
                runner_key: None,
            },
        )
        .unwrap();
        assert_eq!(
            report.status,
            CaseRunStatus::Evaluated,
            "{}",
            human_summary(&report)
        );
        assert_eq!(
            report.execution.as_ref().unwrap().steps[0].state,
            StepExecutionState::Executed
        );
        assert_eq!(report.margins[0].status, VerdictStatus::Fail);
        assert!(human_summary(&report).contains("category outcome: rejected"));
        let generated: Value =
            serde_json::from_slice(&fs::read(temp.0.join("workspace/claims.json")).unwrap())
                .unwrap();
        assert_eq!(generated["claims"][1]["claim"]["value"], "rejected");
        let receipt: Value =
            serde_json::from_slice(&fs::read(temp.0.join("workspace/check/receipt.json")).unwrap())
                .unwrap();
        assert_eq!(
            receipt["invocation"]["adapter_sha256"],
            sha256_file(&case.join("adapter.json")).unwrap().0
        );
        let logged: Value = serde_json::from_str(fs::read_to_string(log).unwrap().trim()).unwrap();
        assert_eq!(logged["claims"][1]["claim"]["value"], "rejected");
    }

    #[cfg(unix)]
    fn write_json(path: &Path, value: &Value) {
        let mut bytes = serde_json::to_vec_pretty(value).unwrap();
        bytes.push(b'\n');
        fs::write(path, bytes).unwrap();
    }

    #[cfg(unix)]
    fn document(case: &Path, id: &str, role: &str, path: &str) -> Value {
        json!({
            "document_id":id, "role":role, "path":path,
            "sha256":sha256_file(&case.join(path)).unwrap().0
        })
    }
}
