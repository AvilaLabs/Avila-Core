//! A deliberately small offline integrity slice for composed case packages.
//!
//! The verifier proves byte identity for package documents and for explicitly
//! resolved external artifacts. It does not establish that those bytes are
//! scientifically correct, qualified, signed, or complete evidence.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::sha256_hex;

pub const CASE_PACKAGE_SCHEMA_VERSION: &str = "avila.core/case-package/v0.1-draft";
pub const PACKAGE_INTEGRITY_REPORT_SCHEMA_VERSION: &str =
    "avila.core/package-integrity-report/v0.1-draft";

const INTEGRITY_NOTICE: &str = "Package integrity checks byte identity only. It does not verify schemas, media semantics, execution receipts, signatures, qualification, practical suitability, or scientific correctness.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CasePackageManifest {
    pub schema_version: String,
    pub case_id: String,
    pub title: String,
    pub documents: Vec<PackageDocument>,
    #[serde(default)]
    pub artifacts: Vec<PackageArtifact>,
    /// Exact implementations the package binds for execution, identified by
    /// the digest of their executable bytes.
    #[serde(default)]
    pub capabilities: Vec<PackageCapability>,
    /// Steps the case runner executes rather than replays, with the staging
    /// layout and the evidence identifiers their outputs receive.
    #[serde(default)]
    pub executions: Vec<PackageExecution>,
    /// Contract inputs a run may supply from outside the package. A supplied
    /// value is hashed and attested for this run; the committed expectations
    /// then describe a different candidate and are not replayed.
    #[serde(default)]
    pub free_inputs: Vec<String>,
    /// Coverage of the contract against a library requirement set held as a
    /// `requirement_set` document, with the case's mapping and omissions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<PackageCoverage>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCoverage {
    /// The `document_id` of the package's `requirement_set` document.
    pub requirement_set: String,
    /// Set requirement id to the contract requirement ids that cover it.
    #[serde(default)]
    pub mapping: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub omissions: Vec<PackageOmission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageOmission {
    pub set_requirement_id: String,
    pub reason: String,
    pub accepted_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageDocument {
    pub document_id: String,
    pub role: String,
    pub path: String,
    pub sha256: String,
    /// The executed step an `execution_receipt` document records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageArtifact {
    pub artifact_id: String,
    pub evidence_ids: Vec<String>,
    pub source_root: String,
    pub path: String,
    pub sha256: String,
}

/// An exact implementation the package binds. The executable digest is the
/// identity; the package name and source coordinates are annotations that
/// help a reader locate it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCapability {
    pub capability_id: String,
    pub package_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    pub executable_sha256: String,
}

/// One step the runner executes with a named case-specific adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageExecution {
    pub step_id: String,
    pub adapter: String,
    pub capability_id: String,
    /// Where each bound input slot is staged inside the step workspace.
    pub inputs: Vec<ExecutionInputStaging>,
    /// The evidence identifier each produced output slot's claim receives.
    pub outputs: Vec<ExecutionOutputBinding>,
    /// Environment keys the adapter requires and the operator must value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionInputStaging {
    pub input_slot: String,
    pub workspace_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionOutputBinding {
    pub output_slot: String,
    pub claim_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageIntegrityStatus {
    Complete,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityCheckState {
    Verified,
    NotChecked,
    Missing,
    Mismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentCheck {
    pub document_id: String,
    pub role: String,
    pub path: String,
    pub expected_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_sha256: Option<String>,
    pub state: IntegrityCheckState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCheck {
    pub artifact_id: String,
    pub evidence_ids: Vec<String>,
    pub source_root: String,
    pub path: String,
    pub expected_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_sha256: Option<String>,
    pub state: IntegrityCheckState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIntegrityReport {
    pub schema_version: String,
    pub case_id: String,
    pub status: PackageIntegrityStatus,
    pub manifest_sha256: String,
    pub documents: Vec<DocumentCheck>,
    pub artifacts: Vec<ArtifactCheck>,
    pub limitations: Vec<String>,
    pub notice: String,
}

/// A checked manifest plus the exact package-document bytes that were checked.
/// External artifact bytes are deliberately not retained in memory after their
/// digests have been computed.
#[derive(Debug)]
pub struct VerifiedCasePackage {
    pub manifest: CasePackageManifest,
    pub integrity: PackageIntegrityReport,
    document_bytes: BTreeMap<String, Vec<u8>>,
}

impl VerifiedCasePackage {
    pub fn document_by_id(&self, document_id: &str) -> Option<&[u8]> {
        self.document_bytes.get(document_id).map(Vec::as_slice)
    }

    pub fn document_by_role(&self, role: &str) -> Option<&[u8]> {
        let document = self
            .manifest
            .documents
            .iter()
            .find(|document| document.role == role)?;
        self.document_by_id(&document.document_id)
    }
}

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("case package manifest is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("invalid case package manifest: {0}")]
    InvalidManifest(String),
    #[error("`{path}` escapes its declared root `{root}`")]
    EscapesRoot { path: String, root: String },
    #[error("cannot read `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
}

/// Verify all package documents and every external artifact whose named root
/// is supplied. An omitted external root is reported as `not_checked`; a root
/// that is supplied but contains a missing or different file fails integrity.
pub fn verify_case_package(
    manifest_bytes: &[u8],
    package_root: &Path,
    source_roots: &BTreeMap<String, PathBuf>,
) -> Result<VerifiedCasePackage, PackageError> {
    let manifest: CasePackageManifest = serde_json::from_slice(manifest_bytes)?;
    validate_manifest(&manifest)?;

    let requested_roots: BTreeSet<&str> = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.source_root.as_str())
        .collect();
    for supplied in source_roots.keys() {
        if !requested_roots.contains(supplied.as_str()) {
            return Err(PackageError::InvalidManifest(format!(
                "source root `{supplied}` was supplied but is not requested by the manifest"
            )));
        }
    }

    let package_root = canonical_directory(package_root)?;
    let mut canonical_source_roots = BTreeMap::new();
    for (name, path) in source_roots {
        canonical_source_roots.insert(name.clone(), canonical_directory(path)?);
    }

    let mut document_bytes = BTreeMap::new();
    let mut documents = Vec::with_capacity(manifest.documents.len());
    for document in &manifest.documents {
        let bytes = read_confined(&package_root, &document.path)?;
        let (actual_sha256, state) = check_bytes(bytes.as_deref(), &document.sha256);
        if let Some(bytes) = bytes {
            document_bytes.insert(document.document_id.clone(), bytes);
        }
        documents.push(DocumentCheck {
            document_id: document.document_id.clone(),
            role: document.role.clone(),
            path: document.path.clone(),
            expected_sha256: document.sha256.clone(),
            actual_sha256,
            state,
        });
    }

    // Artifacts under supplied roots are hashed in parallel: the bound data
    // releases run to hundreds of megabytes, and every run re-hashes them.
    let checks: Vec<Result<(Option<String>, IntegrityCheckState), PackageError>> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = manifest
                .artifacts
                .iter()
                .map(|artifact| {
                    let root = canonical_source_roots.get(&artifact.source_root);
                    scope.spawn(move || match root {
                        Some(root) => check_file(root, &artifact.path, &artifact.sha256),
                        None => Ok((None, IntegrityCheckState::NotChecked)),
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(PackageError::InvalidManifest(
                            "an artifact hashing thread panicked".into(),
                        ))
                    })
                })
                .collect()
        });
    let mut artifacts = Vec::with_capacity(manifest.artifacts.len());
    for (artifact, check) in manifest.artifacts.iter().zip(checks) {
        let (actual_sha256, state) = check?;
        artifacts.push(ArtifactCheck {
            artifact_id: artifact.artifact_id.clone(),
            evidence_ids: artifact.evidence_ids.clone(),
            source_root: artifact.source_root.clone(),
            path: artifact.path.clone(),
            expected_sha256: artifact.sha256.clone(),
            actual_sha256,
            state,
        });
    }

    let failed = documents
        .iter()
        .any(|check| check.state != IntegrityCheckState::Verified)
        || artifacts.iter().any(|check| {
            matches!(
                check.state,
                IntegrityCheckState::Missing | IntegrityCheckState::Mismatch
            )
        });
    let partial = artifacts
        .iter()
        .any(|check| check.state == IntegrityCheckState::NotChecked);
    let status = if failed {
        PackageIntegrityStatus::Failed
    } else if partial {
        PackageIntegrityStatus::Partial
    } else {
        PackageIntegrityStatus::Complete
    };

    let integrity = PackageIntegrityReport {
        schema_version: PACKAGE_INTEGRITY_REPORT_SCHEMA_VERSION.into(),
        case_id: manifest.case_id.clone(),
        status,
        manifest_sha256: digest(manifest_bytes),
        documents,
        artifacts,
        limitations: manifest.limitations.clone(),
        notice: INTEGRITY_NOTICE.into(),
    };
    Ok(VerifiedCasePackage {
        manifest,
        integrity,
        document_bytes,
    })
}

fn validate_manifest(manifest: &CasePackageManifest) -> Result<(), PackageError> {
    if manifest.schema_version != CASE_PACKAGE_SCHEMA_VERSION {
        return Err(PackageError::InvalidManifest(format!(
            "unsupported schema version `{}`; expected `{CASE_PACKAGE_SCHEMA_VERSION}`",
            manifest.schema_version
        )));
    }
    require_nonempty("case_id", &manifest.case_id)?;
    require_nonempty("title", &manifest.title)?;

    let mut document_ids = BTreeSet::new();
    let mut document_paths = BTreeSet::new();
    let mut role_counts = BTreeMap::<&str, usize>::new();
    for document in &manifest.documents {
        require_nonempty("document_id", &document.document_id)?;
        require_nonempty("document role", &document.role)?;
        validate_relative_path(&document.path)?;
        validate_digest(&document.sha256)?;
        if !document_ids.insert(document.document_id.as_str()) {
            return Err(PackageError::InvalidManifest(format!(
                "duplicate document_id `{}`",
                document.document_id
            )));
        }
        if !document_paths.insert(document.path.as_str()) {
            return Err(PackageError::InvalidManifest(format!(
                "duplicate package document path `{}`",
                document.path
            )));
        }
        *role_counts.entry(document.role.as_str()).or_default() += 1;
    }
    for required in ["contract", "registry", "claims"] {
        if role_counts.get(required) != Some(&1) {
            return Err(PackageError::InvalidManifest(format!(
                "manifest must contain exactly one `{required}` document"
            )));
        }
    }
    if role_counts
        .get("expected_campaign_report")
        .copied()
        .unwrap_or(0)
        > 1
    {
        return Err(PackageError::InvalidManifest(
            "manifest may contain at most one `expected_campaign_report` document".into(),
        ));
    }
    if let Some(coverage) = &manifest.coverage {
        require_nonempty("coverage.requirement_set", &coverage.requirement_set)?;
        let names_set_document = manifest.documents.iter().any(|document| {
            document.document_id == coverage.requirement_set && document.role == "requirement_set"
        });
        if !names_set_document {
            return Err(PackageError::InvalidManifest(format!(
                "coverage names document `{}`, which is not a `requirement_set` document of this package",
                coverage.requirement_set
            )));
        }
        for (set_requirement_id, requirement_ids) in &coverage.mapping {
            require_nonempty("coverage mapping key", set_requirement_id)?;
            for requirement_id in requirement_ids {
                require_nonempty("coverage mapping requirement_id", requirement_id)?;
            }
        }
        for omission in &coverage.omissions {
            require_nonempty("omission set_requirement_id", &omission.set_requirement_id)?;
            require_nonempty("omission reason", &omission.reason)?;
            require_nonempty("omission accepted_by", &omission.accepted_by)?;
        }
    } else if role_counts.contains_key("requirement_set") {
        return Err(PackageError::InvalidManifest(
            "a `requirement_set` document is present but the manifest declares no `coverage`"
                .into(),
        ));
    }

    let mut artifact_ids = BTreeSet::new();
    let mut evidence_ids = BTreeSet::new();
    for artifact in &manifest.artifacts {
        require_nonempty("artifact_id", &artifact.artifact_id)?;
        validate_root_name(&artifact.source_root)?;
        validate_relative_path(&artifact.path)?;
        validate_digest(&artifact.sha256)?;
        if artifact.evidence_ids.is_empty() {
            return Err(PackageError::InvalidManifest(format!(
                "artifact `{}` must bind at least one evidence_id",
                artifact.artifact_id
            )));
        }
        if !artifact_ids.insert(artifact.artifact_id.as_str()) {
            return Err(PackageError::InvalidManifest(format!(
                "duplicate artifact_id `{}`",
                artifact.artifact_id
            )));
        }
        for evidence_id in &artifact.evidence_ids {
            require_nonempty("evidence_id", evidence_id)?;
            if !evidence_ids.insert(evidence_id.as_str()) {
                return Err(PackageError::InvalidManifest(format!(
                    "evidence_id `{evidence_id}` is bound by more than one artifact"
                )));
            }
        }
    }

    let mut capability_ids = BTreeSet::new();
    for capability in &manifest.capabilities {
        require_nonempty("capability_id", &capability.capability_id)?;
        require_nonempty("package_id", &capability.package_id)?;
        validate_digest(&capability.executable_sha256)?;
        if !capability_ids.insert(capability.capability_id.as_str()) {
            return Err(PackageError::InvalidManifest(format!(
                "duplicate capability_id `{}`",
                capability.capability_id
            )));
        }
    }

    let mut execution_steps = BTreeSet::new();
    let mut execution_claims = BTreeSet::new();
    for execution in &manifest.executions {
        require_nonempty("execution step_id", &execution.step_id)?;
        require_nonempty("adapter", &execution.adapter)?;
        if !execution_steps.insert(execution.step_id.as_str()) {
            return Err(PackageError::InvalidManifest(format!(
                "step `{}` is declared for execution more than once",
                execution.step_id
            )));
        }
        if !capability_ids.contains(execution.capability_id.as_str()) {
            return Err(PackageError::InvalidManifest(format!(
                "execution of `{}` names capability `{}`, which the package does not declare",
                execution.step_id, execution.capability_id
            )));
        }
        let mut slots = BTreeSet::new();
        let mut staged_paths = BTreeSet::new();
        for input in &execution.inputs {
            require_nonempty("input_slot", &input.input_slot)?;
            validate_relative_path(&input.workspace_path)?;
            if !slots.insert(input.input_slot.as_str()) {
                return Err(PackageError::InvalidManifest(format!(
                    "execution of `{}` stages input slot `{}` twice",
                    execution.step_id, input.input_slot
                )));
            }
            if !staged_paths.insert(input.workspace_path.as_str()) {
                return Err(PackageError::InvalidManifest(format!(
                    "execution of `{}` stages two inputs at `{}`",
                    execution.step_id, input.workspace_path
                )));
            }
        }
        if execution.outputs.is_empty() {
            return Err(PackageError::InvalidManifest(format!(
                "execution of `{}` binds no output claims",
                execution.step_id
            )));
        }
        let mut output_slots = BTreeSet::new();
        for output in &execution.outputs {
            require_nonempty("output_slot", &output.output_slot)?;
            require_nonempty("claim_id", &output.claim_id)?;
            if !output_slots.insert(output.output_slot.as_str()) {
                return Err(PackageError::InvalidManifest(format!(
                    "execution of `{}` binds output slot `{}` twice",
                    execution.step_id, output.output_slot
                )));
            }
            if !execution_claims.insert(output.claim_id.as_str()) {
                return Err(PackageError::InvalidManifest(format!(
                    "claim `{}` is produced by more than one execution output",
                    output.claim_id
                )));
            }
            if !evidence_ids.contains(output.claim_id.as_str()) {
                return Err(PackageError::InvalidManifest(format!(
                    "execution output claim `{}` is not bound to any package artifact; a fresh output must bind to a declared identity",
                    output.claim_id
                )));
            }
        }
    }

    let mut receipt_steps = BTreeSet::new();
    for document in &manifest.documents {
        match (document.role.as_str(), &document.step_id) {
            ("execution_receipt", Some(step_id)) => {
                if !execution_steps.contains(step_id.as_str()) {
                    return Err(PackageError::InvalidManifest(format!(
                        "receipt document `{}` names step `{step_id}`, which is not declared for execution",
                        document.document_id
                    )));
                }
                if !receipt_steps.insert(step_id.as_str()) {
                    return Err(PackageError::InvalidManifest(format!(
                        "step `{step_id}` has more than one execution_receipt document"
                    )));
                }
            }
            ("execution_receipt", None) => {
                return Err(PackageError::InvalidManifest(format!(
                    "receipt document `{}` must name its step_id",
                    document.document_id
                )));
            }
            (_, Some(step_id)) => {
                return Err(PackageError::InvalidManifest(format!(
                    "document `{}` names step `{step_id}` but is not an execution_receipt",
                    document.document_id
                )));
            }
            (_, None) => {}
        }
    }
    Ok(())
}

fn require_nonempty(field: &str, value: &str) -> Result<(), PackageError> {
    if value.trim().is_empty() {
        return Err(PackageError::InvalidManifest(format!(
            "`{field}` must not be empty"
        )));
    }
    Ok(())
}

fn validate_root_name(value: &str) -> Result<(), PackageError> {
    require_nonempty("source_root", value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(PackageError::InvalidManifest(format!(
            "source root `{value}` contains unsupported characters"
        )));
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<PathBuf, PackageError> {
    require_nonempty("path", value)?;
    let path = Path::new(value);
    if !path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(PackageError::InvalidManifest(format!(
            "path `{value}` must be a normalized relative path"
        )));
    }
    Ok(path.to_path_buf())
}

fn validate_digest(value: &str) -> Result<(), PackageError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(PackageError::InvalidManifest(format!(
            "digest `{value}` must use the `sha256:` prefix"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(PackageError::InvalidManifest(format!(
            "digest `{value}` must contain 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, PackageError> {
    let canonical = fs::canonicalize(path).map_err(|source| PackageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let metadata = fs::metadata(&canonical).map_err(|source| PackageError::Io {
        path: canonical.display().to_string(),
        source,
    })?;
    if !metadata.is_dir() {
        return Err(PackageError::InvalidManifest(format!(
            "root `{}` is not a directory",
            path.display()
        )));
    }
    Ok(canonical)
}

fn read_confined(root: &Path, relative: &str) -> Result<Option<Vec<u8>>, PackageError> {
    let Some(canonical) = resolve_confined(root, relative)? else {
        return Ok(None);
    };
    fs::read(&canonical)
        .map(Some)
        .map_err(|source| PackageError::Io {
            path: canonical.display().to_string(),
            source,
        })
}

fn resolve_confined(root: &Path, relative: &str) -> Result<Option<PathBuf>, PackageError> {
    let relative = validate_relative_path(relative)?;
    let candidate = root.join(relative);
    match fs::symlink_metadata(&candidate) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(PackageError::Io {
                path: candidate.display().to_string(),
                source,
            });
        }
    }
    let canonical = fs::canonicalize(&candidate).map_err(|source| PackageError::Io {
        path: candidate.display().to_string(),
        source,
    })?;
    if !canonical.starts_with(root) {
        return Err(PackageError::EscapesRoot {
            path: candidate.display().to_string(),
            root: root.display().to_string(),
        });
    }
    let metadata = fs::metadata(&canonical).map_err(|source| PackageError::Io {
        path: canonical.display().to_string(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(PackageError::InvalidManifest(format!(
            "package entry `{}` is not a regular file",
            candidate.display()
        )));
    }
    Ok(Some(canonical))
}

/// Hash a regular file confined beneath `root` in fixed-size chunks. Returns
/// the prefixed digest and byte length, or `None` when the file is absent.
pub(crate) fn hash_confined_file(
    root: &Path,
    relative: &str,
) -> Result<Option<(String, u64)>, PackageError> {
    let Some(path) = resolve_confined(root, relative)? else {
        return Ok(None);
    };
    let mut file = fs::File::open(&path).map_err(|source| PackageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut length = 0_u64;
    loop {
        let count = file.read(&mut buffer).map_err(|source| PackageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if count == 0 {
            break;
        }
        length += count as u64;
        hasher.update(&buffer[..count]);
    }
    Ok(Some((format!("sha256:{:x}", hasher.finalize()), length)))
}

fn check_file(
    root: &Path,
    relative: &str,
    expected: &str,
) -> Result<(Option<String>, IntegrityCheckState), PackageError> {
    let Some((actual, _)) = hash_confined_file(root, relative)? else {
        return Ok((None, IntegrityCheckState::Missing));
    };
    let state = if actual == expected {
        IntegrityCheckState::Verified
    } else {
        IntegrityCheckState::Mismatch
    };
    Ok((Some(actual), state))
}

fn check_bytes(bytes: Option<&[u8]>, expected: &str) -> (Option<String>, IntegrityCheckState) {
    let Some(bytes) = bytes else {
        return (None, IntegrityCheckState::Missing);
    };
    let actual = digest(bytes);
    let state = if actual == expected {
        IntegrityCheckState::Verified
    } else {
        IntegrityCheckState::Mismatch
    };
    (Some(actual), state)
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "avila-core-package-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture(root: &Path) -> Vec<u8> {
        for name in ["contract.json", "registry.json", "claims.json"] {
            fs::write(root.join(name), name.as_bytes()).unwrap();
        }
        fs::create_dir_all(root.join("source")).unwrap();
        fs::write(root.join("source/artifact.bin"), b"artifact").unwrap();
        serde_json::to_vec(&CasePackageManifest {
            schema_version: CASE_PACKAGE_SCHEMA_VERSION.into(),
            case_id: "CASE-TEST".into(),
            title: "package verifier fixture".into(),
            documents: ["contract", "registry", "claims"]
                .into_iter()
                .map(|role| PackageDocument {
                    document_id: role.into(),
                    role: role.into(),
                    path: format!("{role}.json"),
                    sha256: digest(format!("{role}.json").as_bytes()),
                    step_id: None,
                })
                .collect(),
            artifacts: vec![PackageArtifact {
                artifact_id: "artifact".into(),
                evidence_ids: vec!["input:artifact".into()],
                source_root: "source".into(),
                path: "artifact.bin".into(),
                sha256: digest(b"artifact"),
            }],
            capabilities: Vec::new(),
            executions: Vec::new(),
            free_inputs: Vec::new(),
            coverage: None,
            limitations: vec!["fixture only".into()],
        })
        .unwrap()
    }

    #[test]
    fn omitted_roots_are_partial_and_supplied_roots_are_rehashed() {
        let root = TestDir::new();
        let manifest = fixture(&root.0);

        let partial = verify_case_package(&manifest, &root.0, &BTreeMap::new()).unwrap();
        assert_eq!(partial.integrity.status, PackageIntegrityStatus::Partial);
        assert_eq!(
            partial.integrity.artifacts[0].state,
            IntegrityCheckState::NotChecked
        );

        let sources = BTreeMap::from([("source".into(), root.0.join("source"))]);
        let complete = verify_case_package(&manifest, &root.0, &sources).unwrap();
        assert_eq!(complete.integrity.status, PackageIntegrityStatus::Complete);
        assert_eq!(
            complete.integrity.artifacts[0].state,
            IntegrityCheckState::Verified
        );

        fs::write(root.0.join("source/artifact.bin"), b"different").unwrap();
        let failed = verify_case_package(&manifest, &root.0, &sources).unwrap();
        assert_eq!(failed.integrity.status, PackageIntegrityStatus::Failed);
        assert_eq!(
            failed.integrity.artifacts[0].state,
            IntegrityCheckState::Mismatch
        );
    }

    #[test]
    fn executions_must_name_declared_capabilities_and_bound_claims() {
        let root = TestDir::new();
        let mut manifest: CasePackageManifest = serde_json::from_slice(&fixture(&root.0)).unwrap();
        manifest.executions.push(PackageExecution {
            step_id: "step".into(),
            adapter: "test/adapter@1".into(),
            capability_id: "stub".into(),
            inputs: vec![ExecutionInputStaging {
                input_slot: "a".into(),
                workspace_path: "inputs/a.bin".into(),
            }],
            outputs: vec![ExecutionOutputBinding {
                output_slot: "result".into(),
                claim_id: "step-result".into(),
            }],
            environment: Vec::new(),
        });
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let error = verify_case_package(&bytes, &root.0, &BTreeMap::new()).unwrap_err();
        assert!(error.to_string().contains("does not declare"), "{error}");

        manifest.capabilities.push(PackageCapability {
            capability_id: "stub".into(),
            package_id: "test/stub@1".into(),
            source_repository: None,
            source_commit: None,
            executable_sha256: digest(b"stub"),
        });
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let error = verify_case_package(&bytes, &root.0, &BTreeMap::new()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("not bound to any package artifact"),
            "{error}"
        );

        manifest.artifacts[0]
            .evidence_ids
            .push("step-result".into());
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let package = verify_case_package(&bytes, &root.0, &BTreeMap::new()).unwrap();
        assert_eq!(package.manifest.executions.len(), 1);

        manifest.documents.push(PackageDocument {
            document_id: "receipt".into(),
            role: "execution_receipt".into(),
            path: "receipt.json".into(),
            sha256: digest(b"receipt"),
            step_id: Some("other".into()),
        });
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let error = verify_case_package(&bytes, &root.0, &BTreeMap::new()).unwrap_err();
        assert!(
            error.to_string().contains("not declared for execution"),
            "{error}"
        );
    }

    #[test]
    fn parent_paths_are_rejected_before_reading() {
        let root = TestDir::new();
        let mut manifest: CasePackageManifest = serde_json::from_slice(&fixture(&root.0)).unwrap();
        manifest.documents[0].path = "../contract.json".into();
        let bytes = serde_json::to_vec(&manifest).unwrap();
        assert!(matches!(
            verify_case_package(&bytes, &root.0, &BTreeMap::new()),
            Err(PackageError::InvalidManifest(_))
        ));
    }
}
