//! Deterministic semantic authority for Avila Core.
//!
//! This crate owns exact authoritative values, canonical records, and—over
//! time—the derivation rules identified by a semantic profile. It performs no
//! scientific calculation and grants no scientific qualification.

#![forbid(unsafe_code)]

mod canonical_json;
mod diagnostic;
mod number;
mod predicate;
mod unit;
mod verdict;

pub use canonical_json::{
    CanonicalJsonValue, canonicalize_json, diagnose_authoritative_json, read_authoritative_json,
};
pub use diagnostic::{
    CORE_E7301, CORE_R3301, CORE_S1102, CORE_S1103, CORE_T2001, CORE_T2102, CORE_T2203,
    KernelError, Repair, RepairApplicability,
};
pub use number::{
    EXACT_NUMBER_DECODE_PREFIX, ExactNumber, lower_authored_decimal, read_authoritative_decimal,
    read_authoritative_rational,
};
pub use predicate::{
    ApplicabilityContext, ApplicabilityEvaluator, AttributeSetPredicate, EnvironmentContext,
    FactOperator, FactPredicate, FactRecord, FactSource, FactValue, InputContext, Predicate,
    RangePredicate, SourceRequirement, TruthValue,
};
pub use unit::{CanonicalQuantity, KindDefinition, KindRegistry, Quantity, UnitDefinition};
pub use verdict::{
    Aggregation, AuthoredQuantity, BasisKind, DisplayRounding, EvidenceClaim, EvidenceModel,
    EvidenceState, KernelRequirement, RequirementBasis, RequirementPolicy, RoundingMode,
    VerdictCase, VerdictComparison, VerdictEvaluator, VerdictOutput, VerdictReason, VerdictStatus,
    aggregate_verdicts,
};

/// Draft profile implemented incrementally by this kernel.
pub const SEMANTIC_PROFILE: &str = "avila.core/semantic/0.2-draft";
