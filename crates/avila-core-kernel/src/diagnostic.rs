use serde::Serialize;
use thiserror::Error;

pub const CORE_S1102: &str = "CORE-S1102";
pub const CORE_S1103: &str = "CORE-S1103";
pub const CORE_T2001: &str = "CORE-T2001";
pub const CORE_T2102: &str = "CORE-T2102";
pub const CORE_T2203: &str = "CORE-T2203";
pub const CORE_R3301: &str = "CORE-R3301";
pub const CORE_E7301: &str = "CORE-E7301";

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
/// Callers match `code`, never the explanatory wording. Ownership is a
/// compiler-layer concern. A refusal raised while reading a document names the
/// offending value by JSON Pointer so the compiler can report an exact source
/// location; refusals raised over already-typed values carry no pointer.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code}: {detail}")]
pub struct KernelError {
    code: &'static str,
    detail: String,
    repair: Option<Repair>,
    pointer: Option<String>,
}

impl KernelError {
    pub(crate) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            repair: None,
            pointer: None,
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
            pointer: None,
        }
    }

    pub(crate) fn at_pointer(mut self, pointer: impl Into<String>) -> Self {
        self.pointer = Some(pointer.into());
        self
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

    /// JSON Pointer of the offending value when the refusal was raised while
    /// reading a document; `None` for refusals over already-typed values.
    #[must_use]
    pub fn pointer(&self) -> Option<&str> {
        self.pointer.as_deref()
    }
}
