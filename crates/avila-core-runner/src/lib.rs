//! The composed case workflow and controlled runner.
//!
//! This crate performs I/O: it re-hashes bytes at explicitly supplied roots,
//! stages verified inputs into a fresh workspace, runs exact executables
//! through named adapters, writes and verifies execution receipts, reuses
//! steps whose committed receipts still describe the planned invocation, and
//! generates the claims document before handing evaluation to the I/O-free
//! compiler and kernel. Every front end (the CLI, the desktop shell) drives
//! the same functions and renders the same report; none has semantics of
//! its own.

#![forbid(unsafe_code)]

mod attempt;
mod case_run;
mod diagnostic;
mod execute;

pub use attempt::{
    ATTEMPT_COMPARISON_SCHEMA_VERSION, ATTEMPT_LINEAGE_SCHEMA_VERSION, AttemptChange,
    AttemptComparison, AttemptLineageRequest, AttemptMarginComparison, AttemptMarginUnavailable,
    AttemptMarginUnavailableReason, AttemptRecord, AttemptVerdictTransition,
    AttemptVerdictUnavailable, AttemptVerdictUnavailableReason,
};
pub use case_run::{
    BindingReport, BindingStatus, CapabilityCheck, CapabilityCheckState, CaseRunOptions,
    CaseRunReport, CaseRunStatus, ChangeClass, ChangeRecord, ClaimsReport, ExecutionReport,
    ExecutionStatus, NotExecutedStep, OutputReport, PresentationGateReadiness,
    PresentationGateReport, PresentedEvidence, ReceiptReplayReport, ReceiptSummary, ReplayReport,
    StagedInputReport, StepExecutionReport, StepExecutionState, SuppliedInput, VerdictMargin,
    display_number, execute_case, human_summary, parse_capabilities, parse_environment,
    parse_inputs, parse_named_paths, parse_source_roots,
};
pub use diagnostic::{
    CORE_X1001, CORE_X1002, CORE_X1101, CORE_X1201, CORE_X2001, CORE_X2101, CORE_X2201, CORE_X2301,
    CORE_X2401, CORE_X2402, CORE_X2501, CORE_X2601, CORE_X2701, CORE_X2801, CORE_X3001, CORE_X3101,
    CORE_X3201, CORE_X3301, CORE_X9001, RUNTIME_DIAGNOSTIC_CATALOG, RUNTIME_FINDING_CODES,
    RunFinding, RunStage, explain_runtime,
};
pub use execute::external_checker::{
    EXTERNAL_CHECKER_ADAPTER_SCHEMA_VERSION, EXTERNAL_CHECKER_DOCUMENT_ROLE, ExternalArgument,
    ExternalCheckerAdapter, ExternalClaim, ExternalOutput,
};
pub use execute::{Adapter, AdapterOutput, ExtractedClaim, RUNNER_ID, StepContext};
