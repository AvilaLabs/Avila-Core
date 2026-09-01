//! The evidence-claims document: what executing a compiled contract produced.

use serde::{Deserialize, Serialize};

use crate::document::{QuantityValue, ReviewDisposition};

pub const CLAIMS_SCHEMA_VERSION: &str = "avila.core/evidence-claims/v0.2-draft";
pub const CAMPAIGN_REPORT_SCHEMA_VERSION: &str = "avila.core/campaign-report/v0.2-draft";

/// Claims produced for exactly one compiled snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimsDocument {
    pub schema_version: String,
    pub semantic_profile: String,
    pub compiled_snapshot_sha256: String,
    #[serde(default)]
    pub inputs: Vec<InputAttestation>,
    #[serde(default)]
    pub claims: Vec<OutputClaim>,
    #[serde(default)]
    pub decisions: Vec<ReviewDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputAttestation {
    pub input_id: String,
    pub artifact: ArtifactIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub sha256: String,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputClaim {
    pub claim_id: String,
    pub step_id: String,
    pub output_slot: String,
    pub artifact: ArtifactIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<ProducerIdentity>,
    pub claim: ClaimValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerIdentity {
    pub package_id: String,
    pub sha256: String,
}

/// The admitted uncertainty claim of one output, in the SC-3 models the
/// kernel can reduce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClaimValue {
    Exact {
        nominal: QuantityValue,
    },
    Interval {
        lower: QuantityValue,
        upper: QuantityValue,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nominal: Option<QuantityValue>,
    },
    CoverageInterval {
        lower: QuantityValue,
        upper: QuantityValue,
        nominal: QuantityValue,
        coverage: String,
    },
    WorstCase {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lower: Option<QuantityValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        upper: Option<QuantityValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nominal: Option<QuantityValue>,
    },
    Unquantified {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nominal: Option<QuantityValue>,
    },
}

/// An asserted review decision. Nothing about it is signed or verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecision {
    pub step_id: String,
    pub disposition: ReviewDisposition,
    pub rationale: String,
    pub reviewer: ReviewerIdentity,
    pub attestation: Attestation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewerIdentity {
    pub identity: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attestation {
    Unverified,
}
