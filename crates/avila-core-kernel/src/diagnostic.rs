use serde::Serialize;
use thiserror::Error;

pub const CORE_S1102: &str = "CORE-S1102";
pub const CORE_S1103: &str = "CORE-S1103";
pub const CORE_T2001: &str = "CORE-T2001";
pub const CORE_T2102: &str = "CORE-T2102";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairApplicability {
    MechanicallySafe,
    ConstrainedChoice,
    MethodOwnerJudgment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Repair {
    pub applicability: RepairApplicability,
    pub candidates: Vec<String>,
}

/// A deterministic kernel refusal.
///
/// Callers match `code`, never the explanatory wording. Source locations and
/// ownership are compiler-layer concerns and will wrap this error later.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code}: {detail}")]
pub struct KernelError {
    code: &'static str,
    detail: String,
    repair: Option<Repair>,
}

impl KernelError {
    pub(crate) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            repair: None,
        }
    }

    pub(crate) fn with_repair(
        code: &'static str,
        detail: impl Into<String>,
        repair: Repair,
    ) -> Self {
        Self {
            code,
            detail: detail.into(),
            repair: Some(repair),
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    #[must_use]
    pub fn repair(&self) -> Option<&Repair> {
        self.repair.as_ref()
    }
}
