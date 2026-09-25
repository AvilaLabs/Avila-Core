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

use crate::hash_cache::{FileStamp, HashCache};
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

/// One step the runner executes with a named built-in or package-declared adapter.
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
    /// The digest was not recomputed from bytes: an operator-supplied
    /// `--hash-cache` had a matching path, size, mtime, and (where the
    /// platform exposes them) device and inode for this artifact, and the
    /// recorded digest is used instead. Still compared to the manifest's
    /// bound identity exactly as `Verified` would be. See
    /// `crate::hash_cache` for the trust this state accepts.
    VerifiedCached,
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

/// A checked manifest plus the exact package-document bytes that were
/// checked. External artifact bytes are deliberately not retained in memory
/// after their digests have been computed.
///
/// Construction is sealed: only `verify_case_package` mints one, and only
/// `supply_free_input` may mutate it — the association between manifest,
/// checked bytes, and integrity observations cannot be edited from outside
/// (ADR-0026).
///
/// ```compile_fail
/// // A verified package cannot be fabricated downstream:
/// let package = avila_core_evidence::VerifiedCasePackage::default();
/// ```
#[derive(Debug)]
pub struct VerifiedCasePackage {
    manifest: CasePackageManifest,
    integrity: PackageIntegrityReport,
    document_bytes: BTreeMap<String, Vec<u8>>,
}

/// What one verified byte stands for when a caller supplies a free input
/// (S-036): the input it occupies, the evidence identity it receives, and
/// the digest the bytes were measured to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedFreeInput {
    pub input_id: String,
    pub evidence_id: String,
    pub path: String,
    pub sha256: String,
}

/// The closed outcome of package verification. Only `Verified` and
/// `PartiallyVerified` mint a `VerifiedCasePackage`; a `Refused` outcome
/// yields the integrity report and no package, so failed bytes cannot be
/// passed off as checked material downstream (ADR-0026).
#[derive(Debug)]
pub enum PackageVerification {
    /// Integrity `complete`: every document and every requested artifact
    /// under a supplied root verified.
    Verified(VerifiedCasePackage),
    /// Integrity `partial`: every supplied check verified, but at least one
    /// artifact's source root was not supplied, so it stands `not_checked`.
    /// The package bytes are trustworthy; the absent observation is
    /// explicit in the integrity report.
    PartiallyVerified(VerifiedCasePackage),
    /// Integrity `failed`: a document, or an artifact under a supplied
    /// root, did not match. No package is minted.
    Refused(PackageIntegrityReport),
}

impl PackageVerification {
    /// The package, when one was minted.
    pub fn package(&self) -> Option<&VerifiedCasePackage> {
        match self {
            Self::Verified(package) | Self::PartiallyVerified(package) => Some(package),
            Self::Refused(_) => None,
        }
    }

    /// Consume the outcome into the package, when one was minted.
    pub fn into_package(self) -> Option<VerifiedCasePackage> {
        match self {
            Self::Verified(package) | Self::PartiallyVerified(package) => Some(package),
            Self::Refused(_) => None,
        }
    }

    /// The integrity report on any outcome.
    pub fn integrity_report(&self) -> &PackageIntegrityReport {
        match self {
            Self::Verified(package) | Self::PartiallyVerified(package) => package.integrity(),
            Self::Refused(report) => report,
        }
    }
}

impl VerifiedCasePackage {
    pub(crate) fn new(
        manifest: CasePackageManifest,
        integrity: PackageIntegrityReport,
        document_bytes: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        Self {
            manifest,
            integrity,
            document_bytes,
        }
    }

    /// The checked manifest.
    pub fn manifest(&self) -> &CasePackageManifest {
        &self.manifest
    }

    /// The integrity observations recorded for this package.
    pub fn integrity(&self) -> &PackageIntegrityReport {
        &self.integrity
    }

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

    /// Hash `path` and let it stand for `input_id` in this package: the
    /// manifest's artifact and the integrity record for that evidence id
    /// are replaced by the supplied bytes' measured identity (S-036). Only
    /// a manifest-declared free input, bound to exactly one evidence id,
    /// can be supplied; a multi-evidence artifact cannot be partially
    /// replaced. This is the only mutation a verified package accepts.
    pub fn supply_free_input(
        &mut self,
        input_id: &str,
        path: &Path,
    ) -> Result<SuppliedFreeInput, PackageError> {
        if !self
            .manifest
            .free_inputs
            .iter()
            .any(|free| free == input_id)
        {
            return Err(PackageError::InvalidManifest(format!(
                "input `{input_id}` is not a free input of this package; free inputs: [{}]",
                self.manifest.free_inputs.join(", ")
            )));
        }
        let canonical = path.canonicalize().map_err(|source| PackageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let (sha256, _) = crate::sha256_file(&canonical).map_err(|source| PackageError::Io {
            path: canonical.display().to_string(),
            source,
        })?;
        let evidence_id = format!("input:{input_id}");
        let display = canonical.display().to_string();
        match self
            .manifest
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.evidence_ids.contains(&evidence_id))
        {
            Some(artifact) if artifact.evidence_ids.len() > 1 => {
                return Err(PackageError::InvalidManifest(format!(
                    "input `{input_id}` is bound by artifact `{}` together with other evidence; it cannot be supplied separately",
                    artifact.artifact_id
                )));
            }
            Some(artifact) => {
                artifact.source_root = "supplied".into();
                artifact.path = display.clone();
                artifact.sha256 = sha256.clone();
            }
            None => self.manifest.artifacts.push(PackageArtifact {
                artifact_id: evidence_id.clone(),
                evidence_ids: vec![evidence_id.clone()],
                source_root: "supplied".into(),
                path: display.clone(),
                sha256: sha256.clone(),
            }),
        }
        match self
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
            None => self.integrity.artifacts.push(ArtifactCheck {
                artifact_id: evidence_id.clone(),
                evidence_ids: vec![evidence_id.clone()],
                source_root: "supplied".into(),
                path: display.clone(),
                expected_sha256: sha256.clone(),
                actual_sha256: Some(sha256.clone()),
                state: IntegrityCheckState::Verified,
            }),
        }
        Ok(SuppliedFreeInput {
            input_id: input_id.into(),
            evidence_id,
            path: display,
            sha256,
        })
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

/// An operator-supplied verified-hash cache and the timestamp to record
/// against any entry this call adds or refreshes. Passing `None` re-hashes
/// every supplied artifact from bytes, exactly as before this cache existed.
pub struct HashCacheContext<'a> {
    pub cache: &'a mut HashCache,
    pub verified_at: &'a str,
}

/// Verify all package documents and every external artifact whose named root
/// is supplied. An omitted external root is reported as `not_checked`; a root
/// that is supplied but contains a missing or different file fails integrity.
///
/// `hash_cache` is consulted only for artifacts resolved under a supplied
/// `--source-root`; package documents and any artifact that resolves inside
/// `package_root` itself are always re-hashed from bytes. See
/// `crate::hash_cache` for exactly what a cache hit trusts.
///
/// The result is a `PackageVerification`: `Verified` or `PartiallyVerified`
/// mint a `VerifiedCasePackage`; `Refused` yields only the integrity
/// report, so a failed package cannot be consumed as checked bytes.
pub fn verify_case_package(
    manifest_bytes: &[u8],
    package_root: &Path,
    source_roots: &BTreeMap<String, PathBuf>,
    mut hash_cache: Option<HashCacheContext<'_>>,
) -> Result<PackageVerification, PackageError> {
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

    // Resolve every artifact's confined path first. This is cheap (no bytes
    // read) and lets a cache hit skip hashing entirely rather than only
    // skipping the comparison. A path that resolves inside the package
    // directory itself is never cache-eligible, whatever root named it.
    let mut resolved_paths: Vec<Option<PathBuf>> = Vec::with_capacity(manifest.artifacts.len());
    for artifact in &manifest.artifacts {
        let path = match canonical_source_roots.get(&artifact.source_root) {
            Some(root) => resolve_confined(root, &artifact.path)?,
            None => None,
        };
        resolved_paths.push(path);
    }

    // A hit reuses the recorded digest without reading the file; a miss (or
    // no cache at all) is hashed. Misses are hashed in parallel: the bound
    // data releases run to hundreds of megabytes, and a cold or invalidated
    // cache still re-hashes every one of them.
    let mut results: Vec<Option<(Option<String>, IntegrityCheckState)>> =
        Vec::with_capacity(manifest.artifacts.len());
    let mut pending: Vec<(usize, PathBuf)> = Vec::new();
    let mut pending_stamps: BTreeMap<usize, (String, FileStamp)> = BTreeMap::new();
    for (index, path) in resolved_paths.into_iter().enumerate() {
        let Some(path) = path else {
            let state =
                if canonical_source_roots.contains_key(&manifest.artifacts[index].source_root) {
                    IntegrityCheckState::Missing
                } else {
                    IntegrityCheckState::NotChecked
                };
            results.push(Some((None, state)));
            continue;
        };
        let eligible = !path.starts_with(&package_root);
        if eligible && let Some(context) = hash_cache.as_ref() {
            let metadata = fs::metadata(&path).map_err(|source| PackageError::Io {
                path: path.display().to_string(),
                source,
            })?;
            let stamp = FileStamp::read(&metadata).map_err(|source| PackageError::Io {
                path: path.display().to_string(),
                source,
            })?;
            let key = path.to_string_lossy().into_owned();
            if let Some(cached_sha256) = context.cache.hit(&key, &stamp) {
                let expected = &manifest.artifacts[index].sha256;
                let state = if cached_sha256 == expected {
                    IntegrityCheckState::VerifiedCached
                } else {
                    IntegrityCheckState::Mismatch
                };
                results.push(Some((Some(cached_sha256.to_string()), state)));
                continue;
            }
            pending_stamps.insert(index, (key, stamp));
        }
        pending.push((index, path));
        results.push(None);
    }

    let hashed: Vec<Result<(Option<String>, IntegrityCheckState), PackageError>> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = pending
                .iter()
                .map(|(index, path)| {
                    let expected = manifest.artifacts[*index].sha256.as_str();
                    scope.spawn(move || hash_and_compare(path, expected))
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
    for ((index, _path), outcome) in pending.iter().zip(hashed) {
        let (actual_sha256, state) = outcome?;
        if let (Some(sha256), Some(context)) = (&actual_sha256, hash_cache.as_mut())
            && let Some((key, stamp)) = pending_stamps.remove(index)
        {
            let verified_at = context.verified_at.to_string();
            context
                .cache
                .record(key, stamp, sha256.clone(), verified_at);
        }
        results[*index] = Some((actual_sha256, state));
    }

    let mut artifacts = Vec::with_capacity(manifest.artifacts.len());
    for (artifact, check) in manifest.artifacts.iter().zip(results) {
        let (actual_sha256, state) =
            check.expect("every artifact index is resolved exactly once above");
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
    Ok(match status {
        PackageIntegrityStatus::Failed => PackageVerification::Refused(integrity),
        PackageIntegrityStatus::Partial => PackageVerification::PartiallyVerified(
            VerifiedCasePackage::new(manifest, integrity, document_bytes),
        ),
        PackageIntegrityStatus::Complete => PackageVerification::Verified(
            VerifiedCasePackage::new(manifest, integrity, document_bytes),
        ),
    })
}

pub(crate) fn validate_manifest(manifest: &CasePackageManifest) -> Result<(), PackageError> {
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

pub(crate) fn validate_relative_path(value: &str) -> Result<PathBuf, PackageError> {
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

pub(crate) fn canonical_directory(path: &Path) -> Result<PathBuf, PackageError> {
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

pub(crate) fn resolve_confined(
    root: &Path,
    relative: &str,
) -> Result<Option<PathBuf>, PackageError> {
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

/// Hash an already-resolved regular file in fixed-size chunks. Returns the
/// prefixed digest and byte length.
fn hash_file(path: &Path) -> Result<(String, u64), PackageError> {
    let mut file = fs::File::open(path).map_err(|source| PackageError::Io {
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
    Ok((format!("sha256:{:x}", hasher.finalize()), length))
}

/// Hash a regular file confined beneath `root`. Returns the prefixed digest
/// and byte length, or `None` when the file is absent.
pub(crate) fn hash_confined_file(
    root: &Path,
    relative: &str,
) -> Result<Option<(String, u64)>, PackageError> {
    let Some(path) = resolve_confined(root, relative)? else {
        return Ok(None);
    };
    hash_file(&path).map(Some)
}

/// Hash an already-resolved file and compare it with the manifest's bound
/// identity. The caller has already confirmed the file exists.
fn hash_and_compare(
    path: &Path,
    expected: &str,
) -> Result<(Option<String>, IntegrityCheckState), PackageError> {
    let (actual, _length) = hash_file(path)?;
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

        let partial = verify_case_package(&manifest, &root.0, &BTreeMap::new(), None).unwrap();
        assert_eq!(
            partial.integrity_report().status,
            PackageIntegrityStatus::Partial
        );
        assert_eq!(
            partial.integrity_report().artifacts[0].state,
            IntegrityCheckState::NotChecked
        );

        let sources = BTreeMap::from([("source".into(), root.0.join("source"))]);
        let complete = verify_case_package(&manifest, &root.0, &sources, None).unwrap();
        assert_eq!(
            complete.integrity_report().status,
            PackageIntegrityStatus::Complete
        );
        assert_eq!(
            complete.integrity_report().artifacts[0].state,
            IntegrityCheckState::Verified
        );

        fs::write(root.0.join("source/artifact.bin"), b"different").unwrap();
        let failed = verify_case_package(&manifest, &root.0, &sources, None).unwrap();
        assert_eq!(
            failed.integrity_report().status,
            PackageIntegrityStatus::Failed
        );
        assert_eq!(
            failed.integrity_report().artifacts[0].state,
            IntegrityCheckState::Mismatch
        );
    }

    #[test]
    fn a_relocated_package_verifies_at_the_same_identity() {
        // Identity is content, not location: the same manifest and file
        // bytes at a different package root and source root produce the
        // same manifest digest and the same verified states.
        let first = TestDir::new();
        let manifest = fixture(&first.0);
        let second = TestDir::new();
        for name in ["contract.json", "registry.json", "claims.json"] {
            fs::write(second.0.join(name), name.as_bytes()).unwrap();
        }
        fs::create_dir_all(second.0.join("source")).unwrap();
        fs::write(second.0.join("source/artifact.bin"), b"artifact").unwrap();

        let first_sources = BTreeMap::from([("source".into(), first.0.join("source"))]);
        let second_sources = BTreeMap::from([("source".into(), second.0.join("source"))]);
        let one = verify_case_package(&manifest, &first.0, &first_sources, None).unwrap();
        let two = verify_case_package(&manifest, &second.0, &second_sources, None).unwrap();
        assert_eq!(
            one.integrity_report().status,
            PackageIntegrityStatus::Complete
        );
        assert_eq!(
            two.integrity_report().status,
            PackageIntegrityStatus::Complete
        );
        assert_eq!(
            one.integrity_report().manifest_sha256,
            two.integrity_report().manifest_sha256
        );
        assert_eq!(
            one.integrity_report().artifacts[0].state,
            two.integrity_report().artifacts[0].state
        );
    }

    #[test]
    fn admission_is_not_a_manifest_field() {
        // SC-8: admission is an organization's recorded policy judgment, not
        // a field on the manifest — an `admitted` field cannot be forged onto
        // a package; the schema refuses it outright.
        let root = TestDir::new();
        let mut manifest: serde_json::Value = serde_json::from_slice(&fixture(&root.0)).unwrap();
        manifest["admitted"] = serde_json::json!(true);
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let error = verify_case_package(&bytes, &root.0, &BTreeMap::new(), None).unwrap_err();
        assert!(error.to_string().contains("unknown field"), "{error}");
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
        let error = verify_case_package(&bytes, &root.0, &BTreeMap::new(), None).unwrap_err();
        assert!(error.to_string().contains("does not declare"), "{error}");

        manifest.capabilities.push(PackageCapability {
            capability_id: "stub".into(),
            package_id: "test/stub@1".into(),
            source_repository: None,
            source_commit: None,
            executable_sha256: digest(b"stub"),
        });
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let error = verify_case_package(&bytes, &root.0, &BTreeMap::new(), None).unwrap_err();
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
        let package = verify_case_package(&bytes, &root.0, &BTreeMap::new(), None)
            .unwrap()
            .into_package()
            .expect("a valid manifest mints a package");
        assert_eq!(package.manifest().executions.len(), 1);

        manifest.documents.push(PackageDocument {
            document_id: "receipt".into(),
            role: "execution_receipt".into(),
            path: "receipt.json".into(),
            sha256: digest(b"receipt"),
            step_id: Some("other".into()),
        });
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let error = verify_case_package(&bytes, &root.0, &BTreeMap::new(), None).unwrap_err();
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
            verify_case_package(&bytes, &root.0, &BTreeMap::new(), None),
            Err(PackageError::InvalidManifest(_))
        ));
    }

    // --- S-038: opt-in verified-hash cache -----------------------------

    /// A fixture whose "source" root is genuinely external to the package
    /// directory (unlike `fixture()`'s nested `source/`), so its artifact is
    /// cache-eligible.
    fn external_fixture(package_root: &Path, external_source: &Path) -> Vec<u8> {
        for name in ["contract.json", "registry.json", "claims.json"] {
            fs::write(package_root.join(name), name.as_bytes()).unwrap();
        }
        fs::write(external_source.join("artifact.bin"), b"artifact").unwrap();
        serde_json::to_vec(&CasePackageManifest {
            schema_version: CASE_PACKAGE_SCHEMA_VERSION.into(),
            case_id: "CASE-TEST".into(),
            title: "hash cache fixture".into(),
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
                source_root: "external".into(),
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

    fn context(cache: &mut HashCache) -> HashCacheContext<'_> {
        HashCacheContext {
            cache,
            verified_at: "2026-09-04T00:00:00Z",
        }
    }

    #[test]
    fn cold_run_populates_and_warm_run_hits_the_cache() {
        let root = TestDir::new();
        let external = TestDir::new();
        let manifest = external_fixture(&root.0, &external.0);
        let roots = BTreeMap::from([("external".to_string(), external.0.clone())]);

        let mut cache = HashCache::new();
        let cold =
            verify_case_package(&manifest, &root.0, &roots, Some(context(&mut cache))).unwrap();
        assert_eq!(
            cold.integrity_report().artifacts[0].state,
            IntegrityCheckState::Verified,
            "a cold cache must not fabricate a hit"
        );
        assert_eq!(
            cache.entries.len(),
            1,
            "the cold run must populate the cache"
        );

        let warm =
            verify_case_package(&manifest, &root.0, &roots, Some(context(&mut cache))).unwrap();
        assert_eq!(
            warm.integrity_report().artifacts[0].state,
            IntegrityCheckState::VerifiedCached,
            "a warm hit must be reported as the distinct cached state, never plain verified"
        );
        assert_eq!(
            warm.integrity_report().artifacts[0].actual_sha256,
            cold.integrity_report().artifacts[0].actual_sha256,
            "the cached digest must be the one the cold run actually measured"
        );
    }

    #[test]
    fn a_size_change_misses_the_cache() {
        let root = TestDir::new();
        let external = TestDir::new();
        let mut manifest: CasePackageManifest =
            serde_json::from_slice(&external_fixture(&root.0, &external.0)).unwrap();
        let roots = BTreeMap::from([("external".to_string(), external.0.clone())]);
        let mut cache = HashCache::new();
        verify_case_package(
            &serde_json::to_vec(&manifest).unwrap(),
            &root.0,
            &roots,
            Some(context(&mut cache)),
        )
        .unwrap();

        // Grow the file (same mtime is not guaranteed here; the point is
        // that a size change alone is sufficient to miss).
        fs::write(external.0.join("artifact.bin"), b"artifact-with-more-bytes").unwrap();
        manifest.artifacts[0].sha256 = digest(b"artifact-with-more-bytes");
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let rehashed =
            verify_case_package(&bytes, &root.0, &roots, Some(context(&mut cache))).unwrap();
        assert_eq!(
            rehashed.integrity_report().artifacts[0].state,
            IntegrityCheckState::Verified,
            "a changed size must miss the cache and re-hash from bytes"
        );
    }

    #[test]
    fn an_mtime_change_misses_the_cache() {
        let root = TestDir::new();
        let external = TestDir::new();
        let manifest = external_fixture(&root.0, &external.0);
        let roots = BTreeMap::from([("external".to_string(), external.0.clone())]);
        let mut cache = HashCache::new();
        verify_case_package(&manifest, &root.0, &roots, Some(context(&mut cache))).unwrap();

        // Same bytes, but the modification time is moved forward explicitly
        // (not by sleeping, which a coarse filesystem clock could hide).
        let path = external.0.join("artifact.bin");
        let bumped =
            fs::metadata(&path).unwrap().modified().unwrap() + std::time::Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(bumped)
            .unwrap();
        let rewritten =
            verify_case_package(&manifest, &root.0, &roots, Some(context(&mut cache))).unwrap();
        assert_eq!(
            rewritten.integrity_report().artifacts[0].state,
            IntegrityCheckState::Verified,
            "a changed mtime must miss the cache even though the bytes are unchanged"
        );
    }

    /// The cache's documented limitation: a cache hit trusts the recorded
    /// stamp, not the bytes. An actor who can rewrite a file while
    /// preserving its size and modification time is not caught by this
    /// cache; that is exactly the trust statement in SECURITY.md and
    /// `crate::hash_cache`, demonstrated rather than hidden.
    #[test]
    fn adversarial_same_size_and_mtime_with_different_bytes_is_accepted_from_cache() {
        let root = TestDir::new();
        let external = TestDir::new();
        let mut manifest: CasePackageManifest =
            serde_json::from_slice(&external_fixture(&root.0, &external.0)).unwrap();
        let roots = BTreeMap::from([("external".to_string(), external.0.clone())]);
        let mut cache = HashCache::new();
        verify_case_package(
            &serde_json::to_vec(&manifest).unwrap(),
            &root.0,
            &roots,
            Some(context(&mut cache)),
        )
        .unwrap();

        let path = external.0.join("artifact.bin");
        let original_modified = fs::metadata(&path).unwrap().modified().unwrap();
        // Same length as b"artifact" (8 bytes), different content.
        fs::write(&path, b"ARTIFACT").unwrap();
        // Windows requires write access to update file timestamps.
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(original_modified)
            .unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 8);
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            original_modified
        );

        // The manifest still names the ORIGINAL bytes' digest, exactly as an
        // operator who trusts the cache and never re-blessed the package
        // would leave it.
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let accepted =
            verify_case_package(&bytes, &root.0, &roots, Some(context(&mut cache))).unwrap();
        assert_eq!(
            accepted.integrity_report().artifacts[0].state,
            IntegrityCheckState::VerifiedCached,
            "documented limitation: a stamp match is accepted from cache without reading bytes"
        );

        // Without the cache, the same tampering is caught immediately.
        manifest.artifacts[0].sha256 = digest(b"artifact"); // unchanged, still wrong for "ARTIFACT"
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let honest = verify_case_package(&bytes, &root.0, &roots, None).unwrap();
        assert_eq!(
            honest.integrity_report().artifacts[0].state,
            IntegrityCheckState::Mismatch,
            "hashing from bytes must still catch what the cache could not"
        );
    }

    #[test]
    fn a_cache_hit_disagreeing_with_the_manifest_still_fails_closed() {
        let root = TestDir::new();
        let external = TestDir::new();
        let mut manifest: CasePackageManifest =
            serde_json::from_slice(&external_fixture(&root.0, &external.0)).unwrap();
        let roots = BTreeMap::from([("external".to_string(), external.0.clone())]);
        let mut cache = HashCache::new();
        verify_case_package(
            &serde_json::to_vec(&manifest).unwrap(),
            &root.0,
            &roots,
            Some(context(&mut cache)),
        )
        .unwrap();

        // The file and the cache are both untouched; only the manifest's
        // bound identity now names a different digest, as it would after an
        // edit to the manifest that was never followed by a rehash.
        manifest.artifacts[0].sha256 = digest(b"a different expectation entirely");
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let result =
            verify_case_package(&bytes, &root.0, &roots, Some(context(&mut cache))).unwrap();
        assert_eq!(
            result.integrity_report().artifacts[0].state,
            IntegrityCheckState::Mismatch,
            "a cache hit must still be compared to the manifest's bound identity"
        );
        assert_eq!(
            result.integrity_report().status,
            PackageIntegrityStatus::Failed
        );
    }

    #[test]
    fn corrupt_cache_contents_are_ignored_by_verify_case_package_itself() {
        // verify_case_package never reads the cache file; a corrupt file on
        // disk is entirely the caller's concern (see hash_cache::tests and
        // the runner's CORE-X1003 finding). What matters here is that an
        // in-memory cache with a wrong entry for this path never crashes the
        // verifier; it is just another miss or another disagreement, both
        // already exercised above.
        let root = TestDir::new();
        let external = TestDir::new();
        let manifest = external_fixture(&root.0, &external.0);
        let roots = BTreeMap::from([("external".to_string(), external.0.clone())]);
        let mut cache = HashCache::new();
        cache.record(
            external
                .0
                .join("artifact.bin")
                .to_string_lossy()
                .into_owned(),
            FileStamp {
                size: 999,
                mtime_ns: 0,
                dev: None,
                ino: None,
            },
            "sha256:0000000000000000000000000000000000000000000000000000000000000".into(),
            "2020-01-01T00:00:00Z".into(),
        );
        let result =
            verify_case_package(&manifest, &root.0, &roots, Some(context(&mut cache))).unwrap();
        assert_eq!(
            result.integrity_report().artifacts[0].state,
            IntegrityCheckState::Verified,
            "a stale stamp is a miss, hashed fresh, never a panic"
        );
    }

    #[test]
    fn package_documents_are_never_cached() {
        let root = TestDir::new();
        let manifest = fixture(&root.0); // "source" is nested inside the package root
        let mut cache = HashCache::new();
        // Seed a wrong entry for the contract document's path; if the
        // document loop ever consulted a cache, this would corrupt its
        // integrity check silently.
        cache.record(
            root.0.join("contract.json").to_string_lossy().into_owned(),
            FileStamp {
                size: 0,
                mtime_ns: 0,
                dev: None,
                ino: None,
            },
            "sha256:0000000000000000000000000000000000000000000000000000000000000".into(),
            "2020-01-01T00:00:00Z".into(),
        );
        let package = verify_case_package(
            &manifest,
            &root.0,
            &BTreeMap::new(),
            Some(context(&mut cache)),
        )
        .unwrap();
        assert_eq!(
            package.integrity_report().documents[0].state,
            IntegrityCheckState::Verified
        );
    }

    #[test]
    fn artifacts_inside_the_case_directory_are_always_rehashed() {
        // fixture()'s "source" root resolves inside the package directory,
        // exactly like CASE-002's "case" root does for its expected/*
        // outputs; it must never be cache-eligible even when a cache is
        // supplied and primed with a wrong entry for it.
        let root = TestDir::new();
        let manifest = fixture(&root.0);
        let sources = BTreeMap::from([("source".to_string(), root.0.join("source"))]);
        let mut cache = HashCache::new();
        cache.record(
            root.0
                .join("source/artifact.bin")
                .to_string_lossy()
                .into_owned(),
            FileStamp {
                size: 0,
                mtime_ns: 0,
                dev: None,
                ino: None,
            },
            "sha256:0000000000000000000000000000000000000000000000000000000000000".into(),
            "2020-01-01T00:00:00Z".into(),
        );
        let package =
            verify_case_package(&manifest, &root.0, &sources, Some(context(&mut cache))).unwrap();
        assert_eq!(
            package.integrity_report().artifacts[0].state,
            IntegrityCheckState::Verified,
            "an in-package artifact must be rehashed, ignoring any cache entry"
        );
    }
}
