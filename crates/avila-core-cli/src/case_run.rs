use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::{
    AdmissionState, CampaignReport, CampaignStatus, ClaimsDocument, CompilationStatus,
    CompileReport, CompiledContract, compile_documents, evaluate_campaign,
};
use avila_core_evidence::{
    CasePackageManifest, IntegrityCheckState, PackageIntegrityReport, PackageIntegrityStatus,
    VerifiedCasePackage, verify_case_package,
};
use avila_core_kernel::VerdictStatus;
use serde::Serialize;

const CASE_RUN_REPORT_SCHEMA_VERSION: &str = "avila.core/case-run-report/v0.1-draft";
const CASE_RUN_NOTICE: &str = "This workflow separates byte-integrity checks from semantic compilation and campaign evaluation. Re-hashing bytes proves identity only; structural admission and a Core verdict do not establish scientific correctness, qualification, certification, or regulatory approval.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseRunStatus {
    Evaluated,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    Verified,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReport {
    pub status: BindingStatus,
    pub evidence_records: usize,
    pub bound_evidence_records: usize,
    pub required_review_policies: usize,
    pub bound_review_policies: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayReport {
    pub document_id: String,
    pub matches: bool,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseRunReport {
    pub schema_version: String,
    pub case_id: String,
    pub title: String,
    pub status: CaseRunStatus,
    pub integrity: PackageIntegrityReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bindings: Option<BindingReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile: Option<CompileReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign: Option<CampaignReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplayReport>,
    pub notice: String,
}

impl CaseRunReport {
    pub fn succeeded(&self) -> bool {
        self.status == CaseRunStatus::Evaluated
    }
}

pub fn parse_source_roots(values: &[String]) -> Result<BTreeMap<String, PathBuf>, Box<dyn Error>> {
    let mut roots = BTreeMap::new();
    for value in values {
        let Some((name, path)) = value.split_once('=') else {
            return Err(format!(
                "source root `{value}` must have the form NAME=PATH (for example, aftermatter=../project-aftermatter)"
            )
            .into());
        };
        if name.is_empty() || path.is_empty() {
            return Err(
                format!("source root `{value}` must contain a non-empty name and path").into(),
            );
        }
        if roots.insert(name.into(), PathBuf::from(path)).is_some() {
            return Err(format!("source root `{name}` was supplied more than once").into());
        }
    }
    Ok(roots)
}

pub fn execute_case(
    case_or_manifest: &Path,
    source_roots: &BTreeMap<String, PathBuf>,
) -> Result<CaseRunReport, Box<dyn Error>> {
    let manifest_path = if case_or_manifest.is_dir() {
        case_or_manifest.join("package.json")
    } else {
        case_or_manifest.to_path_buf()
    };
    let manifest_bytes = fs::read(&manifest_path)?;
    let package_root = manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let package = verify_case_package(&manifest_bytes, package_root, source_roots)?;

    let mut report = CaseRunReport {
        schema_version: CASE_RUN_REPORT_SCHEMA_VERSION.into(),
        case_id: package.manifest.case_id.clone(),
        title: package.manifest.title.clone(),
        status: CaseRunStatus::Rejected,
        integrity: package.integrity.clone(),
        bindings: None,
        compile: None,
        campaign: None,
        replay: None,
        notice: CASE_RUN_NOTICE.into(),
    };

    // A missing root is an explicit partial check. A supplied-but-missing or
    // different artifact is a failed integrity gate and compilation stops.
    if package.integrity.status == PackageIntegrityStatus::Failed {
        return Ok(report);
    }

    let contract = required_document(&package, "contract")?;
    let registry = required_document(&package, "registry")?;
    let claims_bytes = required_document(&package, "claims")?;

    let compile = compile_documents(contract, registry)?;
    if compile.status == CompilationStatus::Rejected {
        report.compile = Some(compile);
        return Ok(report);
    }
    let compiled = compile
        .compiled
        .as_ref()
        .ok_or("compiler reported `compiled` without a compiled snapshot")?;
    let claims: ClaimsDocument = serde_json::from_slice(claims_bytes)?;
    let bindings = verify_bindings(&package.manifest, &claims, compiled);
    let bindings_failed = bindings.status == BindingStatus::Failed;
    report.bindings = Some(bindings);
    report.compile = Some(compile);
    if bindings_failed {
        return Ok(report);
    }

    let campaign = evaluate_campaign(contract, registry, claims_bytes)?;
    let campaign_rejected = campaign.status == CampaignStatus::Rejected;
    report.replay = replay_expected(&package, &campaign)?;
    let replay_failed = report.replay.as_ref().is_some_and(|replay| !replay.matches);
    report.campaign = Some(campaign);
    if !campaign_rejected && !replay_failed {
        report.status = CaseRunStatus::Evaluated;
    }
    Ok(report)
}

fn required_document<'a>(
    package: &'a VerifiedCasePackage,
    role: &str,
) -> Result<&'a [u8], Box<dyn Error>> {
    package
        .document_by_role(role)
        .ok_or_else(|| format!("verified package has no readable `{role}` document").into())
}

fn verify_bindings(
    manifest: &CasePackageManifest,
    claims: &ClaimsDocument,
    compiled: &CompiledContract,
) -> BindingReport {
    let mut issues = Vec::new();
    let mut expected = BTreeMap::<String, String>::new();
    for input in &claims.inputs {
        insert_evidence(
            &mut expected,
            format!("input:{}", input.input_id),
            input.artifact.sha256.clone(),
            &mut issues,
        );
    }
    for claim in &claims.claims {
        insert_evidence(
            &mut expected,
            claim.claim_id.clone(),
            claim.artifact.sha256.clone(),
            &mut issues,
        );
    }

    let mut seen = BTreeSet::new();
    let mut bound_evidence_records = 0;
    for artifact in &manifest.artifacts {
        for evidence_id in &artifact.evidence_ids {
            let Some(expected_sha256) = expected.get(evidence_id) else {
                issues.push(format!(
                    "artifact `{}` binds unknown evidence record `{evidence_id}`",
                    artifact.artifact_id
                ));
                continue;
            };
            seen.insert(evidence_id.clone());
            if expected_sha256 == &artifact.sha256 {
                bound_evidence_records += 1;
            } else {
                issues.push(format!(
                    "artifact `{}` digest does not match evidence record `{evidence_id}`",
                    artifact.artifact_id
                ));
            }
        }
    }
    for evidence_id in expected.keys() {
        if !seen.contains(evidence_id) {
            issues.push(format!(
                "evidence record `{evidence_id}` has no package artifact binding"
            ));
        }
    }

    let required_policies: BTreeSet<String> = compiled
        .workflow
        .iter()
        .filter_map(|step| step.review_obligation.as_ref())
        .map(|review| review.reviewer_eligibility_policy.sha256.clone())
        .collect();
    let package_policies: BTreeSet<String> = manifest
        .documents
        .iter()
        .filter(|document| document.role == "review_policy")
        .map(|document| document.sha256.clone())
        .collect();
    let bound_review_policies = required_policies.intersection(&package_policies).count();
    for digest in required_policies.difference(&package_policies) {
        issues.push(format!(
            "compiled review obligation requires policy `{digest}`, but the package does not contain it"
        ));
    }
    for digest in package_policies.difference(&required_policies) {
        issues.push(format!(
            "package review policy `{digest}` is not referenced by the compiled contract"
        ));
    }

    BindingReport {
        status: if issues.is_empty() {
            BindingStatus::Verified
        } else {
            BindingStatus::Failed
        },
        evidence_records: expected.len(),
        bound_evidence_records,
        required_review_policies: required_policies.len(),
        bound_review_policies,
        issues,
    }
}

fn insert_evidence(
    expected: &mut BTreeMap<String, String>,
    evidence_id: String,
    sha256: String,
    issues: &mut Vec<String>,
) {
    if expected.insert(evidence_id.clone(), sha256).is_some() {
        issues.push(format!(
            "claims document repeats evidence record `{evidence_id}`"
        ));
    }
}

fn replay_expected(
    package: &VerifiedCasePackage,
    campaign: &CampaignReport,
) -> Result<Option<ReplayReport>, Box<dyn Error>> {
    let Some(document) = package
        .manifest
        .documents
        .iter()
        .find(|document| document.role == "expected_campaign_report")
    else {
        return Ok(None);
    };
    let bytes = package
        .document_by_id(&document.document_id)
        .ok_or("expected campaign report was not readable after package verification")?;
    let expected: serde_json::Value = serde_json::from_slice(bytes)?;
    let actual = serde_json::to_value(campaign)?;
    Ok(Some(ReplayReport {
        document_id: document.document_id.clone(),
        matches: expected == actual,
    }))
}

pub fn human_summary(report: &CaseRunReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Avila Core case workflow");
    let _ = writeln!(out, "{} — {}", report.case_id, report.title);

    let verified_documents = report
        .integrity
        .documents
        .iter()
        .filter(|check| check.state == IntegrityCheckState::Verified)
        .count();
    let verified_artifacts: Vec<_> = report
        .integrity
        .artifacts
        .iter()
        .filter(|check| check.state == IntegrityCheckState::Verified)
        .collect();
    let verified_evidence: usize = verified_artifacts
        .iter()
        .map(|check| check.evidence_ids.len())
        .sum();
    let total_evidence: usize = report
        .integrity
        .artifacts
        .iter()
        .map(|check| check.evidence_ids.len())
        .sum();
    let unchecked_roots: BTreeSet<_> = report
        .integrity
        .artifacts
        .iter()
        .filter(|check| check.state == IntegrityCheckState::NotChecked)
        .map(|check| check.source_root.as_str())
        .collect();

    let _ = writeln!(out, "\n1. PACKAGE INTEGRITY");
    let _ = writeln!(
        out,
        "   [{}] {verified_documents}/{} package documents re-hashed",
        if verified_documents == report.integrity.documents.len() {
            "VERIFIED"
        } else {
            "FAILED"
        },
        report.integrity.documents.len()
    );
    let _ = writeln!(
        out,
        "   [{}] {}/{} external artifacts re-hashed ({verified_evidence}/{total_evidence} evidence records)",
        integrity_label(report.integrity.status),
        verified_artifacts.len(),
        report.integrity.artifacts.len()
    );
    if !unchecked_roots.is_empty() {
        let _ = writeln!(
            out,
            "   not checked: source root(s) {}",
            unchecked_roots.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    for check in report.integrity.documents.iter().filter(|check| {
        matches!(
            check.state,
            IntegrityCheckState::Missing | IntegrityCheckState::Mismatch
        )
    }) {
        let _ = writeln!(out, "   {:?}: {}", check.state, check.path);
    }
    for check in report.integrity.artifacts.iter().filter(|check| {
        matches!(
            check.state,
            IntegrityCheckState::Missing | IntegrityCheckState::Mismatch
        )
    }) {
        let _ = writeln!(
            out,
            "   {:?}: {}:{}",
            check.state, check.source_root, check.path
        );
    }

    let _ = writeln!(out, "\n2. IDENTITY BINDING");
    match &report.bindings {
        Some(bindings) => {
            let _ = writeln!(
                out,
                "   [{}] {}/{} evidence identities; {}/{} review-policy identities",
                binding_label(bindings.status),
                bindings.bound_evidence_records,
                bindings.evidence_records,
                bindings.bound_review_policies,
                bindings.required_review_policies
            );
            for issue in &bindings.issues {
                let _ = writeln!(out, "   issue: {issue}");
            }
        }
        None => {
            let _ = writeln!(out, "   [NOT RUN] package integrity did not pass its gate");
        }
    }

    let _ = writeln!(out, "\n3. COMPILE");
    match report.compile.as_ref() {
        Some(compile) => match compile.compiled.as_ref() {
            Some(compiled) => {
                let _ = writeln!(
                    out,
                    "   [COMPILED] {} revision {}",
                    compiled.contract_id, compiled.contract_revision
                );
                for (index, step) in compiled.workflow.iter().enumerate() {
                    let connector = if index + 1 == compiled.workflow.len() {
                        "└─"
                    } else {
                        "├─"
                    };
                    let _ = writeln!(
                        out,
                        "   {connector} {} [{}@{}]",
                        step.step_id, step.capability_type.id, step.capability_type.major
                    );
                }
                let _ = writeln!(
                    out,
                    "   {} requirement(s); snapshot {}",
                    compiled.requirements.len(),
                    compiled.snapshot_sha256
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "   [REJECTED] {} compiler finding(s)",
                    compile.findings.len()
                );
            }
        },
        None => {
            let _ = writeln!(out, "   [NOT RUN]");
        }
    }

    let _ = writeln!(out, "\n4. EVALUATE");
    match report.campaign.as_ref() {
        Some(campaign) => {
            let admitted = campaign
                .admissions
                .iter()
                .filter(|record| record.state == AdmissionState::Admitted)
                .count();
            let _ = writeln!(
                out,
                "   [EVALUATED] {admitted}/{} evidence records structurally admitted",
                campaign.admissions.len()
            );
            for verdict in &campaign.verdicts {
                let _ = writeln!(
                    out,
                    "   [{}] {} — {}",
                    verdict_label(verdict.verdict.status),
                    verdict.requirement_id,
                    verdict.verdict.rule
                );
            }
            if let Some(identity) = &campaign.campaign_sha256 {
                let _ = writeln!(out, "   campaign {identity}");
            }
        }
        None => {
            let _ = writeln!(out, "   [NOT RUN]");
        }
    }

    if let Some(replay) = &report.replay {
        let _ = writeln!(out, "\n5. REPLAY");
        let _ = writeln!(
            out,
            "   [{}] generated campaign report {} committed expectation",
            if replay.matches { "MATCH" } else { "MISMATCH" },
            if replay.matches {
                "matches"
            } else {
                "differs from"
            }
        );
    }

    let _ = writeln!(
        out,
        "\nOutcome: {}. Byte integrity is {}; {}",
        match report.status {
            CaseRunStatus::Evaluated => "workflow evaluated",
            CaseRunStatus::Rejected => "workflow rejected",
        },
        match report.integrity.status {
            PackageIntegrityStatus::Complete => "complete for every declared artifact",
            PackageIntegrityStatus::Partial =>
                "partial because at least one source root was not supplied",
            PackageIntegrityStatus::Failed => "failed",
        },
        if report.campaign.is_some() {
            "requirement results remain exactly as reported above."
        } else {
            "no requirement result was produced."
        },
    );
    out
}

fn integrity_label(status: PackageIntegrityStatus) -> &'static str {
    match status {
        PackageIntegrityStatus::Complete => "COMPLETE",
        PackageIntegrityStatus::Partial => "PARTIAL",
        PackageIntegrityStatus::Failed => "FAILED",
    }
}

fn binding_label(status: BindingStatus) -> &'static str {
    match status {
        BindingStatus::Verified => "VERIFIED",
        BindingStatus::Failed => "FAILED",
    }
}

fn verdict_label(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Pass => "PASS",
        VerdictStatus::Fail => "FAIL",
        VerdictStatus::Inconclusive => "INCONCLUSIVE",
        VerdictStatus::NotEvaluated => "NOT_EVALUATED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case_000() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-000-actinv-aftermatter")
    }

    #[test]
    fn case_000_runs_without_external_roots_and_names_every_gap() {
        let report = execute_case(&case_000(), &BTreeMap::new()).unwrap();
        assert_eq!(report.status, CaseRunStatus::Evaluated);
        assert_eq!(report.integrity.status, PackageIntegrityStatus::Partial);
        assert!(
            report
                .integrity
                .documents
                .iter()
                .all(|check| { check.state == IntegrityCheckState::Verified })
        );
        assert!(
            report
                .integrity
                .artifacts
                .iter()
                .all(|check| { check.state == IntegrityCheckState::NotChecked })
        );
        let bindings = report.bindings.as_ref().unwrap();
        assert_eq!(bindings.status, BindingStatus::Verified);
        assert_eq!(bindings.bound_evidence_records, 16);
        assert_eq!(bindings.bound_review_policies, 1);
        assert_eq!(report.campaign.as_ref().unwrap().verdicts.len(), 2);
        assert!(report.replay.as_ref().unwrap().matches);

        let summary = human_summary(&report);
        assert!(summary.contains("activation [actinv.activation-inventory@1]"));
        assert!(summary.contains("CASE-000-R1 — not_evaluated.review_pending"));
        assert!(summary.contains("source root(s) actinv-data, aftermatter"));
    }
}
