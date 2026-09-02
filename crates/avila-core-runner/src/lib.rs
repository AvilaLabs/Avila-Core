//! The composed case workflow and its case-specific runner.
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

mod case_run;
mod execute;

pub use case_run::{
    BindingReport, BindingStatus, CapabilityCheck, CapabilityCheckState, CaseRunOptions,
    CaseRunReport, CaseRunStatus, ChangeClass, ChangeRecord, ClaimsReport, ExecutionReport,
    ExecutionStatus, NotExecutedStep, OutputReport, PresentationGateReadiness,
    PresentationGateReport, PresentedEvidence, ReceiptReplayReport, ReceiptSummary, ReplayReport,
    StagedInputReport, StepExecutionReport, StepExecutionState, SuppliedInput, VerdictMargin,
    display_number, execute_case, human_summary, parse_capabilities, parse_environment,
    parse_inputs, parse_named_paths, parse_source_roots,
};
pub use execute::{Adapter, AdapterOutput, ExtractedClaim, RUNNER_ID, StepContext};
