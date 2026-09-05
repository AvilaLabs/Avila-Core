//! Ed25519 signing and verification for manifests, receipts, and campaign
//! log lines (ADR-0015).
//!
//! This module is the workspace's only cryptographic dependency boundary:
//! `ed25519-dalek` and `getrandom` are used here and nowhere else. Every
//! signing and verification operation signs or checks exactly one 32-byte
//! SHA-256 digest that the caller has already computed from whatever
//! canonical bytes are being signed; this module never decides what those
//! bytes are.
//!
//! **Boundary.** A signature proves possession of a key at signing time. It
//! does not establish the correctness of what was signed, qualification,
//! review, or regulatory suitability. See `docs/adr/0015-signed-manifests-and-receipts.md`.

use std::collections::BTreeSet;
use std::fmt;

use avila_core_kernel::canonicalize_json;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SIGNATURE_SCHEMA_VERSION: &str = "avila.core/signature/v0.1-draft";
pub const TRUST_ROOT_SCHEMA_VERSION: &str = "avila.core/trust-root/v0.1-draft";
pub const ALGORITHM_ED25519: &str = "ed25519";
pub const SIGNATURE_NOTICE: &str = "A signature proves possession of a key at signing time. It does not establish the correctness of what was signed, qualification, review, or regulatory suitability.";

/// A key's declared purpose. A requester key signs the manifest digest and,
/// through it, every document the manifest binds. A runner key signs each
/// execution receipt and each campaign log line. The role travels with the
/// key in the trust root and is checked at verification time, so a key
/// listed only as `runner` never satisfies a check that requires `requester`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyRole {
    Requester,
    Runner,
}

impl fmt::Display for KeyRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Requester => "requester",
            Self::Runner => "runner",
        })
    }
}

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("`{0}` must be exactly 32 bytes")]
    WrongKeyLength(String),
    #[error("`{0}` is not valid hexadecimal: {1}")]
    InvalidHex(String, String),
    #[error("signature must be exactly 64 bytes, hex encoded")]
    WrongSignatureLength,
    #[error("public key bytes do not encode a valid Ed25519 point")]
    InvalidPublicKey,
    #[error("random seed generation failed: {0}")]
    Random(String),
    #[error("signature does not verify against the listed key")]
    DoesNotVerify,
    #[error("key `{key_id}` is not listed under role `{role}` in the supplied trust root")]
    KeyNotListed { key_id: String, role: KeyRole },
    #[error("unsupported signature algorithm `{0}`; only `{ALGORITHM_ED25519}` is implemented")]
    UnsupportedAlgorithm(String),
    #[error("signature document is not valid JSON: {0}")]
    InvalidDocument(#[from] serde_json::Error),
    #[error(
        "signature document has unsupported schema_version `{0}`; expected `{SIGNATURE_SCHEMA_VERSION}`"
    )]
    UnsupportedSignatureSchema(String),
    #[error("digest `{0}` must use the `sha256:` prefix")]
    MissingDigestPrefix(String),
    #[error("cannot canonicalize the signing target: {0}")]
    Canonicalization(String),
    #[error(
        "trust root has unsupported schema_version `{found}`; expected `{TRUST_ROOT_SCHEMA_VERSION}`"
    )]
    UnsupportedTrustRootSchema { found: String },
    #[error("duplicate key_id `{key_id}` under role `{role}` in trust root")]
    DuplicateTrustRootKey { key_id: String, role: KeyRole },
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(label: &str, hex: &str) -> Result<Vec<u8>, SignatureError> {
    let hex = hex.trim();
    if !hex.len().is_multiple_of(2) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SignatureError::InvalidHex(
            label.into(),
            "not an even-length hexadecimal string".into(),
        ));
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&hex[index..index + 2], 16)
                .map_err(|error| SignatureError::InvalidHex(label.into(), error.to_string()))
        })
        .collect()
}

fn hex_decode_32(label: &str, hex: &str) -> Result<[u8; 32], SignatureError> {
    let bytes = hex_decode(label, hex)?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| SignatureError::WrongKeyLength(label.into()))
}

/// The key identifier: the SHA-256 of the public key bytes, hex encoded.
/// Distinct from a content digest (no `sha256:` prefix), so a reader never
/// confuses a key id with an artifact identity.
#[must_use]
pub fn key_id_from_public_bytes(public_key: &[u8; 32]) -> String {
    hex_encode(&Sha256::digest(public_key))
}

pub fn key_id_from_public_hex(public_key_hex: &str) -> Result<String, SignatureError> {
    let bytes = hex_decode_32("public key", public_key_hex)?;
    Ok(key_id_from_public_bytes(&bytes))
}

/// A freshly generated keypair. `seed` is private key material: callers must
/// write it only to an operator-owned file with restrictive permissions and
/// must never print or log it.
#[derive(Clone)]
pub struct GeneratedKeyPair {
    pub role: KeyRole,
    pub seed: [u8; 32],
    pub public_key_hex: String,
    pub key_id: String,
}

impl fmt::Debug for GeneratedKeyPair {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneratedKeyPair")
            .field("role", &self.role)
            .field("seed", &"<redacted>")
            .field("public_key_hex", &self.public_key_hex)
            .field("key_id", &self.key_id)
            .finish()
    }
}

/// Generate a fresh Ed25519 keypair from an operating-system random seed.
pub fn generate_keypair(role: KeyRole) -> Result<GeneratedKeyPair, SignatureError> {
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).map_err(|error| SignatureError::Random(error.to_string()))?;
    let public_key = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    Ok(GeneratedKeyPair {
        role,
        seed,
        public_key_hex: hex_encode(&public_key),
        key_id: key_id_from_public_bytes(&public_key),
    })
}

/// Derive the hex-encoded public key for a raw 32-byte seed.
#[must_use]
pub fn public_key_hex_from_seed(seed: &[u8; 32]) -> String {
    hex_encode(&SigningKey::from_bytes(seed).verifying_key().to_bytes())
}

/// Parse a seed file's raw bytes. Seed files written by `avila-core keys
/// generate` are exactly 32 raw bytes; this never accepts hex text, so a
/// public key file can never be mistaken for a seed.
pub fn parse_seed_bytes(bytes: &[u8]) -> Result<[u8; 32], SignatureError> {
    <[u8; 32]>::try_from(bytes).map_err(|_| SignatureError::WrongKeyLength("seed file".into()))
}

/// Sign a 32-byte digest with a raw seed. Returns the signer's key id and the
/// hex-encoded signature.
#[must_use]
pub fn sign_digest(seed: &[u8; 32], digest: &[u8; 32]) -> (String, String) {
    let signing_key = SigningKey::from_bytes(seed);
    let public_key = signing_key.verifying_key().to_bytes();
    let signature: Signature = signing_key.sign(digest);
    (
        key_id_from_public_bytes(&public_key),
        hex_encode(&signature.to_bytes()),
    )
}

/// Verify a digest's signature against an explicit hex-encoded public key.
/// `Ok(true)` iff the signature verifies; `Ok(false)` for well-formed
/// material that simply does not verify; `Err` only for malformed input.
pub fn verify_digest(
    public_key_hex: &str,
    digest: &[u8; 32],
    signature_hex: &str,
) -> Result<bool, SignatureError> {
    let public_bytes = hex_decode_32("public key", public_key_hex)?;
    let verifying_key =
        VerifyingKey::from_bytes(&public_bytes).map_err(|_| SignatureError::InvalidPublicKey)?;
    let signature_bytes = hex_decode("signature", signature_hex)?;
    let signature_bytes: [u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|_| SignatureError::WrongSignatureLength)?;
    let signature = Signature::from_bytes(&signature_bytes);
    Ok(verifying_key.verify(digest, &signature).is_ok())
}

// --- Trust root -------------------------------------------------------

/// One key a run accepts, and the role under which it accepts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustRootEntry {
    pub key_id: String,
    pub public_key_hex: String,
    pub role: KeyRole,
}

/// The requester and runner public keys a run accepts, supplied from outside
/// the package (`run --trust-root FILE`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustRoot {
    pub schema_version: String,
    pub keys: Vec<TrustRootEntry>,
}

impl TrustRoot {
    pub fn parse(bytes: &[u8]) -> Result<Self, SignatureError> {
        let root: TrustRoot = serde_json::from_slice(bytes)?;
        if root.schema_version != TRUST_ROOT_SCHEMA_VERSION {
            return Err(SignatureError::UnsupportedTrustRootSchema {
                found: root.schema_version,
            });
        }
        let mut seen = BTreeSet::new();
        for entry in &root.keys {
            if !seen.insert((entry.key_id.clone(), entry.role)) {
                return Err(SignatureError::DuplicateTrustRootKey {
                    key_id: entry.key_id.clone(),
                    role: entry.role,
                });
            }
            // Fail closed on a malformed key immediately, rather than at
            // first use deep inside a run.
            hex_decode_32("trust root public key", &entry.public_key_hex)?;
        }
        Ok(root)
    }

    /// A listed key with this id under exactly this role. A key id present
    /// only under a different role does not match here: this is what makes
    /// "a package signed by the runner key instead of the requester key" a
    /// refusal even though the key and signature are both well-formed.
    #[must_use]
    pub fn find(&self, key_id: &str, role: KeyRole) -> Option<&TrustRootEntry> {
        self.keys
            .iter()
            .find(|entry| entry.key_id == key_id && entry.role == role)
    }
}

// --- Detached signature document ---------------------------------------

/// What a detached signature names as signed: a role, an identifier within
/// that role's namespace, and the exact digest that was signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedDocumentRef {
    pub role: String,
    pub document_id: String,
    pub sha256: String,
}

/// A detached signature: `avila.core/signature/v0.1-draft`. Bound into a
/// case package like any other document; it never changes the bytes of what
/// it signs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureDocument {
    pub schema_version: String,
    pub signed_document: SignedDocumentRef,
    pub key_id: String,
    pub algorithm: String,
    pub signature_hex: String,
    pub notice: String,
}

pub fn parse_signature_document(bytes: &[u8]) -> Result<SignatureDocument, SignatureError> {
    Ok(serde_json::from_slice(bytes)?)
}

/// Decode a `sha256:`-prefixed hex digest, such as any package document's
/// bound `sha256` field, into raw bytes. Public so a caller that already
/// holds a document's bound identity can check or build a signature against
/// it without re-deriving the digest from bytes.
pub fn digest_from_prefixed(value: &str) -> Result<[u8; 32], SignatureError> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| SignatureError::MissingDigestPrefix(value.into()))?;
    hex_decode_32("prefixed digest", hex)
}

/// The raw 32-byte digest a signature document's signature was made over.
pub fn signed_target_digest(document: &SignatureDocument) -> Result<[u8; 32], SignatureError> {
    digest_from_prefixed(&document.signed_document.sha256)
}

/// Build and sign a detached signature document over an already-computed
/// digest. `signed_document_sha256` must carry the `sha256:` prefix, matching
/// every other content identity in this repository.
pub fn build_signature_document(
    seed: &[u8; 32],
    signed_role: impl Into<String>,
    signed_document_id: impl Into<String>,
    signed_document_sha256: impl Into<String>,
    digest: &[u8; 32],
) -> SignatureDocument {
    let (key_id, signature_hex) = sign_digest(seed, digest);
    SignatureDocument {
        schema_version: SIGNATURE_SCHEMA_VERSION.into(),
        signed_document: SignedDocumentRef {
            role: signed_role.into(),
            document_id: signed_document_id.into(),
            sha256: signed_document_sha256.into(),
        },
        key_id,
        algorithm: ALGORITHM_ED25519.into(),
        signature_hex,
        notice: SIGNATURE_NOTICE.into(),
    }
}

/// Verify a signature document's own signature against a trust root's key
/// listed under `expected_role`. `Ok(key_id)` on success.
pub fn verify_signature_document(
    document: &SignatureDocument,
    trust_root: &TrustRoot,
    expected_role: KeyRole,
) -> Result<String, SignatureError> {
    if document.algorithm != ALGORITHM_ED25519 {
        return Err(SignatureError::UnsupportedAlgorithm(
            document.algorithm.clone(),
        ));
    }
    let Some(entry) = trust_root.find(&document.key_id, expected_role) else {
        return Err(SignatureError::KeyNotListed {
            key_id: document.key_id.clone(),
            role: expected_role,
        });
    };
    let digest = signed_target_digest(document)?;
    if !verify_digest(&entry.public_key_hex, &digest, &document.signature_hex)? {
        return Err(SignatureError::DoesNotVerify);
    }
    Ok(entry.key_id.clone())
}

/// Internal consistency of a signature document, checkable without any
/// trust root: well-formed algorithm and hex fields, and the recorded
/// `signed_document.sha256` actually matches the digest the caller expected
/// to be signed (the target the document claims to cover). This never
/// performs a cryptographic check; it is exactly the "checked for internal
/// consistency" half of ADR-0015 clause 3 for a run without `--trust-root`.
pub fn check_internal_consistency(
    document: &SignatureDocument,
    expected_digest: &[u8; 32],
) -> Result<(), SignatureError> {
    if document.schema_version != SIGNATURE_SCHEMA_VERSION {
        return Err(SignatureError::UnsupportedSignatureSchema(
            document.schema_version.clone(),
        ));
    }
    if document.algorithm != ALGORITHM_ED25519 {
        return Err(SignatureError::UnsupportedAlgorithm(
            document.algorithm.clone(),
        ));
    }
    // Malformed hex is caught here even without a key to check against.
    hex_decode("signature_hex", &document.signature_hex)?;
    let actual = signed_target_digest(document)?;
    if &actual != expected_digest {
        return Err(SignatureError::DoesNotVerify);
    }
    Ok(())
}

/// The digest a manifest signature covers: the manifest's own bytes with the
/// signature document's entry in `documents[]` removed, canonicalized. This
/// definition is symmetric before and after the entry is bound — removing an
/// id that is not yet present is a no-op, so signing (before the entry
/// exists) and verifying (after it exists) compute the identical digest.
pub fn manifest_signing_digest(
    manifest_bytes: &[u8],
    signature_document_id: &str,
) -> Result<[u8; 32], SignatureError> {
    let mut manifest: Value = serde_json::from_slice(manifest_bytes)?;
    if let Some(documents) = manifest.get_mut("documents").and_then(Value::as_array_mut) {
        documents.retain(|document| {
            document.get("document_id").and_then(Value::as_str) != Some(signature_document_id)
        });
    }
    let bytes = serde_json::to_vec(&manifest)?;
    let canonical = canonicalize_json(&bytes)
        .map_err(|error| SignatureError::Canonicalization(error.to_string()))?;
    Ok(Sha256::digest(&canonical).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_schemas_name_the_runtime_versions() {
        let signature_schema: Value = serde_json::from_str(include_str!(
            "../../../schemas/signature.v0.1-draft.schema.json"
        ))
        .unwrap();
        assert_eq!(
            signature_schema["properties"]["schema_version"]["const"],
            SIGNATURE_SCHEMA_VERSION
        );
        let trust_root_schema: Value = serde_json::from_str(include_str!(
            "../../../schemas/trust-root.v0.1-draft.schema.json"
        ))
        .unwrap();
        assert_eq!(
            trust_root_schema["properties"]["schema_version"]["const"],
            TRUST_ROOT_SCHEMA_VERSION
        );
    }

    #[test]
    fn generated_keypair_signs_and_verifies() {
        let pair = generate_keypair(KeyRole::Requester).unwrap();
        assert_eq!(pair.key_id.len(), 64);
        assert_eq!(pair.public_key_hex.len(), 64);
        assert_eq!(public_key_hex_from_seed(&pair.seed), pair.public_key_hex);

        let digest = Sha256::digest(b"hello").into();
        let (key_id, signature_hex) = sign_digest(&pair.seed, &digest);
        assert_eq!(key_id, pair.key_id);
        assert!(verify_digest(&pair.public_key_hex, &digest, &signature_hex).unwrap());

        let other_digest = Sha256::digest(b"tampered").into();
        assert!(!verify_digest(&pair.public_key_hex, &other_digest, &signature_hex).unwrap());
    }

    #[test]
    fn two_generated_keypairs_differ() {
        let a = generate_keypair(KeyRole::Runner).unwrap();
        let b = generate_keypair(KeyRole::Runner).unwrap();
        assert_ne!(a.seed, b.seed);
        assert_ne!(a.key_id, b.key_id);
    }

    #[test]
    fn trust_root_role_separation_is_exact() {
        let pair = generate_keypair(KeyRole::Runner).unwrap();
        let root = TrustRoot {
            schema_version: TRUST_ROOT_SCHEMA_VERSION.into(),
            keys: vec![TrustRootEntry {
                key_id: pair.key_id.clone(),
                public_key_hex: pair.public_key_hex.clone(),
                role: KeyRole::Runner,
            }],
        };
        assert!(root.find(&pair.key_id, KeyRole::Runner).is_some());
        assert!(
            root.find(&pair.key_id, KeyRole::Requester).is_none(),
            "a key listed only as runner must not satisfy a requester check"
        );
    }

    #[test]
    fn duplicate_trust_root_key_is_refused() {
        let pair = generate_keypair(KeyRole::Requester).unwrap();
        let entry = TrustRootEntry {
            key_id: pair.key_id,
            public_key_hex: pair.public_key_hex,
            role: KeyRole::Requester,
        };
        let bytes = serde_json::to_vec(&TrustRoot {
            schema_version: TRUST_ROOT_SCHEMA_VERSION.into(),
            keys: vec![entry.clone(), entry],
        })
        .unwrap();
        assert!(matches!(
            TrustRoot::parse(&bytes),
            Err(SignatureError::DuplicateTrustRootKey { .. })
        ));
    }

    #[test]
    fn manifest_signing_digest_is_stable_across_binding() {
        let before =
            br#"{"schema_version":"x","documents":[{"document_id":"contract","role":"contract"}]}"#;
        let digest_before = manifest_signing_digest(before, "signature-manifest").unwrap();

        let after = br#"{"schema_version":"x","documents":[{"document_id":"contract","role":"contract"},{"document_id":"signature-manifest","role":"signature"}]}"#;
        let digest_after = manifest_signing_digest(after, "signature-manifest").unwrap();

        assert_eq!(
            digest_before, digest_after,
            "removing the not-yet-added entry must equal removing the now-added entry"
        );
    }

    #[test]
    fn build_and_verify_signature_document_round_trips() {
        let pair = generate_keypair(KeyRole::Runner).unwrap();
        let digest = Sha256::digest(b"receipt bytes").into();
        let document = build_signature_document(
            &pair.seed,
            "execution_receipt",
            "receipt-step-a",
            format!("sha256:{}", hex_encode(&Sha256::digest(b"receipt bytes"))),
            &digest,
        );
        check_internal_consistency(&document, &digest).unwrap();

        let root = TrustRoot {
            schema_version: TRUST_ROOT_SCHEMA_VERSION.into(),
            keys: vec![TrustRootEntry {
                key_id: pair.key_id.clone(),
                public_key_hex: pair.public_key_hex,
                role: KeyRole::Runner,
            }],
        };
        let matched = verify_signature_document(&document, &root, KeyRole::Runner).unwrap();
        assert_eq!(matched, pair.key_id);
        assert!(matches!(
            verify_signature_document(&document, &root, KeyRole::Requester),
            Err(SignatureError::KeyNotListed { .. })
        ));
    }

    #[test]
    fn tampered_signature_document_fails_internal_consistency() {
        let pair = generate_keypair(KeyRole::Runner).unwrap();
        let digest = Sha256::digest(b"receipt bytes").into();
        let mut document = build_signature_document(
            &pair.seed,
            "execution_receipt",
            "receipt-step-a",
            format!("sha256:{}", hex_encode(&Sha256::digest(b"receipt bytes"))),
            &digest,
        );
        document.signed_document.sha256 = format!(
            "sha256:{}",
            hex_encode(&Sha256::digest(b"a different document entirely"))
        );
        assert!(matches!(
            check_internal_consistency(&document, &digest),
            Err(SignatureError::DoesNotVerify)
        ));
    }
}
