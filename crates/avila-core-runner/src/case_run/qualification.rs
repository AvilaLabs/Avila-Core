//! Loading the package's qualification records and checking each one against
//! the capability it names, before any envelope is evaluated against a run's
//! facts (ADR-0008).

use std::collections::BTreeMap;
use std::error::Error;

use avila_core_compiler::QualificationRecord;
use avila_core_compiler::SourceLocation;
use avila_core_compiler::parse_qualification;
use avila_core_evidence::VerifiedCasePackage;
use avila_core_evidence::signature;
use avila_core_kernel::KindRegistry;

use crate::case_run::FindingClass;
use crate::diagnostic::{CORE_X3404, RunStage};

use super::RunFinding;

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
///
/// When the contract's execution policy names recognized issuers
/// (`recognized_qualification_owners`), a listed owner's record must also
/// carry a signature document over its bound bytes that verifies under the
/// declared key — an unsigned or unverifiable record is refused here, not
/// attached, with a `CORE-X3404` finding. A record whose owner is not listed
/// still loads: its assessment attaches to the step's claims as data, and the
/// campaign's recognition check refuses it as `CORE-A4601` evidence.
pub(crate) fn load_qualifications(
    package: &VerifiedCasePackage,
    recognized_owners: &BTreeMap<String, String>,
) -> Result<(Vec<BoundQualification>, Vec<RunFinding>), Box<dyn Error>> {
    let mut bound = Vec::new();
    let mut findings = Vec::new();
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
        // Recognition: a listed owner's record stands only behind a
        // signature document over its bound bytes that verifies under the
        // issuer's declared key. An unlisted owner's record still loads —
        // its assessment attaches as data and the campaign refuses it as
        // `CORE-A4601` evidence — but a listed owner's unsigned or
        // unverifiable record is refused here.
        if let Some(declared_key) = recognized_owners.get(&record.owner)
            && let Some(reason) = check_recognition_signature(package, document, declared_key)
        {
            findings.push(RunFinding::runtime(
                CORE_X3404,
                FindingClass::Inadmissible,
                RunStage::PackageIntegrity,
                "policy_owner",
                SourceLocation::new(
                    "manifest",
                    format!("/documents/*/document_id={}", document.document_id),
                ),
                format!(
                    "qualification `{}` names recognized issuer `{}` but {reason} — the record is not applied",
                    record.qualification_id, record.owner
                ),
            ));
            continue;
        }
        bound.push(BoundQualification {
            sha256: document.sha256.clone(),
            record,
        });
    }
    Ok((bound, findings))
}

/// Verify a bound qualification document's signature under the issuer key
/// the contract's recognition policy declares. `Some(reason)` describes
/// each failure; `None` only when a signature document covers the record's
/// bound bytes and verifies under exactly that key.
fn check_recognition_signature(
    package: &VerifiedCasePackage,
    document: &avila_core_evidence::PackageDocument,
    declared_key: &str,
) -> Option<String> {
    let Ok(declared_key_id) = signature::key_id_from_public_hex(declared_key) else {
        return Some("the declared issuer key is malformed".into());
    };
    let Some((_, signature_document)) =
        super::signing::find_signature_for(package, "qualification", &document.document_id)
    else {
        return Some("carries no signature document over its bound bytes".into());
    };
    if signature_document.signed_document.sha256 != document.sha256 {
        return Some("its signature covers different bytes than the package binds".into());
    }
    if signature_document.key_id != declared_key_id {
        return Some(format!(
            "its signature is by `{}`, not the declared issuer key",
            signature_document.key_id
        ));
    }
    let Ok(digest) = signature::signed_target_digest(&signature_document) else {
        return Some("its signature's target digest is malformed".into());
    };
    match signature::verify_digest(declared_key, &digest, &signature_document.signature_hex) {
        Ok(true) => None,
        Ok(false) => Some("its signature does not verify under the declared issuer key".into()),
        Err(error) => Some(format!("its signature could not be verified — {error}")),
    }
}
