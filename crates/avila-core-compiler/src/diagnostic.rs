use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CORE_S1101: &str = "CORE-S1101";
pub const CORE_S1102: &str = "CORE-S1102";
pub const CORE_S1103: &str = "CORE-S1103";
pub const CORE_S1301: &str = "CORE-S1301";
pub const CORE_A4201: &str = "CORE-A4201";
pub const CORE_A4301: &str = "CORE-A4301";
pub const CORE_E7001: &str = "CORE-E7001";
pub const CORE_E7002: &str = "CORE-E7002";
pub const CORE_E7101: &str = "CORE-E7101";
pub const CORE_E7103: &str = "CORE-E7103";
pub const CORE_E7201: &str = "CORE-E7201";
pub const CORE_E7301: &str = "CORE-E7301";
pub const CORE_E7401: &str = "CORE-E7401";
pub const CORE_R3101: &str = "CORE-R3101";
pub const CORE_R3102: &str = "CORE-R3102";
pub const CORE_R3201: &str = "CORE-R3201";
pub const CORE_R3202: &str = "CORE-R3202";
pub const CORE_R3203: &str = "CORE-R3203";
pub const CORE_R3301: &str = "CORE-R3301";
pub const CORE_R3401: &str = "CORE-R3401";
pub const CORE_R3501: &str = "CORE-R3501";
pub const CORE_R3601: &str = "CORE-R3601";
pub const CORE_R3602: &str = "CORE-R3602";
pub const CORE_T2001: &str = "CORE-T2001";
pub const CORE_T2101: &str = "CORE-T2101";
pub const CORE_T2102: &str = "CORE-T2102";
pub const CORE_T2103: &str = "CORE-T2103";
pub const CORE_T2104: &str = "CORE-T2104";
pub const CORE_T2201: &str = "CORE-T2201";
pub const CORE_T2203: &str = "CORE-T2203";
pub const CORE_T2301: &str = "CORE-T2301";
pub const CORE_T2401: &str = "CORE-T2401";
pub const CORE_T2402: &str = "CORE-T2402";
pub const CORE_T2501: &str = "CORE-T2501";
pub const CORE_T2601: &str = "CORE-T2601";

/// Every code a compile report can carry, including `CORE-S1103`, which the
/// authoritative reader raises for a duplicate object key.
pub const COMPILER_FINDING_CODES: &[&str] = &[
    CORE_A4201, CORE_A4301, CORE_E7001, CORE_E7002, CORE_E7101, CORE_E7103, CORE_E7201, CORE_E7301,
    CORE_E7401, CORE_R3101, CORE_R3102, CORE_R3201, CORE_R3202, CORE_R3203, CORE_R3301, CORE_R3401,
    CORE_R3501, CORE_R3601, CORE_R3602, CORE_S1101, CORE_S1102, CORE_S1103, CORE_S1301, CORE_T2001,
    CORE_T2101, CORE_T2102, CORE_T2103, CORE_T2104, CORE_T2201, CORE_T2203, CORE_T2301, CORE_T2401,
    CORE_T2402, CORE_T2501, CORE_T2601,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingClass {
    Missing,
    Invalid,
    Unsatisfied,
    Inadmissible,
    Notice,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLocation {
    pub document: String,
    pub pointer: String,
}

impl SourceLocation {
    #[must_use]
    pub fn new(document: impl Into<String>, pointer: impl Into<String>) -> Self {
        Self {
            document: document.into(),
            pointer: pointer.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairApplicability {
    MechanicallySafe,
    ConstrainedChoice,
    MethodOwnerJudgment,
}

/// One edit of a repair alternative, in RFC 6902 JSON Patch form, so any
/// tool that applies JSON Patch can apply a repair without understanding it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepairEdit {
    Replace { path: String, value: Value },
    Add { path: String, value: Value },
    Remove { path: String },
}

/// A bounded repair: what kind of authority may apply it, a human label per
/// alternative, and, when the compiler can state the exact bytes, the edits
/// that realize each alternative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticRepair {
    pub applicability: RepairApplicability,
    /// One label per alternative.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
    /// Index-aligned with `candidates` when present: `edits[i]` realizes
    /// `candidates[i]`. An alternative whose exact bytes the compiler cannot
    /// state has an empty patch. Absent entirely when no alternative has one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<Vec<RepairEdit>>,
}

impl DiagnosticRepair {
    /// Alternatives the compiler can only name, such as capability types a
    /// method owner might add.
    #[must_use]
    pub fn labels(applicability: RepairApplicability, candidates: Vec<String>) -> Self {
        Self {
            applicability,
            candidates,
            edits: Vec::new(),
        }
    }

    /// Alternatives that each replace the value at `path` with one candidate.
    #[must_use]
    pub fn replacements(
        applicability: RepairApplicability,
        path: &str,
        candidates: Vec<String>,
    ) -> Self {
        let edits = candidates
            .iter()
            .map(|candidate| {
                vec![RepairEdit::Replace {
                    path: path.to_owned(),
                    value: Value::String(candidate.clone()),
                }]
            })
            .collect();
        Self {
            applicability,
            candidates,
            edits,
        }
    }

    /// A single alternative that removes the value at `path`.
    #[must_use]
    pub fn removal(
        applicability: RepairApplicability,
        label: impl Into<String>,
        path: &str,
    ) -> Self {
        Self {
            applicability,
            candidates: vec![label.into()],
            edits: vec![vec![RepairEdit::Remove {
                path: path.to_owned(),
            }]],
        }
    }

    /// Appends one alternative with its label and edits.
    #[must_use]
    pub fn alternative(mut self, label: impl Into<String>, edits: Vec<RepairEdit>) -> Self {
        self.candidates.push(label.into());
        self.edits.push(edits);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreDiagnostic {
    pub code: String,
    pub class: FindingClass,
    pub owner: String,
    pub primary: SourceLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<SourceLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repairs: Vec<DiagnosticRepair>,
    pub message: String,
}

impl CoreDiagnostic {
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        class: FindingClass,
        owner: impl Into<String>,
        primary: SourceLocation,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            class,
            owner: owner.into(),
            primary,
            related: Vec::new(),
            repairs: Vec::new(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn with_related(mut self, related: Vec<SourceLocation>) -> Self {
        self.related = related;
        self
    }

    #[must_use]
    pub fn with_repair(mut self, repair: DiagnosticRepair) -> Self {
        self.repairs.push(repair);
        self
    }

    #[must_use]
    pub const fn blocks_compilation(&self) -> bool {
        !matches!(self.class, FindingClass::Notice)
    }
}
