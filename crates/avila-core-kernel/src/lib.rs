//! Deterministic semantic authority for Avila Core.
//!
//! This crate owns exact authoritative values, canonical records, and—over
//! time—the derivation rules identified by a semantic profile. It performs no
//! scientific calculation and grants no scientific qualification.

#![forbid(unsafe_code)]

mod canonical_json;
mod diagnostic;
mod number;

pub use canonical_json::{CanonicalJsonValue, canonicalize_json, read_authoritative_json};
pub use diagnostic::{CORE_S1102, CORE_S1103, KernelError};
pub use number::{
    ExactNumber, lower_authored_decimal, read_authoritative_decimal, read_authoritative_rational,
};

/// Draft profile implemented incrementally by this kernel.
pub const SEMANTIC_PROFILE: &str = "avila.core/semantic/0.2-draft";
