//! Supplying a designer's free contract input for a run and validating it
//! against its role's declared schema before anything is staged (S-036).

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;

use avila_core_compiler::{
    CompiledContract, FindingClass, RegistrySnapshot, SourceLocation, SourceRef, locate,
    validate_against_schema,
};
use avila_core_evidence::{
    ArtifactCheck, IntegrityCheckState, PackageArtifact, VerifiedCasePackage, sha256_file,
};
use avila_core_kernel::{diagnose_authoritative_json, read_authoritative_json};

use super::{CORE_X1301, CaseRunOptions, RunFinding, RunStage, SuppliedInput};

/// Hash each supplied free input and let it stand for its contract input in
/// this run: the manifest's artifact and the integrity check for that
/// evidence record are replaced by the supplied bytes' identity.
pub(crate) fn supply_free_inputs(
    package: &mut VerifiedCasePackage,
    options: &CaseRunOptions,
) -> Result<Vec<SuppliedInput>, Box<dyn Error>> {
    let mut supplied = Vec::new();
    for (input_id, path) in &options.inputs {
        if !package.manifest.free_inputs.contains(input_id) {
            return Err(format!(
                "input `{input_id}` is not a free input of this package; free inputs: [{}]",
                package.manifest.free_inputs.join(", ")
            )
            .into());
        }
        let canonical = fs::canonicalize(path)
            .map_err(|error| format!("input `{input_id}` at `{}`: {error}", path.display()))?;
        let (sha256, _) = sha256_file(&canonical)?;
        let evidence_id = format!("input:{input_id}");
        let display = canonical.display().to_string();
        match package
            .manifest
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.evidence_ids.contains(&evidence_id))
        {
            Some(artifact) if artifact.evidence_ids.len() > 1 => {
                return Err(format!(
                    "input `{input_id}` is bound by artifact `{}` together with other evidence; it cannot be supplied separately",
                    artifact.artifact_id
                )
                .into());
            }
            Some(artifact) => {
                artifact.source_root = "supplied".into();
                artifact.path = display.clone();
                artifact.sha256 = sha256.clone();
            }
            None => package.manifest.artifacts.push(PackageArtifact {
                artifact_id: evidence_id.clone(),
                evidence_ids: vec![evidence_id.clone()],
                source_root: "supplied".into(),
                path: display.clone(),
                sha256: sha256.clone(),
            }),
        }
        match package
            .integrity
            .artifacts
            .iter_mut()
            .find(|check| check.evidence_ids.contains(&evidence_id))
        {
            Some(check) => {
                check.source_root = "supplied".into();
                check.path = display.clone();
                check.expected_sha256 = sha256.clone();
                check.actual_sha256 = Some(sha256.clone());
                check.state = IntegrityCheckState::Verified;
            }
            None => package.integrity.artifacts.push(ArtifactCheck {
                artifact_id: evidence_id.clone(),
                evidence_ids: vec![evidence_id.clone()],
                source_root: "supplied".into(),
                path: display.clone(),
                expected_sha256: sha256.clone(),
                actual_sha256: Some(sha256.clone()),
                state: IntegrityCheckState::Verified,
            }),
        }
        supplied.push(SuppliedInput {
            input_id: input_id.clone(),
            evidence_id,
            path: display,
            sha256,
        });
    }
    Ok(supplied)
}

/// Validates every supplied free input against the embedded JSON Schema its
/// role declares, before anything is staged or executed. A role with no
/// declared `input_schema` is unaffected. The shared compiler shape
/// validator produces the per-pointer findings; every one is reported here
/// under `CORE-X1301` so a designer's malformed candidate is refused before
/// it can surface as an adapter traceback or a silently defaulted fact.
pub(crate) fn validate_free_inputs(
    compiled: &CompiledContract,
    registry_bytes: &[u8],
    supplied: &[SuppliedInput],
) -> Result<Vec<RunFinding>, Box<dyn Error>> {
    if supplied.is_empty() {
        return Ok(Vec::new());
    }
    let registry: RegistrySnapshot = serde_json::from_slice(registry_bytes)?;
    let mut findings = Vec::new();
    for input in supplied {
        let Some(contract_input) = compiled
            .inputs
            .iter()
            .find(|candidate| candidate.input_id == input.input_id)
        else {
            continue;
        };
        let Some(role) = registry
            .roles
            .iter()
            .find(|role| role.role == contract_input.role)
        else {
            continue;
        };
        let Some(schema) = &role.input_schema else {
            continue;
        };
        let bytes = fs::read(&input.path).map_err(|error| {
            format!(
                "free input `{}` at `{}`: {error}",
                input.input_id, input.path
            )
        })?;
        let document = format!("input:{}", input.input_id);
        match read_authoritative_json(&bytes) {
            Ok(value) => {
                let mut diagnostics = Vec::new();
                validate_against_schema(schema, &document, "requester", &value, &mut diagnostics);
                for diagnostic in diagnostics {
                    let message =
                        locate_message(&bytes, &diagnostic.primary.pointer, &diagnostic.message);
                    findings.push(RunFinding::runtime(
                        CORE_X1301,
                        diagnostic.class,
                        RunStage::FreeInputValidation,
                        diagnostic.owner,
                        diagnostic.primary,
                        message,
                    ));
                }
            }
            Err(_) => {
                let (_, refusals) = diagnose_authoritative_json(&bytes);
                for refusal in refusals {
                    let pointer = refusal.pointer().unwrap_or_default().to_owned();
                    let message = locate_message(&bytes, &pointer, refusal.detail());
                    findings.push(RunFinding::runtime(
                        CORE_X1301,
                        FindingClass::Invalid,
                        RunStage::FreeInputValidation,
                        "requester",
                        SourceLocation::new(document.clone(), pointer),
                        message,
                    ));
                }
            }
        }
    }
    Ok(findings)
}

/// Appends the located `document:line:column` of `pointer` in `source` to a
/// finding message, when the bytes are locatable JSON. Presentation only:
/// consumers keep matching the code and pointer, never this text.
fn locate_message(source: &[u8], pointer: &str, message: &str) -> String {
    match locate(source, pointer) {
        Some(span) => format!("{message} (line {}, column {})", span.line, span.column),
        None => message.to_owned(),
    }
}

/// Every step a supplied input reaches through the compiled bindings, so
/// their committed claims are not carried and their receipts not replayed.
pub(crate) fn steps_reached_by_inputs(
    compiled: &CompiledContract,
    supplied: &[SuppliedInput],
) -> BTreeSet<String> {
    let inputs: BTreeSet<&str> = supplied
        .iter()
        .map(|input| input.input_id.as_str())
        .collect();
    let mut reached = BTreeSet::new();
    for step in &compiled.workflow {
        let hit = step.bindings.iter().any(|binding| match &binding.source {
            SourceRef::ContractInput { input_id } => inputs.contains(input_id.as_str()),
            SourceRef::StepOutput { step_id, .. } => reached.contains(step_id),
        });
        if hit {
            reached.insert(step.step_id.clone());
        }
    }
    reached
}
