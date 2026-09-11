//! Human rendering of a `CaseRunReport`: the concise text view `avila-core
//! run` prints by default, and the small number-, label-, and
//! coverage-formatting helpers behind it. Nothing here changes a finding, a
//! verdict, or any value the JSON report carries; it only decides how that
//! already-decided content reads for a person or an iterating agent.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use avila_core_compiler::{
    AdmissionState, BasisKind, CoverageReport, CoverageState, CoverageStatus, EnvelopeState,
};
use avila_core_evidence::{
    IntegrityCheckState, OutputState, PackageIntegrityStatus, ReceiptCheckState,
};
use avila_core_kernel::{ExactNumber, TruthValue, VerdictStatus};
use serde_json::Value;

use super::compare::write_attempt_comparison;
use super::{
    BindingStatus, CaseRunReport, CaseRunStatus, ExecutionStatus, PresentationGateReadiness,
    StepExecutionState,
};
use crate::attempt::AttemptChange;
use crate::diagnostic::RunStage;
use crate::execute::claims::canonical_decimal;

/// Renders an exact canonical value for people: the terminating decimal when
/// it is short, otherwise a value rounded to four decimal places and marked
/// approximate. The report keeps the exact value; only the summary rounds.
pub fn display_number(text: &str) -> String {
    let Ok(value) = ExactNumber::from_canonical(text) else {
        return text.to_string();
    };
    if let Ok(decimal) = canonical_decimal(&value)
        && decimal.len() <= 12
    {
        return decimal;
    }
    let Ok(scaled) = value
        .checked_mul_integer(10_000)
        .and_then(|scaled| scaled.round_half_even_integer())
    else {
        return text.to_string();
    };
    let sign = if scaled < 0 { "-" } else { "" };
    let magnitude = scaled.unsigned_abs();
    format!("~{sign}{}.{:04}", magnitude / 10_000, magnitude % 10_000)
}

pub(crate) fn display_signed_number(text: &str) -> String {
    let rendered = display_comparison_number(text);
    let Ok(value) = ExactNumber::from_canonical(text) else {
        return rendered;
    };
    if !value.is_positive() {
        return rendered;
    }
    rendered
        .strip_prefix('~')
        .map_or_else(|| format!("+{rendered}"), |value| format!("~+{value}"))
}

pub(crate) fn display_comparison_number(text: &str) -> String {
    let rendered = display_number(text);
    let Ok(value) = ExactNumber::from_canonical(text) else {
        return rendered;
    };
    if !value.is_zero() && matches!(rendered.as_str(), "~0.0000" | "~-0.0000") {
        text.to_string()
    } else {
        rendered
    }
}

pub(crate) fn display_unit(unit: Option<&str>) -> String {
    unit.filter(|unit| !unit.is_empty())
        .map_or_else(String::new, |unit| format!(" {unit}"))
}

/// Append one line describing this run to the campaign log.
fn write_coverage_summary(out: &mut String, coverage: &CoverageReport) {
    let label = match coverage.status {
        CoverageStatus::Complete => "COMPLETE",
        CoverageStatus::Incomplete => "INCOMPLETE",
    };
    let _ = writeln!(
        out,
        "   coverage of requirement set {} revision {} ({}): [{label}] {} covered, {} omitted with a stated reason, {} omissible, {} unstated, {} covered only on a weaker basis",
        coverage.set_id,
        coverage.set_revision,
        coverage.set_sha256,
        coverage.count(CoverageState::Covered),
        coverage.count(CoverageState::OmittedStated),
        coverage.count(CoverageState::Omissible),
        coverage.count(CoverageState::OmittedUnstated),
        coverage.count(CoverageState::CoveredUnderBasis),
    );
    for issue in &coverage.issues {
        let _ = writeln!(out, "      issue: {issue}");
    }
    for entry in &coverage.entries {
        let state = match entry.state {
            CoverageState::Covered => "COVERED",
            CoverageState::CoveredUnderBasis => "UNDER BASIS",
            CoverageState::OmittedStated => "OMITTED",
            CoverageState::Omissible => "OMISSIBLE",
            CoverageState::OmittedUnstated => "UNSTATED",
        };
        let detail = match entry.state {
            CoverageState::Covered | CoverageState::CoveredUnderBasis => entry
                .covered_by
                .iter()
                .map(|cover| {
                    format!(
                        "{} ({}{})",
                        cover.requirement_id,
                        basis_word(cover.basis),
                        if cover.adequate {
                            ""
                        } else {
                            ", below the set's minimum basis"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
            CoverageState::OmittedStated => format!(
                "{} (accepted by {})",
                entry.reason.as_deref().unwrap_or(""),
                entry.accepted_by.as_deref().unwrap_or("")
            ),
            CoverageState::Omissible => "the set permits silent omission".into(),
            CoverageState::OmittedUnstated => "no reason stated".into(),
        };
        let _ = writeln!(
            out,
            "      [{state}] {} — {detail}",
            entry.set_requirement_id
        );
        for issue in &entry.issues {
            let _ = writeln!(out, "         issue: {issue}");
        }
    }
    if !coverage.additional_requirements.is_empty() {
        let _ = writeln!(
            out,
            "      beyond the set: {}",
            coverage.additional_requirements.join(", ")
        );
    }
}

fn basis_word(basis: BasisKind) -> &'static str {
    match basis {
        BasisKind::Nominal => "nominal",
        BasisKind::Bounded => "bounded",
        BasisKind::Enclosure => "enclosure",
    }
}

pub fn human_summary(report: &CaseRunReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Avila Core case workflow");
    let _ = writeln!(out, "{} — {}", report.case_id, report.title);
    if let Some(attempt) = &report.attempt {
        let relation = attempt.parent_attempt_id.as_ref().map_or_else(
            || "root baseline".to_string(),
            |parent| format!("parent `{parent}`"),
        );
        let _ = writeln!(
            out,
            "Attempt `{}` — generation {}, {relation}; candidate `{}` {}",
            attempt.attempt_id,
            attempt.generation,
            attempt.candidate_input,
            attempt.candidate_artifact_sha256
        );
        for change in attempt.changes.iter().take(12) {
            let detail = match change {
                AttemptChange::Added { value, .. } => {
                    format!("added {}", compact_json(value))
                }
                AttemptChange::Removed { value, .. } => {
                    format!("removed {}", compact_json(value))
                }
                AttemptChange::Replaced { before, after, .. } => {
                    format!("{} -> {}", compact_json(before), compact_json(after))
                }
            };
            let _ = writeln!(out, "   change {}: {detail}", change.pointer());
        }
        if attempt.changes.len() > 12 {
            let _ = writeln!(
                out,
                "   … {} more change(s) in the JSON report",
                attempt.changes.len() - 12
            );
        }
        if let Some(comparison) = &report.attempt_comparison {
            write_attempt_comparison(&mut out, comparison);
        }
    }

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
        .filter(|check| {
            matches!(
                check.state,
                IntegrityCheckState::Verified | IntegrityCheckState::VerifiedCached
            )
        })
        .collect();
    let cached_artifacts = report
        .integrity
        .artifacts
        .iter()
        .filter(|check| check.state == IntegrityCheckState::VerifiedCached)
        .count();
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
        "   [{}] {}/{} external artifacts verified{} ({verified_evidence}/{total_evidence} evidence records)",
        integrity_label(report.integrity.status),
        verified_artifacts.len(),
        report.integrity.artifacts.len(),
        if cached_artifacts > 0 {
            format!(", {cached_artifacts} from --hash-cache")
        } else {
            String::new()
        }
    );
    if !unchecked_roots.is_empty() {
        let _ = writeln!(
            out,
            "   not checked: source root(s) {}",
            unchecked_roots.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    for input in &report.supplied_inputs {
        let _ = writeln!(
            out,
            "   supplied: input `{}` = {} {}",
            input.input_id, input.path, input.sha256
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

    let _ = writeln!(out, "\n2. COMPILE");
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
                    compiled.requirements.len() + compiled.categorical_requirements.len(),
                    compiled.snapshot_sha256
                );
                if let Some(coverage) = &report.coverage {
                    write_coverage_summary(&mut out, coverage);
                }
            }
            None => {
                let _ = writeln!(
                    out,
                    "   [REJECTED] {} compiler finding(s)",
                    compile.findings.len()
                );
                if let Some(rendered) = &report.rendered_findings {
                    for line in rendered.lines() {
                        let _ = writeln!(out, "   {line}");
                    }
                }
            }
        },
        None => {
            let _ = writeln!(out, "   [NOT RUN]");
        }
    }

    let _ = writeln!(out, "\n3. EXECUTE");
    match report.execution.as_ref() {
        Some(execution) => {
            for step in &execution.steps {
                match step.state {
                    StepExecutionState::Executed => {
                        let _ = writeln!(
                            out,
                            "   [EXECUTED] {} via {} ({}, {})",
                            step.step_id,
                            step.capability_id,
                            step.capability
                                .as_ref()
                                .map_or("?", |capability| capability.package_id.as_str()),
                            step.capability
                                .as_ref()
                                .and_then(|capability| capability.actual_sha256.as_deref())
                                .unwrap_or("?")
                        );
                        let verified_inputs = step
                            .inputs
                            .iter()
                            .filter(|input| {
                                matches!(
                                    input.integrity,
                                    IntegrityCheckState::Verified
                                        | IntegrityCheckState::VerifiedCached
                                )
                            })
                            .count();
                        if let Some(receipt) = &step.receipt {
                            let _ = writeln!(
                                out,
                                "      {verified_inputs}/{} staged inputs verified; exit {} in {} ms; {}/{} declared outputs collected",
                                step.inputs.len(),
                                receipt
                                    .exit_status
                                    .map_or("none".to_string(), |code| code.to_string()),
                                receipt.duration_ms,
                                step.outputs
                                    .iter()
                                    .filter(|output| output.state == OutputState::Collected)
                                    .count(),
                                step.outputs.len()
                            );
                        }
                        for output in &step.outputs {
                            let _ = writeln!(
                                out,
                                "      {} {}{}",
                                output.workspace_path,
                                output.sha256.as_deref().unwrap_or("missing"),
                                match output.reproduces_bound_artifact {
                                    Some(true) => " — reproduces the bound artifact",
                                    Some(false) => " — DIFFERS from the bound artifact",
                                    None => "",
                                }
                            );
                        }
                        if let Some(receipt) = &step.receipt {
                            let _ = writeln!(
                                out,
                                "      receipt {} {} [{}]{}",
                                receipt.workspace_path,
                                receipt.sha256,
                                step.verification.as_ref().map_or("NOT VERIFIED", |check| {
                                    match check.state {
                                        ReceiptCheckState::Verified => "VERIFIED",
                                        ReceiptCheckState::Failed => "FAILED",
                                    }
                                }),
                                match &step.replay {
                                    Some(replay) if replay.matches =>
                                        "; [MATCH] committed receipt".to_string(),
                                    Some(replay) => format!(
                                        "; [DRIFT] committed receipt: {}",
                                        replay.differences.join("; ")
                                    ),
                                    None => String::new(),
                                }
                            );
                        }
                        if !step.absent_slots.is_empty() {
                            let _ = writeln!(
                                out,
                                "      absent optional claim(s): {}",
                                step.absent_slots.join(", ")
                            );
                        }
                    }
                    StepExecutionState::Reused => {
                        let _ = writeln!(
                            out,
                            "   [REUSED] {} — committed receipt {} matches the planned invocation {}; {} output(s) verified at their bound identities; nothing ran",
                            step.step_id,
                            step.receipt
                                .as_ref()
                                .map_or("?", |receipt| receipt.workspace_path.as_str()),
                            step.planned_invocation_sha256.as_deref().unwrap_or("?"),
                            step.outputs.len()
                        );
                        if !step.absent_slots.is_empty() {
                            let _ = writeln!(
                                out,
                                "      absent optional claim(s): {}",
                                step.absent_slots.join(", ")
                            );
                        }
                    }
                    StepExecutionState::Planned => {
                        let _ = writeln!(
                            out,
                            "   [PLANNED] {} would execute (invocation {})",
                            step.step_id,
                            step.planned_invocation_sha256.as_deref().unwrap_or("?")
                        );
                    }
                    StepExecutionState::NotRun => {
                        if report.invalidated_steps.contains(&step.step_id) {
                            let _ = writeln!(
                                out,
                                "   [NOT RUN] {} — capability `{}` not supplied; a supplied input reaches this step, so its committed claims describe the reference input and are not carried",
                                step.step_id, step.capability_id
                            );
                        } else {
                            let _ = writeln!(
                                out,
                                "   [NOT RUN] {} — capability `{}` not supplied; its committed claims are evaluated as recorded attestations",
                                step.step_id, step.capability_id
                            );
                        }
                    }
                    StepExecutionState::Refused => {
                        let _ = writeln!(out, "   [REFUSED] {}", step.step_id);
                    }
                    StepExecutionState::Failed => {
                        let _ = writeln!(out, "   [FAILED] {}", step.step_id);
                    }
                }
                if !step.changes.is_empty()
                    && matches!(
                        step.state,
                        StepExecutionState::Executed
                            | StepExecutionState::Planned
                            | StepExecutionState::NotRun
                    )
                {
                    let _ = writeln!(
                        out,
                        "      {}: {}",
                        if step.state == StepExecutionState::Executed {
                            "rerun because"
                        } else {
                            "would rerun because"
                        },
                        step.changes
                            .iter()
                            .map(|change| format!("{:?}: {}", change.class, change.detail))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
                if let Some(assessment) = &step.qualification {
                    let state = match assessment.state {
                        EnvelopeState::Inside => "INSIDE",
                        EnvelopeState::Outside => "OUTSIDE",
                        EnvelopeState::Unknown => "UNKNOWN",
                    };
                    let failed: Vec<String> = assessment
                        .terms
                        .iter()
                        .filter(|term| term.result != TruthValue::True)
                        .map(|term| format!("{} -> {:?}", term.predicate, term.result))
                        .collect();
                    let _ = writeln!(
                        out,
                        "      envelope {} rev {}: [{state}] {}/{} terms hold{}",
                        assessment.qualification_id,
                        assessment.revision,
                        assessment.terms.len() - failed.len(),
                        assessment.terms.len(),
                        if failed.is_empty() {
                            String::new()
                        } else {
                            format!("; {}", failed.join("; "))
                        }
                    );
                    for issue in &assessment.issues {
                        let _ = writeln!(out, "         issue: {issue}");
                    }
                }
                for finding in &step.findings {
                    let _ = writeln!(
                        out,
                        "      [{}] {}\n         next: {}",
                        finding.code, finding.message, finding.next_action
                    );
                }
            }
            if !execution.not_executed.is_empty() {
                let _ = writeln!(
                    out,
                    "   not executed: {}",
                    execution
                        .not_executed
                        .iter()
                        .map(|step| format!("{} ({})", step.step_id, step.reason))
                        .collect::<Vec<_>>()
                        .join("; ")
                );
            }
            if let Some(workspace) = &execution.workspace {
                let _ = writeln!(out, "   workspace {workspace}");
            }
        }
        None => {
            let _ = writeln!(
                out,
                "   [NOT RUN] {}",
                if report
                    .compile
                    .as_ref()
                    .is_some_and(|c| c.compiled.is_some())
                {
                    "the package declares no execution"
                } else {
                    "compilation did not pass its gate"
                }
            );
        }
    }

    let _ = writeln!(out, "\n4. CLAIMS");
    match (&report.claims, &report.bindings) {
        (Some(claims), bindings) => {
            let _ = writeln!(
                out,
                "   [GENERATED] {} input attestations from package identities; {} claims from executed outputs; {} from reused outputs; {} recorded claims carried",
                claims.input_attestations,
                claims.executed_claims,
                claims.reused_claims,
                claims.recorded_claims
            );
            for claim in &claims.evidence_claims {
                if let (Some(slot), Some(value)) = (
                    claim.get("output_slot").and_then(Value::as_str),
                    claim.pointer("/claim/value").and_then(Value::as_str),
                ) {
                    let _ = writeln!(out, "   category {slot}: {value}");
                }
            }
            if claims.invalidated_claims > 0 {
                let _ = writeln!(
                    out,
                    "   not carried: {} recorded claim(s) for step(s) reached by a supplied input ({})",
                    claims.invalidated_claims,
                    report.invalidated_steps.join(", ")
                );
            }
            if report.replay_applicable {
                let _ = writeln!(
                    out,
                    "   [{}] generated claims {} committed claims.json",
                    if claims.matches_committed {
                        "MATCH"
                    } else {
                        "MISMATCH"
                    },
                    if claims.matches_committed {
                        "match"
                    } else {
                        "differ from"
                    }
                );
            } else {
                let _ = writeln!(
                    out,
                    "   [NOT APPLICABLE] committed claims describe the reference candidate, not the supplied input(s)"
                );
            }
            match bindings {
                Some(bindings) => {
                    let _ = writeln!(
                        out,
                        "   [{}] {}/{} evidence identities bound{}; {}/{} presentation-policy identities",
                        binding_label(bindings.status),
                        bindings.bound_evidence_records,
                        bindings.evidence_records,
                        match (
                            bindings.receipted_evidence_records,
                            bindings.withheld_evidence_records
                        ) {
                            (0, 0) => String::new(),
                            (receipted, 0) => {
                                format!(
                                    " ({receipted} carried by receipt for supplied-input steps)"
                                )
                            }
                            (0, withheld) => {
                                format!(
                                    " ({withheld} withheld: reached by a supplied input and not run)"
                                )
                            }
                            (receipted, withheld) => format!(
                                " ({receipted} carried by receipt for supplied-input steps; {withheld} withheld, not run)"
                            ),
                        },
                        bindings.bound_presentation_policies,
                        bindings.required_presentation_policies
                    );
                    for issue in &bindings.issues {
                        let _ = writeln!(out, "   issue: {issue}");
                    }
                }
                None => {
                    let _ = writeln!(out, "   [NOT BOUND]");
                }
            }
        }
        (None, _) if report.status == CaseRunStatus::Planned => {
            let _ = writeln!(out, "   [NOT RUN] plan only");
        }
        (None, _) => {
            let _ = writeln!(out, "   [NOT RUN] an earlier gate did not pass");
        }
    }

    let _ = writeln!(out, "\n5. EVALUATE");
    match report.campaign.as_ref() {
        None if report.status == CaseRunStatus::Planned => {
            let _ = writeln!(out, "   [NOT RUN] plan only");
        }
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
                let margin = report
                    .margins
                    .iter()
                    .find(|margin| margin.requirement_id == verdict.requirement_id);
                let numbers = margin.map_or(String::new(), |margin| {
                    let unit = margin.unit.as_deref().unwrap_or("");
                    let mut parts = Vec::new();
                    if let (Some(lower), Some(upper)) = (&margin.lower, &margin.upper) {
                        parts.push(format!(
                            "[{}, {}] {unit}",
                            display_number(lower),
                            display_number(upper)
                        ));
                    } else if let Some(nominal) = &margin.nominal {
                        parts.push(format!("nominal {} {unit}", display_number(nominal)));
                    }
                    if let Some(limit) = &margin.limit {
                        parts.push(format!("limit {} {unit}", display_number(limit)));
                    }
                    if let Some(value) = &margin.margin {
                        parts.push(format!("margin {} {unit}", display_number(value)));
                    }
                    if let Some(value) = &margin.observed_category {
                        parts.push(format!("observed {value}"));
                    }
                    if let Some(values) = &margin.accepted_categories {
                        parts.push(format!("accepted {}", values.join(" | ")));
                    }
                    if parts.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", parts.join("; "))
                    }
                });
                let _ = writeln!(
                    out,
                    "   [{}] {} — {}{numbers}",
                    verdict_label(verdict.verdict.status),
                    verdict.requirement_id,
                    verdict.verdict.rule
                );
                let reasons: Vec<String> = verdict
                    .verdict
                    .reasons
                    .iter()
                    .filter_map(|reason| match reason {
                        avila_core_kernel::VerdictReason::EvidenceState { evidence_id, state } => {
                            Some(format!("{evidence_id}: {state}"))
                        }
                        avila_core_kernel::VerdictReason::CodeOwner { code, owner } => {
                            Some(format!("{code} (owner {owner})"))
                        }
                        _ => None,
                    })
                    .collect();
                if verdict.verdict.status == VerdictStatus::NotEvaluated && !reasons.is_empty() {
                    let _ = writeln!(out, "      because: {}", reasons.join("; "));
                }
            }
            if let Some(identity) = &campaign.campaign_sha256 {
                let _ = writeln!(out, "   campaign {identity}");
            }
        }
        None => {
            let _ = writeln!(out, "   [NOT RUN]");
        }
    }

    if !report.presentation_gates.is_empty() {
        let _ = writeln!(out, "\n6. OPTIONAL PRESENTATION");
        for gate in &report.presentation_gates {
            let state = match gate.readiness {
                PresentationGateReadiness::ReadyForAgent => "READY FOR AGENT",
                PresentationGateReadiness::AwaitingEvidence => "AWAITING EVIDENCE",
            };
            let _ = writeln!(
                out,
                "   [{state}] {} — optional agent practicality gate; {}/{} dossier artifacts present; request {}",
                gate.step_id,
                gate.presented_evidence.len(),
                gate.presented_evidence.len() + gate.missing_evidence.len(),
                gate.request_sha256,
            );
            for instruction in &gate.instructions {
                let _ = writeln!(out, "      instruction: {instruction}");
            }
            for source in &gate.missing_evidence {
                let _ = writeln!(out, "      missing: {}", source.label());
            }
        }
    }

    let replay_section = if report.presentation_gates.is_empty() {
        6
    } else {
        7
    };
    if !report.replay_applicable && report.campaign.is_some() {
        let _ = writeln!(out, "\n{replay_section}. REPLAY");
        let _ = writeln!(
            out,
            "   [NOT APPLICABLE] free input(s) supplied; committed expectations describe the reference candidate"
        );
    }
    if let Some(replay) = &report.replay {
        let _ = writeln!(out, "\n{replay_section}. REPLAY");
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

    if !report.findings.is_empty() {
        let _ = writeln!(
            out,
            "\nACTIONABLE FEEDBACK ({} finding(s))",
            report.findings.len()
        );
        for finding in &report.findings {
            let step = finding
                .step_id
                .as_deref()
                .map(|step_id| format!(" step {step_id}"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "   [{} · {}{step} · owner {}] {}",
                finding.code,
                run_stage_label(finding.stage),
                finding.owner,
                finding.message
            );
            let _ = writeln!(out, "      next: {}", finding.next_action);
        }
    }

    let execution_phrase = match report.execution.as_ref().map(|execution| execution.status) {
        Some(ExecutionStatus::Executed) => {
            "every declared step executed or was reused under a verified receipt"
        }
        Some(ExecutionStatus::Reused) => {
            "every declared step was reused from a committed receipt whose invocation identity and outputs still verify, so nothing ran"
        }
        Some(ExecutionStatus::Planned) => "execution was planned only",
        Some(ExecutionStatus::NotRun) => "no step was executed",
        Some(ExecutionStatus::Partial) => {
            "some declared steps executed and others were not supplied"
        }
        Some(ExecutionStatus::Refused) => "execution was refused",
        Some(ExecutionStatus::Failed) => "execution failed",
        None if report
            .compile
            .as_ref()
            .is_some_and(|compile| compile.compiled.is_some()) =>
        {
            "no execution is declared"
        }
        None => "execution was not reached",
    };
    let _ = writeln!(
        out,
        "\nOutcome: {}. Byte integrity is {}; {execution_phrase}; {}",
        match report.status {
            CaseRunStatus::Evaluated => "workflow evaluated",
            CaseRunStatus::Rejected => "workflow rejected",
            CaseRunStatus::Planned => "workflow planned, not run",
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

fn compact_json(value: &Value) -> String {
    const MAX_CHARS: usize = 160;
    let rendered = serde_json::to_string(value).unwrap_or_else(|_| "<unavailable>".into());
    let count = rendered.chars().count();
    if count <= MAX_CHARS {
        rendered
    } else {
        format!("{}…", rendered.chars().take(MAX_CHARS).collect::<String>())
    }
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

pub(crate) fn verdict_label(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Pass => "PASS",
        VerdictStatus::Fail => "FAIL",
        VerdictStatus::Inconclusive => "INCONCLUSIVE",
        VerdictStatus::NotEvaluated => "NOT_EVALUATED",
    }
}

fn run_stage_label(stage: RunStage) -> &'static str {
    match stage {
        RunStage::PackageIntegrity => "package_integrity",
        RunStage::Compilation => "compilation",
        RunStage::FreeInputValidation => "free_input_validation",
        RunStage::Coverage => "coverage",
        RunStage::AttemptPlanning => "attempt_planning",
        RunStage::ExecutionPlanning => "execution_planning",
        RunStage::Execution => "execution",
        RunStage::ReceiptVerification => "receipt_verification",
        RunStage::ClaimGeneration => "claim_generation",
        RunStage::EvidenceBinding => "evidence_binding",
        RunStage::CampaignEvaluation => "campaign_evaluation",
        RunStage::Replay => "replay",
        RunStage::Infrastructure => "infrastructure",
    }
}
