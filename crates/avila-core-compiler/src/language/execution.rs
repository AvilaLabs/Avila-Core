//! Execution-side documents of the language lifecycle (spec §1):
//! `plan` emits the execution plan binding the analysis identity;
//! `execute` — the runner's domain — returns observation records whose
//! receipts are re-hashed at `evaluate`, where digest-bound observations
//! discharge the plan's runtime obligations and requirement verdicts are
//! derived. `analyze` itself executes nothing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::document::ValueDecl;
use super::model::{NumericValue, SemanticValue};

/// The execution-plan document schema (`avila.core/execution-plan/v0.1-draft`).
pub const EXECUTION_PLAN_SCHEMA_VERSION: &str = "avila.core/execution-plan/v0.1-draft";
/// The observation-set schema (`avila.core/language-observations/v0.1-draft`).
pub const OBSERVATIONS_SCHEMA_VERSION: &str = "avila.core/language-observations/v0.1-draft";
/// The language invocation-receipt schema (`avila.core/language-receipt/v0.1-draft`).
pub const RECEIPT_SCHEMA_VERSION: &str = "avila.core/language-receipt/v0.1-draft";
/// The evaluation-record schema (`avila.core/language-evaluation/v0.1-draft`).
pub const EVALUATION_SCHEMA_VERSION: &str = "avila.core/language-evaluation/v0.1-draft";

/// Canonical staged bytes of a typed value — the single recipe the plan's
/// declared input digests, the runner's staged input files, and evaluate's
/// recompute all share. The document is a `ValueDecl` (`kind`, the numeric
/// payload, `unit`); its canonical bytes are the invocation's input identity.
pub(crate) fn staged_value_bytes(value: &SemanticValue) -> Result<Vec<u8>, String> {
    // Optional fields serialize as `null` — the canonical reader refuses
    // nulls, so absent fields are stripped to their `default` presence.
    let mut json = serde_json::to_value(&value_decl(value)?).map_err(|e| e.to_string())?;
    if let Some(map) = json.as_object_mut() {
        map.retain(|_, v| !v.is_null());
    }
    let bytes = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    avila_core_kernel::canonicalize_json(&bytes).map_err(|error| error.to_string())
}

/// Canonical bytes of any serializable record — strips absent `null`
/// fields (the canonical grammar carries only present fields), then
/// canonicalizes. The recipe every execution-document digest shares.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.retain(|_, v| !v.is_null());
                map.values_mut().for_each(strip);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut json = serde_json::to_value(value).map_err(|e| e.to_string())?;
    strip(&mut json);
    let bytes = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    avila_core_kernel::canonicalize_json(&bytes).map_err(|e| e.to_string())
}

/// The `ValueDecl` a staged value serializes to — `kind` is the claim
/// vocabulary (`exact`/`enclosure`/`nominal`), the payload the exact or
/// interval numbers the claim carries.
pub(crate) fn value_decl(value: &SemanticValue) -> Result<ValueDecl, String> {
    let mut decl = ValueDecl {
        kind: value.ty.claim.as_str().into(),
        value: None,
        lower: None,
        upper: None,
        unit: value.unit.clone(),
    };
    match value.value.as_ref() {
        Some(NumericValue::Exact(number)) => decl.value = Some(number.canonical_rational()),
        Some(NumericValue::Enclosure(enclosure)) => {
            decl.lower = Some(enclosure.lower.canonical_rational());
            decl.upper = Some(enclosure.upper.canonical_rational());
        }
        None => return Err("the binding carries no stageable value".into()),
    }
    Ok(decl)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanContext {
    pub program_sha256: String,
    pub library_sha256: String,
    pub analysis_sha256: String,
}

/// One input the executable must be staged with — the operand's program
/// binding plus, when a value exists, the canonical bytes' digest. An
/// operand without a value (`unavailable` or unestablished) is declared
/// `unstaged`; the invocation cannot run, and its obligations stay open —
/// that is the plan's honest statement, not a defect the runner hides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedInput {
    pub binding: String,
    /// `staged` — the canonical bytes and digest exist; `unstaged` — no
    /// value reaches this slot and the invocation cannot execute.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValueDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// What the invocation must produce — the method's declared output shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Produces {
    pub quantity_kind: String,
    pub unit: String,
}

/// One `apply` whose implementation is external: the executable identity
/// the runner resolves, every declared effect, the staged inputs, and the
/// runtime obligations this invocation is placed to discharge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedInvocation {
    /// Body position (`body[{index}]`) — the application site the
    /// observation binds to.
    pub at: String,
    pub bind: String,
    pub method: String,
    pub executable: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<String>,
    pub inputs: BTreeMap<String, PlannedInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produces: Option<Produces>,
    /// Runtime obligation ids (`body[{index}].ensures[{position}]` and
    /// `body[{index}].observation`) this invocation answers.
    pub obligations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPlanDocument {
    pub schema_version: String,
    pub profile: String,
    /// `ready` with the invocations listed; `refused` when the analysis is
    /// not plan-ready — a plan that cannot preserve an obligation refuses
    /// rather than omitting it.
    pub state: String,
    pub context: PlanContext,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invocations: Vec<PlannedInvocation>,
    /// The blocking findings' codes when `state` is `refused`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refused_by: Vec<String>,
    /// Canonical identity of the body above — recomputed on read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_sha256: Option<String>,
}

/// The runner's observation for one application site: the recorded input
/// and output digests, the collected output document, and the receipt —
/// all re-verified by digest at `evaluate` (spec §7 [O1]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationRecord {
    /// Application site — `body[{index}]` — matching the plan entry.
    pub at: String,
    pub bind: String,
    /// Executable identity the runner resolved (`synthetic/<name>@<rev>`).
    pub executable: String,
    /// Slot → digest of the staged input bytes the invocation actually saw.
    pub inputs: BTreeMap<String, String>,
    /// The collected output's document digest (`sha256:` of its canonical
    /// bytes) — absent when no output was produced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_sha256: Option<String>,
    /// The collected output document itself — the executable's answer.
    /// `evaluate` re-derives its digest and admits it against the declared
    /// output type; absent when the invocation produced nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<ValueDecl>,
    /// Digest of the receipt document — receipt bytes re-hash to it.
    pub receipt_sha256: String,
    /// The full receipt document (re-hashed on bind).
    pub receipt: LanguageReceipt,
}

/// The runner's receipt for a language invocation — the same discipline as
/// the case-run receipt (invocation identity over capability, inputs, and
/// invocation; process outcome and outputs recorded as results) in the
/// language's vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageReceipt {
    pub schema_version: String,
    /// The plan this invocation belongs to.
    pub plan_sha256: String,
    /// Application site.
    pub at: String,
    /// The resolved executable identity and its byte digest.
    pub executable: String,
    pub executable_sha256: String,
    /// Staged inputs as the process saw them.
    pub inputs: Vec<ReceiptInput>,
    /// What was asked: argv, working directory (workspace-relative),
    /// timeout. Recomputed into `invocation_sha256` together with the
    /// capability and staged inputs.
    pub invocation: LanguageInvocation,
    pub invocation_sha256: String,
    pub process: ProcessOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<ReceiptLog>,
    pub outputs: Vec<ReceiptOutput>,
    pub runner: RunnerIdentity,
    /// `completed` | `failed` | `timed_out`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptInput {
    pub slot: String,
    pub workspace_path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageInvocation {
    pub arguments: Vec<String>,
    pub working_directory: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessOutcome {
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptLog {
    pub stream: String,
    pub workspace_path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptOutput {
    pub output_id: String,
    pub workspace_path: String,
    /// `collected` | `partial` | `missing`.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerIdentity {
    pub runner: String,
    pub os: String,
    pub arch: String,
}

/// The observation set — supplied material at `evaluate`, carrying the
/// plan identity it answers so foreign observations fail the context
/// binding, not just the digest comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationsDocument {
    pub schema_version: String,
    pub profile: String,
    /// The plan these observations answer — must equal the recomputed
    /// plan's identity or every observation is foreign.
    pub plan_sha256: String,
    pub observations: Vec<ObservationRecord>,
    /// Canonical identity of the body above — recomputed on read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observations_sha256: Option<String>,
}

/// The evaluation's context record — everything the verdicts ran under:
/// document identities, the plan identity, and the supplied material's
/// identity. Recomputed at replay; a difference invalidates the record.
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationContext {
    pub semantic_profile: String,
    pub program_sha256: String,
    pub library_sha256: String,
    pub analysis_sha256: String,
    pub plan_sha256: String,
    /// Identity of the supplied observations document.
    pub observations_sha256: String,
    /// Supplied lifecycle material as `key=state` pairs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lifecycle: Vec<String>,
}

/// What the evaluation made of one supplied observation.
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationOutcome {
    pub at: String,
    pub bind: String,
    /// `bound` — digests verified and the output admitted;
    /// `rejected` — a digest or shape failed (foreign material is refused,
    /// never silently dropped); `absent` — the site needed an observation
    /// and none arrived.
    pub state: String,
    pub detail: String,
}

/// One requirement's evaluated verdict — the analysis's `pending` state
/// resolved by the supplied observations.
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementVerdict {
    pub status: String,
    pub rule: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_value: Option<String>,
}

/// One rule application in the evaluation's derivation record.
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleOutcome {
    /// `observe` ([O1]) — the invocation's digest binding held;
    /// `discharge` ([O2]) — the named check replayed over observed values;
    /// `bounded.{ge,le}` ([V-*]) — the requirement comparison.
    pub rule: String,
    /// The application site or requirement id the rule answered for.
    pub subject: String,
    /// `established` | `open` | `refuted` — and `pass`/`fail`/`inconclusive`/
    /// `not_evaluated` for the requirement comparison.
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageEvaluation {
    pub schema_version: String,
    pub profile: String,
    pub context: EvaluationContext,
    /// Canonical identity of `context` — what a replay recomputes first.
    pub context_sha256: String,
    /// Every runtime obligation's post-execution state (`discharged`,
    /// `open`, `refuted`) with its evidence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obligations: Vec<RuleOutcome>,
    /// Per-observation binding results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<ObservationOutcome>,
    /// Requirement id → evaluated verdict (pass | fail | inconclusive |
    /// not_evaluated).
    pub requirements: BTreeMap<String, RequirementVerdict>,
    /// Document-admission failures — an observations document that never
    /// decoded explains itself here rather than by an absent record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_findings: Vec<String>,
    /// The evaluated analysis's findings — a foreign observation surfaces
    /// as `observation_foreign` here, with its detail.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<super::analyze::LanguageFinding>,
    /// Rule applications in the order they ran — the replayable argument.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applications: Vec<RuleOutcome>,
    /// Canonical identity of the body above — recomputed on read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation_sha256: Option<String>,
}

/// Invocation identity — the digest over capability, staged inputs, and
/// invocation properties that `receipt.invocation_sha256` must equal. The
/// runner computes it at issue; `evaluate` re-derives it on read (§7 [O1]).
pub fn invocation_identity(receipt: &LanguageReceipt) -> Option<String> {
    use sha2::{Digest, Sha256};
    let identity = serde_json::json!({
        "executable": receipt.executable,
        "executable_sha256": receipt.executable_sha256,
        "inputs": receipt.inputs,
        "invocation": receipt.invocation,
    });
    let bytes = serde_json::to_vec(&identity).ok()?;
    let canonical = avila_core_kernel::canonicalize_json(&bytes).ok()?;
    Some(format!("sha256:{:x}", Sha256::digest(&canonical)))
}
