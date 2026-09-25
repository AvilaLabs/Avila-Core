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
mod context;
mod derivation;
mod document;
mod verdicts;
mod verify;

use avila_core_kernel::{SEMANTIC_PROFILE, VerdictOutput, VerdictStatus, canonicalize_json};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::compile::{Compilation, CompilerError, compile_documents};
use crate::diagnostic::{
    CORE_E7001, CORE_E7401, CORE_E7402, CORE_E7501, CORE_S1102, CoreDiagnostic, FindingClass,
    SourceLocation,
};
use crate::document::{CompletionBlock, SourceRef};

pub use context::{
    ArtifactObservations, CONTEXT_SCHEMA_VERSION, CheckedAdmissions, ContextError, ContextRecord,
    DerivedVerdicts, EvaluationContext, EvaluationError,
};
pub use derivation::{
    ChangedUse, DERIVATION_SCHEMA_VERSION, DerivationDiff, MAX_RULE_APPLICATIONS, RuleApplication,
    RulePremise, VerdictDerivation, explain_derivation_changes,
};
pub use document::{
    ArtifactIdentity, CAMPAIGN_REPORT_SCHEMA_VERSION, CLAIMS_SCHEMA_VERSION, ClaimValue,
    ClaimsDocument, InputAttestation, OutputClaim, ProducerIdentity,
};
pub use verify::{
    DerivationCheck, DerivationCheckState, DerivationVerification, verify_derivation,
};

/// The notice a digest-only evaluation reports: no artifact bytes were
/// supplied, so none were checked.
pub const CAMPAIGN_NOTICE: &str = "Campaign evaluation admits claims under the executable type-level subset of the draft admission rules and derives verdicts from admitted claims under the draft profile. Artifact bytes, execution receipts, package identities, signatures, qualification, policy snapshots, and invalidation are not checked. Optional practical review controls presentation outside this evaluator and cannot alter a technical verdict. No verdict here is scientific truth, certification, or regulatory approval.";

/// The notice when at least one artifact observation was supplied and
/// checked: the byte-check claim must be accurate, so the wording differs
/// from the digest-only `CAMPAIGN_NOTICE`.
pub const CAMPAIGN_NOTICE_OBSERVED: &str = "Campaign evaluation admits claims under the executable type-level subset of the draft admission rules and derives verdicts from admitted claims under the draft profile. Artifact bytes are checked only when supplied as observations; execution receipts, package identities, signatures, and fresh qualification assessment are outside this evaluator. Optional practical review controls presentation outside this evaluator and cannot alter a technical verdict. No verdict here is scientific truth, certification, or regulatory approval.";

/// Why a derivation or report was produced without the checks it names —
/// the notice attached to a derivation artifact.
pub const DERIVATION_NOTICE: &str = "A verdict derivation records the rule applications one evaluation performed under its bound context: which premises were checked, their states, and the conclusion each rule reached. Replaying the derivation re-runs the checks; it never trusts this file's conclusions.";

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

/// The outcome of an in-context campaign evaluation: the report, the bound
/// context, and the verdict derivation. `context`/`derivation` are absent
/// only when no context could be bound (the registry or claims bytes never
/// reached a bound state, or compilation itself rejected).
#[derive(Debug)]
pub struct CampaignEvaluation {
    report: CampaignReport,
    context: Option<EvaluationContext>,
    derivation: Option<VerdictDerivation>,
}

impl CampaignEvaluation {
    pub fn report(&self) -> &CampaignReport {
        &self.report
    }

    /// The bound context, when one was established.
    pub fn context(&self) -> Option<&EvaluationContext> {
        self.context.as_ref()
    }

    /// The derivation record, when a context existed to record under.
    pub fn derivation(&self) -> Option<&VerdictDerivation> {
        self.derivation.as_ref()
    }

    /// Consume the outcome into its campaign report.
    pub fn into_report(self) -> CampaignReport {
        self.report
    }

    /// A rejected report with no bound context.
    fn refused(report: CampaignReport) -> Self {
        Self {
            report,
            context: None,
            derivation: None,
        }
    }
}

/// Compiles the contract, admits the claims against the compiled snapshot,
/// and derives one verdict per requirement. This is the digest-only path:
/// it runs the full in-context evaluation with no supplied artifact
/// observations and returns the report.
pub fn evaluate_campaign(
    contract_bytes: &[u8],
    registry_bytes: &[u8],
    claims_bytes: &[u8],
) -> Result<CampaignReport, CompilerError> {
    evaluate_campaign_in_context(
        contract_bytes,
        registry_bytes,
        claims_bytes,
        ArtifactObservations::none(),
    )
    .map(CampaignEvaluation::into_report)
}

/// The context-bound evaluation (ADR-0026): compiles the contract, binds
/// registry, claims, policy, qualification material, and the supplied
/// artifact observations into one `EvaluationContext`, admits under it,
/// derives verdicts under it, and returns the report with the replayable
/// derivation. `observations` records only digests the caller actually
/// re-hashed via `ArtifactObservations::check_*`.
pub fn evaluate_campaign_in_context(
    contract_bytes: &[u8],
    registry_bytes: &[u8],
    claims_bytes: &[u8],
    observations: ArtifactObservations,
) -> Result<CampaignEvaluation, CompilerError> {
    let compilation = compile_documents(contract_bytes, registry_bytes)?;
    let Compilation::Compiled(checked) = compilation else {
        return Ok(CampaignEvaluation::refused(rejected(
            compilation.into_report().findings,
            None,
            None,
        )));
    };
    let compiled = checked.contract().clone();
    let mut findings = checked.report().findings.clone();

    let context = match EvaluationContext::bind(
        &compiled,
        registry_bytes,
        claims_bytes,
        observations,
    ) {
        Ok(context) => context,
        Err(ContextError::ClaimsUnreadable {
            findings: claims_findings,
        }) => {
            let mut findings = findings;
            findings.extend(claims_findings);
            return Ok(CampaignEvaluation::refused(rejected(
                findings,
                Some(compiled.snapshot_sha256().to_string()),
                None,
            )));
        }
        Err(ContextError::RegistryMismatch { expected, found }) => {
            findings.push(CoreDiagnostic::new(
                CORE_E7402,
                FindingClass::Inadmissible,
                "executor",
                SourceLocation::new("registry", "/"),
                format!(
                    "registry hashes to `{found}` but the compiled snapshot was built against `{expected}` — refusing to bind a mixed context"
                ),
            ));
            return Ok(CampaignEvaluation::refused(rejected(
                findings,
                Some(compiled.snapshot_sha256().to_string()),
                None,
            )));
        }
        Err(ContextError::RegistryUnreadable(detail)) => {
            findings.push(CoreDiagnostic::new(
                CORE_E7402,
                FindingClass::Invalid,
                "executor",
                SourceLocation::new("registry", "/"),
                format!("registry bytes could not be bound: {detail}"),
            ));
            return Ok(CampaignEvaluation::refused(rejected(
                findings,
                Some(compiled.snapshot_sha256().to_string()),
                None,
            )));
        }
    };
    let claims_sha256 = context.record().claims_sha256.clone();

    if context.claims().schema_version != CLAIMS_SCHEMA_VERSION
        || context.claims().semantic_profile != SEMANTIC_PROFILE
    {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            "executor",
            SourceLocation::new("claims", "/schema_version"),
            format!("claims must declare `{CLAIMS_SCHEMA_VERSION}` under `{SEMANTIC_PROFILE}`"),
        ));
        let refusals = vec![bind_refusal(
            &context,
            "claims_schema",
            "unsupported",
            CORE_S1102,
        )];
        return Ok(refused_evaluation(
            rejected(
                findings,
                Some(compiled.snapshot_sha256().to_string()),
                Some(claims_sha256),
            ),
            context,
            refusals,
        ));
    }
    if context.claims().compiled_snapshot_sha256 != compiled.snapshot_sha256() {
        findings.push(CoreDiagnostic::new(
            CORE_E7001,
            FindingClass::Inadmissible,
            "executor",
            SourceLocation::new("claims", "/compiled_snapshot_sha256"),
            format!(
                "claims were produced for `{}`, but these documents compile to `{}`",
                context.claims().compiled_snapshot_sha256,
                compiled.snapshot_sha256()
            ),
        ));
        let refusals = vec![bind_refusal(
            &context,
            "compiled_snapshot",
            "mismatch",
            CORE_E7001,
        )];
        return Ok(refused_evaluation(
            rejected(
                findings,
                Some(compiled.snapshot_sha256().to_string()),
                Some(claims_sha256),
            ),
            context,
            refusals,
        ));
    }

    let (admissions, mut admission_findings) = context.admit();
    findings.append(&mut admission_findings);
    let verdicts = match context.derive_verdicts(&admissions) {
        Ok(verdicts) => verdicts,
        Err(EvaluationError::ContextMismatch { expected, found }) => {
            findings.push(CoreDiagnostic::new(
                CORE_E7401,
                FindingClass::Inadmissible,
                "executor",
                SourceLocation::new("claims", "/"),
                format!(
                    "admissions were minted under context `{found}`, which does not match the bound context `{expected}`"
                ),
            ));
            let refusals = vec![bind_refusal(&context, "admissions", "mismatch", CORE_E7401)];
            return Ok(refused_evaluation(
                rejected(
                    findings,
                    Some(compiled.snapshot_sha256().to_string()),
                    Some(claims_sha256),
                ),
                context,
                refusals,
            ));
        }
    };
    let mut applications =
        Vec::with_capacity(1 + admissions.applications().len() + verdicts.applications().len());
    applications.push(bound_application(&context));
    applications.extend(admissions.applications().iter().cloned());
    applications.extend(verdicts.applications().iter().cloned());
    if applications.len() > MAX_RULE_APPLICATIONS {
        findings.push(CoreDiagnostic::new(
            CORE_E7501,
            FindingClass::Inadmissible,
            "executor",
            SourceLocation::new("derivation", "/applications"),
            format!(
                "the evaluation produced more than {MAX_RULE_APPLICATIONS} rule applications — the derivation bound was exceeded"
            ),
        ));
        let refusals = vec![bind_refusal(
            &context,
            "derivation",
            "exhausted",
            CORE_E7501,
        )];
        return Ok(refused_evaluation(
            rejected(
                findings,
                Some(compiled.snapshot_sha256().to_string()),
                Some(claims_sha256),
            ),
            context,
            refusals,
        ));
    }
    let completion = compiled
        .completion()
        .map(|block| assess_completion(block, verdicts.records()));
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
        compiled_snapshot_sha256: compiled.snapshot_sha256(),
        claims_sha256: &claims_sha256,
        findings: &findings,
        admissions: admissions.records(),
        verdicts: verdicts.records(),
        completion: completion.as_ref(),
    };
    let bytes = serde_json::to_vec(&body)
        .map_err(|error| CompilerError::Serialization(error.to_string()))?;
    let canonical = canonicalize_json(&bytes)
        .map_err(|error| CompilerError::InternalCanonicalization(error.to_string()))?;
    let campaign_sha256 = format!("sha256:{:x}", Sha256::digest(&canonical));

    let derivation = assemble_derivation(&context, applications, Some(&campaign_sha256));

    Ok(CampaignEvaluation {
        report: CampaignReport {
            schema_version: CAMPAIGN_REPORT_SCHEMA_VERSION.into(),
            semantic_profile: SEMANTIC_PROFILE.into(),
            status: CampaignStatus::Evaluated,
            compiled_snapshot_sha256: Some(compiled.snapshot_sha256().to_string()),
            claims_sha256: Some(claims_sha256),
            findings,
            admissions: admissions.records().to_vec(),
            verdicts: verdicts.records().to_vec(),
            completion,
            campaign_sha256: Some(campaign_sha256),
            notice: if context.observations().is_empty() {
                CAMPAIGN_NOTICE.into()
            } else {
                CAMPAIGN_NOTICE_OBSERVED.into()
            },
        },
        context: Some(context),
        derivation: Some(derivation),
    })
}

/// The `context.bind` application recording a successful binding — the
/// derivation's entry point naming every bound identity.
fn bound_application(context: &EvaluationContext) -> RuleApplication {
    let record = context.record();
    let mut application = RuleApplication::new("context.bind", context.context_sha256(), "bound");
    application.premises.push(RulePremise {
        kind: "compiled_snapshot".into(),
        id: record.compiled_snapshot_sha256.clone(),
        state: "bound".into(),
    });
    application.premises.push(RulePremise {
        kind: "registry".into(),
        id: format!(
            "{}@{}:{}",
            record.registry_id, record.registry_revision, record.registry_sha256
        ),
        state: "bound".into(),
    });
    application.premises.push(RulePremise {
        kind: "claims".into(),
        id: record.claims_sha256.clone(),
        state: "bound".into(),
    });
    application.premises.push(RulePremise {
        kind: "execution_policy".into(),
        id: "compiled".into(),
        state: "bound".into(),
    });
    for qualification in &record.qualifications {
        application.premises.push(RulePremise {
            kind: "qualification".into(),
            id: qualification.clone(),
            state: "bound".into(),
        });
    }
    for observed in &record.artifact_observations {
        application.premises.push(RulePremise {
            kind: "artifact_observation".into(),
            id: observed.clone(),
            state: "checked".into(),
        });
    }
    application
}

/// A refusal application for an evaluation rejected after binding — the
/// premises name which bound check failed.
fn bind_refusal(
    context: &EvaluationContext,
    premise: &str,
    state: &str,
    code: &str,
) -> RuleApplication {
    let mut application = RuleApplication::new("context.bind", context.context_sha256(), "refused");
    application.premises.push(RulePremise {
        kind: premise.into(),
        id: context.record().claims_sha256.clone(),
        state: state.into(),
    });
    application.reasons.push(code.into());
    application
}

/// A rejected report plus a derivation recording the refusal under the
/// bound context.
fn refused_evaluation(
    report: CampaignReport,
    context: EvaluationContext,
    refusals: Vec<RuleApplication>,
) -> CampaignEvaluation {
    let derivation = assemble_derivation(&context, refusals, None);
    CampaignEvaluation {
        report,
        context: Some(context),
        derivation: Some(derivation),
    }
}

/// Assemble the derivation document and stamp its canonical identity.
fn assemble_derivation(
    context: &EvaluationContext,
    applications: Vec<RuleApplication>,
    campaign_sha256: Option<&str>,
) -> VerdictDerivation {
    let mut derivation = VerdictDerivation {
        schema_version: DERIVATION_SCHEMA_VERSION.into(),
        semantic_profile: SEMANTIC_PROFILE.into(),
        evaluator: EVALUATOR_ID.into(),
        context_sha256: context.context_sha256().to_string(),
        context: context.record().clone(),
        applications,
        campaign_sha256: campaign_sha256.map(str::to_string),
        derivation_sha256: None,
        notice: DERIVATION_NOTICE.into(),
    };
    derivation.derivation_sha256 = derivation
        .recompute_identity()
        .map(Some)
        .unwrap_or_default();
    derivation
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

    #[test]
    fn error_disclosures_do_not_exist_on_a_claim() {
        // ADR-0006 clause 9: the receipt discloses representation and
        // numerical error; the claims vocabulary cannot name either, so
        // nothing carries them toward the kernel — recorded, never combined.
        let claim = serde_json::json!({
            "claim_id": "c",
            "step_id": "s",
            "output_slot": "out",
            "artifact": {"sha256": format!("sha256:{}", "a".repeat(64)), "media_type": "m"},
            "claim": {"model": "unquantified"},
            "representation_error": {"value": "1", "unit": "1"},
        });
        assert!(serde_json::from_value::<OutputClaim>(claim).is_err());
        let claim = serde_json::json!({
            "claim_id": "c",
            "step_id": "s",
            "output_slot": "out",
            "artifact": {"sha256": format!("sha256:{}", "a".repeat(64)), "media_type": "m"},
            "claim": {"model": "unquantified"},
            "numerical_error": {"value": "1", "unit": "1"},
        });
        assert!(serde_json::from_value::<OutputClaim>(claim).is_err());
    }
}
