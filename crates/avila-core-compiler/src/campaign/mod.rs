//! Campaign evaluation: admission of claims against a compiled snapshot and
//! four-state verdicts from what was admitted.
//!
//! This is the first executable slice of ADR-0006 SC-10 and SC-11. It admits
//! claims under the type-level subset of the admission conditions (A3 parents,
//! A5 presence, A6 model, shape, media, and units, and slot cardinality) and
//! derives verdicts with the kernel. It does not read artifact bytes, verify
//! receipts, signatures, or package identities, evaluate qualification or
//! policy snapshots, or invalidate anything. Optional practical-review stages
//! are outside technical evidence admission and cannot alter a verdict.

mod admission;
mod document;
mod verdicts;

use std::collections::{BTreeMap, BTreeSet};

use avila_core_kernel::{SEMANTIC_PROFILE, VerdictOutput, VerdictStatus, canonicalize_json};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::compile::registry::RegistryIndex;
use crate::compile::schema::SchemaDocument;
use crate::compile::source::read_document;
use crate::compile::{CompilerError, DocumentIdentity, compile_documents};
use crate::diagnostic::{CORE_E7001, CORE_S1102, CoreDiagnostic, FindingClass, SourceLocation};
use crate::document::{CompletionBlock, RegistrySnapshot, SourceRef};

pub use document::{
    ArtifactIdentity, CAMPAIGN_REPORT_SCHEMA_VERSION, CLAIMS_SCHEMA_VERSION, ClaimValue,
    ClaimsDocument, InputAttestation, OutputClaim, ProducerIdentity,
};

pub const CAMPAIGN_NOTICE: &str = "Campaign evaluation admits claims under the executable type-level subset of the draft admission rules and derives verdicts from admitted claims under the draft profile. Artifact bytes, execution receipts, package identities, signatures, qualification, policy snapshots, and invalidation are not checked. Optional practical review controls presentation outside this evaluator and cannot alter a technical verdict. No verdict here is scientific truth, certification, or regulatory approval.";
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
    /// SC-9 clause 6 delivery assessment — present only when the contract
    /// declares a `completion` block. Never a verdict input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<CompletionAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_sha256: Option<String>,
    pub notice: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionStatus {
    Complete,
    Incomplete,
}

/// The contract's delivery statement evaluated against derived verdicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionAssessment {
    pub status: CompletionStatus,
    pub entries: Vec<CompletionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionEntry {
    pub requirement_id: String,
    pub verdict: VerdictStatus,
    pub fulfilling: bool,
    /// Why the verdict does or does not fulfill delivery — the declared
    /// rule names the reason, not the outcome.
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRecord {
    pub evidence_id: String,
    pub source: SourceRef,
    pub state: AdmissionState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<AdmissionReason>,
    /// Present only when the evaluator was given artifact bytes: whether a
    /// supplied file's bytes actually hashed to the attested identity.
    /// Absent keeps the report — and its identity — unchanged for
    /// digest-only evaluations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactCheck>,
}

/// Byte-level evidence behind an attested artifact identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCheck {
    /// The identity the attestation declared.
    pub sha256: String,
    /// `verified` when a supplied file's bytes hashed to the declared
    /// identity; `not_checked` when no supplied file did — the attested
    /// bytes were not produced for this evaluation.
    pub check: ArtifactCheckState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCheckState {
    Verified,
    NotChecked,
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
}

/// Compiles the contract, admits the claims against the compiled snapshot,
/// and derives one verdict per requirement.
pub fn evaluate_campaign(
    contract_bytes: &[u8],
    registry_bytes: &[u8],
    claims_bytes: &[u8],
) -> Result<CampaignReport, CompilerError> {
    evaluate_campaign_with_artifacts(
        contract_bytes,
        registry_bytes,
        claims_bytes,
        &BTreeSet::new(),
    )
}

/// `evaluate_campaign` plus byte-level evidence: `artifact_digests` is the
/// set of digests the caller actually re-hashed from supplied files. Every
/// attested artifact then carries an explicit check — `verified` when its
/// declared identity was produced, `not_checked` when it was not. An empty
/// set is a digest-only evaluation and the records are unchanged.
pub fn evaluate_campaign_with_artifacts(
    contract_bytes: &[u8],
    registry_bytes: &[u8],
    claims_bytes: &[u8],
    artifact_digests: &BTreeSet<String>,
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

    let mut admissions = admission::admit(&compiled, &registry, &claims, &mut findings);
    if !artifact_digests.is_empty() {
        let attested: BTreeMap<String, &str> = claims
            .inputs
            .iter()
            .map(|attestation| {
                (
                    format!("input:{}", attestation.input_id),
                    attestation.artifact.sha256.as_str(),
                )
            })
            .chain(
                claims
                    .claims
                    .iter()
                    .map(|claim| (claim.claim_id.clone(), claim.artifact.sha256.as_str())),
            )
            .collect();
        for record in &mut admissions {
            if let Some(sha256) = attested.get(record.evidence_id.as_str()) {
                record.artifact = Some(ArtifactCheck {
                    sha256: (*sha256).to_string(),
                    check: if artifact_digests.contains(*sha256) {
                        ArtifactCheckState::Verified
                    } else {
                        ArtifactCheckState::NotChecked
                    },
                });
            }
        }
    }
    let boundary = VerdictBoundary {
        semantic_profile: SEMANTIC_PROFILE.into(),
        compiler: compiled.compiler.clone(),
        evaluator: EVALUATOR_ID.into(),
        compiled_snapshot_sha256: compiled.snapshot_sha256.clone(),
        claims_sha256: claims_sha256.clone(),
    };
    let verdicts = verdicts::evaluate(&compiled, &registry, &claims, &admissions, &boundary);
    let completion = compiled
        .completion
        .as_ref()
        .map(|block| assess_completion(block, &verdicts));
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
        #[serde(skip_serializing_if = "Option::is_none")]
        completion: Option<&'a CompletionAssessment>,
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
        completion: completion.as_ref(),
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
        completion,
        campaign_sha256: Some(campaign_sha256),
        notice: CAMPAIGN_NOTICE.into(),
    })
}

/// SC-9 clause 6: the declared delivery statement against the derived
/// verdicts. A verdict fulfills delivery when its state is declared
/// fulfilling — and for `inconclusive`, only under a permitted named
/// reason. `not_evaluated` never fulfills, whatever the block declares.
fn assess_completion(block: &CompletionBlock, verdicts: &[VerdictRecord]) -> CompletionAssessment {
    let inconclusive_fulfilling = block
        .fulfilling_verdicts
        .contains(&VerdictStatus::Inconclusive);
    let entries: Vec<CompletionEntry> = verdicts
        .iter()
        .map(|record| {
            let status = record.verdict.status;
            let (fulfilling, reason) = match status {
                VerdictStatus::NotEvaluated => (
                    false,
                    "`not_evaluated` never completes a substantive contract".to_string(),
                ),
                VerdictStatus::Inconclusive if !inconclusive_fulfilling => (
                    false,
                    "`inconclusive` is not a declared fulfilling verdict".to_string(),
                ),
                VerdictStatus::Inconclusive
                    if !block
                        .permitted_inconclusive_reasons
                        .contains(&record.verdict.rule) =>
                {
                    (
                        false,
                        format!(
                            "inconclusive reason `{}` is not permitted",
                            record.verdict.rule
                        ),
                    )
                }
                status if block.fulfilling_verdicts.contains(&status) => (
                    true,
                    format!(
                        "`{}` is a declared fulfilling verdict",
                        verdict_label(status)
                    ),
                ),
                status => (
                    false,
                    format!(
                        "`{}` is not a declared fulfilling verdict",
                        verdict_label(status)
                    ),
                ),
            };
            CompletionEntry {
                requirement_id: record.requirement_id.clone(),
                verdict: status,
                fulfilling,
                reason,
            }
        })
        .collect();
    let status = if entries.iter().all(|entry| entry.fulfilling) {
        CompletionStatus::Complete
    } else {
        CompletionStatus::Incomplete
    };
    CompletionAssessment { status, entries }
}

const fn verdict_label(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Pass => "pass",
        VerdictStatus::Fail => "fail",
        VerdictStatus::Inconclusive => "inconclusive",
        VerdictStatus::NotEvaluated => "not_evaluated",
    }
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
        completion: None,
        campaign_sha256: None,
        notice: CAMPAIGN_NOTICE.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::SourceRef;

    fn verdict_record(status: VerdictStatus, rule: &str) -> VerdictRecord {
        VerdictRecord {
            requirement_id: "req".into(),
            statement: String::new(),
            metric: SourceRef::ContractInput {
                input_id: "any".into(),
            },
            evidence_ids: Vec::new(),
            verdict: VerdictOutput {
                status,
                rule: rule.into(),
                aggregation: None,
                canonical_unit: None,
                limit_canonical: None,
                lower_canonical: None,
                upper_canonical: None,
                nominal_canonical: None,
                tolerance_canonical: None,
                coverage: None,
                basis_visible: None,
                numbers_present: None,
                observed_category: None,
                accepted_categories: None,
                reasons: Vec::new(),
                display_upper_text: None,
            },
            boundary: VerdictBoundary {
                semantic_profile: String::new(),
                compiler: String::new(),
                evaluator: String::new(),
                compiled_snapshot_sha256: String::new(),
                claims_sha256: String::new(),
            },
        }
    }

    fn block(fulfilling: &[VerdictStatus], permitted: &[&str]) -> CompletionBlock {
        CompletionBlock {
            fulfilling_verdicts: fulfilling.to_vec(),
            permitted_inconclusive_reasons: permitted
                .iter()
                .map(|reason| (*reason).into())
                .collect(),
        }
    }

    #[test]
    fn a_declared_pass_verdict_completes() {
        let assessment = assess_completion(
            &block(&[VerdictStatus::Pass], &[]),
            &[verdict_record(VerdictStatus::Pass, "bounded.le.within")],
        );
        assert_eq!(assessment.status, CompletionStatus::Complete);
        assert!(assessment.entries[0].fulfilling);
    }

    #[test]
    fn an_inconclusive_verdict_completes_only_under_a_permitted_reason() {
        let declared = block(&[VerdictStatus::Inconclusive], &["bounded.le.crossing"]);
        let permitted = assess_completion(
            &declared,
            &[verdict_record(
                VerdictStatus::Inconclusive,
                "bounded.le.crossing",
            )],
        );
        assert_eq!(permitted.status, CompletionStatus::Complete);

        let unlisted = assess_completion(
            &declared,
            &[verdict_record(
                VerdictStatus::Inconclusive,
                "bounded.le.upper_only",
            )],
        );
        assert_eq!(unlisted.status, CompletionStatus::Incomplete);
        assert!(!unlisted.entries[0].fulfilling);
    }

    #[test]
    fn an_inconclusive_verdict_never_completes_when_not_declared_fulfilling() {
        let assessment = assess_completion(
            &block(&[VerdictStatus::Pass], &["bounded.le.crossing"]),
            &[verdict_record(
                VerdictStatus::Inconclusive,
                "bounded.le.crossing",
            )],
        );
        assert_eq!(assessment.status, CompletionStatus::Incomplete);
        assert!(!assessment.entries[0].fulfilling);
    }

    #[test]
    fn not_evaluated_never_completes() {
        let assessment = assess_completion(
            &block(&[VerdictStatus::Pass], &[]),
            &[verdict_record(
                VerdictStatus::NotEvaluated,
                "not_evaluated.no_bound",
            )],
        );
        assert_eq!(assessment.status, CompletionStatus::Incomplete);
        assert!(!assessment.entries[0].fulfilling);
    }
}
