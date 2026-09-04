//! Coverage of a compiled contract against a library requirement set.
//!
//! A search optimizes exactly what its contract states. A requirement set is
//! a library's owned list of what any contract in its domain must address; the
//! case declares which contract requirements cover each entry and states, with
//! an accepting owner, why any entry is omitted. Coverage is assessed here
//! without I/O: the set's bytes were verified by the package and are passed
//! in, and the result is a report, not a verdict. An unstated omission of an
//! entry the set says must be stated makes coverage incomplete; the runner
//! refuses to spend evaluation on an incomplete requirement set.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::compile::CompiledContract;
use crate::document::{BasisKind, Comparison};

pub const REQUIREMENT_SET_SCHEMA_VERSION: &str = "avila.core/requirement-set/v0.1-draft";
pub const COVERAGE_REPORT_SCHEMA_VERSION: &str = "avila.core/coverage-report/v0.1-draft";

/// A library's requirement set: what any contract in its domain must address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementSet {
    pub schema_version: String,
    pub set_id: String,
    pub revision: u64,
    pub owner: String,
    pub title: String,
    pub requirements: Vec<SetRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetRequirement {
    pub set_requirement_id: String,
    pub statement: String,
    /// The quantity kind a covering contract requirement must compare.
    pub kind: String,
    pub comparison: Comparison,
    /// The weakest basis on which a covering requirement may be evaluated.
    pub minimum_basis: BasisKind,
    pub omission: OmissionPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmissionPolicy {
    /// Leaving the entry uncovered requires a stated reason and an accepting
    /// owner.
    MustState,
    /// The entry may be left uncovered without a statement.
    MayOmit,
}

/// What the case declares about its coverage of the set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageDeclaration {
    /// Set requirement id to the contract requirement ids that cover it.
    #[serde(default)]
    pub mapping: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub omissions: Vec<DeclaredOmission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredOmission {
    pub set_requirement_id: String,
    pub reason: String,
    pub accepted_by: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// At least one mapped contract requirement matches the entry on a basis
    /// at least as strong as the set requires.
    Covered,
    /// Mapped requirements match only on a weaker basis than the set
    /// requires: the search would be guided but nothing could establish it.
    CoveredUnderBasis,
    /// Not covered, with a stated reason and an accepting owner.
    OmittedStated,
    /// Not covered, and the set permits silent omission.
    Omissible,
    /// Not covered, no statement, and the set requires one.
    OmittedUnstated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatus {
    Complete,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoveringRequirement {
    pub requirement_id: String,
    pub basis: BasisKind,
    /// Whether this requirement's basis meets the set's minimum.
    pub adequate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageEntry {
    pub set_requirement_id: String,
    pub statement: String,
    pub state: CoverageState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covered_by: Vec<CoveringRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageReport {
    pub schema_version: String,
    pub set_id: String,
    pub set_revision: u64,
    pub set_sha256: String,
    pub set_owner: String,
    pub contract_id: String,
    pub contract_revision: u64,
    pub status: CoverageStatus,
    pub entries: Vec<CoverageEntry>,
    /// Contract requirements no set entry claims: allowed, and listed so a
    /// reviewer sees what the contract adds beyond the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_requirements: Vec<String>,
    /// Declaration errors: unknown ids, contradictions, empty statements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

impl CoverageReport {
    pub fn count(&self, state: CoverageState) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.state == state)
            .count()
    }
}

/// Parse and validate a requirement set from its bytes.
pub fn parse_requirement_set(bytes: &[u8]) -> Result<RequirementSet, String> {
    let set: RequirementSet =
        serde_json::from_slice(bytes).map_err(|error| format!("requirement set: {error}"))?;
    if set.schema_version != REQUIREMENT_SET_SCHEMA_VERSION {
        return Err(format!(
            "requirement set schema `{}` is not `{REQUIREMENT_SET_SCHEMA_VERSION}`",
            set.schema_version
        ));
    }
    for (field, value) in [
        ("set_id", &set.set_id),
        ("owner", &set.owner),
        ("title", &set.title),
    ] {
        if value.trim().is_empty() {
            return Err(format!("requirement set `{field}` must not be empty"));
        }
    }
    if set.revision == 0 {
        return Err("requirement set revision must be at least 1".into());
    }
    if set.requirements.is_empty() {
        return Err("requirement set must contain at least one requirement".into());
    }
    let mut ids = BTreeSet::new();
    for requirement in &set.requirements {
        for (field, value) in [
            ("set_requirement_id", &requirement.set_requirement_id),
            ("statement", &requirement.statement),
            ("kind", &requirement.kind),
        ] {
            if value.trim().is_empty() {
                return Err(format!(
                    "requirement set entry `{}`: `{field}` must not be empty",
                    requirement.set_requirement_id
                ));
            }
        }
        if !ids.insert(requirement.set_requirement_id.as_str()) {
            return Err(format!(
                "requirement set entry `{}` is declared twice",
                requirement.set_requirement_id
            ));
        }
    }
    Ok(set)
}

fn basis_rank(basis: BasisKind) -> u8 {
    match basis {
        BasisKind::Nominal => 0,
        BasisKind::Bounded => 1,
        BasisKind::Enclosure => 2,
    }
}

/// Assess how the compiled contract covers the set under the declaration.
pub fn assess_coverage(
    compiled: &CompiledContract,
    set: &RequirementSet,
    set_sha256: &str,
    declaration: &CoverageDeclaration,
) -> CoverageReport {
    let mut issues = Vec::new();
    let set_ids: BTreeSet<&str> = set
        .requirements
        .iter()
        .map(|requirement| requirement.set_requirement_id.as_str())
        .collect();
    for key in declaration.mapping.keys() {
        if !set_ids.contains(key.as_str()) {
            issues.push(format!(
                "mapping names `{key}`, which is not in requirement set `{}`",
                set.set_id
            ));
        }
    }
    let mut omissions = BTreeMap::<&str, &DeclaredOmission>::new();
    for omission in &declaration.omissions {
        let id = omission.set_requirement_id.as_str();
        if !set_ids.contains(id) {
            issues.push(format!(
                "omission names `{id}`, which is not in requirement set `{}`",
                set.set_id
            ));
            continue;
        }
        if omission.reason.trim().is_empty() || omission.accepted_by.trim().is_empty() {
            issues.push(format!(
                "omission of `{id}` must state a reason and who accepted it"
            ));
            continue;
        }
        if declaration
            .mapping
            .get(id)
            .is_some_and(|ids| !ids.is_empty())
        {
            issues.push(format!(
                "`{id}` is both mapped to contract requirements and declared omitted"
            ));
            continue;
        }
        if omissions.insert(id, omission).is_some() {
            issues.push(format!("omission of `{id}` is declared twice"));
        }
    }

    let mut mapped_contract_ids = BTreeSet::new();
    let mut entries = Vec::with_capacity(set.requirements.len());
    for requirement in &set.requirements {
        let id = requirement.set_requirement_id.as_str();
        let mut entry_issues = Vec::new();
        let mut covered_by = Vec::new();
        for requirement_id in declaration.mapping.get(id).into_iter().flatten() {
            let Some(contract_requirement) = compiled
                .requirements
                .iter()
                .find(|candidate| &candidate.requirement_id == requirement_id)
            else {
                if compiled
                    .categorical_requirements
                    .iter()
                    .any(|candidate| &candidate.requirement_id == requirement_id)
                {
                    entry_issues.push(format!(
                        "mapped contract requirement `{requirement_id}` is categorical and cannot cover a quantitative requirement-set entry"
                    ));
                } else {
                    entry_issues.push(format!(
                        "mapped contract requirement `{requirement_id}` does not exist"
                    ));
                }
                continue;
            };
            mapped_contract_ids.insert(requirement_id.clone());
            if contract_requirement.limit.kind != requirement.kind {
                entry_issues.push(format!(
                    "`{requirement_id}` compares kind `{}`, the set entry requires `{}`",
                    contract_requirement.limit.kind, requirement.kind
                ));
                continue;
            }
            if contract_requirement.comparison != requirement.comparison {
                entry_issues.push(format!(
                    "`{requirement_id}` uses comparison `{:?}`, the set entry requires `{:?}`",
                    contract_requirement.comparison, requirement.comparison
                ));
                continue;
            }
            let basis = contract_requirement.basis.kind;
            covered_by.push(CoveringRequirement {
                requirement_id: requirement_id.clone(),
                basis,
                adequate: basis_rank(basis) >= basis_rank(requirement.minimum_basis),
            });
        }
        let (state, reason, accepted_by) = if covered_by.iter().any(|cover| cover.adequate) {
            (CoverageState::Covered, None, None)
        } else if !covered_by.is_empty() {
            entry_issues.push(format!(
                "covered only on a basis weaker than the set's minimum `{:?}`; a guide is not evidence",
                requirement.minimum_basis
            ));
            (CoverageState::CoveredUnderBasis, None, None)
        } else if let Some(omission) = omissions.get(id) {
            (
                CoverageState::OmittedStated,
                Some(omission.reason.clone()),
                Some(omission.accepted_by.clone()),
            )
        } else if requirement.omission == OmissionPolicy::MayOmit {
            (CoverageState::Omissible, None, None)
        } else {
            entry_issues.push(
                "not covered and no omission is stated; the set requires a reason and an accepting owner"
                    .into(),
            );
            (CoverageState::OmittedUnstated, None, None)
        };
        entries.push(CoverageEntry {
            set_requirement_id: requirement.set_requirement_id.clone(),
            statement: requirement.statement.clone(),
            state,
            covered_by,
            reason,
            accepted_by,
            issues: entry_issues,
        });
    }

    let additional_requirements = compiled
        .requirements
        .iter()
        .map(|requirement| requirement.requirement_id.clone())
        .filter(|id| !mapped_contract_ids.contains(id))
        .chain(
            compiled
                .categorical_requirements
                .iter()
                .map(|requirement| requirement.requirement_id.clone()),
        )
        .collect();
    let incomplete = !issues.is_empty()
        || entries.iter().any(|entry| {
            !entry.issues.is_empty()
                || matches!(
                    entry.state,
                    CoverageState::CoveredUnderBasis | CoverageState::OmittedUnstated
                )
        });
    CoverageReport {
        schema_version: COVERAGE_REPORT_SCHEMA_VERSION.into(),
        set_id: set.set_id.clone(),
        set_revision: set.revision,
        set_sha256: set_sha256.into(),
        set_owner: set.owner.clone(),
        contract_id: compiled.contract_id.clone(),
        contract_revision: compiled.contract_revision,
        status: if incomplete {
            CoverageStatus::Incomplete
        } else {
            CoverageStatus::Complete
        },
        entries,
        additional_requirements,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_documents;

    fn case_001() -> CompiledContract {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-001-shield-search");
        let contract = std::fs::read(root.join("contract.json")).unwrap();
        let registry = std::fs::read(root.join("registry.json")).unwrap();
        compile_documents(&contract, &registry)
            .unwrap()
            .compiled
            .unwrap()
    }

    fn set_json(extra: &str) -> Vec<u8> {
        format!(
            r#"{{
  "schema_version": "avila.core/requirement-set/v0.1-draft",
  "set_id": "test/set", "revision": 1, "owner": "test", "title": "Test set",
  "requirements": [
    {{ "set_requirement_id": "dose", "statement": "dose", "kind": "nuclear.ambient-dose-equivalent-rate", "comparison": "less_than_or_equal", "minimum_basis": "bounded", "omission": "must_state" }},
    {{ "set_requirement_id": "mass", "statement": "mass", "kind": "core.mass", "comparison": "less_than_or_equal", "minimum_basis": "bounded", "omission": "must_state" }},
    {{ "set_requirement_id": "activation", "statement": "activation", "kind": "nuclear.specific-activity", "comparison": "less_than_or_equal", "minimum_basis": "bounded", "omission": "must_state" }},
    {{ "set_requirement_id": "skyshine", "statement": "skyshine", "kind": "nuclear.ambient-dose-equivalent-rate", "comparison": "less_than_or_equal", "minimum_basis": "bounded", "omission": "may_omit" }}{extra}
  ]
}}"#
        )
        .into_bytes()
    }

    fn declaration(json: &str) -> CoverageDeclaration {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_complete_declaration_covers_states_and_permits() {
        let set = parse_requirement_set(&set_json("")).unwrap();
        let report = assess_coverage(
            &case_001(),
            &set,
            "sha256:x",
            &declaration(
                r#"{"mapping":{"dose":["SHIELD-R1-screen","SHIELD-R2-transport"],"mass":["SHIELD-R3-mass"]},
                    "omissions":[{"set_requirement_id":"activation","reason":"no activation capability bound","accepted_by":"owner"}]}"#,
            ),
        );
        assert_eq!(report.status, CoverageStatus::Complete, "{report:?}");
        assert_eq!(report.count(CoverageState::Covered), 2);
        assert_eq!(report.count(CoverageState::OmittedStated), 1);
        assert_eq!(report.count(CoverageState::Omissible), 1);
        let dose = &report.entries[0];
        assert!(
            dose.covered_by
                .iter()
                .any(|cover| cover.requirement_id == "SHIELD-R1-screen" && !cover.adequate)
        );
        assert!(
            dose.covered_by
                .iter()
                .any(|cover| cover.requirement_id == "SHIELD-R2-transport" && cover.adequate)
        );
        assert_eq!(
            report.additional_requirements,
            vec!["SHIELD-R4-thickness".to_string()]
        );
    }

    #[test]
    fn an_unstated_must_state_omission_is_incomplete() {
        let set = parse_requirement_set(&set_json("")).unwrap();
        let report = assess_coverage(
            &case_001(),
            &set,
            "sha256:x",
            &declaration(
                r#"{"mapping":{"dose":["SHIELD-R2-transport"],"mass":["SHIELD-R3-mass"]}}"#,
            ),
        );
        assert_eq!(report.status, CoverageStatus::Incomplete);
        assert_eq!(report.count(CoverageState::OmittedUnstated), 1);
        assert!(report.entries[2].issues[0].contains("no omission is stated"));
    }

    #[test]
    fn coverage_only_on_a_weaker_basis_is_incomplete() {
        let set = parse_requirement_set(&set_json("")).unwrap();
        let report = assess_coverage(
            &case_001(),
            &set,
            "sha256:x",
            &declaration(
                r#"{"mapping":{"dose":["SHIELD-R1-screen"],"mass":["SHIELD-R3-mass"]},
                    "omissions":[{"set_requirement_id":"activation","reason":"r","accepted_by":"o"}]}"#,
            ),
        );
        assert_eq!(report.status, CoverageStatus::Incomplete);
        assert_eq!(report.entries[0].state, CoverageState::CoveredUnderBasis);
    }

    #[test]
    fn declaration_errors_are_reported_and_incomplete() {
        let set = parse_requirement_set(&set_json("")).unwrap();
        let report = assess_coverage(
            &case_001(),
            &set,
            "sha256:x",
            &declaration(
                r#"{"mapping":{"dose":["SHIELD-R3-mass","SHIELD-R9"],"mass":["SHIELD-R3-mass"],"ghost":[]},
                    "omissions":[{"set_requirement_id":"mass","reason":"r","accepted_by":"o"},
                                 {"set_requirement_id":"activation","reason":"","accepted_by":"o"},
                                 {"set_requirement_id":"nope","reason":"r","accepted_by":"o"}]}"#,
            ),
        );
        assert_eq!(report.status, CoverageStatus::Incomplete);
        assert!(report.issues.iter().any(|issue| issue.contains("`ghost`")));
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.contains("both mapped"))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.contains("must state a reason"))
        );
        assert!(report.issues.iter().any(|issue| issue.contains("`nope`")));
        let dose = &report.entries[0];
        assert!(
            dose.issues
                .iter()
                .any(|issue| issue.contains("compares kind"))
        );
        assert!(
            dose.issues
                .iter()
                .any(|issue| issue.contains("does not exist"))
        );
    }

    #[test]
    fn a_malformed_set_is_refused() {
        assert!(parse_requirement_set(b"{}").is_err());
        let duplicate = set_json(
            r#", { "set_requirement_id": "dose", "statement": "again", "kind": "core.mass", "comparison": "less_than_or_equal", "minimum_basis": "bounded", "omission": "must_state" }"#,
        );
        assert!(
            parse_requirement_set(&duplicate)
                .unwrap_err()
                .contains("twice")
        );
    }
}
