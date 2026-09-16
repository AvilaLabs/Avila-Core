//! Bound plans: the strict, content-identified reading of `run --plan`.
//!
//! A plan report's step states say what the workflow *would* do — `planned`
//! means it would attempt execution — without checking the operator supplies
//! an execution needs. The bound plan checks them: the declared capability's
//! bytes are hashed at the supplied path (never executed) and the required
//! environment keys are checked by name, so a step decided `execute` has
//! every identity it depends on bound and verified. Anything short is
//! `blocked` with named reasons, and `unresolved` aggregates the roots and
//! capabilities still needed for the plan to be ready. Binding a plan is
//! read-only; only the normal run path executes or reuses.

use serde::Serialize;

use super::{CaseRunReport, CaseRunStatus, ChangeRecord};

pub const BOUND_PLAN_SCHEMA_VERSION: &str = "avila.core/bound-plan/v0.1-draft";

/// Plan-level readiness for a `--plan` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundPlanStatus {
    /// Every declared step is decided `reuse_committed` or `execute`; the
    /// plan could run as bound.
    Ready,
    /// At least one step is `blocked` on an operator supply; each step's
    /// `blockers` and the plan's `unresolved` name what is missing.
    Blocked,
    /// At least one step is `refused` outright — a structural or policy
    /// refusal, not a missing supply. Findings carry the reason codes.
    Refused,
    /// Planning never produced an execution section: an earlier gate
    /// rejected the workflow, or the package declares no executions.
    /// `note` says which.
    Unavailable,
}

/// One declared step's bound decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundDecision {
    /// The committed receipt's invocation identity equals the planned one
    /// and every recorded output still verifies; nothing needs to run.
    ReuseCommitted,
    /// The step would execute: the invocation is bound, the declared
    /// capability's bytes verified at the supplied path, and every required
    /// environment key is valued.
    Execute,
    /// The step cannot execute as bound; `blockers` name the reasons.
    Blocked,
    /// The step is refused outright — undeclared in the compiled workflow,
    /// an adapter or output-slot mismatch, or a policy refusal.
    Refused,
    /// The compiled step declares no execution: its committed claim is
    /// evaluated as a recorded attestation, or its dossier is materialized
    /// for an optional presentation gate.
    NotExecuted,
}

/// A source root whose artifacts did not all verify, or a capability whose
/// bytes a blocked step still needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedRequirement {
    pub kind: UnresolvedKind,
    pub name: String,
    /// `not_checked`, `missing`, or `mismatch` for a source root;
    /// `not_supplied`, `missing`, or `mismatch` for a capability.
    pub state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedKind {
    SourceRoot,
    Capability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoundStep {
    pub step_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<String>,
    pub decision: BoundDecision,
    /// Identity of the invocation planned from the bound inputs,
    /// parameters, and capability — the value a committed receipt must
    /// equal to be reused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_invocation_sha256: Option<String>,
    /// For `execute`: the SC-12 change classes that defeated committed-
    /// receipt reuse.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<ChangeRecord>,
    /// For `reuse_committed`: the committed receipt document reused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reused_receipt: Option<String>,
    /// Named reasons a `blocked` step cannot run — `capability_not_supplied`,
    /// `capability_missing`, `capability_mismatch`, `inputs_unverified`,
    /// `environment_not_supplied` — or the finding codes a `refused` step
    /// carries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
    /// Environment keys the step's execution requires that were not valued.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_environment: Vec<String>,
    /// The bound check on the supplied capability path: `verified` only when
    /// the file's bytes hash to the pinned executable digest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_state: Option<super::CapabilityCheckState>,
    /// For `not_executed` steps: why no execution is declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A `--plan` run's bound plan: per-step decisions plus the named supplies
/// still missing, derived deterministically from the same report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoundPlan {
    pub schema_version: String,
    pub case_id: String,
    /// Identity of the package manifest this plan was bound against.
    pub manifest_sha256: String,
    /// Identity of the compiled snapshot the plan's invocations derive from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled_snapshot_sha256: Option<String>,
    pub status: BoundPlanStatus,
    /// Every compiled workflow step, in order: declared executions with
    /// their bound decision, then steps with no declared execution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<BoundStep>,
    /// The roots and capabilities the plan still needs; each names a
    /// blocked step's unmet supply or a root whose artifacts did not all
    /// verify.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<UnresolvedRequirement>,
    /// For `unavailable`: why no plan exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl BoundPlan {
    /// The bound plan of a `--plan` run that never reached execution
    /// planning: rejected at an earlier gate, or bound to a package that
    /// declares no executions.
    pub fn unavailable(report: &CaseRunReport) -> Self {
        Self {
            schema_version: BOUND_PLAN_SCHEMA_VERSION.to_string(),
            case_id: report.case_id.clone(),
            manifest_sha256: report.integrity.manifest_sha256.clone(),
            compiled_snapshot_sha256: report
                .compile
                .as_ref()
                .and_then(|compile| compile.compiled.as_ref())
                .map(|compiled| compiled.snapshot_sha256.clone()),
            status: BoundPlanStatus::Unavailable,
            steps: Vec::new(),
            unresolved: Vec::new(),
            note: Some(
                if matches!(report.status, CaseRunStatus::Rejected) {
                    "the workflow was rejected before execution planning; the report's findings name the gate"
                } else {
                    "the package declares no executions; its committed claims are evaluated as recorded attestations"
                }
                .to_string(),
            ),
        }
    }
}
