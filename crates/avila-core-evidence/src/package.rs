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

const INTEGRITY_NOTICE: &str = "Package integrity checks byte identity only. It does not verify schemas, media semantics, execution receipts, signatures, qualification, professional review, or scientific correctness.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CasePackageManifest {
    pub schema_version: String,
    pub case_id: String,
    pub title: String,
    pub documents: Vec<PackageDocument>,
    #[serde(default)]
    pub artifacts: Vec<PackageArtifact>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageDocument {
    pub document_id: String,
    pub role: String,
    pub path: String,
    pub sha256: String,
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

    let mut artifacts = Vec::with_capacity(manifest.artifacts.len());
    for artifact in &manifest.artifacts {
        let Some(root) = canonical_source_roots.get(&artifact.source_root) else {
            artifacts.push(ArtifactCheck {
                artifact_id: artifact.artifact_id.clone(),
                evidence_ids: artifact.evidence_ids.clone(),
                source_root: artifact.source_root.clone(),
                path: artifact.path.clone(),
                expected_sha256: artifact.sha256.clone(),
                actual_sha256: None,
                state: IntegrityCheckState::NotChecked,
            });
            continue;
        };
        let (actual_sha256, state) = check_file(root, &artifact.path, &artifact.sha256)?;
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

fn check_file(
    root: &Path,
    relative: &str,
    expected: &str,
) -> Result<(Option<String>, IntegrityCheckState), PackageError> {
    let Some(path) = resolve_confined(root, relative)? else {
        return Ok((None, IntegrityCheckState::Missing));
    };
    let mut file = fs::File::open(&path).map_err(|source| PackageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|source| PackageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual = format!("sha256:{:x}", hasher.finalize());
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
                })
                .collect(),
            artifacts: vec![PackageArtifact {
                artifact_id: "artifact".into(),
                evidence_ids: vec!["input:artifact".into()],
                source_root: "source".into(),
                path: "artifact.bin".into(),
                sha256: digest(b"artifact"),
            }],
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
