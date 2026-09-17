//! Executes the committed authority fixture suite: detached Ed25519
//! signature documents verified against trust roots. The material is the
//! committed CASE-001 manifest signature and examples trust root, so every
//! fixture is produced over bytes that exist in the repository.

use avila_core_evidence::signature::{
    KeyRole, SignatureError, TrustRoot, parse_signature_document, verify_signature_document,
};
use serde::Deserialize;

const SUITE: &str =
    include_str!("../../../fixtures/semantic-core/authority/authority-cases.v1.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    fixture_set: String,
    version: u32,
    semantic_profile: String,
    fixtures: Vec<Fixture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    fixture_id: String,
    clause: String,
    signature_document: serde_json::Value,
    trust_root: serde_json::Value,
    expected_role: String,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    status: String,
    key_id: Option<String>,
    error: Option<String>,
}

fn error_label(error: &SignatureError) -> &'static str {
    match error {
        SignatureError::DoesNotVerify => "does_not_verify",
        SignatureError::KeyNotListed { .. } => "key_not_listed",
        SignatureError::UnsupportedAlgorithm(_) => "unsupported_algorithm",
        SignatureError::UnsupportedSignatureSchema(_) => "unsupported_signature_schema",
        SignatureError::UnsupportedTrustRootSchema { .. } => "unsupported_trust_root_schema",
        SignatureError::DuplicateTrustRootKey { .. } => "duplicate_trust_root_key",
        SignatureError::MissingDigestPrefix(_) => "missing_digest_prefix",
        SignatureError::InvalidHex(..) => "invalid_hex",
        SignatureError::InvalidPublicKey => "invalid_public_key",
        SignatureError::WrongKeyLength(_) => "wrong_key_length",
        SignatureError::WrongSignatureLength => "wrong_signature_length",
        SignatureError::InvalidDocument(_) => "invalid_document",
        SignatureError::Canonicalization(_) => "canonicalization",
        SignatureError::Random(_) => "random",
    }
}

#[test]
fn authority_fixtures_are_executable() {
    let suite: Suite = serde_json::from_str(SUITE).expect("authority suite is JSON");
    assert_eq!(suite.fixture_set, "authority-cases");
    assert_eq!(suite.version, 1);
    assert_eq!(suite.semantic_profile, "avila.core/semantic/0.2-draft");
    assert_eq!(
        suite.fixtures.len(),
        4,
        "update the corpus count intentionally"
    );

    for fixture in suite.fixtures {
        assert!(
            fixture.clause.starts_with("AU-"),
            "{} does not name its ADR clause",
            fixture.fixture_id
        );
        let document_bytes = serde_json::to_vec(&fixture.signature_document).unwrap();
        let document = parse_signature_document(&document_bytes).unwrap_or_else(|error| {
            panic!("{} signature document parse: {error}", fixture.fixture_id)
        });
        let trust_root: TrustRoot = serde_json::from_value(fixture.trust_root.clone())
            .unwrap_or_else(|error| panic!("{} trust root parse: {error}", fixture.fixture_id));
        let expected_role = match fixture.expected_role.as_str() {
            "requester" => KeyRole::Requester,
            "runner" => KeyRole::Runner,
            other => panic!("{} names unknown role {other}", fixture.fixture_id),
        };
        let result = verify_signature_document(&document, &trust_root, expected_role);
        match fixture.expected.status.as_str() {
            "verified" => {
                let key_id = result.unwrap_or_else(|error| {
                    panic!("{} expected verified: {error}", fixture.fixture_id)
                });
                assert_eq!(
                    Some(key_id.as_str()),
                    fixture.expected.key_id.as_deref(),
                    "{} verified key id",
                    fixture.fixture_id
                );
            }
            "rejected" => {
                let error = match result {
                    Ok(_) => panic!("{} unexpectedly verified", fixture.fixture_id),
                    Err(error) => error,
                };
                assert_eq!(
                    fixture.expected.error.as_deref().unwrap(),
                    error_label(&error),
                    "{} rejection kind",
                    fixture.fixture_id
                );
            }
            other => panic!("{} expected unknown status {other}", fixture.fixture_id),
        }
    }
}
