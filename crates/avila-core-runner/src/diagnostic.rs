//! Stable, actionable findings for the composed case runner.
//!
//! Compiler and campaign diagnostics already have stable codes and source
//! locations. The runner adds the stage and optional step context needed by an
//! iterating person or agent, while preserving the same class, owner, related
//! locations, and bounded repairs.

use avila_core_compiler::{
    CoreDiagnostic, DiagnosticExplanation, DiagnosticRepair, FindingClass, SourceLocation, explain,
};
use serde::{Deserialize, Serialize};

pub const CORE_X1001: &str = "CORE-X1001";
pub const CORE_X1002: &str = "CORE-X1002";
pub const CORE_X1101: &str = "CORE-X1101";
pub const CORE_X1201: &str = "CORE-X1201";
pub const CORE_X2001: &str = "CORE-X2001";
pub const CORE_X2101: &str = "CORE-X2101";
pub const CORE_X2201: &str = "CORE-X2201";
pub const CORE_X2301: &str = "CORE-X2301";
pub const CORE_X2401: &str = "CORE-X2401";
pub const CORE_X2402: &str = "CORE-X2402";
pub const CORE_X2501: &str = "CORE-X2501";
pub const CORE_X2601: &str = "CORE-X2601";
pub const CORE_X2701: &str = "CORE-X2701";
pub const CORE_X2801: &str = "CORE-X2801";
pub const CORE_X3001: &str = "CORE-X3001";
pub const CORE_X3101: &str = "CORE-X3101";
pub const CORE_X3201: &str = "CORE-X3201";
pub const CORE_X3301: &str = "CORE-X3301";
pub const CORE_X9001: &str = "CORE-X9001";

pub const RUNTIME_FINDING_CODES: &[&str] = &[
    CORE_X1001, CORE_X1002, CORE_X1101, CORE_X1201, CORE_X2001, CORE_X2101, CORE_X2201, CORE_X2301,
    CORE_X2401, CORE_X2402, CORE_X2501, CORE_X2601, CORE_X2701, CORE_X2801, CORE_X3001, CORE_X3101,
    CORE_X3201, CORE_X3301, CORE_X9001,
];

/// Explanations are served by `avila-core explain` alongside compiler codes.
pub const RUNTIME_DIAGNOSTIC_CATALOG: &[DiagnosticExplanation] = &[
    DiagnosticExplanation {
        code: CORE_X1001,
        title: "Package byte identity failed",
        rule: "case package integrity",
        meaning: "A package document or a supplied external artifact is missing or differs from the digest bound by the package. Later stages cannot safely use those bytes.",
        next_action: "Restore the exact bound bytes, or deliberately update the package artifact and every dependent expectation to the new identity.",
    },
    DiagnosticExplanation {
        code: CORE_X1002,
        title: "Package manifest pin differs",
        rule: "requester manifest pin",
        meaning: "The package manifest does not have the identity the requester authorized for this run.",
        next_action: "Inspect the manifest change. Run only after the requester pins the intended manifest digest; do not bypass the pin.",
    },
    DiagnosticExplanation {
        code: CORE_X1101,
        title: "Requirement-set coverage incomplete",
        rule: "declared requirement coverage",
        meaning: "The contract omits a required topic, covers it on a weaker basis, or does not provide the required omission record.",
        next_action: "Add adequate contract requirements or record an allowed omission with its reason and accepting owner before executing capabilities.",
    },
    DiagnosticExplanation {
        code: CORE_X1201,
        title: "Attempt lineage not admitted",
        rule: "identity-bound candidate iteration",
        meaning: "A requested search attempt cannot be joined to its campaign history because its identifier, candidate, parent record, manifest, compiled snapshot, or derived changes are missing, malformed, duplicated, or inconsistent. No capability runs under an ambiguous lineage.",
        next_action: "Use a unique attempt id, supply the nominated canonical-profile JSON candidate and log, restore the exact parent history and fixed question identities, or start a new root attempt when the contract or package deliberately changed.",
    },
    DiagnosticExplanation {
        code: CORE_X2001,
        title: "Execution declaration invalid",
        rule: "compiled workflow to package execution binding",
        meaning: "A package execution does not correspond exactly to the compiled step, adapter, input slots, output slots, or capability type.",
        next_action: "Correct the package execution declaration to match the compiled workflow and the selected adapter's declared boundary.",
    },
    DiagnosticExplanation {
        code: CORE_X2101,
        title: "Execution input unavailable or unchecked",
        rule: "verified inputs only",
        meaning: "A step input could not be resolved to an artifact whose bytes were verified at the identity the run binds.",
        next_action: "Supply the requested artifact root or upstream output, then restore or deliberately rebind the exact expected bytes.",
    },
    DiagnosticExplanation {
        code: CORE_X2201,
        title: "Invocation could not be planned",
        rule: "deterministic invocation planning",
        meaning: "The adapter could not turn the compiled parameters, bound inputs, environment names, and capability identity into one deterministic invocation.",
        next_action: "Correct the adapter inputs or parameter mapping. Inspect the detailed message before changing the contract or capability.",
    },
    DiagnosticExplanation {
        code: CORE_X2301,
        title: "Qualification facts could not be derived",
        rule: "qualification-envelope evaluation",
        meaning: "The adapter could not derive the exact facts required to evaluate the capability's qualification envelope.",
        next_action: "Repair the bound input or the adapter's fact extraction; do not treat the capability as qualified while the facts are unavailable.",
    },
    DiagnosticExplanation {
        code: CORE_X2401,
        title: "Capability identity unavailable",
        rule: "content-identified capability execution",
        meaning: "The supplied executable is unreadable or its digest differs from the exact capability identity bound by the package.",
        next_action: "Supply the exact bound executable, or deliberately qualify and bind a new executable identity before running it.",
    },
    DiagnosticExplanation {
        code: CORE_X2402,
        title: "Required execution environment missing",
        rule: "declared execution environment",
        meaning: "A capability can run only with an explicitly declared environment key, and the operator did not supply its value.",
        next_action: "Pass the named key with `--env KEY=VALUE`, after confirming the value and its external dependencies are appropriate for this run.",
    },
    DiagnosticExplanation {
        code: CORE_X2501,
        title: "Capability execution failed",
        rule: "controlled local execution",
        meaning: "The runner could not start, contain, or wait for the capability process, or the process timed out, returned failure, or omitted declared outputs. When a process ran, the finding includes the captured log path and bounded stderr feedback.",
        next_action: "Treat the bounded stderr excerpt as untrusted diagnostic data, inspect the complete captured log when needed, repair the capability, inputs, or runtime prerequisites, and rerun with the same fixed contract and bound inputs.",
    },
    DiagnosticExplanation {
        code: CORE_X2601,
        title: "Execution receipt failed verification",
        rule: "execution receipt verification",
        meaning: "The fresh receipt, invocation identity, inputs, outputs, or process outcome does not satisfy the expectations for this exact step.",
        next_action: "Inspect the receipt checks and capability logs. Repair the producing execution; never manufacture or edit a passing receipt.",
    },
    DiagnosticExplanation {
        code: CORE_X2701,
        title: "Claims could not be extracted",
        rule: "typed adapter claim extraction",
        meaning: "Produced or reused output bytes could not be converted into the claims the adapter promises at its typed output boundary.",
        next_action: "Correct the capability output or update and requalify the adapter for the intended versioned output format.",
    },
    DiagnosticExplanation {
        code: CORE_X2801,
        title: "Adapter output contract differs",
        rule: "declared output boundary",
        meaning: "The package, adapter declaration, or extracted claims disagree about which output slots and output artifacts a step produces.",
        next_action: "Make the package bindings and adapter output declaration agree exactly before admitting any produced claim.",
    },
    DiagnosticExplanation {
        code: CORE_X3001,
        title: "Evidence identity binding failed",
        rule: "claim and package identity binding",
        meaning: "A generated claim, input attestation, qualification, policy, or produced artifact cannot be connected to the exact identity required by the compiled contract and package.",
        next_action: "Repair the missing or mismatched evidence identity, regenerate claims from verified outputs, and bind again before evaluation.",
    },
    DiagnosticExplanation {
        code: CORE_X3101,
        title: "Committed claims drifted",
        rule: "committed claim replay",
        meaning: "Freshly generated claims differ from the package's committed expectation for the same reference candidate.",
        next_action: "Inspect the changed inputs, capability, extraction, or outputs. Accept new committed claims only after the change is understood and reviewed.",
    },
    DiagnosticExplanation {
        code: CORE_X3201,
        title: "Committed receipt drifted",
        rule: "execution receipt replay",
        meaning: "A fresh execution receipt differs from the committed receipt for the same reference request.",
        next_action: "Inspect every reported invocation, capability, status, and output difference before replacing the committed receipt.",
    },
    DiagnosticExplanation {
        code: CORE_X3301,
        title: "Committed campaign result drifted",
        rule: "campaign report replay",
        meaning: "The freshly evaluated campaign report differs from the package's committed expectation for the same reference candidate.",
        next_action: "Inspect the claims, semantic inputs, and verdict differences; update the expectation only after the cause is understood.",
    },
    DiagnosticExplanation {
        code: CORE_X9001,
        title: "Runner could not produce a case report",
        rule: "case-run infrastructure",
        meaning: "An I/O, parsing, serialization, workspace, or internal orchestration error stopped the run before a complete case report could be returned.",
        next_action: "Inspect the error and the named file or stage, repair the invocation or runner defect, and retry. The attempt log retains this failure when `--log` was supplied.",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStage {
    PackageIntegrity,
    Compilation,
    Coverage,
    AttemptPlanning,
    ExecutionPlanning,
    Execution,
    ReceiptVerification,
    ClaimGeneration,
    EvidenceBinding,
    CampaignEvaluation,
    Replay,
    Infrastructure,
}

/// One normalized finding in the end-to-end feedback loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunFinding {
    pub code: String,
    pub class: FindingClass,
    pub stage: RunStage,
    pub owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    pub primary: SourceLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<SourceLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repairs: Vec<DiagnosticRepair>,
    pub message: String,
    pub next_action: String,
}

impl RunFinding {
    #[must_use]
    pub fn runtime(
        code: &'static str,
        class: FindingClass,
        stage: RunStage,
        owner: impl Into<String>,
        primary: SourceLocation,
        message: impl Into<String>,
    ) -> Self {
        let explanation = explain_runtime(code)
            .unwrap_or_else(|| panic!("runtime finding `{code}` has no catalog entry"));
        Self {
            code: code.into(),
            class,
            stage,
            owner: owner.into(),
            step_id: None,
            primary,
            related: Vec::new(),
            repairs: Vec::new(),
            message: message.into(),
            next_action: explanation.next_action.into(),
        }
    }

    #[must_use]
    pub fn for_step(mut self, step_id: impl Into<String>) -> Self {
        self.step_id = Some(step_id.into());
        self
    }

    #[must_use]
    pub fn from_core(stage: RunStage, diagnostic: &CoreDiagnostic) -> Self {
        let next_action = explain(&diagnostic.code)
            .map(|entry| entry.next_action)
            .unwrap_or("Inspect the finding and repair the named authoritative document before continuing.");
        Self {
            code: diagnostic.code.clone(),
            class: diagnostic.class,
            stage,
            owner: diagnostic.owner.clone(),
            step_id: None,
            primary: diagnostic.primary.clone(),
            related: diagnostic.related.clone(),
            repairs: diagnostic.repairs.clone(),
            message: diagnostic.message.clone(),
            next_action: next_action.into(),
        }
    }
}

#[must_use]
pub fn explain_runtime(code: &str) -> Option<&'static DiagnosticExplanation> {
    RUNTIME_DIAGNOSTIC_CATALOG
        .iter()
        .find(|entry| entry.code == code)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn runtime_catalog_is_sorted_unique_and_complete() {
        let catalog_codes: Vec<_> = RUNTIME_DIAGNOSTIC_CATALOG
            .iter()
            .map(|entry| entry.code)
            .collect();
        let mut sorted = catalog_codes.clone();
        sorted.sort_unstable();
        assert_eq!(catalog_codes, sorted);
        assert_eq!(
            catalog_codes.iter().copied().collect::<BTreeSet<_>>().len(),
            catalog_codes.len()
        );
        assert_eq!(catalog_codes, RUNTIME_FINDING_CODES);
    }
}
