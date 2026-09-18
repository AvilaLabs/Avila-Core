//! Loading the package's qualification records and checking each one against
//! the capability it names, before any envelope is evaluated against a run's
//! facts (ADR-0008).

use std::collections::BTreeMap;
use std::error::Error;

use avila_core_compiler::QualificationRecord;
use avila_core_compiler::SourceLocation;
use avila_core_compiler::parse_qualification;
use avila_core_compiler::parse_revocation;
use avila_core_evidence::VerifiedCasePackage;
use avila_core_evidence::signature;
use avila_core_kernel::KindRegistry;

use crate::case_run::FindingClass;
use crate::diagnostic::{CORE_X3404, CORE_X3405, RunStage};

use super::RunFinding;

/// A qualification record the package binds, already checked against the
/// capability it names.
pub struct BoundQualification {
    pub sha256: String,
    pub record: QualificationRecord,
}

/// What envelope evaluation needs: the bound records, the lifecycle facts
/// the bound set asserts over them, and the registry's quantity kinds.
pub struct Envelopes {
    pub records: Vec<BoundQualification>,
    /// Bound record digest → digest of the bound record superseding it.
    /// A record is dead when another bound record names its digest in
    /// `supersedes`; its assessment still attaches, marked, but no claim
    /// citing it can satisfy a bounded or enclosure requirement.
    pub superseded_by: BTreeMap<String, String>,
    /// Bound record digest → digest of the bound `qualification_revocation`
    /// document withdrawing it.
    pub revoked_by: BTreeMap<String, String>,
    pub kinds: KindRegistry,
}

/// The package's qualification evidence: the bound records, the lifecycle
/// facts the bound set asserts over them (supersession edges carried on the
/// superseding records, withdrawals carried on `qualification_revocation`
/// documents), and any findings the load produced.
pub struct LoadedQualifications {
    pub records: Vec<BoundQualification>,
    pub superseded_by: BTreeMap<String, String>,
    pub revoked_by: BTreeMap<String, String>,
    pub findings: Vec<RunFinding>,
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
///
/// Lifecycle: a bound record is dead when another bound record names its
/// digest in `supersedes`, or a bound `qualification_revocation` names it.
/// A revocation against a recognized owner's record must itself verify
/// under the declared issuer key — an unverifiable one is ignored with a
/// `CORE-X3405` finding and the record stands. Without a listed owner a
/// revocation is package-asserted and applies unsigned: it can only deny
/// evidence, never manufacture acceptance.
pub(crate) fn load_qualifications(
    package: &VerifiedCasePackage,
    recognized_owners: &BTreeMap<String, String>,
) -> Result<LoadedQualifications, Box<dyn Error>> {
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
        // The pair the record covers must be exercised by the workflow —
        // unless the record is bound only as a supersession witness: a
        // record carrying `supersedes` may replace a record for a
        // capability this package no longer executes. It still must name a
        // bound capability implementation exactly, checked just above, and
        // it never applies to a step whose pair is not exercised.
        if record.supersedes.is_empty()
            && !package.manifest.executions.iter().any(|execution| {
                execution.adapter == record.adapter
                    && execution.capability_id == record.capability.capability_id
            })
        {
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
            && let Some(reason) =
                check_issuer_signature(package, "qualification", document, declared_key)
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

    // Supersession: a bound record is dead where another bound record names
    // its digest in `supersedes`. The edge rides on the superseding record,
    // so it carries exactly that record's own authenticity — under issuer
    // recognition a listed owner's record was already signature-checked
    // above; without it the edge is package-asserted.
    let mut superseded_by = BTreeMap::new();
    for dead in &bound {
        if let Some(superseder) = bound
            .iter()
            .find(|candidate| candidate.record.supersedes.contains(&dead.sha256))
        {
            superseded_by.insert(dead.sha256.clone(), superseder.sha256.clone());
        }
    }

    // Revocation: a bound `qualification_revocation` naming a bound record's
    // digest withdraws it. Against a recognized owner's record the document
    // must verify under the declared issuer key — an unverifiable one is
    // ignored with a finding and the record stands. Without a listed owner
    // it is package-asserted and applies unsigned.
    let mut revoked_by = BTreeMap::new();
    for document in package
        .manifest
        .documents
        .iter()
        .filter(|document| document.role == "qualification_revocation")
    {
        let bytes = package
            .document_by_id(&document.document_id)
            .ok_or_else(|| {
                format!(
                    "qualification revocation `{}` has no bytes",
                    document.document_id
                )
            })?;
        let revocation = parse_revocation(bytes)
            .map_err(|error| format!("document `{}`: {error}", document.document_id))?;
        let Some(target) = bound
            .iter()
            .find(|bound| bound.sha256 == revocation.record_sha256)
        else {
            continue;
        };
        if let Some(declared_key) = recognized_owners.get(&target.record.owner)
            && let Some(reason) =
                check_issuer_signature(package, "qualification_revocation", document, declared_key)
        {
            findings.push(RunFinding::runtime(
                CORE_X3405,
                FindingClass::Inadmissible,
                RunStage::PackageIntegrity,
                "policy_owner",
                SourceLocation::new(
                    "manifest",
                    format!("/documents/*/document_id={}", document.document_id),
                ),
                format!(
                    "revocation of qualification `{}` {reason} — the withdrawal is ignored and the record stands",
                    revocation.qualification_id
                ),
            ));
            continue;
        }
        revoked_by.insert(target.sha256.clone(), document.sha256.clone());
    }

    Ok(LoadedQualifications {
        records: bound,
        superseded_by,
        revoked_by,
        findings,
    })
}

/// Verify a bound document's signature under the issuer key the contract's
/// recognition policy declares for its owner. `Some(reason)` describes each
/// failure; `None` only when a signature document covers the document's
/// bound bytes and verifies under exactly that key.
fn check_issuer_signature(
    package: &VerifiedCasePackage,
    role: &str,
    document: &avila_core_evidence::PackageDocument,
    declared_key: &str,
) -> Option<String> {
    let Ok(declared_key_id) = signature::key_id_from_public_hex(declared_key) else {
        return Some("the declared issuer key is malformed".into());
    };
    let Some((_, signature_document)) =
        super::signing::find_signature_for(package, role, &document.document_id)
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
