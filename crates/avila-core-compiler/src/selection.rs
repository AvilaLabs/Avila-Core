//! Provider selection records: the org's recorded decision of which
//! capability implementation serves each step (ADR-0020, SC-8).
//!
//! The engine does not perform selection — it verifies the recorded
//! selection. A requester-signed `capability_selection` document names the
//! registry snapshot that bounded discovery, every considered candidate
//! with its decision and reasons, the ordered criteria applied, and the
//! disclosures policy demands (Avila-provided, self-preference check, cost
//! estimate and confirmation). The runner checks the record against the
//! package it claims to describe — the selected triple must be exactly the
//! capability the manifest binds — and against the contract's declared
//! `execution_policy` rules. A refusal happens at bind/plan time and never
//! enters a verdict.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::document::VersionedRef;

pub const SELECTION_SCHEMA_VERSION: &str = "avila.core/capability-selection/v0.1-draft";

/// The criteria a selection may legitimately weigh (SC-8.4).
pub const LEGITIMATE_CRITERIA: &[&str] = &[
    "cost",
    "time",
    "locality",
    "technical",
    "diversity",
    "preference",
];

/// Criteria that may never appear, visibly or otherwise (SC-8.4).
pub const BANNED_CRITERIA: &[&str] = &["provider_payment", "avila_margin"];

/// One package's recorded capability selections for its workflow steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySelection {
    pub schema_version: String,
    /// The exact registry snapshot the candidates were discovered against.
    pub registry_snapshot: RegistrySnapshotRef,
    /// How candidates were found — data, never interpreted.
    #[serde(default)]
    pub query: String,
    /// Per-step selection records.
    pub selections: Vec<StepSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrySnapshotRef {
    pub registry_id: String,
    pub revision: u64,
    pub sha256: String,
}

/// One step's recorded selection: every candidate considered, the criteria
/// applied, and the disclosures the org's policy requires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepSelection {
    pub step_id: String,
    /// Every candidate considered; exactly one may carry decision
    /// `selected`, and none may lack a recorded decision (SC-8.6).
    pub candidates: Vec<SelectionCandidate>,
    /// The ordered list of criteria applied — closed vocabulary, banned
    /// values refused (`CORE-P5601`).
    #[serde(default)]
    pub criteria: Vec<String>,
    /// Whether the selected implementation is Avila-provided (SC-8.7).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub avila_provided: bool,
    /// The self-preference check applied — required when `avila_provided`
    /// and the policy forbids unpinned self-preference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_preference_check: Option<SelfPreferenceCheck>,
    /// The recorded cost estimate for the selected candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_estimate: Option<CostEstimate>,
    /// Who confirmed a cost above the cap — the recorded human datum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_confirmed_by: Option<String>,
}

/// A capability implementation triple — the same identity the manifest
/// binds: the registry type, the manifest capability id, the adapter
/// identifier, and the executable bytes' digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateTriple {
    pub capability_type: VersionedRef,
    pub capability_id: String,
    pub adapter: String,
    pub executable_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionCandidate {
    /// The implementation triple the manifest would bind for this
    /// candidate — kept flat beside `decision`/`reasons` so
    /// `deny_unknown_fields` stays honest.
    pub capability_type: VersionedRef,
    pub capability_id: String,
    pub adapter: String,
    pub executable_sha256: String,
    /// `selected` for at most one candidate; `excluded` for one ruled out
    /// by a stated reason; `inadmissible` for one that fails policy.
    /// Optional so an omitted decision is a checkable refusal
    /// (`CORE-P5602`), not a parse error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<CandidateDecision>,
    /// Why this decision — each reason carries the rule id and a fact.
    #[serde(default)]
    pub reasons: Vec<CandidateReason>,
}

impl SelectionCandidate {
    pub fn triple(&self) -> CandidateTriple {
        CandidateTriple {
            capability_type: self.capability_type.clone(),
            capability_id: self.capability_id.clone(),
            adapter: self.adapter.clone(),
            executable_sha256: self.executable_sha256.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateDecision {
    Selected,
    Excluded,
    Inadmissible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateReason {
    pub rule_id: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfPreferenceCheck {
    /// The check applied — from the org's check vocabulary; the engine
    /// requires its presence, not its content.
    pub check_id: String,
    pub justification: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostEstimate {
    /// Exact-number string in `currency`.
    pub value: String,
    pub currency: String,
}

impl StepSelection {
    /// The (at most one) candidate carrying decision `selected`.
    pub fn selected(&self) -> Option<&SelectionCandidate> {
        self.candidates
            .iter()
            .find(|candidate| candidate.decision == Some(CandidateDecision::Selected))
    }
}

/// Parse and structurally validate a `capability_selection` document:
/// schema marker, nonempty identities, at most one `selected` candidate
/// per step, `sha256:`-prefixed digests, and unique step entries.
pub fn parse_selection(bytes: &[u8]) -> Result<CapabilitySelection, Box<dyn std::error::Error>> {
    let selection: CapabilitySelection = serde_json::from_slice(bytes)?;
    if selection.schema_version != SELECTION_SCHEMA_VERSION {
        return Err(format!(
            "schema_version `{}` is not `{}`",
            selection.schema_version, SELECTION_SCHEMA_VERSION
        )
        .into());
    }
    if selection.registry_snapshot.registry_id.is_empty() {
        return Err("registry_snapshot.registry_id is empty".into());
    }
    let mut seen_steps = BTreeSet::new();
    for entry in &selection.selections {
        if entry.step_id.is_empty() {
            return Err("a selection entry has an empty step_id".into());
        }
        if !seen_steps.insert(entry.step_id.as_str()) {
            return Err(format!(
                "step `{}` carries more than one selection entry",
                entry.step_id
            )
            .into());
        }
        if entry.candidates.is_empty() {
            return Err(format!("step `{}` records no candidates", entry.step_id).into());
        }
        let selected_count = entry
            .candidates
            .iter()
            .filter(|candidate| candidate.decision == Some(CandidateDecision::Selected))
            .count();
        if selected_count > 1 {
            return Err(format!(
                "step `{}` marks {selected_count} candidates selected; at most one may win",
                entry.step_id
            )
            .into());
        }
        for candidate in &entry.candidates {
            if !candidate.executable_sha256.starts_with("sha256:")
                || candidate.executable_sha256.len() != "sha256:".len() + 64
                || !candidate.executable_sha256["sha256:".len()..]
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(format!(
                    "step `{}` candidate executable_sha256 `{}` is not a lowercase sha256 digest",
                    entry.step_id, candidate.executable_sha256
                )
                .into());
            }
        }
    }
    Ok(selection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn selection_json(candidates: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema_version": SELECTION_SCHEMA_VERSION,
            "registry_snapshot": {
                "registry_id": "test.registry",
                "revision": 1,
                "sha256": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            "query": "registry scan",
            "selections": [{
                "step_id": "transport",
                "candidates": candidates,
                "criteria": ["technical", "cost"]
            }]
        }))
        .unwrap()
    }

    fn candidate(decision: &str, executable: &str) -> serde_json::Value {
        json!({
            "capability_type": {"id": "test.transport", "major": 1},
            "capability_id": "transport-bin",
            "adapter": "test/transport@1",
            "executable_sha256": executable,
            "decision": decision,
            "reasons": [{"rule_id": "criteria.technical", "detail": "winner"}]
        })
    }

    #[test]
    fn a_valid_selection_parses() {
        let bytes = selection_json(json!([
            candidate("selected", &format!("sha256:{}", "a".repeat(64))),
            candidate("excluded", &format!("sha256:{}", "b".repeat(64)))
        ]));
        let parsed = parse_selection(&bytes).unwrap();
        assert_eq!(parsed.selections.len(), 1);
        assert_eq!(
            parsed.selections[0].selected().unwrap().capability_id,
            "transport-bin"
        );
    }

    #[test]
    fn two_selected_candidates_are_refused() {
        let bytes = selection_json(json!([
            candidate("selected", &format!("sha256:{}", "a".repeat(64))),
            candidate("selected", &format!("sha256:{}", "b".repeat(64)))
        ]));
        assert!(
            parse_selection(&bytes)
                .unwrap_err()
                .to_string()
                .contains("at most one")
        );
    }

    #[test]
    fn a_malformed_digest_is_refused() {
        let bytes = selection_json(json!([candidate("selected", "sha256:not-hex")]));
        assert!(
            parse_selection(&bytes)
                .unwrap_err()
                .to_string()
                .contains("not a lowercase sha256")
        );
    }
}
