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
pub const CORE_X1003: &str = "CORE-X1003";
pub const CORE_X1004: &str = "CORE-X1004";
pub const CORE_X1005: &str = "CORE-X1005";
pub const CORE_X1101: &str = "CORE-X1101";
pub const CORE_X1201: &str = "CORE-X1201";
pub const CORE_X1301: &str = "CORE-X1301";
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
pub const CORE_X3401: &str = "CORE-X3401";
pub const CORE_X3404: &str = "CORE-X3404";
pub const CORE_X3405: &str = "CORE-X3405";
pub const CORE_X6401: &str = "CORE-X6401";
pub const CORE_X6402: &str = "CORE-X6402";
pub const CORE_X6403: &str = "CORE-X6403";
pub const CORE_X6404: &str = "CORE-X6404";
pub const CORE_X6501: &str = "CORE-X6501";
pub const CORE_X6502: &str = "CORE-X6502";
pub const CORE_X9001: &str = "CORE-X9001";
pub const CORE_P5101: &str = "CORE-P5101";
pub const CORE_P5102: &str = "CORE-P5102";
pub const CORE_P5103: &str = "CORE-P5103";
pub const CORE_P5201: &str = "CORE-P5201";
pub const CORE_P5301: &str = "CORE-P5301";
pub const CORE_P5302: &str = "CORE-P5302";
pub const CORE_P5303: &str = "CORE-P5303";
pub const CORE_P5304: &str = "CORE-P5304";
pub const CORE_P5401: &str = "CORE-P5401";
pub const CORE_P5501: &str = "CORE-P5501";
pub const CORE_P5601: &str = "CORE-P5601";
pub const CORE_P5602: &str = "CORE-P5602";

pub const RUNTIME_FINDING_CODES: &[&str] = &[
    CORE_P5101, CORE_P5102, CORE_P5103, CORE_P5201, CORE_P5301, CORE_P5302, CORE_P5303, CORE_P5304,
    CORE_P5401, CORE_P5501, CORE_P5601, CORE_P5602, CORE_X1001, CORE_X1002, CORE_X1003, CORE_X1004,
    CORE_X1005, CORE_X1101, CORE_X1201, CORE_X1301, CORE_X2001, CORE_X2101, CORE_X2201, CORE_X2301,
    CORE_X2401, CORE_X2402, CORE_X2501, CORE_X2601, CORE_X2701, CORE_X2801, CORE_X3001, CORE_X3101,
    CORE_X3201, CORE_X3301, CORE_X3401, CORE_X3404, CORE_X3405, CORE_X6401, CORE_X6402, CORE_X6403,
    CORE_X6404, CORE_X6501, CORE_X6502, CORE_X9001,
];

/// Explanations are served by `avila-core explain` alongside compiler codes.
pub const RUNTIME_DIAGNOSTIC_CATALOG: &[DiagnosticExplanation] = &[
    DiagnosticExplanation {
        code: CORE_P5101,
        title: "No eligible capability candidate",
        rule: "SC-8.6: recorded selection",
        meaning: "A step's bound `capability_selection` record lists candidates but none carries decision `selected` — every considered implementation was excluded or inadmissible. The refusal carries each candidate's recorded reasons; nothing may execute for the step.",
        next_action: "Record a selection that admits a candidate, widen the candidate set in a new record, or loosen the contract policy that made every candidate inadmissible. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5102,
        title: "Bound capability differs from the recorded selection",
        rule: "SC-8.6: recorded selection",
        meaning: "The candidate a `capability_selection` record marks `selected` does not equal the capability the manifest binds for the step — the capability type, capability id, adapter, or executable digest differs. The record and the package disagree, so the selection proves nothing about what would run.",
        next_action: "Re-record the selection against the bound implementation, or re-pin the manifest to the recorded selection. The owner is the requester.",
    },
    DiagnosticExplanation {
        code: CORE_P5103,
        title: "Selection record required but absent",
        rule: "SC-8.6: recorded selection",
        meaning: "The contract's execution policy declares provider-selection rules, but no `capability_selection` document is bound, a step has no selection entry, or — under `require_signatures` — the record carries no requester signature that verifies. A policy without its record cannot be checked.",
        next_action: "Bind and sign a `capability_selection` document recording every step's candidates and decisions. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5201,
        title: "Candidate excluded by contract constraint",
        rule: "SC-8.4: legitimate criteria",
        meaning: "A selection record marks a candidate `excluded` with reason `contract_constraint` — the candidate lost to a contract rule, not to a merit ranking. Informational: the exclusion is visible rather than hidden in a score.",
        next_action: "None required; the record is honest. Loosen the constraint in a contract revision if the excluded candidate should compete.",
    },
    DiagnosticExplanation {
        code: CORE_P5301,
        title: "Selected provider denied or not allowed",
        rule: "SC-8.4: admissibility before optimization",
        meaning: "The selected candidate's capability-type owner appears in `execution_policy.deny_providers`, or `allow_providers` is declared and the owner is not in it. Provider constraints gate admissibility — an inadmissible implementation cannot win on any merit.",
        next_action: "Select an implementation from a permitted provider, or revise the contract's provider lists. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5302,
        title: "Provider independence violated",
        rule: "SC-8: organization policy",
        meaning: "`execution_policy.require_provider_independence` is declared but two steps selected capability types sharing an owner — the same provider would serve both, so a provider-level failure or bias could reach both steps at once.",
        next_action: "Select a differently-owned implementation for one step, or drop the independence declaration. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5303,
        title: "Declared maturity below the policy floor",
        rule: "SC-8.5: maturity is a policy fact",
        meaning: "The selected capability type's registry-declared `maturity` is below `execution_policy.maturity_floor`, or the type declares no maturity at all. Maturity gates admissibility as a declared fact — it is never read as a quality score.",
        next_action: "Select a type whose provider declares a qualifying maturity, declare the maturity in a registry revision, or lower the floor. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5304,
        title: "Diverse implementations required",
        rule: "SC-8: organization policy",
        meaning: "`execution_policy.require_diverse_implementations` is declared but two steps selected the same executable digest — identical bytes cannot catch an implementation fault the other shares.",
        next_action: "Select a distinct implementation for one step, or drop the diversity declaration. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5401,
        title: "Cost cap exceeded without recorded confirmation",
        rule: "SC-8.4: visible cost criteria",
        meaning: "The selected cost estimate exceeds `execution_policy.cost_cap` and the record carries no `cost_confirmed_by`. Cost may legitimately drive selection, but a breach of the declared cap requires a recorded human confirmation before execution.",
        next_action: "Record the confirmation in the selection document, select a cheaper candidate, or raise the cap in a contract revision. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5501,
        title: "Avila-provided selection without self-preference check",
        rule: "SC-8.7: self-preference disclosure",
        meaning: "`execution_policy.forbid_self_preference` is declared and the selected implementation is `avila_provided`, but the record carries no `self_preference_check`. Choosing one's own implementation is not forbidden — hiding the check applied is.",
        next_action: "Record the self-preference check and its justification in the selection document, or select a non-Avila implementation. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5601,
        title: "Forbidden selection criterion",
        rule: "SC-8.4: legitimate criteria",
        meaning: "A selection record's `criteria` names a value outside the legitimate vocabulary (`cost`, `time`, `locality`, `technical`, `diversity`, `preference`) — including the banned `provider_payment` and `avila_margin`, which may never rank a selection, visibly or otherwise.",
        next_action: "Re-record the selection with only legitimate criteria. Provider payment and Avila margin can never appear. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: CORE_P5602,
        title: "Candidate without a recorded decision",
        rule: "SC-8.6: every candidate decided",
        meaning: "A selection record lists a candidate that carries no `decision` or no `reasons` — a considered implementation whose fate is unrecorded defeats the audit the record exists for.",
        next_action: "Record every considered candidate's decision and reasons. The owner is the policy owner.",
    },
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
        code: CORE_X1003,
        title: "Hash cache could not be used or updated",
        rule: "opt-in verified-hash cache (S-038)",
        meaning: "The `--hash-cache` file could not be read as this schema's JSON, or a fresh entry could not be written back to it. Every artifact this run needed was still hashed from bytes as if no cache were supplied; nothing about package integrity is weakened by this notice.",
        next_action: "Inspect the named cache file. Delete it to let Core rebuild it from a clean state, or repair the path's permissions; the run's verdicts do not depend on this file.",
    },
    DiagnosticExplanation {
        code: CORE_X1004,
        title: "Package manifest signature not verified",
        rule: "ADR-0015 clause 3: signed manifests",
        meaning: "`run --trust-root FILE` was supplied and the package manifest's bound `signature` document (role `manifest`) is missing, internally inconsistent, made with a key not listed under the `requester` role in the supplied trust root, or does not cryptographically verify. A package whose manifest signature does not verify against a listed requester key is refused before compilation, exactly where the requester's manifest pin (S-030, `CORE-X1002`) refuses today.",
        next_action: "Sign the manifest with a requester key listed in the trust root (`avila-core sign manifest`), or supply the correct trust root. Never bypass this by omitting `--trust-root`.",
    },
    DiagnosticExplanation {
        code: CORE_X1005,
        title: "Signed execution required",
        rule: "ADR-0015 clause 7: execution_policy.require_signatures",
        meaning: "The compiled contract's `execution_policy.require_signatures` is true, and either no `--trust-root` was supplied at all, or a declared execution step's operative evidence (a reused, freshly executed, or not-run receipt) carries no signature verified against a listed runner key. The default, permissive behavior of falling back to a rerun or a visible `not_run` step is not available for a contract that requires signed execution: the whole run is refused instead.",
        next_action: "Supply `--trust-root FILE` naming the requester and runner keys this contract requires, and produce every step's receipt through a run signed with `--runner-key FILE`, or set `require_signatures` to false only with the policy owner's deliberate agreement to accept unsigned evidence.",
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
        code: CORE_X1301,
        title: "Free input violates its declared schema",
        rule: "registry-declared role input_schema",
        meaning: "A supplied free input (`run --input NAME=PATH`) fills a role that declares an `input_schema`, and the file's content does not satisfy it: an unknown key, a wrong value family, a missing required property, a value outside its `enum` or `const`, no matching `oneOf` branch, or a non-canonical decimal where one is required. The run is refused before anything is staged or executed.",
        next_action: "Correct the free input at the reported pointer to satisfy its role's declared schema, then rerun. The owner is the requester who supplied the input.",
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
        code: CORE_X3401,
        title: "Reuse rule refused",
        rule: "SC-12.3 reuse rules",
        meaning: "A `reuse_rule` document could not authorize reuse: it was unsigned or its signature did not verify against a requester key in the supplied trust root, its `not_after` had passed, its scope names a step or bound input slot the compiled contract does not contain (a rule can only narrow, never widen), or the document could not be evaluated at all. The refused rule is inapplicable — default invalidation runs and the affected step reruns.",
        next_action: "Re-sign the rule with a listed requester key, renew its expiry, or correct its scope to a real binding edge; otherwise accept the rerun.",
    },
    DiagnosticExplanation {
        code: CORE_X3404,
        title: "Qualification record refused under recognition policy",
        rule: "SC-7; execution_policy.recognized_qualification_owners",
        meaning: "The contract names issuers whose qualification records are recognized, and this bound record's owner is listed, but the record carries no signature document over its bound bytes that verifies under the issuer's declared key — it is unsigned, its signature covers different bytes, or the signature does not verify. The record is not applied: the step's claims carry no qualification assessment from it. A record whose owner is not listed loads normally and is refused later as unrecognized evidence (`CORE-A4601`).",
        next_action: "Sign the record with the issuer key the contract declares (`avila-core sign document --id <document> --key <seed>`) and rebind the signature document, or remove the issuer from `recognized_qualification_owners` if the record is not meant to stand as recognized evidence.",
    },
    DiagnosticExplanation {
        code: CORE_X3405,
        title: "Qualification revocation could not be authenticated",
        rule: "SC-7; execution_policy.recognized_qualification_owners",
        meaning: "A bound `qualification_revocation` document names a record whose owner is a recognized issuer, but the document carries no signature over its bound bytes that verifies under the issuer's declared key — it is unsigned, covers different bytes, or does not verify. The withdrawal is ignored and the record stands: only the issuer the contract declares may withdraw a recognized record. A revocation against an unlisted owner's record is package-asserted and applies unsigned, since it can only deny evidence.",
        next_action: "Sign the revocation with the issuer key the contract declares and rebind the signature document, or remove the document if the record was not meant to be withdrawn.",
    },
    DiagnosticExplanation {
        code: CORE_X6401,
        title: "Presentation-gate deadline lapsed",
        rule: "ADR-0021; SC-13 presentation deadline",
        meaning: "A presentation gate's recorded `respond_by` deadline passed with no routing recorded. The lapse is a finding, not a decision: the gate stays open, the technical verdict is unchanged, and the surrounding workflow decides what an overdue review means.",
        next_action: "Record the routing or extend the deadline in a new record. The owner is the presentation workflow — Core produces no verdict consequence.",
    },
    DiagnosticExplanation {
        code: CORE_X6402,
        title: "Illegal state transition recorded",
        rule: "ADR-0021 clause 3: legality table",
        meaning: "A `state_transition` log record names a move the subject kind's closed vocabulary does not allow — a state outside the vocabulary, a self-transition, an edge missing from the table (such as `completed` back to `running`), or a `from_state` that does not equal the subject's derived state. The record is refused: the campaign log fails validation rather than accepting a move that did not happen.",
        next_action: "Repair or remove the offending line. A legitimate move records the state the log actually derives, inside the subject kind's table. The owner is the actor who recorded it.",
    },
    DiagnosticExplanation {
        code: CORE_X6403,
        title: "Transition attestation missing or wrong role",
        rule: "ADR-0021 clause 4: actor authorization",
        meaning: "A `state_transition` names an attestation that does not exist in the log, cites it at the wrong digest, carries a signature that does not verify under the record's own role, or asserts a role different from the one the transition class requires — for example a `policy_owner` move signed by a `requester`. The record is refused.",
        next_action: "Bind the transition to the exact attestation record, signed under a key listed for the role that class requires. The owner is the actor whose authority the move claims.",
    },
    DiagnosticExplanation {
        code: CORE_X6404,
        title: "Resume disagrees with recorded state",
        rule: "ADR-0021 clause 5: resumption",
        meaning: "A run was appended for a campaign whose recorded state is terminal or blocked — the log's transition records say the campaign ended or suspended, yet new execution evidence appeared. Evidence and action disagree: either the transition was wrong or the run should not have happened.",
        next_action: "Reconcile the log: record the transition that legitimately reopened the campaign, or remove the run row if it was recorded against the wrong campaign. The owner is the requester.",
    },
    DiagnosticExplanation {
        code: CORE_X6501,
        title: "Routing record does not match the materialized gate",
        rule: "ADR-0024; SC-11 A9",
        meaning: "A `staged_review_record` answers a step with no declared presentation gate, binds a `request_sha256` different from what the run materialized, names an eligibility policy different from the gate's binding, records a disposition the request does not allow, or was rewritten after its `record_sha256` bound. The record is quarantined — the artifact, admission state, and verdict it was attached to are byte-for-byte unchanged.",
        next_action: "Reproduce the routing honestly: route the exact dossier the materialized request binds and record a disposition the gate's declaration allows. The owner is the requester.",
    },
    DiagnosticExplanation {
        code: CORE_X6502,
        title: "Routing record malformed",
        rule: "ADR-0024; staged-review-record schema",
        meaning: "A package-bound `staged_review_record` has no bytes, does not parse, or carries a schema_version other than `avila.core/staged-review-record/v0.1-draft`. The record cannot stand as a recorded routing.",
        next_action: "Repair the document to the staged-review-record schema, or remove it — an absent record is a clean state. The owner is the requester.",
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
    FreeInputValidation,
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
