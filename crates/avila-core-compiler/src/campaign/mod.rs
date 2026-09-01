//! Campaign evaluation: admission of claims against a compiled snapshot and
//! four-state verdicts from what was admitted.
//!
//! This is the first executable slice of ADR-0006 SC-10 and SC-11. It admits
//! claims under the type-level subset of the admission conditions (A3 parents,
//! A5 presence, A6 model, shape, media, and units, and slot cardinality) and
//! derives verdicts with the kernel. It does not read artifact bytes, verify
//! receipts, signatures, or package identities, evaluate qualification or
//! policy snapshots, or invalidate anything. Review decisions are recorded as
//! unverified assertions and withhold `PASS` until present.

mod admission;
mod document;
mod verdicts;

use avila_core_kernel::{SEMANTIC_PROFILE, VerdictOutput, canonicalize_json};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::compile::registry::RegistryIndex;
use crate::compile::schema::SchemaDocument;
use crate::compile::source::read_document;
use crate::compile::{CompilerError, DocumentIdentity, compile_documents};
use crate::diagnostic::{CORE_E7001, CORE_S1102, CoreDiagnostic, FindingClass, SourceLocation};
use crate::document::{RegistrySnapshot, SourceRef};

pub use document::{
    ArtifactIdentity, Attestation, CAMPAIGN_REPORT_SCHEMA_VERSION, CLAIMS_SCHEMA_VERSION,
    ClaimValue, ClaimsDocument, InputAttestation, OutputClaim, ProducerIdentity, ReviewDecision,
    ReviewerIdentity,
};

pub const CAMPAIGN_NOTICE: &str = "Campaign evaluation admits claims under the executable type-level subset of the draft admission rules and derives verdicts from admitted claims under the draft profile. Artifact bytes, execution receipts, package identities, signatures, qualification, policy snapshots, and invalidation are not checked, and review decisions are unverified assertions. No verdict here is scientific truth, certification, or regulatory approval.";
const EVALUATOR_ID: &str = concat!("avila.core/kernel-rust@", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    Evaluated,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignReport {
    pub schema_version: String,
    pub semantic_profile: String,
    pub status: CampaignStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled_snapshot_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<CoreDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admissions: Vec<AdmissionRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verdicts: Vec<VerdictRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_sha256: Option<String>,
    pub notice: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRecord {
    pub evidence_id: String,
    pub source: SourceRef,
    pub state: AdmissionState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<AdmissionReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionState {
    Admitted,
    Quarantined,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionReason {
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictRecord {
    pub requirement_id: String,
    pub statement: String,
    pub metric: SourceRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_ids: Vec<String>,
    pub verdict: VerdictOutput,
    pub boundary: VerdictBoundary,
}

/// The conditions under which a verdict holds. It is a derivation from the
/// named records under the named rules, never an unqualified claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictBoundary {
    pub semantic_profile: String,
    pub compiler: String,
    pub evaluator: String,
    pub compiled_snapshot_sha256: String,
    pub claims_sha256: String,
    pub review_attestation: Attestation,
}

/// Compiles the contract, admits the claims against the compiled snapshot,
/// and derives one verdict per requirement.
pub fn evaluate_campaign(
    contract_bytes: &[u8],
    registry_bytes: &[u8],
    claims_bytes: &[u8],
) -> Result<CampaignReport, CompilerError> {
    let compile = compile_documents(contract_bytes, registry_bytes)?;
    let Some(compiled) = compile.compiled else {
        return Ok(rejected(compile.findings, None, None));
    };
    let mut findings = compile.findings;
    let registry: RegistrySnapshot = serde_json::from_slice(registry_bytes)
        .map_err(|error| CompilerError::Serialization(error.to_string()))?;
    let mut registry_findings = Vec::new();
    let registry = RegistryIndex::build(&registry, &mut registry_findings);
    debug_assert!(
        registry_findings.is_empty(),
        "a compiled contract has a valid registry"
    );

    let mut identities: Vec<DocumentIdentity> = Vec::new();
    let Some(claims) = read_document::<ClaimsDocument>(
        "claims",
        SchemaDocument::Claims,
        claims_bytes,
        &mut identities,
        &mut findings,
    ) else {
        return Ok(rejected(findings, Some(compiled.snapshot_sha256), None));
    };
    let claims_sha256 = identities
        .iter()
        .find(|identity| identity.document == "claims")
        .map(|identity| identity.sha256.clone())
        .expect("a parsed claims document has an identity");

    if claims.schema_version != CLAIMS_SCHEMA_VERSION || claims.semantic_profile != SEMANTIC_PROFILE
    {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            "executor",
            SourceLocation::new("claims", "/schema_version"),
            format!("claims must declare `{CLAIMS_SCHEMA_VERSION}` under `{SEMANTIC_PROFILE}`"),
        ));
        return Ok(rejected(
            findings,
            Some(compiled.snapshot_sha256),
            Some(claims_sha256),
        ));
    }
    if claims.compiled_snapshot_sha256 != compiled.snapshot_sha256 {
        findings.push(CoreDiagnostic::new(
            CORE_E7001,
            FindingClass::Inadmissible,
            "executor",
            SourceLocation::new("claims", "/compiled_snapshot_sha256"),
            format!(
                "claims were produced for `{}`, but these documents compile to `{}`",
                claims.compiled_snapshot_sha256, compiled.snapshot_sha256
            ),
        ));
        return Ok(rejected(
            findings,
            Some(compiled.snapshot_sha256),
            Some(claims_sha256),
        ));
    }

    let (admissions, decisions) = admission::admit(&compiled, &registry, &claims, &mut findings);
    let boundary = VerdictBoundary {
        semantic_profile: SEMANTIC_PROFILE.into(),
        compiler: compiled.compiler.clone(),
        evaluator: EVALUATOR_ID.into(),
        compiled_snapshot_sha256: compiled.snapshot_sha256.clone(),
        claims_sha256: claims_sha256.clone(),
        review_attestation: Attestation::Unverified,
    };
    let verdicts = verdicts::evaluate(
        &compiled,
        &registry,
        &claims,
        &admissions,
        &decisions,
        &boundary,
    );
    crate::compile::sort_findings(&mut findings);

    #[derive(Serialize)]
    struct IdentityBody<'a> {
        schema_version: &'a str,
        semantic_profile: &'a str,
        status: CampaignStatus,
        compiled_snapshot_sha256: &'a str,
        claims_sha256: &'a str,
        findings: &'a [CoreDiagnostic],
        admissions: &'a [AdmissionRecord],
        verdicts: &'a [VerdictRecord],
    }
    let body = IdentityBody {
        schema_version: CAMPAIGN_REPORT_SCHEMA_VERSION,
        semantic_profile: SEMANTIC_PROFILE,
        status: CampaignStatus::Evaluated,
        compiled_snapshot_sha256: &compiled.snapshot_sha256,
        claims_sha256: &claims_sha256,
        findings: &findings,
        admissions: &admissions,
        verdicts: &verdicts,
    };
    let bytes = serde_json::to_vec(&body)
        .map_err(|error| CompilerError::Serialization(error.to_string()))?;
    let canonical = canonicalize_json(&bytes)
        .map_err(|error| CompilerError::InternalCanonicalization(error.to_string()))?;
    let campaign_sha256 = format!("sha256:{:x}", Sha256::digest(&canonical));

    Ok(CampaignReport {
        schema_version: CAMPAIGN_REPORT_SCHEMA_VERSION.into(),
        semantic_profile: SEMANTIC_PROFILE.into(),
        status: CampaignStatus::Evaluated,
        compiled_snapshot_sha256: Some(compiled.snapshot_sha256),
        claims_sha256: Some(claims_sha256),
        findings,
        admissions,
        verdicts,
        campaign_sha256: Some(campaign_sha256),
        notice: CAMPAIGN_NOTICE.into(),
    })
}

fn rejected(
    findings: Vec<CoreDiagnostic>,
    compiled_snapshot_sha256: Option<String>,
    claims_sha256: Option<String>,
) -> CampaignReport {
    CampaignReport {
        schema_version: CAMPAIGN_REPORT_SCHEMA_VERSION.into(),
        semantic_profile: SEMANTIC_PROFILE.into(),
        status: CampaignStatus::Rejected,
        compiled_snapshot_sha256,
        claims_sha256,
        findings,
        admissions: Vec::new(),
        verdicts: Vec::new(),
        campaign_sha256: None,
        notice: CAMPAIGN_NOTICE.into(),
    }
}
