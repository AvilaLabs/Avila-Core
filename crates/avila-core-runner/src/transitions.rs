//! ADR-0021: state-transition records and actor attestations.
//!
//! Campaign and contract states move only through recorded transitions.
//! The engine does not perform transitions — it verifies recorded ones:
//! each `state_transition` log record must be legal under the subject
//! kind's closed vocabulary, its `from_state` must equal the subject's
//! current derived state, and the `attestation` it names must be signed
//! under a key listed for the role that transition class requires.
//!
//! Step states are derived, never recorded: a step's position comes from
//! its receipts and the run log; `step`-kind transitions exist only for
//! moves evidence cannot derive (cancellation of an in-flight step under
//! supersession).

use serde::{Deserialize, Serialize};
use sha2::Digest;

use avila_core_evidence::signature::{KeyRole, SignatureDocument};

pub const ATTESTATION_SCHEMA_VERSION: &str = "avila.core/attestation/v0.1-draft";
pub const STATE_TRANSITION_SCHEMA_VERSION: &str = "avila.core/state-transition/v0.1-draft";

/// The statements an attestation may carry (SC-14): the vocabulary is
/// closed; the engine checks the word, never the prose detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationStatement {
    Approves,
    Authors,
    Reviews,
    Waives,
    Rescinds,
}

/// What kind of actor produced the record — a stated attribution, never
/// inferred (same posture as ADR-0019 `created_by`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Person,
    Agent,
    Tool,
}

/// The digest-bound thing an attestation is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestationSubject {
    /// `contract`, `campaign`, `transition`, or `manifest` — which kind of
    /// record the statement covers.
    pub kind: AttestationSubjectKind,
    /// The subject's bound identity (contract `id@rev`, campaign id,
    /// transition id, or manifest id).
    pub identity: String,
    /// The exact document digest the statement covers.
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationSubjectKind {
    Contract,
    Campaign,
    Transition,
    Manifest,
    /// ADR-0023: an `organization_policy` document — the subject of a
    /// replacement authorization.
    OrganizationPolicy,
}

/// The second identity an attestation binds — named, not digest-pinned,
/// because the contract side pins the attestation (a digest in both
/// directions is a cycle). ADR-0023's `replaces` attestation carries
/// `target: {kind: contract, identity: "id@rev"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestationTarget {
    pub kind: AttestationSubjectKind,
    pub identity: String,
}

/// The actor attribution: who produced the record, as stated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorRef {
    pub actor_id: String,
    pub actor_kind: ActorKind,
}

/// `avila.core/attestation/v0.1-draft`: the authority primitive SC-14
/// names — a key, in a role, signing a statement about an exact subject.
/// The record never asserts the statement is true; it asserts the
/// attribution and the signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attestation {
    pub schema_version: String,
    pub attestation_id: String,
    pub subject: AttestationSubject,
    pub statement: AttestationStatement,
    /// Inert text carrying the stated detail of the statement.
    #[serde(default)]
    pub detail: String,
    /// ADR-0023: the second identity the statement binds — for a
    /// replacement authorization, the contract `id@rev` it authorizes.
    /// Named, never digest-pinned (the contract pins this record).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<AttestationTarget>,
    pub actor: ActorRef,
    /// The technical role the actor asserts — must equal the role the
    /// signature's key is listed under in the trust root.
    pub role: KeyRole,
    /// The actor's signature over this record's canonical bytes with
    /// this member absent.
    pub signature: SignatureDocument,
}

/// Which kind of subject a transition moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionSubjectKind {
    Contract,
    Campaign,
    Step,
}

impl TransitionSubjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::Campaign => "campaign",
            Self::Step => "step",
        }
    }
}

/// The subject a `state_transition` record moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionSubject {
    pub kind: TransitionSubjectKind,
    /// The subject's bound identity — the contract `id@rev`, the campaign
    /// id, or the step id within the campaign.
    pub identity: String,
}

/// A campaign state (SC-13). Terminal states never transition out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignState {
    Planned,
    Running,
    Blocked,
    Completed,
    Cancelled,
    Superseded,
    Invalidated,
}

/// A contract status (SC-9.1). `Retired` is terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Draft,
    InReview,
    Approved,
    Retired,
}

/// A step state (SC-13) — the derived vocabulary; only `Cancelled` may be
/// *recorded* by a step-kind transition, since every other step state is
/// derivable from receipts and the run log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Reused,
    Staged,
    Running,
    Collecting,
    Validating,
    Admitted,
    Quarantined,
    Failed,
    Skipped,
    Cancelled,
}

/// One step effect a transition records — which steps it cancels. The
/// cancelled step's receipts remain evidence; cancellation is a state
/// mark, not a deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepEffect {
    Cancelled,
}

/// The attestation a transition names as its actor authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestationRef {
    pub attestation_id: String,
    /// The exact attestation record digest the transition cites.
    pub sha256: String,
}

/// `avila.core/state-transition/v0.1-draft`: one immutable record per
/// move, appended to the campaign log under the same lock as attempts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateTransition {
    pub schema_version: String,
    pub transition_id: String,
    pub subject: TransitionSubject,
    pub from_state: String,
    pub to_state: String,
    /// The attestation record under which the actor signs — a digest-pinned
    /// reference to a log-carried `attestation` record.
    pub actor_attestation: AttestationRef,
    /// The recorded instant the actor states the move happened at.
    pub at: String,
    /// Inert text — never interpreted.
    #[serde(default)]
    pub rationale: String,
    /// Optional per-step effects — an amendment-driven campaign
    /// supersession uses this to name the in-flight steps it cancelled.
    #[serde(default)]
    pub step_effects: Vec<StepEffectEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepEffectEntry {
    pub step_id: String,
    pub effect: StepEffect,
}

/// The initial state each subject kind starts from when no transition
/// has been recorded yet.
pub fn initial_state(kind: TransitionSubjectKind) -> &'static str {
    match kind {
        TransitionSubjectKind::Contract => "draft",
        TransitionSubjectKind::Campaign => "planned",
        TransitionSubjectKind::Step => "pending",
    }
}

/// Whether `(from, to)` is a legal transition for the subject kind
/// (ADR-0021 clause 3's closed table). `to` may equal `from` only where
/// the vocabulary makes the self-move meaningless — self-transitions are
/// never legal.
pub fn legal_transition(kind: TransitionSubjectKind, from_state: &str, to_state: &str) -> bool {
    if from_state == to_state {
        return false;
    }
    match kind {
        TransitionSubjectKind::Campaign => matches!(
            (from_state, to_state),
            ("planned", "running")
                | ("planned", "cancelled")
                | ("running", "blocked")
                | ("running", "completed")
                | ("running", "cancelled")
                | ("running", "superseded")
                | ("blocked", "running")
                | ("blocked", "cancelled")
                | ("blocked", "superseded")
                | ("completed", "invalidated")
        ),
        TransitionSubjectKind::Contract => matches!(
            (from_state, to_state),
            ("draft", "in_review")
                | ("in_review", "draft")
                | ("in_review", "approved")
                | ("approved", "retired")
        ),
        // Every other step state is derived from receipts and run rows;
        // the only move a record makes is cancellation of an in-flight
        // step, legal from any recorded non-terminal position.
        TransitionSubjectKind::Step => {
            to_state == "cancelled"
                && from_state != "cancelled"
                && state_in_vocabulary(kind, from_state)
        }
    }
}

/// The key role a transition's actor attestation must carry for the move
/// to be authorized (ADR-0021 clause 4). Returns `None` for an illegal
/// transition — legality is checked first.
pub fn required_role(
    kind: TransitionSubjectKind,
    from_state: &str,
    to_state: &str,
) -> Option<KeyRole> {
    if !legal_transition(kind, from_state, to_state) {
        return None;
    }
    let role = match kind {
        // Contract status moves are requester-signed; a policy owner signs
        // the invalidation of a completed campaign; a runner key records
        // system effects (step cancellation under supersession).
        TransitionSubjectKind::Contract => KeyRole::Requester,
        TransitionSubjectKind::Campaign => match to_state {
            "invalidated" => KeyRole::PolicyOwner,
            "superseded" => KeyRole::Requester,
            _ => KeyRole::Requester,
        },
        TransitionSubjectKind::Step => KeyRole::Runner,
    };
    Some(role)
}

/// Whether the subject-kind vocabulary even contains this state name —
/// a state outside the vocabulary is structurally impossible, exactly as
/// a campaign state cannot appear on a contract.
pub fn state_in_vocabulary(kind: TransitionSubjectKind, state: &str) -> bool {
    match kind {
        TransitionSubjectKind::Campaign => matches!(
            state,
            "planned"
                | "running"
                | "blocked"
                | "completed"
                | "cancelled"
                | "superseded"
                | "invalidated"
        ),
        TransitionSubjectKind::Contract => {
            matches!(state, "draft" | "in_review" | "approved" | "retired")
        }
        TransitionSubjectKind::Step => matches!(
            state,
            "pending"
                | "reused"
                | "staged"
                | "running"
                | "collecting"
                | "validating"
                | "admitted"
                | "quarantined"
                | "failed"
                | "skipped"
                | "cancelled"
        ),
    }
}

/// What the caller supplies to append an attestation; the signature is
/// built here so the signed digest is always this record's own canonical
/// bytes.
#[derive(Debug, Clone)]
pub struct AttestationRequest {
    pub attestation_id: String,
    pub subject: AttestationSubject,
    pub statement: AttestationStatement,
    pub detail: String,
    /// ADR-0023: the second identity the statement binds — required for
    /// replacement authorizations, absent elsewhere.
    pub target: Option<AttestationTarget>,
    pub actor: ActorRef,
    pub role: KeyRole,
}

/// What the caller supplies to append a transition. `from_state` is not
/// supplied: the record must claim the state the log derives under the
/// append lock, never the caller's expectation of it.
#[derive(Debug, Clone)]
pub struct TransitionRequest {
    pub transition_id: String,
    pub subject: TransitionSubject,
    pub to_state: String,
    /// The attestation the actor signs under — resolved to its log-carried
    /// digest under the append lock.
    pub attestation_id: String,
    pub at: String,
    pub rationale: String,
    pub step_effects: Vec<StepEffectEntry>,
}

/// The canonical digest an attestation signs: the record's canonical bytes
/// with `signature` absent.
fn attestation_signing_material(
    record: &Attestation,
) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
    let mut value = serde_json::to_value(record)?;
    value
        .as_object_mut()
        .and_then(|object| object.remove("signature"));
    let canonical = avila_core_kernel::canonicalize_json(&serde_json::to_vec(&value)?)?;
    let sha256 = crate::attempt::digest(&canonical);
    Ok((canonical, sha256))
}

/// Append an attestation record: sign it with `actor_key` (the key the
/// record's `role` claims), then append the signed record as a log line.
/// When `trust_root` is supplied the just-built signature is verified
/// before anything lands — a key not listed for the record's role never
/// reaches the log.
pub fn append_attestation(
    log_path: &std::path::Path,
    request: &AttestationRequest,
    actor_key: &[u8; 32],
    runner_key: Option<[u8; 32]>,
    trust_root: Option<&avila_core_evidence::signature::TrustRoot>,
) -> Result<(Attestation, String), Box<dyn std::error::Error>> {
    crate::attempt::validate_identifier("attestation", &request.attestation_id)?;
    if request.subject.identity.trim().is_empty() {
        return Err("attestation subject identity must not be empty".into());
    }
    if !request.subject.sha256.starts_with("sha256:") {
        return Err("attestation subject sha256 must carry the `sha256:` prefix".into());
    }
    if request.actor.actor_id.trim().is_empty() {
        return Err("attestation actor_id must not be empty".into());
    }
    let mut record = Attestation {
        schema_version: ATTESTATION_SCHEMA_VERSION.into(),
        attestation_id: request.attestation_id.clone(),
        subject: request.subject.clone(),
        statement: request.statement,
        detail: request.detail.clone(),
        target: request.target.clone(),
        actor: request.actor.clone(),
        role: request.role,
        signature: avila_core_evidence::signature::SignatureDocument {
            schema_version: String::new(),
            signed_document: avila_core_evidence::signature::SignedDocumentRef {
                role: String::new(),
                document_id: String::new(),
                sha256: String::new(),
            },
            key_id: String::new(),
            algorithm: String::new(),
            signature_hex: String::new(),
            notice: String::new(),
        },
    };
    let (canonical, sha256) = attestation_signing_material(&record)?;
    let digest: [u8; 32] = sha2::Sha256::digest(&canonical).into();
    record.signature = avila_core_evidence::signature::build_signature_document(
        actor_key,
        "attestation",
        &request.attestation_id,
        sha256,
        &digest,
    );
    if let Some(trust_root) = trust_root {
        avila_core_evidence::signature::verify_signature_document(
            &record.signature,
            trust_root,
            request.role,
        )
        .map_err(|error| {
            format!(
                "attestation `{}` does not verify under role `{}`: {error}",
                request.attestation_id, request.role
            )
        })?;
    }
    let line = crate::case_run::log::record_line("attestation", &record, runner_key)?;
    let record_sha256 = format!(
        "sha256:{}",
        avila_core_evidence::sha256_hex(line.as_bytes())
    );
    crate::case_run::log::append_log_line(log_path, &line, |_| Ok(()))?;
    Ok((record, record_sha256))
}

/// The state the log derives for a subject identity — the last legal
/// transition's `to_state`, or the kind's initial state when no
/// transition names it.
pub(crate) fn derived_state(
    view: &crate::history::LogView,
    kind: TransitionSubjectKind,
    identity: &str,
) -> String {
    let subject_key = format!("{}:{identity}", kind.label());
    let mut state = initial_state(kind).to_string();
    for entry in &view.transitions {
        if format!(
            "{}:{}",
            entry.record.subject.kind.label(),
            entry.record.subject.identity
        ) == subject_key
        {
            state = entry.record.to_state.clone();
        }
    }
    state
}

/// Append a state-transition record. The `from_state` is derived under
/// the same lock as the append, so a concurrent writer cannot slip a
/// conflicting move between the check and the write.
pub fn append_transition(
    log_path: &std::path::Path,
    request: &TransitionRequest,
    runner_key: Option<[u8; 32]>,
    trust_root: Option<&avila_core_evidence::signature::TrustRoot>,
) -> Result<(StateTransition, String), Box<dyn std::error::Error>> {
    crate::attempt::validate_identifier("transition", &request.transition_id)?;
    if request.subject.identity.trim().is_empty() {
        return Err("transition subject identity must not be empty".into());
    }
    if request.at.trim().is_empty() {
        return Err("transition `at` must not be empty".into());
    }
    crate::attempt::validate_identifier("attestation", &request.attestation_id)?;
    let build = |view: &crate::history::LogView,
                 log_path: &std::path::Path|
     -> Result<StateTransition, String> {
        let from_state = derived_state(view, request.subject.kind, &request.subject.identity);
        let attestation = view
            .attestations
            .get(&request.attestation_id)
            .ok_or_else(|| {
                format!(
                    "CORE-X6403: transition `{}` names attestation `{}`, which does not exist in `{}`",
                    request.transition_id,
                    request.attestation_id,
                    log_path.display()
                )
            })?;
        Ok(StateTransition {
            schema_version: STATE_TRANSITION_SCHEMA_VERSION.into(),
            transition_id: request.transition_id.clone(),
            subject: request.subject.clone(),
            from_state,
            to_state: request.to_state.clone(),
            actor_attestation: AttestationRef {
                attestation_id: request.attestation_id.clone(),
                sha256: attestation.record_sha256.clone(),
            },
            at: request.at.clone(),
            rationale: request.rationale.clone(),
            step_effects: request.step_effects.clone(),
        })
    };
    let view = crate::history::parse_log(&crate::history::read_log(log_path)?, log_path)?;
    crate::history::validate_log(&view, trust_root)?;
    let record = build(&view, log_path)?;
    check_transition_admission(&view, &record)?;
    let line = crate::case_run::log::record_line("state_transition", &record, runner_key)?;
    let record_sha256 = format!(
        "sha256:{}",
        avila_core_evidence::sha256_hex(line.as_bytes())
    );
    let expected = record.clone();
    crate::case_run::log::append_log_line(log_path, &line, move |content| {
        let view = crate::history::parse_log(content, log_path)?;
        crate::history::validate_log(&view, trust_root)?;
        let derived = build(&view, log_path)?;
        if derived != expected {
            return Err(format!(
                "transition `{}` no longer describes the derived state",
                expected.transition_id
            ));
        }
        check_transition_admission(&view, &expected)
    })?;
    Ok((record, record_sha256))
}

/// The single-record half of the transition fold: the move must be legal
/// for the subject kind and its attestation must carry the required role.
fn check_transition_admission(
    view: &crate::history::LogView,
    record: &StateTransition,
) -> Result<(), String> {
    let kind = record.subject.kind;
    if !legal_transition(kind, &record.from_state, &record.to_state) {
        return Err(format!(
            "CORE-X6402: `{}` is an illegal {} transition: `{}` cannot move to `{}`",
            record.transition_id,
            kind.label(),
            record.from_state,
            record.to_state
        ));
    }
    let attestation = view
        .attestations
        .get(&record.actor_attestation.attestation_id)
        .ok_or_else(|| {
            format!(
                "CORE-X6403: transition `{}` names attestation `{}`, which does not exist",
                record.transition_id, record.actor_attestation.attestation_id
            )
        })?;
    let required =
        required_role(kind, &record.from_state, &record.to_state).expect("legality checked above");
    if attestation.record.role != required {
        return Err(format!(
            "CORE-X6403: transition `{}` requires a `{required}` attestation, but `{}` asserts `{}`",
            record.transition_id, record.actor_attestation.attestation_id, attestation.record.role
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_vocabulary_and_legality_table() {
        // SC-13: the closed vocabulary.
        for state in [
            "planned",
            "running",
            "blocked",
            "completed",
            "cancelled",
            "superseded",
            "invalidated",
        ] {
            assert!(
                state_in_vocabulary(TransitionSubjectKind::Campaign, state),
                "{state}"
            );
        }
        assert!(!state_in_vocabulary(
            TransitionSubjectKind::Campaign,
            "admitted"
        ));
        assert!(!state_in_vocabulary(
            TransitionSubjectKind::Campaign,
            "retired"
        ));

        // ADR-0021 clause 3's table.
        for (from, to) in [
            ("planned", "running"),
            ("planned", "cancelled"),
            ("running", "blocked"),
            ("running", "completed"),
            ("running", "cancelled"),
            ("running", "superseded"),
            ("blocked", "running"),
            ("blocked", "cancelled"),
            ("blocked", "superseded"),
            ("completed", "invalidated"),
        ] {
            assert!(
                legal_transition(TransitionSubjectKind::Campaign, from, to),
                "{from}->{to}"
            );
        }
        // Illegal edges: forward-skip, backward, self, terminal exit.
        for (from, to) in [
            ("planned", "completed"),
            ("planned", "planned"),
            ("running", "planned"),
            ("running", "running"),
            ("completed", "running"),
            ("completed", "completed"),
            ("superseded", "running"),
            ("cancelled", "running"),
            ("invalidated", "running"),
            ("blocked", "completed"),
        ] {
            assert!(
                !legal_transition(TransitionSubjectKind::Campaign, from, to),
                "{from}->{to}"
            );
        }
        assert_eq!(initial_state(TransitionSubjectKind::Campaign), "planned");
    }

    #[test]
    fn contract_vocabulary_and_legality_table() {
        // SC-9.1: draft | in_review | approved | retired. A semantic edit
        // produces a new draft with a supersedes edge — never an in-place
        // move, so `superseded` is not a contract state.
        for state in ["draft", "in_review", "approved", "retired"] {
            assert!(
                state_in_vocabulary(TransitionSubjectKind::Contract, state),
                "{state}"
            );
        }
        assert!(!state_in_vocabulary(
            TransitionSubjectKind::Contract,
            "superseded"
        ));
        assert!(!state_in_vocabulary(
            TransitionSubjectKind::Contract,
            "running"
        ));
        for (from, to) in [
            ("draft", "in_review"),
            ("in_review", "draft"),
            ("in_review", "approved"),
            ("approved", "retired"),
        ] {
            assert!(
                legal_transition(TransitionSubjectKind::Contract, from, to),
                "{from}->{to}"
            );
        }
        for (from, to) in [
            ("draft", "approved"),
            ("draft", "retired"),
            ("in_review", "in_review"),
            ("approved", "draft"),
            ("retired", "approved"),
            ("retired", "draft"),
        ] {
            assert!(
                !legal_transition(TransitionSubjectKind::Contract, from, to),
                "{from}->{to}"
            );
        }
        assert_eq!(initial_state(TransitionSubjectKind::Contract), "draft");
    }

    #[test]
    fn step_vocabulary_and_legality() {
        // SC-13's full step vocabulary is derivable; records exist only for
        // cancellation of an in-flight step.
        for state in [
            "pending",
            "reused",
            "staged",
            "running",
            "collecting",
            "validating",
            "admitted",
            "quarantined",
            "failed",
            "skipped",
            "cancelled",
        ] {
            assert!(
                state_in_vocabulary(TransitionSubjectKind::Step, state),
                "{state}"
            );
        }
        // The only recorded move is cancellation of an in-flight step,
        // legal from any non-cancelled in-vocabulary position.
        for from in [
            "pending",
            "reused",
            "staged",
            "running",
            "collecting",
            "validating",
            "admitted",
            "failed",
            "skipped",
        ] {
            assert!(
                legal_transition(TransitionSubjectKind::Step, from, "cancelled"),
                "{from}->cancelled"
            );
        }
        for (from, to) in [
            ("cancelled", "cancelled"),
            ("cancelled", "running"),
            ("pending", "running"),
            ("running", "admitted"),
            ("running", "failed"),
        ] {
            assert!(
                !legal_transition(TransitionSubjectKind::Step, from, to),
                "{from}->{to}"
            );
        }
        assert_eq!(initial_state(TransitionSubjectKind::Step), "pending");
    }

    #[test]
    fn required_roles_follow_the_transition_class() {
        // Contract status moves and ordinary campaign moves are
        // requester-signed.
        assert_eq!(
            required_role(TransitionSubjectKind::Contract, "in_review", "approved"),
            Some(KeyRole::Requester)
        );
        assert_eq!(
            required_role(TransitionSubjectKind::Campaign, "planned", "running"),
            Some(KeyRole::Requester)
        );
        assert_eq!(
            required_role(TransitionSubjectKind::Campaign, "running", "superseded"),
            Some(KeyRole::Requester)
        );
        // `completed -> invalidated` carries organizational authority.
        assert_eq!(
            required_role(TransitionSubjectKind::Campaign, "completed", "invalidated"),
            Some(KeyRole::PolicyOwner)
        );
        // Step cancellation is a runner-keyed system effect.
        assert_eq!(
            required_role(TransitionSubjectKind::Step, "running", "cancelled"),
            Some(KeyRole::Runner)
        );
        // Unknown edge -> no role.
        assert_eq!(
            required_role(TransitionSubjectKind::Campaign, "completed", "running"),
            None
        );
    }
}
