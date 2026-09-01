use serde::{Deserialize, Serialize};

pub const CORE_S1101: &str = "CORE-S1101";
pub const CORE_S1102: &str = "CORE-S1102";
pub const CORE_S1301: &str = "CORE-S1301";
pub const CORE_A4301: &str = "CORE-A4301";
pub const CORE_R3101: &str = "CORE-R3101";
pub const CORE_R3102: &str = "CORE-R3102";
pub const CORE_R3201: &str = "CORE-R3201";
pub const CORE_R3202: &str = "CORE-R3202";
pub const CORE_R3203: &str = "CORE-R3203";
pub const CORE_R3301: &str = "CORE-R3301";
pub const CORE_R3401: &str = "CORE-R3401";
pub const CORE_R3501: &str = "CORE-R3501";
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticRepair {
    pub applicability: RepairApplicability,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
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
