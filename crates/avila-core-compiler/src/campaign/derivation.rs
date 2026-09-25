//! The replayable record of an evaluation's rule applications (ADR-0026).
//!
//! A `VerdictDerivation` is a portable document: it names the bound context
//! every premise was checked under, lists the rule applications in the
//! order the checks ran, and carries its own canonical identity. It is
//! emitted beside the campaign report — never inside it — so existing
//! reports, claims, and signed packages acquire no new bytes.
//!
//! The document is ordinary data. Nothing about deserializing one grants
//! authority: a verdict or admission is only authoritative when produced
//! by the checks themselves. What the record makes possible is replay —
//! an independent verifier can reconstruct each inference and reject a
//! conclusion that does not follow, even when the outer digest was
//! honestly recomputed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use avila_core_kernel::canonicalize_json;

use super::context::ContextRecord;

/// The schema the derivation document declares.
pub const DERIVATION_SCHEMA_VERSION: &str = "avila.core/verdict-derivation/v0.1-draft";

/// The largest derivation one evaluation may emit. Premises are linear in
/// the bound inputs, claims, and requirements, so a bound exists; an
/// evaluation that would exceed it is refused (`CORE-E7501`) rather than
/// allowed to produce an unbounded record.
pub const MAX_RULE_APPLICATIONS: usize = 4_096;

/// One premise a rule application consulted. `kind` names the check class —
/// never free text; `id` names the checked object (a claim id, a digest, a
/// slot); `state` records the outcome of that check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulePremise {
    pub kind: String,
    pub id: String,
    pub state: String,
}

/// One application of a named rule: which premises it consulted and the
/// narrow conclusion they support. Missing or contradicted premises are
/// recorded with their refusal state, not omitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleApplication {
    pub rule: String,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub premises: Vec<RulePremise>,
    pub conclusion: String,
    /// Finding codes the application raised (`CORE-…` or
    /// `evidence:state` verdict reasons).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

impl RuleApplication {
    pub(crate) fn new(
        rule: &str,
        subject: impl Into<String>,
        conclusion: impl Into<String>,
    ) -> Self {
        Self {
            rule: rule.into(),
            subject: subject.into(),
            premises: Vec::new(),
            conclusion: conclusion.into(),
            reasons: Vec::new(),
        }
    }
}

/// The verdict derivation document: every rule application the evaluation
/// performed, bound to one context identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictDerivation {
    pub schema_version: String,
    pub semantic_profile: String,
    pub evaluator: String,
    /// Identity of the evaluation context record below.
    pub context_sha256: String,
    /// The bound context the applications ran under.
    pub context: ContextRecord,
    /// Every rule application, in the order the checks ran.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applications: Vec<RuleApplication>,
    /// The campaign report this derivation belongs to; absent when the
    /// evaluation was refused before a report identity existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campaign_sha256: Option<String>,
    /// Canonical identity of the body above — recomputed on read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivation_sha256: Option<String>,
    pub notice: String,
}

impl VerdictDerivation {
    /// The document's semantic body — everything `derivation_sha256`
    /// covers, excluding the hash itself.
    pub fn identity_body(&self) -> Self {
        let mut body = self.clone();
        body.derivation_sha256 = None;
        body
    }

    /// Canonical identity of the body, for producers stamping the record
    /// and verifiers recomputing it.
    pub fn recompute_identity(&self) -> Result<String, String> {
        let body = self.identity_body();
        let bytes = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
        let canonical = canonicalize_json(&bytes).map_err(|error| error.to_string())?;
        Ok(format!("sha256:{:x}", Sha256::digest(&canonical)))
    }
}

/// What the diff found for one `(rule, subject)` pair across two
/// derivations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChangedUse {
    /// The application exists only under `before`.
    Removed {
        rule: String,
        subject: String,
        before_conclusion: String,
    },
    /// The application exists only under `after`.
    Added {
        rule: String,
        subject: String,
        after_conclusion: String,
    },
    /// Same rule and subject, different conclusion: the premise kinds that
    /// differ name which bound material drove the change.
    ConclusionChanged {
        rule: String,
        subject: String,
        before_conclusion: String,
        after_conclusion: String,
        /// Premise kinds whose `id`/`state` differ, sorted.
        changed_premise_kinds: Vec<String>,
    },
    /// Same conclusion, different premises — an unchanged value whose
    /// justification moved.
    PremisesChanged {
        rule: String,
        subject: String,
        conclusion: String,
        changed_premise_kinds: Vec<String>,
    },
}

/// The answer to "which uses changed" between two derivations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivationDiff {
    /// Whether both derivations name the same bound context — when not,
    /// every application is a cross-context comparison and `uses` still
    /// reports per-rule differences.
    pub same_context: bool,
    pub before_context_sha256: String,
    pub after_context_sha256: String,
    pub uses: Vec<ChangedUse>,
    /// One sentence per changed use, naming the rule, the subject, and the
    /// premise kinds that drove the change.
    pub explanations: Vec<String>,
}

/// Compare two derivations and explain which bound uses changed. Two
/// derivations with equal verdicts but different premises stay distinct —
/// the diff reports the premise-level difference, not just the conclusion.
pub fn explain_derivation_changes(
    before: &VerdictDerivation,
    after: &VerdictDerivation,
) -> DerivationDiff {
    let before_apps: BTreeMap<(&str, &str), &RuleApplication> = before
        .applications
        .iter()
        .map(|application| {
            (
                (application.rule.as_str(), application.subject.as_str()),
                application,
            )
        })
        .collect();
    let after_apps: BTreeMap<(&str, &str), &RuleApplication> = after
        .applications
        .iter()
        .map(|application| {
            (
                (application.rule.as_str(), application.subject.as_str()),
                application,
            )
        })
        .collect();

    let mut uses = Vec::new();
    let mut keys: Vec<(&str, &str)> = before_apps
        .keys()
        .chain(after_apps.keys())
        .copied()
        .collect();
    keys.sort();
    keys.dedup();
    for key in keys {
        match (before_apps.get(&key), after_apps.get(&key)) {
            (Some(prior), None) => uses.push(ChangedUse::Removed {
                rule: key.0.into(),
                subject: key.1.into(),
                before_conclusion: prior.conclusion.clone(),
            }),
            (None, Some(current)) => uses.push(ChangedUse::Added {
                rule: key.0.into(),
                subject: key.1.into(),
                after_conclusion: current.conclusion.clone(),
            }),
            (Some(prior), Some(current)) => {
                let changed = changed_premise_kinds(prior, current);
                if prior.conclusion != current.conclusion {
                    uses.push(ChangedUse::ConclusionChanged {
                        rule: key.0.into(),
                        subject: key.1.into(),
                        before_conclusion: prior.conclusion.clone(),
                        after_conclusion: current.conclusion.clone(),
                        changed_premise_kinds: changed,
                    });
                } else if !changed.is_empty() || prior.reasons != current.reasons {
                    uses.push(ChangedUse::PremisesChanged {
                        rule: key.0.into(),
                        subject: key.1.into(),
                        conclusion: current.conclusion.clone(),
                        changed_premise_kinds: changed,
                    });
                }
            }
            (None, None) => {}
        }
    }

    let explanations = uses.iter().map(explain_use).collect();
    DerivationDiff {
        same_context: before.context_sha256 == after.context_sha256,
        before_context_sha256: before.context_sha256.clone(),
        after_context_sha256: after.context_sha256.clone(),
        uses,
        explanations,
    }
}

/// Premise kinds whose `id`/`state` set differs between two applications.
fn changed_premise_kinds(before: &RuleApplication, after: &RuleApplication) -> Vec<String> {
    let before_premises: std::collections::BTreeSet<(&str, &str, &str)> = before
        .premises
        .iter()
        .map(|premise| {
            (
                premise.kind.as_str(),
                premise.id.as_str(),
                premise.state.as_str(),
            )
        })
        .collect();
    let after_premises: std::collections::BTreeSet<(&str, &str, &str)> = after
        .premises
        .iter()
        .map(|premise| {
            (
                premise.kind.as_str(),
                premise.id.as_str(),
                premise.state.as_str(),
            )
        })
        .collect();
    let mut kinds: Vec<String> = before_premises
        .symmetric_difference(&after_premises)
        .map(|(kind, _, _)| (*kind).to_string())
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

fn explain_use(change: &ChangedUse) -> String {
    match change {
        ChangedUse::Removed {
            rule,
            subject,
            before_conclusion,
        } => format!(
            "`{subject}` no longer applies rule `{rule}` — its use ended with `{before_conclusion}`"
        ),
        ChangedUse::Added {
            rule,
            subject,
            after_conclusion,
        } => format!(
            "`{subject}` now applies rule `{rule}` — a new use concluding `{after_conclusion}`"
        ),
        ChangedUse::ConclusionChanged {
            rule,
            subject,
            before_conclusion,
            after_conclusion,
            changed_premise_kinds,
        } => format!(
            "`{subject}` under `{rule}` changed `{before_conclusion}` → `{after_conclusion}`; the changed premises are: {}",
            changed_premise_kinds.join(", ")
        ),
        ChangedUse::PremisesChanged {
            rule,
            subject,
            conclusion,
            changed_premise_kinds,
        } => format!(
            "`{subject}` under `{rule}` still concludes `{conclusion}`, but its justification changed in: {}",
            changed_premise_kinds.join(", ")
        ),
    }
}
