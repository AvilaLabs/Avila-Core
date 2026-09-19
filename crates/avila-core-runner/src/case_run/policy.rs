//! ADR-0023: the signature half of the organization-policy boundary.
//!
//! The compiler proves the merge structurally — the pinned floor resolves,
//! the relation vocabulary holds, `tightens` is checked field by field,
//! and a `replaces` attestation names this contract and policy. What it
//! cannot do is authenticate: the trust root lives here. A floor whose
//! signature cannot verify under a `policy_owner` key is meaningless —
//! the same posture ADR-0015 takes for requester-signed runner receipts.
//!
//! - `CORE-A4904` — the floor's signature is absent (also caught at
//!   compile), malformed, fails internal consistency, or cannot verify
//!   under the supplied trust root's `policy_owner` keys. No trust root
//!   means no verifiable floor: the run refuses.
//! - `CORE-A4903` — the `replaces` attestation's signature fails the
//!   same checks; the binding fields were already proven at compile.

use avila_core_compiler::{ContractSource, FindingClass, SourceLocation};
use avila_core_evidence::VerifiedCasePackage;
use avila_core_evidence::signature::{
    self, KeyRole, SignatureDocument, TrustRoot, check_internal_consistency,
};
use sha2::{Digest, Sha256};

use crate::diagnostic::{CORE_A4903, CORE_A4904, RunFinding, RunStage};

/// Verify the signature side of every org-policy pin the contract
/// declares. Returns the refusal findings — empty means every named
/// signature authenticates.
pub(crate) fn check_org_policy_signatures(
    contract_bytes: &[u8],
    package: &VerifiedCasePackage,
    trust_root: Option<&TrustRoot>,
) -> Vec<RunFinding> {
    let Ok(contract) = serde_json::from_slice::<ContractSource>(contract_bytes) else {
        return Vec::new(); // a contract that does not parse never reaches here
    };
    let policy = &contract.execution_policy;
    let Some(pin) = &policy.organization_policy else {
        return Vec::new();
    };
    let mut findings = Vec::new();

    // The pinned floor — resolution-by-digest matches the compiler's.
    let Some(bytes) = bound_document(package, "organization_policy", &pin.sha256) else {
        return Vec::new(); // an unbound pin is CORE-A4902 at compile
    };
    match decode_policy_signature(bytes) {
        Ok(signature) => {
            verify_document_signature(
                &signature,
                bytes,
                trust_root,
                CORE_A4904,
                "organization_policy",
                &format!(
                    "organization policy `{}` revision {}",
                    pin.policy_id, pin.policy_revision
                ),
                &mut findings,
            );
        }
        Err(message) => findings.push(org_finding(CORE_A4904, "organization_policy", message)),
    }

    // A `replaces` attestation's signature — the compiler checked the
    // binding fields; here the attestation must authenticate.
    if let Some(attestation_sha256) = &policy.replacement_attestation
        && let Some(bytes) = bound_document(package, "attestation", attestation_sha256)
    {
        match decode_policy_signature(bytes) {
            Ok(signature) => {
                verify_document_signature(
                    &signature,
                    bytes,
                    trust_root,
                    CORE_A4903,
                    "attestation",
                    &format!("replacement attestation at {attestation_sha256}"),
                    &mut findings,
                );
            }
            Err(message) => findings.push(org_finding(CORE_A4903, "attestation", message)),
        }
    }
    findings
}

/// The bound document of `role` whose canonical digest equals `sha256`.
fn bound_document<'a>(
    package: &'a VerifiedCasePackage,
    role: &str,
    sha256: &str,
) -> Option<&'a [u8]> {
    package
        .manifest
        .documents
        .iter()
        .filter(|document| document.role == role)
        .filter_map(|document| package.document_by_id(&document.document_id))
        .find(|bytes| canonical_sha256(bytes).as_deref() == Some(sha256))
}

/// Canonical digest of a document's bytes — the same canonicalization the
/// compiler's pins use.
fn canonical_sha256(bytes: &[u8]) -> Option<String> {
    let value = avila_core_kernel::read_authoritative_json(bytes).ok()?;
    serde_json::to_vec(&value)
        .ok()
        .map(|canonical| format!("sha256:{:x}", Sha256::digest(&canonical)))
}

/// Decode the `signature` member as an ADR-0015 signature document.
fn decode_policy_signature(bytes: &[u8]) -> Result<SignatureDocument, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("document does not parse: {error}"))?;
    let signature = value
        .get("signature")
        .ok_or_else(|| "the document carries no `signature` member".to_string())?;
    serde_json::from_value(signature.clone())
        .map_err(|error| format!("the `signature` member is malformed: {error}"))
}

/// Internal consistency first — the signature must cover this document's
/// canonical bytes minus `signature` — then cryptographic verification
/// under the trust root's `policy_owner` keys.
fn verify_document_signature(
    signature: &SignatureDocument,
    document_bytes: &[u8],
    trust_root: Option<&TrustRoot>,
    code: &'static str,
    document: &str,
    what: &str,
    findings: &mut Vec<RunFinding>,
) {
    let Some(digest) = signing_digest(document_bytes) else {
        findings.push(org_finding(
            code,
            document,
            format!("{what} does not canonicalize — its signature cannot be evaluated"),
        ));
        return;
    };
    if let Err(error) = check_internal_consistency(signature, &digest) {
        findings.push(org_finding(
            code,
            document,
            format!("{what}'s signature is internally inconsistent: {error}"),
        ));
        return;
    }
    match trust_root {
        Some(root) => {
            if let Err(error) =
                signature::verify_signature_document(signature, root, KeyRole::PolicyOwner)
            {
                findings.push(org_finding(
                    code,
                    document,
                    format!(
                        "{what}'s signature does not verify under a `policy_owner` trust-root key: {error}"
                    ),
                ));
            }
        }
        None => findings.push(org_finding(
            code,
            document,
            format!(
                "{what} requires a `policy_owner` signature, but no --trust-root was supplied to verify against — an unverifiable floor cannot stand"
            ),
        )),
    }
}

/// The digest a document signature covers: the record's canonical bytes
/// with `signature` absent — the attestation convention.
fn signing_digest(document_bytes: &[u8]) -> Option<[u8; 32]> {
    let mut value: serde_json::Value = serde_json::from_slice(document_bytes).ok()?;
    value
        .as_object_mut()
        .and_then(|object| object.remove("signature"));
    let canonical = avila_core_kernel::canonicalize_json(&serde_json::to_vec(&value).ok()?).ok()?;
    Some(Sha256::digest(&canonical).into())
}

fn org_finding(code: &'static str, document: &str, message: String) -> RunFinding {
    RunFinding::runtime(
        code,
        FindingClass::Inadmissible,
        RunStage::Compilation,
        "policy_owner",
        SourceLocation::new(document, "/signature"),
        message,
    )
}
