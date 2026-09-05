//! ADR-0015 signature status for a case run: whether the package manifest
//! and each step's operative receipt carry a signature verified against a
//! supplied trust root, checked for internal consistency only, or absent.
//!
//! This module never decides policy (whether an unsigned outcome is
//! acceptable); it only reports what is true about the signatures present.
//! `case_run.rs` uses the reported status to gate SC-12 reuse and
//! `execution_policy.require_signatures`.

use avila_core_evidence::VerifiedCasePackage;
use avila_core_evidence::signature::{
    KeyRole, SignatureDocument, TrustRoot, check_internal_consistency, digest_from_prefixed,
    manifest_signing_digest, parse_signature_document, verify_signature_document,
};
use serde::Serialize;

/// The signature state of one document, as far as this run can tell it.
/// Without a trust root, a bound signature is `NotChecked`, never
/// `Verified`: ADR-0015 clause 3 is explicit that internal consistency alone
/// never earns the word "verified".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SignatureStatus {
    /// No signature document names this target at all.
    Unsigned,
    /// A signature document names this target and is internally consistent
    /// (well-formed, and its recorded digest matches recomputation), but no
    /// trust root was supplied to check it against.
    NotChecked,
    /// A signature document names this target, is internally consistent,
    /// and cryptographically verifies against a key listed under the
    /// expected role in the supplied trust root.
    Verified { signed_by: String },
    /// A signature document names this target but fails internal
    /// consistency, names a key not listed under the expected role, or does
    /// not cryptographically verify.
    Invalid { reason: String },
}

impl SignatureStatus {
    #[must_use]
    pub fn is_verified(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }

    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Unsigned => "unsigned".into(),
            Self::NotChecked => "signature not checked (no trust root supplied)".into(),
            Self::Verified { signed_by } => format!("verified, signed by {signed_by}"),
            Self::Invalid { reason } => format!("invalid: {reason}"),
        }
    }
}

/// Find the bound `signature` document, if any, whose content names exactly
/// this `(role, document_id)` as its signed target. Returns the signature
/// package document's own `document_id` alongside its parsed content: the
/// former is what a manifest signature's own digest rule excludes from the
/// manifest it covers.
fn find_signature_for(
    package: &VerifiedCasePackage,
    target_role: &str,
    target_document_id: &str,
) -> Option<(String, SignatureDocument)> {
    package
        .manifest
        .documents
        .iter()
        .filter(|document| document.role == "signature")
        .find_map(|document| {
            let bytes = package.document_by_id(&document.document_id)?;
            let parsed = parse_signature_document(bytes).ok()?;
            (parsed.signed_document.role == target_role
                && parsed.signed_document.document_id == target_document_id)
                .then(|| (document.document_id.clone(), parsed))
        })
}

/// The manifest's requester-signature status. `manifest_bytes` must be the
/// exact raw bytes `verify_case_package` was given, because the signing
/// digest is computed over those bytes with the signature document's own
/// entry removed.
pub(super) fn manifest_signature_status(
    package: &VerifiedCasePackage,
    manifest_bytes: &[u8],
    trust_root: Option<&TrustRoot>,
) -> SignatureStatus {
    let Some((signature_document_id, document)) =
        find_signature_for(package, "manifest", &package.manifest.case_id)
    else {
        return SignatureStatus::Unsigned;
    };
    let expected_digest = match manifest_signing_digest(manifest_bytes, &signature_document_id) {
        Ok(digest) => digest,
        Err(error) => {
            return SignatureStatus::Invalid {
                reason: error.to_string(),
            };
        }
    };
    if let Err(error) = check_internal_consistency(&document, &expected_digest) {
        return SignatureStatus::Invalid {
            reason: error.to_string(),
        };
    }
    match trust_root {
        None => SignatureStatus::NotChecked,
        Some(trust_root) => {
            match verify_signature_document(&document, trust_root, KeyRole::Requester) {
                Ok(signed_by) => SignatureStatus::Verified { signed_by },
                Err(error) => SignatureStatus::Invalid {
                    reason: error.to_string(),
                },
            }
        }
    }
}

/// A committed execution receipt's runner-signature status. The receipt is
/// named by `step_id` (stable across re-blessing, unlike a document id) and
/// its currently bound identity `receipt_document_sha256` (the package's
/// `sha256:`-prefixed digest for that `execution_receipt` document).
pub(super) fn receipt_signature_status(
    package: &VerifiedCasePackage,
    step_id: &str,
    receipt_document_sha256: &str,
    trust_root: Option<&TrustRoot>,
) -> SignatureStatus {
    let Some((_, document)) = find_signature_for(package, "execution_receipt", step_id) else {
        return SignatureStatus::Unsigned;
    };
    let expected_digest = match digest_from_prefixed(receipt_document_sha256) {
        Ok(digest) => digest,
        Err(error) => {
            return SignatureStatus::Invalid {
                reason: error.to_string(),
            };
        }
    };
    if let Err(error) = check_internal_consistency(&document, &expected_digest) {
        return SignatureStatus::Invalid {
            reason: error.to_string(),
        };
    }
    match trust_root {
        None => SignatureStatus::NotChecked,
        Some(trust_root) => {
            match verify_signature_document(&document, trust_root, KeyRole::Runner) {
                Ok(signed_by) => SignatureStatus::Verified { signed_by },
                Err(error) => SignatureStatus::Invalid {
                    reason: error.to_string(),
                },
            }
        }
    }
}

/// The status of a signature this run just produced itself, for a freshly
/// executed step's receipt (workspace-only; not yet bound in any package).
/// `Verified` only when the signing key is also listed under `runner` in the
/// supplied trust root, so a run cannot bless its own unlisted key as
/// trusted merely by using it.
pub(super) fn fresh_signature_status(
    runner_key_id: Option<&str>,
    trust_root: Option<&TrustRoot>,
) -> SignatureStatus {
    let Some(key_id) = runner_key_id else {
        return SignatureStatus::Unsigned;
    };
    match trust_root {
        None => SignatureStatus::NotChecked,
        Some(trust_root) => match trust_root.find(key_id, KeyRole::Runner) {
            Some(entry) => SignatureStatus::Verified {
                signed_by: entry.key_id.clone(),
            },
            None => SignatureStatus::Invalid {
                reason: format!(
                    "runner key `{key_id}` is not listed under role `runner` in the supplied trust root"
                ),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_signature_status_requires_listed_runner_key() {
        assert_eq!(
            fresh_signature_status(None, None),
            SignatureStatus::Unsigned
        );
        assert_eq!(
            fresh_signature_status(Some("abc"), None),
            SignatureStatus::NotChecked
        );
        let root = TrustRoot {
            schema_version: avila_core_evidence::signature::TRUST_ROOT_SCHEMA_VERSION.into(),
            keys: vec![],
        };
        assert!(matches!(
            fresh_signature_status(Some("abc"), Some(&root)),
            SignatureStatus::Invalid { .. }
        ));
    }

    #[test]
    fn signature_status_describe_never_panics_and_names_the_reason() {
        assert_eq!(SignatureStatus::Unsigned.describe(), "unsigned");
        assert!(
            SignatureStatus::Invalid {
                reason: "bad".into()
            }
            .describe()
            .contains("bad")
        );
        assert!(
            SignatureStatus::Verified {
                signed_by: "k".into()
            }
            .describe()
            .contains('k')
        );
    }
}
