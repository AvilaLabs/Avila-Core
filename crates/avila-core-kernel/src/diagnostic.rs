use thiserror::Error;

pub const CORE_S1102: &str = "CORE-S1102";
pub const CORE_S1103: &str = "CORE-S1103";

/// A deterministic kernel refusal.
///
/// Callers match `code`, never the explanatory wording. Source locations and
/// ownership are compiler-layer concerns and will wrap this error later.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code}: {detail}")]
pub struct KernelError {
    code: &'static str,
    detail: String,
}

impl KernelError {
    pub(crate) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
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
}
