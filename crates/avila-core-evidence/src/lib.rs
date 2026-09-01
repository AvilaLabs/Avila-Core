//! Portable evidence records for Avila Core.
//!
//! A bundle is a container for claims and their lineage. Creating a bundle does
//! not establish that any claim is correct, qualified, certified, or approved.
//! Verdict records will be typed by the kernel's verdict output once the
//! package format is specified; until then a bundle carries records only.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const EVIDENCE_SCHEMA_VERSION: &str = "avila.core/evidence-bundle/v0.1";

pub fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceBundle {
    pub schema_version: String,
    pub bundle_id: String,
    pub contract_id: String,
    pub state: BundleState,
    pub created_at: String,
    #[serde(default)]
    pub records: Vec<EvidenceRecord>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

impl EvidenceBundle {
    pub fn empty_draft(
        bundle_id: impl Into<String>,
        contract_id: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: EVIDENCE_SCHEMA_VERSION.into(),
            bundle_id: bundle_id.into(),
            contract_id: contract_id.into(),
            state: BundleState::Draft,
            created_at: created_at.into(),
            records: Vec::new(),
            limitations: vec![
                "Draft bundle: no scientific, regulatory, or safety claim is established.".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleState {
    Draft,
    Complete,
    Invalidated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRecord {
    pub evidence_id: String,
    pub class: EvidenceClass,
    pub media_type: String,
    pub uri: String,
    pub sha256: String,
    pub producer: String,
    #[serde(default)]
    pub parent_evidence_ids: Vec<String>,
    #[serde(default)]
    pub annotations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    Input,
    Plan,
    ExecutionReceipt,
    Output,
    Review,
    Verdict,
    Log,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable() {
        assert_eq!(
            sha256_hex(b"avila-core"),
            "0dc17a04db72201b542eb57667fa1f2eaddbc8a035f9b44b7dde1875c58b2f3e"
        );
    }

    #[test]
    fn new_bundle_is_non_claiming() {
        let bundle = EvidenceBundle::empty_draft("bundle-1", "contract-1", "specimen");
        assert_eq!(bundle.state, BundleState::Draft);
        assert!(bundle.records.is_empty());
    }
}
