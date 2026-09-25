//! Relocatable package export.
//!
//! `export_package` gathers a verified case package — its manifest, every
//! confined document, and every declared artifact resolved under the
//! operator-supplied source roots — into one output directory. The manifest
//! and documents land verbatim at their manifest-declared paths; artifacts
//! land under `roots/<source_root>/<path>`. Nothing is rewritten: the
//! manifest is the package identity, and the relocation test in this crate
//! pins that identity as content, not location.
//!
//! The report is a content-identified `export-report/v0.1-draft` document.
//! It records each copied byte's re-hashed digest — the copy is checked, not
//! trusted — and the `roots/` layout it wrote so a receiver can point
//! `--source-root <name>=<bundle>/roots/<name>` at it. Export refuses a
//! package whose integrity does not verify: a bundle that ships unmet or
//! mismatched bytes would misrepresent what it claims to carry.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::package::{
    PackageError, canonical_directory, resolve_confined, validate_manifest, verify_case_package,
};
use crate::{CasePackageManifest, sha256_hex};

pub const EXPORT_REPORT_SCHEMA_VERSION: &str = "avila.core/export-report/v0.1-draft";
const EXPORT_NOTICE: &str = "This report records a byte-verified copy of a case package. It is not an approval, a certification, or a run authorization; the exported package must still be checked or verified before any run.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedDocument {
    pub document_id: String,
    pub role: String,
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedArtifact {
    pub artifact_id: String,
    pub evidence_ids: Vec<String>,
    pub source_root: String,
    pub path: String,
    /// The artifact's location inside the bundle, relative to the bundle root.
    pub bundle_path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExportReport {
    pub schema_version: String,
    pub status: ExportStatus,
    pub case_id: String,
    pub manifest_sha256: String,
    pub documents: Vec<ExportedDocument>,
    pub artifacts: Vec<ExportedArtifact>,
    /// Bundle-relative directory each named source root was exported to.
    pub source_roots: BTreeMap<String, String>,
    pub export_sha256: String,
    pub notice: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    Exported,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error("package integrity is {0:?}; export requires every document and artifact to verify")]
    IntegrityNotComplete(crate::PackageIntegrityStatus),
    #[error("output directory `{0}` exists and is not empty")]
    OutputNotEmpty(String),
    #[error("cannot write `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("exported bytes for `{id}` do not match the manifest's bound digest")]
    CopyMismatch { id: String },
    #[error("cannot serialize the export report: {0}")]
    Serialization(String),
}

/// Verify `package_root`'s manifest, documents, and every declared artifact
/// under `source_roots`, then copy all of it into `out_dir`. `out_dir` must
/// be absent or empty; the bundle keeps manifest paths verbatim and places
/// artifacts under `roots/<source_root>/`.
///
/// Every artifact is re-hashed *from the copy* and compared to the manifest's
/// bound digest — the recorded `sha256` is measured on the exported bytes, so
/// a copy that lands wrong fails the export rather than shipping a report
/// that claims bytes the bundle does not carry.
pub fn export_package(
    package_root: &Path,
    source_roots: &BTreeMap<String, PathBuf>,
    out_dir: &Path,
) -> Result<ExportReport, ExportError> {
    let manifest_bytes = read_at(&package_root.join("package.json"))?;
    let manifest: CasePackageManifest =
        serde_json::from_slice(&manifest_bytes).map_err(PackageError::InvalidJson)?;
    validate_manifest(&manifest)?;

    let package_root = canonical_directory(package_root)?;
    let mut canonical_roots = BTreeMap::new();
    for (name, path) in source_roots {
        canonical_roots.insert(name.clone(), canonical_directory(path)?);
    }

    let verified =
        match verify_case_package(&manifest_bytes, &package_root, &canonical_roots, None)? {
            crate::PackageVerification::Verified(package) => package,
            other => {
                return Err(ExportError::IntegrityNotComplete(
                    other.integrity_report().status,
                ));
            }
        };

    prepare_out_dir(out_dir)?;

    // Manifest and documents land verbatim at their declared paths.
    write_at(&out_dir.join("package.json"), &manifest_bytes)?;
    let mut documents = Vec::with_capacity(manifest.documents.len());
    for document in &manifest.documents {
        let bytes = verified
            .document_by_id(&document.document_id)
            .expect("a complete integrity check read every document");
        let relative = crate::package::validate_relative_path(&document.path)?;
        let target = out_dir.join(&relative);
        write_at(&target, bytes)?;
        documents.push(ExportedDocument {
            document_id: document.document_id.clone(),
            role: document.role.clone(),
            path: document.path.clone(),
            sha256: document.sha256.clone(),
        });
    }

    let mut artifacts = Vec::with_capacity(manifest.artifacts.len());
    let mut exported_roots = BTreeMap::new();
    for artifact in &manifest.artifacts {
        let root = canonical_roots
            .get(&artifact.source_root)
            .expect("a complete integrity check resolved every root");
        let source = resolve_confined(root, &artifact.path)?
            .expect("a complete integrity check found every artifact");
        let relative = crate::package::validate_relative_path(&artifact.path)?;
        let bundle_path = Path::new("roots")
            .join(&artifact.source_root)
            .join(&relative);
        let target = out_dir.join(&bundle_path);
        copy_verified(&source, &target, &artifact.sha256, &artifact.artifact_id)?;
        let (_, bytes) = crate::sha256_file(&target).map_err(|source| ExportError::Io {
            path: target.display().to_string(),
            source,
        })?;
        artifacts.push(ExportedArtifact {
            artifact_id: artifact.artifact_id.clone(),
            evidence_ids: artifact.evidence_ids.clone(),
            source_root: artifact.source_root.clone(),
            path: artifact.path.clone(),
            // Report paths are always forward-slash separated so the export
            // report's content identity does not depend on the host's
            // separator convention.
            bundle_path: format!("roots/{}/{}", artifact.source_root, artifact.path),
            sha256: artifact.sha256.clone(),
            bytes,
        });
        exported_roots.insert(
            artifact.source_root.clone(),
            format!("roots/{}", artifact.source_root),
        );
    }

    #[derive(Serialize)]
    struct IdentityBody<'a> {
        schema_version: &'a str,
        status: ExportStatus,
        case_id: &'a str,
        manifest_sha256: &'a str,
        documents: &'a [ExportedDocument],
        artifacts: &'a [ExportedArtifact],
        source_roots: &'a BTreeMap<String, String>,
    }

    let body = IdentityBody {
        schema_version: EXPORT_REPORT_SCHEMA_VERSION,
        status: ExportStatus::Exported,
        case_id: &manifest.case_id,
        manifest_sha256: &verified.integrity().manifest_sha256,
        documents: &documents,
        artifacts: &artifacts,
        source_roots: &exported_roots,
    };
    let bytes = serde_json::to_vec(&body).map_err(|e| ExportError::Serialization(e.to_string()))?;
    let canonical = avila_core_kernel::canonicalize_json(&bytes)
        .map_err(|e| ExportError::Serialization(e.to_string()))?;
    let export_sha256 = format!("sha256:{}", sha256_hex(canonical));

    let report = ExportReport {
        schema_version: EXPORT_REPORT_SCHEMA_VERSION.into(),
        status: ExportStatus::Exported,
        case_id: manifest.case_id.clone(),
        manifest_sha256: verified.integrity().manifest_sha256.clone(),
        documents,
        artifacts,
        source_roots: exported_roots,
        export_sha256,
        notice: EXPORT_NOTICE.into(),
    };
    let report_bytes = serde_json::to_vec_pretty(&report)
        .map_err(|e| ExportError::Serialization(e.to_string()))?;
    write_at(&out_dir.join("export-report.json"), &report_bytes)?;
    Ok(report)
}

fn prepare_out_dir(out_dir: &Path) -> Result<(), ExportError> {
    match fs::metadata(out_dir) {
        Ok(metadata) => {
            if !metadata.is_dir() || fs::read_dir(out_dir).map_or(true, |mut d| d.next().is_some())
            {
                return Err(ExportError::OutputNotEmpty(out_dir.display().to_string()));
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(out_dir)
            .map_err(|source| ExportError::Io {
                path: out_dir.display().to_string(),
                source,
            }),
        Err(source) => Err(ExportError::Io {
            path: out_dir.display().to_string(),
            source,
        }),
    }
}

fn read_at(path: &Path) -> Result<Vec<u8>, ExportError> {
    fs::read(path).map_err(|source| ExportError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn write_at(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ExportError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::write(path, bytes).map_err(|source| ExportError::Io {
        path: path.display().to_string(),
        source,
    })
}

/// Copy `source` to `target`, then hash the *destination* and compare to the
/// manifest's bound digest. Checking the copy — not the source — is what makes
/// the recorded digest honest about what the bundle carries.
fn copy_verified(
    source: &Path,
    target: &Path,
    expected_sha256: &str,
    artifact_id: &str,
) -> Result<(), ExportError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source_err| ExportError::Io {
            path: parent.display().to_string(),
            source: source_err,
        })?;
    }
    fs::copy(source, target).map_err(|source_err| ExportError::Io {
        path: format!("{} -> {}", source.display(), target.display()),
        source: source_err,
    })?;
    let (actual, _) = crate::sha256_file(target).map_err(|source_err| ExportError::Io {
        path: target.display().to_string(),
        source: source_err,
    })?;
    if actual != expected_sha256 {
        return Err(ExportError::CopyMismatch {
            id: artifact_id.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::package::{
        CASE_PACKAGE_SCHEMA_VERSION, PackageArtifact, PackageDocument, PackageIntegrityStatus,
        verify_case_package,
    };
    use sha2::{Digest, Sha256};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "avila-core-export-{}-{sequence}",
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

    fn digest(bytes: &[u8]) -> String {
        format!("sha256:{:x}", Sha256::digest(bytes))
    }

    fn fixture(root: &Path) -> Vec<u8> {
        for name in ["contract.json", "registry.json", "claims.json"] {
            fs::write(root.join(name), name.as_bytes()).unwrap();
        }
        fs::create_dir_all(root.join("source/nested")).unwrap();
        fs::write(root.join("source/nested/artifact.bin"), b"artifact").unwrap();
        serde_json::to_vec(&CasePackageManifest {
            schema_version: CASE_PACKAGE_SCHEMA_VERSION.into(),
            case_id: "CASE-TEST".into(),
            title: "export fixture".into(),
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
                path: "nested/artifact.bin".into(),
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
    fn export_gathers_a_verified_package_and_rehashes_the_copies() {
        let source = TestDir::new();
        fs::write(source.0.join("package.json"), fixture(&source.0)).unwrap();
        let out = TestDir::new();
        fs::remove_dir(&out.0).unwrap();

        let report = export_package(
            &source.0,
            &BTreeMap::from([("source".into(), source.0.join("source"))]),
            &out.0,
        )
        .unwrap();

        assert_eq!(report.status, ExportStatus::Exported);
        assert_eq!(report.documents.len(), 3);
        assert_eq!(report.artifacts.len(), 1);
        assert_eq!(
            report.artifacts[0].bundle_path,
            "roots/source/nested/artifact.bin"
        );
        assert_eq!(
            report.source_roots.get("source").map(String::as_str),
            Some("roots/source")
        );

        // The bundle verifies at the same package identity under its own roots.
        let bundle_manifest = fs::read(out.0.join("package.json")).unwrap();
        let verified = verify_case_package(
            &bundle_manifest,
            &out.0,
            &BTreeMap::from([("source".into(), out.0.join("roots/source"))]),
            None,
        )
        .unwrap();
        let verified = verified
            .into_package()
            .expect("a complete export verifies the bundle");
        assert_eq!(
            verified.integrity().status,
            PackageIntegrityStatus::Complete
        );
        assert_eq!(verified.integrity().manifest_sha256, report.manifest_sha256);

        // The report is content-identified: recomputing the canonical body
        // minus the digest field and informational notice reproduces
        // export_sha256.
        let mut body = serde_json::to_value(&report).unwrap();
        let object = body.as_object_mut().unwrap();
        let recorded = object.remove("export_sha256").unwrap();
        object.remove("notice");
        let canonical =
            avila_core_kernel::canonicalize_json(serde_json::to_string(&body).unwrap().as_bytes())
                .unwrap();
        assert_eq!(
            recorded,
            serde_json::Value::String(format!("sha256:{}", sha256_hex(canonical)))
        );
        assert!(out.0.join("export-report.json").is_file());
    }

    #[test]
    fn export_refuses_a_package_with_unmet_roots() {
        let source = TestDir::new();
        fs::write(source.0.join("package.json"), fixture(&source.0)).unwrap();
        let out = std::env::temp_dir().join(format!(
            "avila-core-export-unwritten-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&out);

        let error = export_package(&source.0, &BTreeMap::new(), &out).unwrap_err();
        assert!(matches!(
            error,
            ExportError::IntegrityNotComplete(PackageIntegrityStatus::Partial)
        ));
        assert!(!out.exists(), "a refused export writes nothing");
    }

    #[test]
    fn export_refuses_a_nonempty_output_dir() {
        let source = TestDir::new();
        fs::write(source.0.join("package.json"), fixture(&source.0)).unwrap();
        let out = TestDir::new();
        fs::write(out.0.join("occupied"), b"x").unwrap();

        let error = export_package(
            &source.0,
            &BTreeMap::from([("source".into(), source.0.join("source"))]),
            &out.0,
        )
        .unwrap_err();
        assert!(matches!(error, ExportError::OutputNotEmpty(_)));
    }
}
