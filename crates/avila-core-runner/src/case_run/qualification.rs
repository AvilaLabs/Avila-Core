//! Loading the package's qualification records and checking each one against
//! the capability it names, before any envelope is evaluated against a run's
//! facts (ADR-0008).

use std::error::Error;

use avila_core_compiler::QualificationRecord;
use avila_core_compiler::parse_qualification;
use avila_core_evidence::VerifiedCasePackage;
use avila_core_kernel::KindRegistry;

/// A qualification record the package binds, already checked against the
/// capability it names.
pub struct BoundQualification {
    pub sha256: String,
    pub record: QualificationRecord,
}

/// What envelope evaluation needs: the bound records and the registry's
/// quantity kinds.
pub struct Envelopes {
    pub records: Vec<BoundQualification>,
    pub kinds: KindRegistry,
}

/// Load the package's `qualification` documents and refuse any whose bound
/// executable is not the one the package binds under that capability id.
pub(crate) fn load_qualifications(
    package: &VerifiedCasePackage,
) -> Result<Vec<BoundQualification>, Box<dyn Error>> {
    let mut bound = Vec::new();
    for document in package
        .manifest
        .documents
        .iter()
        .filter(|document| document.role == "qualification")
    {
        let bytes = package
            .document_by_id(&document.document_id)
            .ok_or_else(|| {
                format!(
                    "qualification document `{}` has no bytes",
                    document.document_id
                )
            })?;
        let record = parse_qualification(bytes)
            .map_err(|error| format!("document `{}`: {error}", document.document_id))?;
        let capability = package
            .manifest
            .capabilities
            .iter()
            .find(|capability| capability.capability_id == record.capability.capability_id)
            .ok_or_else(|| {
                format!(
                    "qualification `{}` names capability `{}`, which the package does not bind",
                    record.qualification_id, record.capability.capability_id
                )
            })?;
        if capability.executable_sha256 != record.capability.executable_sha256 {
            return Err(format!(
                "qualification `{}` covers executable {} but the package binds {} as `{}`",
                record.qualification_id,
                record.capability.executable_sha256,
                capability.executable_sha256,
                capability.capability_id
            )
            .into());
        }
        if !package.manifest.executions.iter().any(|execution| {
            execution.adapter == record.adapter
                && execution.capability_id == record.capability.capability_id
        }) {
            return Err(format!(
                "qualification `{}` covers adapter `{}` under `{}`, but no execution uses that pair",
                record.qualification_id, record.adapter, record.capability.capability_id
            )
            .into());
        }
        bound.push(BoundQualification {
            sha256: document.sha256.clone(),
            record,
        });
    }
    Ok(bound)
}
