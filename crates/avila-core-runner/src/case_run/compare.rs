//! Requirement margins and identity-bound attempt-to-parent comparison:
//! the exact numbers behind a verdict, and the typed deltas between one
//! attempt's results and its parent's.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::Write as _;

use avila_core_compiler::{CampaignReport, Comparison, CompiledContract};
use avila_core_kernel::{ExactNumber, VerdictStatus};
use serde::{Deserialize, Serialize};

use super::{
    CaseRunOptions, CaseRunReport, display_comparison_number, display_signed_number, display_unit,
    verdict_label,
};
use crate::attempt::{
    ATTEMPT_COMPARISON_SCHEMA_VERSION, AttemptComparison, AttemptMarginComparison,
    AttemptMarginUnavailable, AttemptMarginUnavailableReason, AttemptRecord,
    AttemptVerdictTransition, AttemptVerdictUnavailable, AttemptVerdictUnavailableReason,
    parent_verdict_values,
};
use crate::execute::claims::canonical_decimal;

/// One requirement's outcome with the numeric or categorical values that
/// decided it, for search and for people. Read from the kernel's verdict
/// output; nothing here is re-derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictMargin {
    pub requirement_id: String,
    pub status: VerdictStatus,
    pub rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nominal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_categories: Option<Vec<String>>,
    /// Limit minus the decisive bound for an upper limit, decisive bound
    /// minus limit for a lower limit: positive means inside, negative means
    /// outside. Absent when no number decided the verdict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<String>,
}

/// The numbers behind each verdict and the distance to the limit.
pub(crate) fn margins(
    compiled: &CompiledContract,
    campaign: &CampaignReport,
) -> Vec<VerdictMargin> {
    campaign
        .verdicts
        .iter()
        .map(|verdict| {
            let comparison = compiled
                .requirements
                .iter()
                .find(|requirement| requirement.requirement_id == verdict.requirement_id)
                .map(|requirement| requirement.comparison);
            let output = &verdict.verdict;
            let decisive_upper = output
                .upper_canonical
                .as_deref()
                .or(match output.basis_visible {
                    Some(avila_core_kernel::BasisKind::Nominal) => {
                        output.nominal_canonical.as_deref()
                    }
                    _ => None,
                });
            let decisive_lower = output
                .lower_canonical
                .as_deref()
                .or(match output.basis_visible {
                    Some(avila_core_kernel::BasisKind::Nominal) => {
                        output.nominal_canonical.as_deref()
                    }
                    _ => None,
                });
            let margin = match (comparison, output.limit_canonical.as_deref()) {
                (Some(Comparison::LessThan | Comparison::LessThanOrEqual), Some(limit)) => {
                    decisive_upper.and_then(|upper| exact_difference(limit, upper))
                }
                (Some(Comparison::GreaterThan | Comparison::GreaterThanOrEqual), Some(limit)) => {
                    decisive_lower.and_then(|lower| exact_difference(lower, limit))
                }
                _ => None,
            };
            VerdictMargin {
                requirement_id: verdict.requirement_id.clone(),
                status: output.status,
                rule: output.rule.clone(),
                unit: output.canonical_unit.clone(),
                limit: output.limit_canonical.clone(),
                lower: output.lower_canonical.clone(),
                upper: output.upper_canonical.clone(),
                nominal: output.nominal_canonical.clone(),
                observed_category: output.observed_category.clone(),
                accepted_categories: output.accepted_categories.clone(),
                margin,
            }
        })
        .collect()
}

pub(crate) fn compare_attempt_to_parent(
    options: &CaseRunOptions,
    report: &CaseRunReport,
) -> Result<Option<AttemptComparison>, Box<dyn Error>> {
    let Some(attempt) = &report.attempt else {
        return Ok(None);
    };
    if attempt.parent_attempt_id.is_none() {
        return Ok(None);
    }
    let log_path = options
        .log
        .as_deref()
        .ok_or("a prepared child attempt no longer has a lineage log")?;
    let parent_values = parent_verdict_values(log_path, attempt)?
        .ok_or("a child attempt did not resolve a parent verdict collection")?;
    let parent_margins: Vec<VerdictMargin> = parent_values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            serde_json::from_value(value).map_err(|error| {
                format!(
                    "parent attempt `{}` verdict {} is not a supported Core margin record: {error}",
                    attempt.parent_attempt_id.as_deref().unwrap_or("?"),
                    index + 1
                )
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(Some(compare_attempt_results(
        attempt,
        &parent_margins,
        &report.margins,
    )?))
}

pub(crate) fn compare_attempt_results(
    attempt: &AttemptRecord,
    parent: &[VerdictMargin],
    child: &[VerdictMargin],
) -> Result<AttemptComparison, String> {
    let parent_attempt_id = attempt
        .parent_attempt_id
        .clone()
        .ok_or("a root attempt has no parent result to compare")?;
    let parent_record_sha256 = attempt
        .parent_record_sha256
        .clone()
        .ok_or("a child attempt is missing its parent record identity")?;
    let parent = index_verdict_margins("parent", parent)?;
    let child = index_verdict_margins("child", child)?;
    let requirement_ids: BTreeSet<&str> = parent
        .keys()
        .copied()
        .chain(child.keys().copied())
        .collect();
    let mut verdicts_compared = 0;
    let mut unchanged_verdicts = 0;
    let mut verdict_transitions = Vec::new();
    let mut verdict_comparison_unavailable = Vec::new();
    let mut exact_margin_comparisons = Vec::new();
    let mut margin_comparison_unavailable = Vec::new();

    for requirement_id in requirement_ids {
        let (Some(parent_verdict), Some(child_verdict)) =
            (parent.get(requirement_id), child.get(requirement_id))
        else {
            let reason = if parent.contains_key(requirement_id) {
                AttemptVerdictUnavailableReason::ChildMissing
            } else {
                AttemptVerdictUnavailableReason::ParentMissing
            };
            verdict_comparison_unavailable.push(AttemptVerdictUnavailable {
                requirement_id: requirement_id.into(),
                reason,
            });
            margin_comparison_unavailable.push(AttemptMarginUnavailable {
                requirement_id: requirement_id.into(),
                reason: if parent.contains_key(requirement_id) {
                    AttemptMarginUnavailableReason::ChildMissing
                } else {
                    AttemptMarginUnavailableReason::ParentMissing
                },
            });
            continue;
        };

        verdicts_compared += 1;
        if parent_verdict.status == child_verdict.status {
            unchanged_verdicts += 1;
        } else {
            verdict_transitions.push(AttemptVerdictTransition {
                requirement_id: requirement_id.into(),
                parent_status: parent_verdict.status,
                child_status: child_verdict.status,
            });
        }

        let unavailable = match (&parent_verdict.margin, &child_verdict.margin) {
            (None, None)
                if parent_verdict.observed_category.is_some()
                    || parent_verdict.accepted_categories.is_some()
                    || child_verdict.observed_category.is_some()
                    || child_verdict.accepted_categories.is_some() =>
            {
                Some(AttemptMarginUnavailableReason::NotNumeric)
            }
            (None, None) => Some(AttemptMarginUnavailableReason::BothMissing),
            (None, Some(_)) => Some(AttemptMarginUnavailableReason::ParentMissing),
            (Some(_), None) => Some(AttemptMarginUnavailableReason::ChildMissing),
            (Some(_), Some(_)) if parent_verdict.unit != child_verdict.unit => {
                Some(AttemptMarginUnavailableReason::UnitMismatch)
            }
            (Some(_), Some(_)) if parent_verdict.limit != child_verdict.limit => {
                Some(AttemptMarginUnavailableReason::LimitMismatch)
            }
            (Some(parent_margin), Some(child_margin)) => {
                match exact_difference(child_margin, parent_margin) {
                    Some(delta) => {
                        exact_margin_comparisons.push(AttemptMarginComparison {
                            requirement_id: requirement_id.into(),
                            unit: child_verdict.unit.clone(),
                            parent_margin: parent_margin.clone(),
                            child_margin: child_margin.clone(),
                            delta,
                        });
                        None
                    }
                    None => Some(AttemptMarginUnavailableReason::InvalidNumber),
                }
            }
        };
        if let Some(reason) = unavailable {
            margin_comparison_unavailable.push(AttemptMarginUnavailable {
                requirement_id: requirement_id.into(),
                reason,
            });
        }
    }

    Ok(AttemptComparison {
        schema_version: ATTEMPT_COMPARISON_SCHEMA_VERSION.into(),
        parent_attempt_id,
        parent_record_sha256,
        verdicts_compared,
        unchanged_verdicts,
        verdict_transitions,
        verdict_comparison_unavailable,
        exact_margin_comparisons,
        margin_comparison_unavailable,
    })
}

fn index_verdict_margins<'a>(
    side: &str,
    margins: &'a [VerdictMargin],
) -> Result<BTreeMap<&'a str, &'a VerdictMargin>, String> {
    let mut indexed = BTreeMap::new();
    for margin in margins {
        if indexed
            .insert(margin.requirement_id.as_str(), margin)
            .is_some()
        {
            return Err(format!(
                "{side} result repeats requirement `{}`",
                margin.requirement_id
            ));
        }
    }
    Ok(indexed)
}

fn exact_difference(left: &str, right: &str) -> Option<String> {
    let left = ExactNumber::from_canonical(left).ok()?;
    let right = ExactNumber::from_canonical(right).ok()?;
    let difference = left.checked_sub(&right).ok()?;
    Some(canonical_decimal(&difference).unwrap_or_else(|_| difference.canonical_rational()))
}

pub(crate) fn write_attempt_comparison(out: &mut String, comparison: &AttemptComparison) {
    let _ = writeln!(
        out,
        "   comparison with exact parent: {} verdict(s), {} transition(s); {} exact numeric margin(s)",
        comparison.verdicts_compared,
        comparison.verdict_transitions.len(),
        comparison.exact_margin_comparisons.len()
    );
    let changed_margins: BTreeMap<&str, &AttemptMarginComparison> = comparison
        .exact_margin_comparisons
        .iter()
        .filter(|margin| {
            ExactNumber::from_canonical(&margin.delta).is_ok_and(|delta| !delta.is_zero())
        })
        .map(|margin| (margin.requirement_id.as_str(), margin))
        .collect();
    let meaningful: BTreeSet<&str> = comparison
        .verdict_transitions
        .iter()
        .map(|transition| transition.requirement_id.as_str())
        .chain(changed_margins.keys().copied())
        .collect();
    for requirement_id in meaningful.iter().take(12) {
        let transition = comparison
            .verdict_transitions
            .iter()
            .find(|transition| transition.requirement_id == *requirement_id);
        let margin = changed_margins.get(requirement_id).copied();
        match (transition, margin) {
            (Some(transition), Some(margin)) => {
                let _ = writeln!(
                    out,
                    "   result {requirement_id}: {} -> {}; margin {} -> {}{} (delta {})",
                    verdict_label(transition.parent_status),
                    verdict_label(transition.child_status),
                    display_comparison_number(&margin.parent_margin),
                    display_comparison_number(&margin.child_margin),
                    display_unit(margin.unit.as_deref()),
                    display_signed_number(&margin.delta)
                );
            }
            (Some(transition), None) => {
                let _ = writeln!(
                    out,
                    "   result {requirement_id}: {} -> {}",
                    verdict_label(transition.parent_status),
                    verdict_label(transition.child_status)
                );
            }
            (None, Some(margin)) => {
                let _ = writeln!(
                    out,
                    "   margin {requirement_id}: {} -> {}{} (delta {})",
                    display_comparison_number(&margin.parent_margin),
                    display_comparison_number(&margin.child_margin),
                    display_unit(margin.unit.as_deref()),
                    display_signed_number(&margin.delta)
                );
            }
            (None, None) => {}
        }
    }
    if meaningful.len() > 12 {
        let _ = writeln!(
            out,
            "   … {} more result change(s) in the JSON report",
            meaningful.len() - 12
        );
    }
    let nonnumeric = comparison
        .margin_comparison_unavailable
        .iter()
        .filter(|unavailable| unavailable.reason == AttemptMarginUnavailableReason::NotNumeric)
        .count();
    if nonnumeric > 0 {
        let _ = writeln!(
            out,
            "   no numeric margin: {nonnumeric} categorical requirement(s)"
        );
    }
    let other_unavailable = comparison.margin_comparison_unavailable.len() - nonnumeric;
    if !comparison.verdict_comparison_unavailable.is_empty() || other_unavailable > 0 {
        let _ = writeln!(
            out,
            "   unavailable: {} verdict comparison(s), {other_unavailable} margin comparison(s)",
            comparison.verdict_comparison_unavailable.len(),
        );
    }
}
