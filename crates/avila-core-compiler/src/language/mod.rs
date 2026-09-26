//! The engineering-language fragment (ADR-0027, profile
//! `avila.core/language/0.1-draft`).
//!
//! `analyze_program` is the shared analysis operation spec §9 requires: it
//! admits a program and its pinned method library at the authoritative
//! boundary, runs the §5–§10 rules — related types, claim acceptance, generic
//! method application, generated obligations, premise discharge, lifecycle —
//! and returns the `avila.core/language-analysis/v0.1-draft` record. It
//! executes nothing; `external` methods are typed by their declared
//! postconditions and their discharge remains a runtime obligation.
//!
//! CLI and agent callers reach the same operation through
//! `avila-core language analyze`; nothing caller-facing bypasses it.

mod analyze;
mod document;
mod eval;
mod model;
mod projection;

pub use analyze::{
    AnalysisOptions, BindingReport, BindingTypeReport, DocumentIdentityReport, FINDING_CODES,
    GoalReport, LanguageAnalysis, LanguageFinding, LibraryIdentityReport, LifecycleEntry,
    LifecycleReport, ObligationReport, PlanReport, PlanStepReport, ProgramIdentityReport,
    RequirementReport, ScopedAssumptionReport, VerdictReport, analyze_program,
};
pub use document::{
    ANALYSIS_SCHEMA_VERSION, LANGUAGE_PROFILE, LIBRARY_SCHEMA_VERSION, PROGRAM_SCHEMA_VERSION,
};
pub use projection::{LanguageDocument, ProjectionError, project_document};
