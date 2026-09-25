//! SC-12.3 reuse rules: authorized, signed non-dependence claims.
//!
//! A `reuse_rule` document declares that bytes arriving on one bound input
//! slot do not affect one step's outputs. The runner checks its authority
//! (a `signature` document verified against a requester key in the
//! supplied trust root), its applicability (the scope names a step and a
//! binding edge the compiled contract actually contains — a rule can only
//! narrow, never widen), and its expiry. The runner does not prove the
//! non-dependence; it only checks who stands behind the claim and whether
//! it applies to this change. A rule that fails any check is refused:
//! the change it would have exempted stays disqualifying, so default
//! invalidation runs and the step reruns.

use std::collections::BTreeSet;

use avila_core_compiler::{CompiledContract, SourceLocation};
use avila_core_evidence::signature::TrustRoot;
use avila_core_evidence::{VerifiedCasePackage, signature};
use serde::Deserialize;

use crate::case_run::FindingClass;
use crate::diagnostic::{CORE_X3401, RunStage};

use super::RunFinding;

/// One applicable reuse rule: an authorized, unexpired non-dependence
/// claim whose scope is a real binding edge of the compiled contract.
#[derive(Debug)]
pub(super) struct ResolvedRule {
    /// The rule's stable id — what `exempted_by` and `reused_under` name.
    pub rule_id: String,
    /// The step whose input-slot changes this rule exempts.
    pub step_id: String,
    /// The bound input slot this rule exempts.
    pub input_slot: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReuseRuleDocument {
    schema_version: String,
    rule_id: String,
    scope: ReuseRuleScope,
    /// Required present and nonempty — the claim a signer stands behind.
    /// The kernel checks its presence and the signature over it; it does
    /// not prove the non-dependence the text asserts.
    justification: String,
    validation_evidence: Vec<serde_json::Value>,
    not_after: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReuseRuleScope {
    step_id: String,
    input_slot: String,
}

/// Evaluate every `reuse_rule` document the package commits: parse it,
/// check its scope edge exists in the compiled contract, check its expiry
/// against the run instant, and check its signature against the supplied
/// trust root's requester keys. Returns the applicable rules plus a
/// finding per refusal reason.
pub(super) fn evaluate(
    package: &VerifiedCasePackage,
    compiled: &CompiledContract,
    trust_root: Option<&TrustRoot>,
    now: &str,
) -> (Vec<ResolvedRule>, Vec<RunFinding>) {
    let mut rules = Vec::new();
    let mut findings = Vec::new();
    for document in &package.manifest().documents {
        if document.role != "reuse_rule" {
            continue;
        }
        let label = format!("reuse-rule document `{}`", document.document_id);
        let Some(bytes) = package.document_by_id(&document.document_id) else {
            continue;
        };
        let rule: ReuseRuleDocument = match serde_json::from_slice(bytes) {
            Ok(rule) => rule,
            Err(_) => {
                findings.push(refused(
                    &label,
                    "could not be evaluated — the document is not a well-formed `reuse-rule/v0.1-draft`; unknown applicability means rerun",
                ));
                continue;
            }
        };
        if rule.schema_version != "avila.core/reuse-rule/v0.1-draft" {
            findings.push(refused(
                &label,
                &format!(
                    "declares schema_version `{}`, not `avila.core/reuse-rule/v0.1-draft`",
                    rule.schema_version
                ),
            ));
            continue;
        }
        // The record must actually carry the claim's support: a rule with
        // no justification or no validation evidence is not a
        // non-dependence claim, whatever its scope asserts.
        if rule.justification.trim().is_empty() {
            findings.push(refused(
                &label,
                "carries no justification — 'the files look similar' is not a reuse rule",
            ));
            continue;
        }
        if rule.validation_evidence.is_empty() {
            findings.push(refused(
                &label,
                "names no validation evidence for the non-dependence it asserts",
            ));
            continue;
        }
        // Applicability: the scope must name a step in the compiled
        // workflow and a slot that step actually binds. A rule exempting
        // an edge the contract does not contain widens the claim beyond
        // what it may narrow — refused.
        let bound = compiled
            .workflow()
            .iter()
            .find(|step| step.step_id == rule.scope.step_id)
            .is_some_and(|step| {
                step.bindings
                    .iter()
                    .any(|binding| binding.input_slot == rule.scope.input_slot)
            });
        if !bound {
            findings.push(refused(
                &label,
                &format!(
                    "scope `{}.{}` is not a binding edge in the compiled contract — a reuse rule can only narrow, never widen",
                    rule.scope.step_id, rule.scope.input_slot
                ),
            ));
            continue;
        }
        // Expiry: `not_after` is the last instant the claim stands. Both
        // sides are normalized `YYYY-MM-DDTHH:MM:SSZ`, so lexical order is
        // chronological.
        if now >= rule.not_after.as_str() {
            findings.push(refused(&label, &format!("expired at {}", rule.not_after)));
            continue;
        }
        // Authority: a requester key in the supplied trust root must have
        // signed this exact document's bound bytes. No trust root means
        // applicability is unknown — fail closed with no finding, as no
        // verification was even possible.
        let Some(trust_root) = trust_root else {
            continue;
        };
        let Some((_, signature_document)) =
            super::signing::find_signature_for(package, "reuse_rule", &document.document_id)
        else {
            findings.push(refused(
                &label,
                "carries no signature document over its bound bytes — unsigned",
            ));
            continue;
        };
        if signature_document.signed_document.sha256 != document.sha256 {
            findings.push(refused(
                &label,
                "its signature covers different bytes than the package binds",
            ));
            continue;
        }
        match signature::verify_signature_document(
            &signature_document,
            trust_root,
            signature::KeyRole::Requester,
        ) {
            Ok(_) => rules.push(ResolvedRule {
                rule_id: rule.rule_id,
                step_id: rule.scope.step_id,
                input_slot: rule.scope.input_slot,
            }),
            Err(error) => findings.push(refused(&label, &format!("authority mismatch — {error}"))),
        }
    }
    (rules, findings)
}

/// The `(step_id, input_slot)` edges a resolved rule exempts — the set
/// propagation skips and change records consult.
pub(super) fn exempted_edges(rules: &[ResolvedRule]) -> BTreeSet<(String, String)> {
    rules
        .iter()
        .map(|rule| (rule.step_id.clone(), rule.input_slot.clone()))
        .collect()
}

/// The rule exempting `(step_id, input_slot)`, if one resolved.
pub(super) fn rule_for<'a>(
    rules: &'a [ResolvedRule],
    step_id: &str,
    input_slot: &str,
) -> Option<&'a ResolvedRule> {
    rules
        .iter()
        .find(|rule| rule.step_id == step_id && rule.input_slot == input_slot)
}

fn refused(label: &str, reason: &str) -> RunFinding {
    RunFinding::runtime(
        CORE_X3401,
        FindingClass::Inadmissible,
        RunStage::ExecutionPlanning,
        "policy_owner",
        SourceLocation::new("reuse-rule", ""),
        format!("{label} refused: {reason}"),
    )
}
