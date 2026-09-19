//! Design-history records layered on the campaign log (ADR-0019).
//!
//! Four record kinds extend the attempt lineage without replacing it:
//! design revisions (a proposed state that may exist before any run),
//! assessments (the binding between one revision and one run row's
//! evidence), named references (attributable claims like `baseline`),
//! and contract amendments (deliberate question changes linking two
//! lineage roots). Every kind is a line in the same JSONL log, appended
//! under the same lock, revalidation, and optional signature as run rows.
//!
//! Migration is a lazy projection: an attempt row that cites no revision
//! reads as a derived revision (and derived assessment) named by its
//! attempt id, so legacy logs are navigable without rewriting a byte.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_evidence::sha256_hex;
use avila_core_evidence::signature::TrustRoot;
use avila_core_kernel::canonicalize_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::attempt::{
    AttemptChange, AttemptLineageRequest, AttemptRecord, PriorAttempt, diff_candidate_states,
    digest, load_candidate, validate_identifier, verify_log_line_signature,
};

pub const LOG_RECORD_SCHEMA_VERSION: &str = "avila.core/log-record/v0.1-draft";
pub const DESIGN_REVISION_SCHEMA_VERSION: &str = "avila.core/design-revision/v0.1-draft";
pub const ASSESSMENT_SCHEMA_VERSION: &str = "avila.core/assessment/v0.1-draft";
pub const NAMED_REFERENCE_SCHEMA_VERSION: &str = "avila.core/named-reference/v0.1-draft";
pub const CONTRACT_AMENDMENT_SCHEMA_VERSION: &str = "avila.core/contract-amendment/v0.1-draft";

/// An immutable proposed design state (ADR-0019). A revision is valid the
/// moment it is appended: no execution, verdict, or assessment required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRevision {
    pub schema_version: String,
    pub revision_id: String,
    pub generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_revision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_record_sha256: Option<String>,
    /// The amendment a root revision cites when it continues a lineage
    /// under changed fixed identities. Children never carry one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amendment_id: Option<String>,
    pub fixed_manifest_sha256: String,
    pub fixed_compiled_snapshot_sha256: String,
    pub candidate_input: String,
    pub candidate_artifact_sha256: String,
    pub candidate_state_sha256: String,
    pub candidate_state: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<AttemptChange>,
    /// The actor attribution the caller stated — a claim, never inferred.
    pub created_by: String,
    /// The caller's stated intent; inert text, never instructions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
}

/// The binding between one revision and the evidence one run row
/// produced. The verdicts are a verbatim copy of the cited row's, so the
/// record is citable alone and the copy can be checked for drift; the
/// parent comparison is still derived on read, never stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentRecord {
    pub schema_version: String,
    /// Always the attempt id of the run row this assessment cites.
    pub assessment_id: String,
    pub revision_id: String,
    /// The exact revision row (or revision-defining attempt row) this
    /// assessment bound at append time.
    pub revision_record_sha256: String,
    /// The exact run row this assessment cites as its evidence.
    pub run_record_sha256: String,
    /// The identities the verdicts were derived under.
    pub manifest_sha256: String,
    pub compiled_snapshot_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement_set_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement_set_sha256: Option<String>,
    /// The cited run row's verdicts, copied verbatim at append time.
    pub verdicts: Vec<Value>,
}

/// A name's binding to an exact revision (and assessment, when the name
/// claims a result). A claim about significance, never a verdict input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedReference {
    pub schema_version: String,
    pub name: String,
    pub revision_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assessment_id: Option<String>,
    /// The target this name pointed at before this record, resolved
    /// under the append lock. Absent on a name's first binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_revision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_assessment_id: Option<String>,
    pub actor: String,
    pub rationale: String,
}

/// The deliberate question change linking a superseded lineage root to
/// the new root that continues the campaign under changed fixed
/// identities (ADR-0019, DH-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractAmendment {
    pub schema_version: String,
    pub amendment_id: String,
    /// The superseded root's revision id — an explicit revision record
    /// or a revision-less root attempt's derived id.
    pub superseded_root_id: String,
    /// The exact log line that defines the superseded root.
    pub superseded_record_sha256: String,
    pub superseded_manifest_sha256: String,
    pub superseded_compiled_snapshot_sha256: String,
    pub new_manifest_sha256: String,
    pub new_compiled_snapshot_sha256: String,
    /// Core's typed diff of the two canonical package manifests.
    pub changed_elements: Vec<AttemptChange>,
    pub actor: String,
    pub rationale: String,
}

#[derive(Debug)]
pub(crate) struct RevisionEntry {
    pub(crate) record: DesignRevision,
    pub(crate) record_sha256: String,
    pub(crate) line: usize,
    pub(crate) full_line: Value,
}

#[derive(Debug)]
pub(crate) struct AssessmentEntry {
    pub(crate) record: AssessmentRecord,
    pub(crate) record_sha256: String,
    pub(crate) line: usize,
}

#[derive(Debug)]
pub(crate) struct ReferenceEntry {
    pub(crate) record: NamedReference,
    pub(crate) record_sha256: String,
    pub(crate) line: usize,
}

#[derive(Debug)]
pub(crate) struct AmendmentEntry {
    pub(crate) record: ContractAmendment,
    pub(crate) record_sha256: String,
    pub(crate) line: usize,
}

/// ADR-0021: an attestation record carried in the log, in append order.
#[derive(Debug)]
pub(crate) struct AttestationEntry {
    pub(crate) record: crate::transitions::Attestation,
    pub(crate) record_sha256: String,
    pub(crate) line: usize,
}

/// ADR-0021: a state-transition record carried in the log, in append order.
#[derive(Debug)]
pub(crate) struct TransitionEntry {
    pub(crate) record: crate::transitions::StateTransition,
    pub(crate) record_sha256: String,
    pub(crate) line: usize,
}

/// One parsed campaign log: every lineage-bearing record kind indexed by
/// id. Legacy rows with no `attempt` member and no recognized
/// `record_kind` are outside the lineage model and do not appear here;
/// they remain visible to `core_history`/`core_constellation` as run or
/// untracked records.
#[derive(Debug, Default)]
pub(crate) struct LogView {
    pub(crate) attempts: BTreeMap<String, PriorAttempt>,
    pub(crate) revisions: BTreeMap<String, RevisionEntry>,
    pub(crate) assessments: BTreeMap<String, AssessmentEntry>,
    /// Every reference record for a name, in append order.
    pub(crate) references: BTreeMap<String, Vec<ReferenceEntry>>,
    pub(crate) amendments: BTreeMap<String, AmendmentEntry>,
    /// ADR-0021 attestation records, by id.
    pub(crate) attestations: BTreeMap<String, AttestationEntry>,
    /// ADR-0021 state-transition records, in append order — order matters:
    /// a subject's current state is the last legal transition's `to_state`.
    pub(crate) transitions: Vec<TransitionEntry>,
    /// Run-row sha256 to parsed attempt entry, for assessment citation.
    run_rows_by_sha256: BTreeMap<String, String>,
}

fn non_empty(field: &str, value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(value.to_string())
}

fn parse_record<T: serde::de::DeserializeOwned>(
    entry: &Value,
    line: usize,
    kind: &str,
    path: &Path,
) -> Result<T, String> {
    let record = entry.get("record").ok_or_else(|| {
        format!(
            "line {line} of `{}` is a {kind} row without a `record` member",
            path.display()
        )
    })?;
    serde_json::from_value(record.clone()).map_err(|error| {
        format!(
            "line {line} of `{}` has an invalid {kind} record: {error}",
            path.display()
        )
    })
}

/// Parse every lineage-bearing record kind in one pass.
pub(crate) fn parse_log(content: &str, path: &Path) -> Result<LogView, String> {
    let mut view = LogView::default();
    for (index, raw_line) in content.split('\n').enumerate() {
        if raw_line.trim().is_empty() {
            continue;
        }
        let line = index + 1;
        let entry: Value = serde_json::from_str(raw_line).map_err(|error| {
            format!(
                "line {line} of lineage log `{}` is not valid JSON: {error}",
                path.display()
            )
        })?;
        let record_sha256 = digest(raw_line.as_bytes());
        if entry.get("attempt").is_some() {
            let (id, prior) = crate::attempt::attempt_entry(entry, raw_line, line, path)?;
            view.run_rows_by_sha256.insert(record_sha256, id.clone());
            if let Some(first) = view.attempts.insert(id.clone(), prior) {
                return Err(format!(
                    "attempt id `{id}` occurs on both lines {} and {line} of `{}`",
                    first.line,
                    path.display()
                ));
            }
            continue;
        }
        if entry["schema_version"].as_str() != Some(LOG_RECORD_SCHEMA_VERSION) {
            continue;
        }
        match entry["record_kind"].as_str() {
            Some("design_revision") => {
                let record: DesignRevision = parse_record(&entry, line, "design_revision", path)?;
                let id = record.revision_id.clone();
                let prior = RevisionEntry {
                    record,
                    record_sha256,
                    line,
                    full_line: entry,
                };
                if let Some(first) = view.revisions.insert(id.clone(), prior) {
                    return Err(format!(
                        "revision id `{id}` occurs on both lines {} and {line} of `{}`",
                        first.line,
                        path.display()
                    ));
                }
            }
            Some("assessment") => {
                let record: AssessmentRecord = parse_record(&entry, line, "assessment", path)?;
                let id = record.assessment_id.clone();
                let prior = AssessmentEntry {
                    record,
                    record_sha256,
                    line,
                };
                if let Some(first) = view.assessments.insert(id.clone(), prior) {
                    return Err(format!(
                        "assessment id `{id}` occurs on both lines {} and {line} of `{}`",
                        first.line,
                        path.display()
                    ));
                }
            }
            Some("named_reference") => {
                let record: NamedReference = parse_record(&entry, line, "named_reference", path)?;
                let prior = ReferenceEntry {
                    record,
                    record_sha256,
                    line,
                };
                view.references
                    .entry(prior.record.name.clone())
                    .or_default()
                    .push(prior);
            }
            Some("contract_amendment") => {
                let record: ContractAmendment =
                    parse_record(&entry, line, "contract_amendment", path)?;
                let id = record.amendment_id.clone();
                let prior = AmendmentEntry {
                    record,
                    record_sha256,
                    line,
                };
                if let Some(first) = view.amendments.insert(id.clone(), prior) {
                    return Err(format!(
                        "amendment id `{id}` occurs on both lines {} and {line} of `{}`",
                        first.line,
                        path.display()
                    ));
                }
            }
            Some("attestation") => {
                let record: crate::transitions::Attestation =
                    parse_record(&entry, line, "attestation", path)?;
                let id = record.attestation_id.clone();
                let prior = AttestationEntry {
                    record,
                    record_sha256,
                    line,
                };
                if let Some(first) = view.attestations.insert(id.clone(), prior) {
                    return Err(format!(
                        "attestation id `{id}` occurs on both lines {} and {line} of `{}`",
                        first.line,
                        path.display()
                    ));
                }
            }
            Some("state_transition") => {
                let record: crate::transitions::StateTransition =
                    parse_record(&entry, line, "state_transition", path)?;
                view.transitions.push(TransitionEntry {
                    record,
                    record_sha256,
                    line,
                });
            }
            Some(kind) => {
                return Err(format!(
                    "line {line} of `{}` carries unrecognized record kind `{kind}`",
                    path.display()
                ));
            }
            None => {
                return Err(format!(
                    "line {line} of `{}` is a log record without a record kind",
                    path.display()
                ));
            }
        }
    }
    Ok(view)
}

/// A revision as it exists in the log: either an explicit revision
/// record, or the revision every revision-less attempt row derives.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RevisionRef<'a> {
    Recorded(&'a RevisionEntry),
    Derived(&'a PriorAttempt),
}

impl RevisionRef<'_> {
    pub(crate) fn parent_revision_id(&self, view: &LogView) -> Option<String> {
        match self {
            Self::Recorded(entry) => entry.record.parent_revision_id.clone(),
            // A derived revision's parent is the parent attempt's own
            // resolved revision: its citation when it has one, else the
            // parent's derived id.
            Self::Derived(attempt) => attempt
                .record
                .parent_attempt_id
                .as_ref()
                .and_then(|id| view.attempts.get(id))
                .map(|parent| {
                    parent
                        .revision_id
                        .clone()
                        .unwrap_or_else(|| parent.record.attempt_id.clone())
                }),
        }
    }

    pub(crate) fn parent_record_sha256(&self, view: &LogView) -> Option<String> {
        let parent_id = self.parent_revision_id(view)?;
        resolve_revision(view, &parent_id).map(|parent| parent.record_sha256().to_string())
    }

    pub(crate) fn amendment_id(&self) -> Option<&str> {
        match self {
            Self::Recorded(entry) => entry.record.amendment_id.as_deref(),
            Self::Derived(attempt) => attempt.amendment_id.as_deref(),
        }
    }

    fn fixed_manifest_sha256(&self) -> &str {
        match self {
            Self::Recorded(entry) => &entry.record.fixed_manifest_sha256,
            Self::Derived(attempt) => &attempt.record.fixed_manifest_sha256,
        }
    }

    fn fixed_compiled_snapshot_sha256(&self) -> &str {
        match self {
            Self::Recorded(entry) => &entry.record.fixed_compiled_snapshot_sha256,
            Self::Derived(attempt) => &attempt.record.fixed_compiled_snapshot_sha256,
        }
    }

    fn candidate_input(&self) -> &str {
        match self {
            Self::Recorded(entry) => &entry.record.candidate_input,
            Self::Derived(attempt) => &attempt.record.candidate_input,
        }
    }

    fn candidate_state_sha256(&self) -> &str {
        match self {
            Self::Recorded(entry) => &entry.record.candidate_state_sha256,
            Self::Derived(attempt) => &attempt.record.candidate_state_sha256,
        }
    }

    fn candidate_state(&self) -> &Value {
        match self {
            Self::Recorded(entry) => &entry.record.candidate_state,
            Self::Derived(attempt) => &attempt.record.candidate_state,
        }
    }

    /// The sha256 of the exact log line that defines this revision: the
    /// revision row itself, or the attempt row it is derived from.
    pub(crate) fn record_sha256(&self) -> &str {
        match self {
            Self::Recorded(entry) => &entry.record_sha256,
            Self::Derived(attempt) => &attempt.record_sha256,
        }
    }

    pub(crate) fn line(&self) -> usize {
        match self {
            Self::Recorded(entry) => entry.line,
            Self::Derived(attempt) => attempt.line,
        }
    }

    fn full_line(&self) -> &Value {
        match self {
            Self::Recorded(entry) => &entry.full_line,
            Self::Derived(attempt) => &attempt.full_line,
        }
    }
}

/// Resolve a revision id to its explicit record or to a revision-less
/// attempt row's derived revision. The two id spaces are disjoint by
/// validation, so the lookup is unambiguous.
pub(crate) fn resolve_revision<'a>(view: &'a LogView, id: &str) -> Option<RevisionRef<'a>> {
    if let Some(entry) = view.revisions.get(id) {
        return Some(RevisionRef::Recorded(entry));
    }
    let attempt = view.attempts.get(id)?;
    if attempt.revision_id.is_some() {
        return None;
    }
    Some(RevisionRef::Derived(attempt))
}

/// The revision a run row is evidence for: its citation, or its own
/// derived revision when it cites none.
pub(crate) fn attempt_revision_id(attempt: &PriorAttempt) -> String {
    attempt
        .revision_id
        .clone()
        .unwrap_or_else(|| attempt.record.attempt_id.clone())
}

/// Validate every record kind in a parsed log. A malformed, tampered, or
/// inconsistent line fails the whole log rather than silently dropping
/// out of view — the same boundary the attempt lineage has always held.
pub(crate) fn validate_log(view: &LogView, trust_root: Option<&TrustRoot>) -> Result<(), String> {
    crate::attempt::validate_history(&view.attempts, trust_root)?;
    for (id, entry) in &view.revisions {
        validate_revision(view, id, entry, trust_root)?;
    }
    for attempt in view.attempts.values() {
        validate_attempt_binding(view, attempt)?;
    }
    for (id, entry) in &view.assessments {
        validate_assessment(view, id, entry)?;
    }
    for (name, entries) in &view.references {
        validate_identifier("reference name", name)?;
        let mut previous: Option<&NamedReference> = None;
        for entry in entries {
            validate_reference(view, entry, previous)?;
            previous = Some(&entry.record);
        }
    }
    for (id, entry) in &view.amendments {
        validate_amendment(view, id, entry)?;
    }
    validate_transitions(view, trust_root)?;
    Ok(())
}

/// ADR-0021: fold the attestation and transition records. An attestation
/// must be structurally sound and — when a trust root is supplied — its
/// signature must verify under the record's own role. A transition must be
/// legal under its subject kind's closed table, continue the subject's
/// derived state, and name an attestation signed under the required role.
fn validate_transitions(view: &LogView, trust_root: Option<&TrustRoot>) -> Result<(), String> {
    use crate::transitions::{legal_transition, required_role, state_in_vocabulary};

    for (id, entry) in &view.attestations {
        let record = &entry.record;
        if record.schema_version != crate::transitions::ATTESTATION_SCHEMA_VERSION {
            return Err(format!(
                "attestation `{id}` carries schema_version `{}`, not `{}`",
                record.schema_version,
                crate::transitions::ATTESTATION_SCHEMA_VERSION
            ));
        }
        // A `transition` subject is resolvable inside the log itself: the
        // identity must name a recorded transition and the digest must be
        // its exact record. Other subject kinds bind material the log does
        // not itself carry and are verified where that material is checked.
        if record.subject.kind == crate::transitions::AttestationSubjectKind::Transition {
            let target = view
                .transitions
                .iter()
                .find(|transition| transition.record.transition_id == record.subject.identity)
                .ok_or_else(|| {
                    format!(
                        "attestation `{id}` names transition `{}`, which does not exist in this log",
                        record.subject.identity
                    )
                })?;
            if target.record_sha256 != record.subject.sha256 {
                return Err(format!(
                    "attestation `{id}` binds transition `{}` at a digest that is not its record",
                    record.subject.identity
                ));
            }
        }
        // The signed target is the record's canonical bytes with the
        // `signature` member absent — the same definition the package
        // documents use. Internal consistency is checked unconditionally;
        // trust-root verification runs only when a root is supplied.
        let mut unsigned = serde_json::to_value(record)
            .map_err(|error| format!("attestation `{id}` does not serialize: {error}"))?;
        unsigned
            .as_object_mut()
            .and_then(|object| object.remove("signature"));
        let canonical = avila_core_kernel::canonicalize_json(
            &serde_json::to_vec(&unsigned)
                .map_err(|error| format!("attestation `{id}` does not serialize: {error}"))?,
        )
        .map_err(|error| format!("attestation `{id}` does not canonicalize: {error}"))?;
        let digest: [u8; 32] = Sha256::digest(&canonical).into();
        avila_core_evidence::signature::check_internal_consistency(&record.signature, &digest)
            .map_err(|error| {
                format!(
                    "CORE-X6403: attestation `{id}` signature is not internally consistent: {error}"
                )
            })?;
        if let Some(trust_root) = trust_root {
            avila_core_evidence::signature::verify_signature_document(
                &record.signature,
                trust_root,
                record.role,
            )
            .map_err(|error| {
                format!("CORE-X6403: attestation `{id}` does not verify under its role: {error}")
            })?;
        }
    }

    // Fold transitions in append order per subject identity.
    let mut states: BTreeMap<String, String> = BTreeMap::new();
    for entry in &view.transitions {
        let record = &entry.record;
        let id = &record.transition_id;
        if record.schema_version != crate::transitions::STATE_TRANSITION_SCHEMA_VERSION {
            return Err(format!(
                "state transition `{id}` carries schema_version `{}`, not `{}`",
                record.schema_version,
                crate::transitions::STATE_TRANSITION_SCHEMA_VERSION
            ));
        }
        let kind = record.subject.kind;
        for state in [&record.from_state, &record.to_state] {
            if !state_in_vocabulary(kind, state) {
                return Err(format!(
                    "CORE-X6402: transition `{id}` on line {} names `{state}`, which the {} vocabulary does not contain",
                    entry.line,
                    kind.label()
                ));
            }
        }
        let subject_key = format!("{}:{}", kind.label(), record.subject.identity);
        let prior = states
            .get(&subject_key)
            .cloned()
            .unwrap_or_else(|| crate::transitions::initial_state(kind).to_string());
        if record.from_state != prior {
            return Err(format!(
                "CORE-X6402: transition `{id}` on line {} claims {} `{}` moves from `{}`, but the derived state is `{prior}`",
                entry.line,
                kind.label(),
                record.subject.identity,
                record.from_state
            ));
        }
        if !legal_transition(kind, &record.from_state, &record.to_state) {
            return Err(format!(
                "CORE-X6402: `{id}` on line {} is an illegal {} transition: `{}` cannot move to `{}`",
                entry.line,
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
                    "CORE-X6403: transition `{id}` names attestation `{}`, which does not exist in this log",
                    record.actor_attestation.attestation_id
                )
            })?;
        if attestation.record_sha256 != record.actor_attestation.sha256 {
            return Err(format!(
                "CORE-X6403: transition `{id}` cites attestation `{}` at a digest that does not match the record in this log",
                record.actor_attestation.attestation_id
            ));
        }
        let required = required_role(kind, &record.from_state, &record.to_state)
            .expect("legality checked above");
        if attestation.record.role != required {
            return Err(format!(
                "CORE-X6403: transition `{id}` requires a `{required}` attestation, but `{}` asserts `{}`",
                record.actor_attestation.attestation_id, attestation.record.role
            ));
        }
        states.insert(subject_key, record.to_state.clone());
    }
    Ok(())
}

fn validate_revision(
    view: &LogView,
    revision_id: &str,
    entry: &RevisionEntry,
    trust_root: Option<&TrustRoot>,
) -> Result<(), String> {
    let record = &entry.record;
    validate_identifier("revision", revision_id)?;
    if record.schema_version != DESIGN_REVISION_SCHEMA_VERSION {
        return Err(format!(
            "revision `{revision_id}` uses unsupported schema `{}`",
            record.schema_version
        ));
    }
    if view.attempts.contains_key(revision_id) {
        return Err(format!(
            "revision id `{revision_id}` collides with an attempt id in the same log"
        ));
    }
    validate_identifier("candidate input", &record.candidate_input)?;
    non_empty("revision created_by", &record.created_by)?;
    let observed = crate::attempt::candidate_state_identity(&record.candidate_state)?;
    if observed != record.candidate_state_sha256 {
        return Err(format!(
            "revision `{revision_id}` candidate state hashes to {observed}, not the recorded {}",
            record.candidate_state_sha256
        ));
    }
    match (&record.parent_revision_id, &record.parent_record_sha256) {
        (None, None) => {
            if record.generation != 0 {
                return Err(format!(
                    "root revision `{revision_id}` has generation {}, not 0",
                    record.generation
                ));
            }
            if !record.changes.is_empty() {
                return Err(format!(
                    "root revision `{revision_id}` records changes without a parent"
                ));
            }
            if let Some(amendment_id) = &record.amendment_id {
                let amendment = amendment_before(view, amendment_id, entry.line)?;
                if amendment.record.new_manifest_sha256 != record.fixed_manifest_sha256
                    || amendment.record.new_compiled_snapshot_sha256
                        != record.fixed_compiled_snapshot_sha256
                {
                    return Err(format!(
                        "revision `{revision_id}` cites amendment `{amendment_id}` whose new identities do not match its fixed identities"
                    ));
                }
            }
        }
        (Some(parent_id), Some(parent_sha256)) => {
            if record.amendment_id.is_some() {
                return Err(format!(
                    "revision `{revision_id}` is a child; only roots cite an amendment"
                ));
            }
            validate_identifier("parent revision", parent_id)?;
            let parent = resolve_revision(view, parent_id).ok_or_else(|| {
                format!("revision `{revision_id}` names missing parent `{parent_id}`")
            })?;
            if parent.line() >= entry.line {
                return Err(format!(
                    "revision `{revision_id}` on line {} must follow parent `{parent_id}` on line {}",
                    entry.line,
                    parent.line()
                ));
            }
            if parent.record_sha256() != parent_sha256 {
                return Err(format!(
                    "revision `{revision_id}` binds parent `{parent_id}` as {parent_sha256}, but its defining record hashes to {}",
                    parent.record_sha256()
                ));
            }
            if let Some(trust_root) = trust_root {
                verify_log_line_signature(parent.full_line(), trust_root).map_err(|issue| {
                    format!(
                        "revision `{revision_id}` binds parent `{parent_id}` on line {}, whose log line does not verify: {issue}",
                        parent.line()
                    )
                })?;
            }
            let parent_generation = match parent {
                RevisionRef::Recorded(parent) => parent.record.generation,
                RevisionRef::Derived(parent) => parent.record.generation,
            };
            let expected_generation = parent_generation
                .checked_add(1)
                .ok_or_else(|| format!("parent revision `{parent_id}` generation overflows u64"))?;
            if record.generation != expected_generation {
                return Err(format!(
                    "revision `{revision_id}` generation {} does not follow parent `{parent_id}` generation {}",
                    record.generation, parent_generation
                ));
            }
            if record.fixed_manifest_sha256 != parent.fixed_manifest_sha256()
                || record.fixed_compiled_snapshot_sha256 != parent.fixed_compiled_snapshot_sha256()
            {
                return Err(format!(
                    "revision `{revision_id}` crosses the fixed question identity of parent `{parent_id}`"
                ));
            }
            if record.candidate_input != parent.candidate_input() {
                return Err(format!(
                    "revision `{revision_id}` changes the candidate input tracked by parent `{parent_id}`"
                ));
            }
            let expected_changes =
                diff_candidate_states(parent.candidate_state(), &record.candidate_state);
            if record.changes != expected_changes {
                return Err(format!(
                    "revision `{revision_id}` changes do not match its parent and candidate states"
                ));
            }
        }
        _ => {
            return Err(format!(
                "revision `{revision_id}` must record both its parent id and parent record identity, or neither"
            ));
        }
    }
    Ok(())
}

/// The row-level ADR-0019 members on a recorded run row: a cited
/// revision must resolve and agree with the attempt's own parentage
/// edge, and a cited amendment is legal only on a root.
fn validate_attempt_binding(view: &LogView, attempt: &PriorAttempt) -> Result<(), String> {
    let id = &attempt.record.attempt_id;
    match (&attempt.revision_id, &attempt.assessment_id) {
        (Some(_), Some(assessment_id)) if assessment_id != id => {
            return Err(format!(
                "attempt `{id}` names assessment `{assessment_id}`; an assessment id is always its run's attempt id"
            ));
        }
        (Some(revision_id), Some(_)) => {
            let parent_revision = attempt
                .record
                .parent_attempt_id
                .as_ref()
                .and_then(|parent_id| view.attempts.get(parent_id))
                .map(attempt_revision_id);
            check_revision_edge(
                view,
                revision_id,
                parent_revision.as_deref(),
                attempt.amendment_id.as_deref(),
                &attempt.record.candidate_state_sha256,
                &attempt.record.candidate_input,
                &attempt.record.fixed_manifest_sha256,
                &attempt.record.fixed_compiled_snapshot_sha256,
                attempt.line,
            )
            .map_err(|issue| format!("attempt `{id}` revision binding: {issue}"))?;
        }
        (None, None) => {}
        _ => {
            return Err(format!(
                "attempt `{id}` must record revision id and assessment id together, or neither"
            ));
        }
    }
    if let Some(amendment_id) = &attempt.amendment_id {
        if attempt.record.parent_attempt_id.is_some() {
            return Err(format!(
                "attempt `{id}` is a child; only roots cite an amendment"
            ));
        }
        let amendment = amendment_before(view, amendment_id, attempt.line)?;
        if amendment.record.new_manifest_sha256 != attempt.record.fixed_manifest_sha256
            || amendment.record.new_compiled_snapshot_sha256
                != attempt.record.fixed_compiled_snapshot_sha256
        {
            return Err(format!(
                "attempt `{id}` cites amendment `{amendment_id}` whose new identities do not match its fixed identities"
            ));
        }
    }
    Ok(())
}

fn validate_assessment(
    view: &LogView,
    assessment_id: &str,
    entry: &AssessmentEntry,
) -> Result<(), String> {
    let record = &entry.record;
    validate_identifier("assessment", assessment_id)?;
    if record.schema_version != ASSESSMENT_SCHEMA_VERSION {
        return Err(format!(
            "assessment `{assessment_id}` uses unsupported schema `{}`",
            record.schema_version
        ));
    }
    let run_id = view
        .run_rows_by_sha256
        .get(&record.run_record_sha256)
        .ok_or_else(|| {
            format!(
                "assessment `{assessment_id}` cites run record {} which is not in this log",
                record.run_record_sha256
            )
        })?;
    let run = &view.attempts[run_id];
    if run.line >= entry.line {
        return Err(format!(
            "assessment `{assessment_id}` on line {} must follow its run row on line {}",
            entry.line, run.line
        ));
    }
    if run.record.attempt_id != *assessment_id {
        return Err(format!(
            "assessment `{assessment_id}` cites a run row belonging to attempt `{}`",
            run.record.attempt_id
        ));
    }
    if run.revision_id.as_deref() != Some(record.revision_id.as_str()) {
        return Err(format!(
            "assessment `{assessment_id}` names revision `{}`, but its run row names {:?}",
            record.revision_id, run.revision_id
        ));
    }
    let revision = resolve_revision(view, &record.revision_id).ok_or_else(|| {
        format!(
            "assessment `{assessment_id}` names missing revision `{}`",
            record.revision_id
        )
    })?;
    if revision.record_sha256() != record.revision_record_sha256 {
        return Err(format!(
            "assessment `{assessment_id}` binds revision `{}` as {}, but its defining record hashes to {}",
            record.revision_id,
            record.revision_record_sha256,
            revision.record_sha256()
        ));
    }
    let run_verdicts = run.verdicts.clone().unwrap_or(Value::Array(Vec::new()));
    if Value::Array(record.verdicts.clone()) != run_verdicts {
        return Err(format!(
            "assessment `{assessment_id}` verdicts are not a verbatim copy of its cited run row's"
        ));
    }
    Ok(())
}

fn validate_reference(
    view: &LogView,
    entry: &ReferenceEntry,
    previous: Option<&NamedReference>,
) -> Result<(), String> {
    let record = &entry.record;
    let name = &record.name;
    if record.schema_version != NAMED_REFERENCE_SCHEMA_VERSION {
        return Err(format!(
            "reference `{name}` uses unsupported schema `{}`",
            record.schema_version
        ));
    }
    non_empty("reference actor", &record.actor)?;
    non_empty("reference rationale", &record.rationale)?;
    let revision = resolve_revision(view, &record.revision_id).ok_or_else(|| {
        format!(
            "reference `{name}` names missing revision `{}`",
            record.revision_id
        )
    })?;
    if revision.line() >= entry.line {
        return Err(format!(
            "reference `{name}` on line {} must follow revision `{}` on line {}",
            entry.line,
            record.revision_id,
            revision.line()
        ));
    }
    if let Some(assessment_id) = &record.assessment_id {
        let assessment = view.assessments.get(assessment_id).ok_or_else(|| {
            format!("reference `{name}` names missing assessment `{assessment_id}`")
        })?;
        if assessment.line >= entry.line {
            return Err(format!(
                "reference `{name}` on line {} must follow assessment `{assessment_id}` on line {}",
                entry.line, assessment.line
            ));
        }
        if assessment.record.revision_id != record.revision_id {
            return Err(format!(
                "reference `{name}` names assessment `{assessment_id}` of revision `{}`, not `{}`",
                assessment.record.revision_id, record.revision_id
            ));
        }
    }
    let expected = (
        previous.map(|record| record.revision_id.as_str()),
        previous.and_then(|record| record.assessment_id.as_deref()),
    );
    let recorded = (
        record.superseded_revision_id.as_deref(),
        record.superseded_assessment_id.as_deref(),
    );
    if recorded != expected {
        return Err(format!(
            "reference `{name}` records superseded target {:?}, but the name's binding at that point was {:?}",
            recorded, expected
        ));
    }
    Ok(())
}

fn validate_amendment(
    view: &LogView,
    amendment_id: &str,
    entry: &AmendmentEntry,
) -> Result<(), String> {
    let record = &entry.record;
    validate_identifier("amendment", amendment_id)?;
    if record.schema_version != CONTRACT_AMENDMENT_SCHEMA_VERSION {
        return Err(format!(
            "amendment `{amendment_id}` uses unsupported schema `{}`",
            record.schema_version
        ));
    }
    non_empty("amendment actor", &record.actor)?;
    non_empty("amendment rationale", &record.rationale)?;
    let superseded = resolve_revision(view, &record.superseded_root_id).ok_or_else(|| {
        format!(
            "amendment `{amendment_id}` supersedes missing root `{}`",
            record.superseded_root_id
        )
    })?;
    if superseded.line() >= entry.line {
        return Err(format!(
            "amendment `{amendment_id}` on line {} must follow superseded root `{}` on line {}",
            entry.line,
            record.superseded_root_id,
            superseded.line()
        ));
    }
    if superseded.parent_revision_id(view).is_some() {
        return Err(format!(
            "amendment `{amendment_id}` supersedes `{}`, which is not a lineage root",
            record.superseded_root_id
        ));
    }
    if superseded.record_sha256() != record.superseded_record_sha256 {
        return Err(format!(
            "amendment `{amendment_id}` binds superseded root `{}` as {}, but its defining record hashes to {}",
            record.superseded_root_id,
            record.superseded_record_sha256,
            superseded.record_sha256()
        ));
    }
    if superseded.fixed_manifest_sha256() != record.superseded_manifest_sha256
        || superseded.fixed_compiled_snapshot_sha256() != record.superseded_compiled_snapshot_sha256
    {
        return Err(format!(
            "amendment `{amendment_id}` records superseded identities that do not match root `{}`",
            record.superseded_root_id
        ));
    }
    Ok(())
}

fn amendment_before<'a>(
    view: &'a LogView,
    amendment_id: &str,
    line: usize,
) -> Result<&'a AmendmentEntry, String> {
    let amendment = view
        .amendments
        .get(amendment_id)
        .ok_or_else(|| format!("amendment `{amendment_id}` does not exist in this log"))?;
    if amendment.line >= line {
        return Err(format!(
            "amendment `{amendment_id}` on line {} does not precede line {line}",
            amendment.line
        ));
    }
    Ok(amendment)
}

/// The consistency a revision citation demands of any record that names
/// one — a run row's `revision_id` member or a reference's target. The
/// revision must exist before the citation, carry the same fixed
/// identities and candidate, and agree on both parentage and amendment
/// edges.
#[allow(clippy::too_many_arguments)]
fn check_revision_edge(
    view: &LogView,
    revision_id: &str,
    parent_revision_id: Option<&str>,
    amendment_id: Option<&str>,
    candidate_state_sha256: &str,
    candidate_input: &str,
    manifest_sha256: &str,
    compiled_snapshot_sha256: &str,
    line: usize,
) -> Result<(), String> {
    let revision = resolve_revision(view, revision_id)
        .ok_or_else(|| format!("revision `{revision_id}` does not exist in this log"))?;
    if revision.line() >= line {
        return Err(format!(
            "revision `{revision_id}` on line {} does not precede line {line}",
            revision.line()
        ));
    }
    if revision.candidate_state_sha256() != candidate_state_sha256 {
        return Err(format!(
            "revision `{revision_id}` proposes candidate state {}, not {candidate_state_sha256}",
            revision.candidate_state_sha256()
        ));
    }
    if revision.candidate_input() != candidate_input {
        return Err(format!(
            "revision `{revision_id}` tracks candidate input `{}`, not `{candidate_input}`",
            revision.candidate_input()
        ));
    }
    if revision.fixed_manifest_sha256() != manifest_sha256
        || revision.fixed_compiled_snapshot_sha256() != compiled_snapshot_sha256
    {
        return Err(format!(
            "revision `{revision_id}` fixes different manifest or compiled snapshot identities"
        ));
    }
    let recorded_parent = revision.parent_revision_id(view);
    if recorded_parent.as_deref() != parent_revision_id {
        return Err(format!(
            "revision `{revision_id}` descends from {:?}, but this record's parent resolves to {:?}",
            recorded_parent, parent_revision_id
        ));
    }
    if revision.amendment_id() != amendment_id {
        return Err(format!(
            "revision `{revision_id}` cites amendment {:?}, but this record cites {:?}",
            revision.amendment_id(),
            amendment_id
        ));
    }
    Ok(())
}

/// The admission-time rules a new run's attempt request must satisfy
/// against the current log: revision binding agreement and the
/// amendment gate that keeps a changed question deliberate (ADR-0019).
/// Re-checked under the append lock before the run row is written.
pub(crate) fn check_attempt_binding(
    view: &LogView,
    request: &AttemptLineageRequest,
    case_id: &str,
    manifest_sha256: &str,
    compiled_snapshot_sha256: &str,
    candidate_state_sha256: &str,
    log_path: &Path,
) -> Result<(), String> {
    if request.amendment_id.is_some() && request.parent_attempt_id.is_some() {
        return Err(
            "a child attempt cannot cite an amendment; amendments link lineage roots".into(),
        );
    }
    if let Some(revision_id) = &request.revision_id {
        let parent_revision = request
            .parent_attempt_id
            .as_ref()
            .and_then(|parent_id| view.attempts.get(parent_id))
            .map(attempt_revision_id);
        check_revision_edge(
            view,
            revision_id,
            parent_revision.as_deref(),
            request.amendment_id.as_deref(),
            candidate_state_sha256,
            &request.candidate_input,
            manifest_sha256,
            compiled_snapshot_sha256,
            usize::MAX,
        )
        .map_err(|issue| format!("revision binding: {issue}"))?;
    }
    if request.parent_attempt_id.is_none() {
        match &request.amendment_id {
            Some(amendment_id) => {
                let amendment = view.amendments.get(amendment_id).ok_or_else(|| {
                    format!(
                        "amendment `{amendment_id}` does not exist in `{}`",
                        log_path.display()
                    )
                })?;
                if amendment.record.new_manifest_sha256 != manifest_sha256
                    || amendment.record.new_compiled_snapshot_sha256 != compiled_snapshot_sha256
                {
                    return Err(format!(
                        "amendment `{amendment_id}` admits identities {} / {}, not this run's {manifest_sha256} / {compiled_snapshot_sha256}",
                        amendment.record.new_manifest_sha256,
                        amendment.record.new_compiled_snapshot_sha256
                    ));
                }
                if let Some(RevisionRef::Derived(superseded)) =
                    resolve_revision(view, &amendment.record.superseded_root_id)
                    && superseded.case_id.as_deref() != Some(case_id)
                {
                    return Err(format!(
                        "amendment `{amendment_id}` supersedes a root of case `{:?}`, not `{case_id}`",
                        superseded.case_id
                    ));
                }
            }
            None => {
                let changed = view
                    .attempts
                    .values()
                    .filter(|attempt| {
                        attempt.record.parent_attempt_id.is_none()
                            && attempt.case_id.as_deref() == Some(case_id)
                            && attempt.record.fixed_manifest_sha256 != manifest_sha256
                    })
                    .map(|attempt| attempt.record.attempt_id.as_str())
                    .collect::<Vec<_>>();
                if !changed.is_empty() {
                    return Err(format!(
                        "case `{case_id}` already has lineage root(s) {} under a different manifest in `{}`; a deliberate question change needs an amendment record (`avila-core amend`) and `--amendment ID`",
                        changed.join(", "),
                        log_path.display()
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The caller's request to append a design revision without running.
pub struct RevisionRequest {
    pub revision_id: String,
    pub parent_revision_id: Option<String>,
    pub amendment_id: Option<String>,
    pub candidate_input: String,
    pub candidate: PathBuf,
    pub fixed_manifest_sha256: String,
    pub fixed_compiled_snapshot_sha256: String,
    pub created_by: String,
    pub intent: Option<String>,
}

pub(crate) fn read_log(path: &Path) -> Result<String, Box<dyn Error>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("cannot read log `{}`: {error}", path.display()).into()),
    }
}

/// Every check a complete revision record must pass against a parsed
/// log — shared by `create_revision`'s prepare step and the under-lock
/// revalidation before append.
fn check_revision_admission(
    view: &LogView,
    record: &DesignRevision,
    log_path: &Path,
) -> Result<(), String> {
    if view.revisions.contains_key(&record.revision_id)
        || view.attempts.contains_key(&record.revision_id)
    {
        return Err(format!(
            "revision id `{}` already exists in `{}`",
            record.revision_id,
            log_path.display()
        ));
    }
    // Re-run the structural validation as if the record were already in
    // the log; validate_revision tolerates the absent self because every
    // lookup names parents, never the record itself.
    let entry = RevisionEntry {
        record: record.clone(),
        record_sha256: String::new(),
        line: usize::MAX,
        full_line: Value::Null,
    };
    validate_revision(view, &record.revision_id, &entry, None)
        .map_err(|issue| format!("{issue} in `{}`", log_path.display()))
}

/// Append one design revision to a campaign log. Returns the appended
/// record and its exact line identity.
pub fn create_revision(
    log_path: &Path,
    request: &RevisionRequest,
    runner_key: Option<[u8; 32]>,
    trust_root: Option<&TrustRoot>,
) -> Result<(DesignRevision, String), Box<dyn Error>> {
    validate_identifier("revision", &request.revision_id)?;
    validate_identifier("candidate input", &request.candidate_input)?;
    if let Some(parent) = &request.parent_revision_id {
        validate_identifier("parent revision", parent)?;
        if parent == &request.revision_id {
            return Err("a revision cannot name itself as its parent".into());
        }
    }
    non_empty("created_by", &request.created_by)?;
    let (candidate_artifact_sha256, candidate_state_sha256, candidate_state) =
        load_candidate(&request.candidate, None)?;
    let view = parse_log(&read_log(log_path)?, log_path)?;
    validate_log(&view, trust_root)?;

    let (generation, parent_record_sha256, changes) =
        if let Some(parent_id) = &request.parent_revision_id {
            let parent = resolve_revision(&view, parent_id).ok_or_else(|| {
                format!(
                    "parent revision `{parent_id}` does not exist in `{}`",
                    log_path.display()
                )
            })?;
            let parent_generation = match parent {
                RevisionRef::Recorded(parent) => parent.record.generation,
                RevisionRef::Derived(parent) => parent.record.generation,
            };
            (
                parent_generation
                    .checked_add(1)
                    .ok_or("revision generation overflowed u64")?,
                Some(parent.record_sha256().to_string()),
                diff_candidate_states(parent.candidate_state(), &candidate_state),
            )
        } else {
            (0, None, Vec::new())
        };
    let record = DesignRevision {
        schema_version: DESIGN_REVISION_SCHEMA_VERSION.into(),
        revision_id: request.revision_id.clone(),
        generation,
        parent_revision_id: request.parent_revision_id.clone(),
        parent_record_sha256,
        amendment_id: request.amendment_id.clone(),
        fixed_manifest_sha256: request.fixed_manifest_sha256.clone(),
        fixed_compiled_snapshot_sha256: request.fixed_compiled_snapshot_sha256.clone(),
        candidate_input: request.candidate_input.clone(),
        candidate_artifact_sha256,
        candidate_state_sha256,
        candidate_state,
        changes,
        created_by: request.created_by.clone(),
        intent: request.intent.clone(),
    };
    check_revision_admission(&view, &record, log_path)?;

    let line = crate::case_run::log::record_line("design_revision", &record, runner_key)?;
    let record_sha256 = format!("sha256:{}", sha256_hex(line.as_bytes()));
    let expected = record.clone();
    crate::case_run::log::append_log_line(log_path, &line, move |content| {
        let view = parse_log(content, log_path)?;
        validate_log(&view, trust_root)?;
        check_revision_admission(&view, &expected, log_path)
    })?;
    Ok((record, record_sha256))
}

/// Append a named-reference binding. `assessment_id`, when supplied, must
/// name an assessment of the same revision. Returns the appended record
/// and its exact line identity.
#[allow(clippy::too_many_arguments)]
pub fn set_reference(
    log_path: &Path,
    name: &str,
    revision_id: &str,
    assessment_id: Option<&str>,
    actor: &str,
    rationale: &str,
    runner_key: Option<[u8; 32]>,
    trust_root: Option<&TrustRoot>,
) -> Result<(NamedReference, String), Box<dyn Error>> {
    validate_identifier("reference name", name)?;
    non_empty("actor", actor)?;
    non_empty("rationale", rationale)?;
    let view = parse_log(&read_log(log_path)?, log_path)?;
    validate_log(&view, trust_root)?;
    let record = NamedReference {
        schema_version: NAMED_REFERENCE_SCHEMA_VERSION.into(),
        name: name.to_string(),
        revision_id: revision_id.to_string(),
        assessment_id: assessment_id.map(str::to_owned),
        superseded_revision_id: view
            .references
            .get(name)
            .and_then(|entries| entries.last())
            .map(|entry| entry.record.revision_id.clone()),
        superseded_assessment_id: view
            .references
            .get(name)
            .and_then(|entries| entries.last())
            .and_then(|entry| entry.record.assessment_id.clone()),
        actor: actor.to_string(),
        rationale: rationale.to_string(),
    };
    check_reference_admission(&view, &record, log_path)?;

    let line = crate::case_run::log::record_line("named_reference", &record, runner_key)?;
    let record_sha256 = format!("sha256:{}", sha256_hex(line.as_bytes()));
    let expected = record.clone();
    crate::case_run::log::append_log_line(log_path, &line, move |content| {
        let view = parse_log(content, log_path)?;
        validate_log(&view, trust_root)?;
        check_reference_admission(&view, &expected, log_path)
    })?;
    Ok((record, record_sha256))
}

fn check_reference_admission(
    view: &LogView,
    record: &NamedReference,
    log_path: &Path,
) -> Result<(), String> {
    let entry = ReferenceEntry {
        record: record.clone(),
        record_sha256: String::new(),
        line: usize::MAX,
    };
    let previous = view
        .references
        .get(&record.name)
        .and_then(|entries| entries.last())
        .map(|entry| &entry.record);
    validate_reference(view, &entry, previous)
        .map_err(|issue| format!("{issue} in `{}`", log_path.display()))
}

/// Append a contract amendment: the deliberate question change that lets
/// a new lineage root continue a case under changed fixed identities.
/// Both manifest files are required so `changed_elements` is Core's
/// derived diff and the prior file provably is the recorded manifest.
#[allow(clippy::too_many_arguments)]
pub fn record_amendment(
    log_path: &Path,
    amendment_id: &str,
    supersedes: &str,
    prior_manifest: &Path,
    new_manifest: &Path,
    new_compiled_snapshot_sha256: &str,
    actor: &str,
    rationale: &str,
    runner_key: Option<[u8; 32]>,
    trust_root: Option<&TrustRoot>,
) -> Result<(ContractAmendment, String), Box<dyn Error>> {
    validate_identifier("amendment", amendment_id)?;
    non_empty("actor", actor)?;
    non_empty("rationale", rationale)?;
    let prior_bytes = fs::read(prior_manifest)
        .map_err(|error| format!("prior manifest `{}`: {error}", prior_manifest.display()))?;
    let prior_canonical = canonicalize_json(&prior_bytes).map_err(|error| {
        format!(
            "prior manifest `{}` is not canonical-profile JSON: {error}",
            prior_manifest.display()
        )
    })?;
    let new_bytes = fs::read(new_manifest)
        .map_err(|error| format!("new manifest `{}`: {error}", new_manifest.display()))?;
    let new_canonical = canonicalize_json(&new_bytes).map_err(|error| {
        format!(
            "new manifest `{}` is not canonical-profile JSON: {error}",
            new_manifest.display()
        )
    })?;
    let prior_state: Value = serde_json::from_slice(&prior_canonical)?;
    let new_state: Value = serde_json::from_slice(&new_canonical)?;
    let new_manifest_sha256 = digest(&new_canonical);

    let view = parse_log(&read_log(log_path)?, log_path)?;
    validate_log(&view, trust_root)?;
    let superseded = resolve_revision(&view, supersedes).ok_or_else(|| {
        format!(
            "superseded root `{supersedes}` does not exist in `{}`",
            log_path.display()
        )
    })?;
    if superseded.fixed_manifest_sha256() != digest(&prior_canonical) {
        return Err(format!(
            "prior manifest `{}` hashes to {}, but superseded root `{supersedes}` recorded {}",
            prior_manifest.display(),
            digest(&prior_canonical),
            superseded.fixed_manifest_sha256()
        )
        .into());
    }
    let record = ContractAmendment {
        schema_version: CONTRACT_AMENDMENT_SCHEMA_VERSION.into(),
        amendment_id: amendment_id.to_string(),
        superseded_root_id: supersedes.to_string(),
        superseded_record_sha256: superseded.record_sha256().to_string(),
        superseded_manifest_sha256: superseded.fixed_manifest_sha256().to_string(),
        superseded_compiled_snapshot_sha256: superseded
            .fixed_compiled_snapshot_sha256()
            .to_string(),
        new_manifest_sha256,
        new_compiled_snapshot_sha256: new_compiled_snapshot_sha256.to_string(),
        changed_elements: diff_candidate_states(&prior_state, &new_state),
        actor: actor.to_string(),
        rationale: rationale.to_string(),
    };
    let entry = AmendmentEntry {
        record: record.clone(),
        record_sha256: String::new(),
        line: usize::MAX,
    };
    validate_amendment(&view, amendment_id, &entry)
        .map_err(|issue| format!("{issue} in `{}`", log_path.display()))?;

    let line = crate::case_run::log::record_line("contract_amendment", &record, runner_key)?;
    let record_sha256 = format!("sha256:{}", sha256_hex(line.as_bytes()));
    crate::case_run::log::append_log_line(log_path, &line, move |content| {
        let view = parse_log(content, log_path)?;
        validate_log(&view, trust_root)?;
        let entry = AmendmentEntry {
            record: record.clone(),
            record_sha256: String::new(),
            line: usize::MAX,
        };
        if view.amendments.contains_key(amendment_id) {
            return Err(format!(
                "amendment id `{amendment_id}` appeared in `{}` while this record was being appended",
                log_path.display()
            ));
        }
        validate_amendment(&view, amendment_id, &entry)
    })?;
    Ok((entry.record, record_sha256))
}

/// The evidence identities an assessment binds to a run row.
pub(crate) struct AssessmentBinding {
    pub attempt: AttemptRecord,
    pub revision_id: String,
    pub run_record_sha256: String,
    pub manifest_sha256: String,
    pub compiled_snapshot_sha256: String,
    pub campaign_sha256: Option<String>,
    pub requirement_set_id: Option<String>,
    pub requirement_set_sha256: Option<String>,
    pub verdicts: Vec<Value>,
}

/// Append the assessment record a revision-bound run is evidence for.
/// Called by the run log append immediately after the run row lands; the
/// cited run-row sha and the revision's defining sha are re-checked under
/// the second append's lock.
pub(crate) fn append_assessment(
    log_path: &Path,
    binding: AssessmentBinding,
    runner_key: Option<[u8; 32]>,
    trust_root: Option<&TrustRoot>,
) -> Result<(), Box<dyn Error>> {
    let view = parse_log(&read_log(log_path)?, log_path)?;
    validate_log(&view, trust_root)?;
    let revision = resolve_revision(&view, &binding.revision_id).ok_or_else(|| {
        format!(
            "revision `{}` disappeared before its assessment was appended",
            binding.revision_id
        )
    })?;
    let record = AssessmentRecord {
        schema_version: ASSESSMENT_SCHEMA_VERSION.into(),
        assessment_id: binding.attempt.attempt_id.clone(),
        revision_id: binding.revision_id.clone(),
        revision_record_sha256: revision.record_sha256().to_string(),
        run_record_sha256: binding.run_record_sha256.clone(),
        manifest_sha256: binding.manifest_sha256,
        compiled_snapshot_sha256: binding.compiled_snapshot_sha256,
        campaign_sha256: binding.campaign_sha256,
        requirement_set_id: binding.requirement_set_id,
        requirement_set_sha256: binding.requirement_set_sha256,
        verdicts: binding.verdicts,
    };
    let line = crate::case_run::log::record_line("assessment", &record, runner_key)?;
    crate::case_run::log::append_log_line(log_path, &line, move |content| {
        let view = parse_log(content, log_path)?;
        validate_log(&view, trust_root)?;
        let entry = AssessmentEntry {
            record: record.clone(),
            record_sha256: String::new(),
            line: usize::MAX,
        };
        if view.assessments.contains_key(&record.assessment_id) {
            return Err(format!(
                "assessment id `{}` appeared while this record was being appended",
                record.assessment_id
            ));
        }
        validate_assessment(&view, &record.assessment_id, &entry)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attempt::{ATTEMPT_LINEAGE_SCHEMA_VERSION, AttemptRecord};
    use avila_core_evidence::signature::KeyRole;
    use serde_json::json;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "avila-core-history-test-{label}-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Write a canonical-profile JSON document and return its path plus the
    /// canonical digest `record_amendment`/`create_revision` record.
    fn document(dir: &Scratch, name: &str, value: Value) -> (PathBuf, String) {
        let path = dir.join(name);
        let canonical = canonicalize_json(&serde_json::to_vec(&value).unwrap()).unwrap();
        fs::write(&path, &canonical).unwrap();
        (path, digest(&canonical))
    }

    fn margin(value: &str) -> Value {
        json!({"requirement_id":"r","status":"pass","rule":"test","unit":"m","margin":value})
    }

    fn revision_request(
        revision_id: &str,
        candidate: PathBuf,
        manifest_sha256: &str,
        snapshot_sha256: &str,
    ) -> RevisionRequest {
        RevisionRequest {
            revision_id: revision_id.into(),
            parent_revision_id: None,
            amendment_id: None,
            candidate_input: "candidate".into(),
            candidate,
            fixed_manifest_sha256: manifest_sha256.into(),
            fixed_compiled_snapshot_sha256: snapshot_sha256.into(),
            created_by: "operator".into(),
            intent: None,
        }
    }

    /// A synthesized run row carrying an attempt record, plus the lineage
    /// request that produced it.
    #[allow(clippy::too_many_arguments)]
    fn run_row(
        attempt_id: &str,
        parent: Option<(&AttemptRecord, &str)>,
        candidate_state: &Value,
        manifest_sha256: &str,
        snapshot_sha256: &str,
        revision_id: Option<&str>,
        amendment_id: Option<&str>,
        verdicts: &[Value],
    ) -> (String, AttemptLineageRequest, AttemptRecord) {
        let mut record = AttemptRecord {
            schema_version: ATTEMPT_LINEAGE_SCHEMA_VERSION.into(),
            attempt_id: attempt_id.into(),
            generation: 0,
            parent_attempt_id: None,
            parent_record_sha256: None,
            fixed_manifest_sha256: manifest_sha256.into(),
            fixed_compiled_snapshot_sha256: snapshot_sha256.into(),
            candidate_input: "candidate".into(),
            candidate_artifact_sha256: digest("candidate bytes"),
            candidate_state_sha256: crate::attempt::candidate_state_identity(candidate_state)
                .unwrap(),
            candidate_state: candidate_state.clone(),
            changes: Vec::new(),
        };
        if let Some((parent, parent_line)) = parent {
            record.generation = parent.generation + 1;
            record.parent_attempt_id = Some(parent.attempt_id.clone());
            record.parent_record_sha256 = Some(digest(parent_line.as_bytes()));
            record.changes = diff_candidate_states(&parent.candidate_state, candidate_state);
        }
        let mut row = json!({
            "schema_version": "avila.core/run-attempt/v0.3-draft",
            "recorded_at": "2026-01-01T00:00:00Z",
            "case_path": "case",
            "case_id": "case",
            "status": "evaluated",
            "manifest_sha256": manifest_sha256,
            "compiled_snapshot_sha256": snapshot_sha256,
            "attempt": record,
            "verdicts": verdicts,
            "steps": [],
            "findings": [],
            "supplied_inputs": [],
        });
        if let Some(revision_id) = revision_id {
            row["revision_id"] = json!(revision_id);
            row["assessment_id"] = json!(attempt_id);
        }
        if let Some(amendment_id) = amendment_id {
            row["amendment_id"] = json!(amendment_id);
        }
        let request = AttemptLineageRequest {
            attempt_id: attempt_id.into(),
            parent_attempt_id: record.parent_attempt_id.clone(),
            candidate_input: "candidate".into(),
            revision_id: revision_id.map(str::to_owned),
            amendment_id: amendment_id.map(str::to_owned),
        };
        (row.to_string(), request, record)
    }

    /// Append a synthesized run row through the same under-lock
    /// revalidation `append_log` uses, so the test exercises admission.
    fn append_run(
        log: &Path,
        line: &str,
        request: &AttemptLineageRequest,
        attempt: &AttemptRecord,
    ) -> Result<(), Box<dyn Error>> {
        let attempt = attempt.clone();
        let request = request.clone();
        crate::case_run::log::append_log_line(log, line, move |content| {
            crate::attempt::revalidate_before_append(log, content, &attempt, &request, "case", None)
        })
    }

    fn view_of(log: &Path) -> LogView {
        let view = parse_log(&fs::read_to_string(log).unwrap(), log).unwrap();
        validate_log(&view, None).unwrap();
        view
    }

    #[test]
    fn a_revision_exists_before_any_run_cites_it() {
        let scratch = Scratch::new("unassessed");
        let log = scratch.join("campaign.jsonl");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        let (record, record_sha256) = create_revision(
            &log,
            &revision_request(
                "rev-001",
                candidate,
                &digest("manifest"),
                &digest("snapshot"),
            ),
            None,
            None,
        )
        .unwrap();
        assert_eq!(record.generation, 0);
        assert!(record_sha256.starts_with("sha256:"));

        let view = view_of(&log);
        assert_eq!(view.revisions.len(), 1);
        assert!(view.assessments.is_empty());
        let revision = resolve_revision(&view, "rev-001").unwrap();
        assert_eq!(revision.record_sha256(), record_sha256);
    }

    #[test]
    fn a_revision_bound_run_appends_an_assessment_copying_verdicts_verbatim() {
        let scratch = Scratch::new("assessed");
        let log = scratch.join("campaign.jsonl");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        let candidate_state: Value =
            serde_json::from_slice(&fs::read(&candidate).unwrap()).unwrap();
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        create_revision(
            &log,
            &revision_request("rev-001", candidate, &manifest, &snapshot),
            None,
            None,
        )
        .unwrap();

        let (line, request, attempt) = run_row(
            "run-001",
            None,
            &candidate_state,
            &manifest,
            &snapshot,
            Some("rev-001"),
            None,
            &[margin("1/3")],
        );
        let run_sha256 = digest(line.as_bytes());
        append_run(&log, &line, &request, &attempt).unwrap();
        append_assessment(
            &log,
            AssessmentBinding {
                attempt: attempt.clone(),
                revision_id: "rev-001".into(),
                run_record_sha256: run_sha256.clone(),
                manifest_sha256: manifest,
                compiled_snapshot_sha256: snapshot,
                campaign_sha256: None,
                requirement_set_id: None,
                requirement_set_sha256: None,
                verdicts: vec![margin("1/3")],
            },
            None,
            None,
        )
        .unwrap();

        let view = view_of(&log);
        let assessment = &view.assessments["run-001"].record;
        assert_eq!(assessment.revision_id, "rev-001");
        assert_eq!(assessment.run_record_sha256, run_sha256);
        assert_eq!(assessment.verdicts, vec![margin("1/3")]);
    }

    #[test]
    fn two_runs_of_one_unchanged_revision_give_two_assessments() {
        let scratch = Scratch::new("two-assessments");
        let log = scratch.join("campaign.jsonl");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        let candidate_state: Value =
            serde_json::from_slice(&fs::read(&candidate).unwrap()).unwrap();
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        create_revision(
            &log,
            &revision_request("rev-001", candidate, &manifest, &snapshot),
            None,
            None,
        )
        .unwrap();

        for (attempt_id, value) in [("run-a", "1/3"), ("run-b", "1/4")] {
            let (line, request, attempt) = run_row(
                attempt_id,
                None,
                &candidate_state,
                &manifest,
                &snapshot,
                Some("rev-001"),
                None,
                &[margin(value)],
            );
            let run_sha256 = digest(line.as_bytes());
            append_run(&log, &line, &request, &attempt).unwrap();
            append_assessment(
                &log,
                AssessmentBinding {
                    attempt,
                    revision_id: "rev-001".into(),
                    run_record_sha256: run_sha256,
                    manifest_sha256: manifest.clone(),
                    compiled_snapshot_sha256: snapshot.clone(),
                    campaign_sha256: None,
                    requirement_set_id: None,
                    requirement_set_sha256: None,
                    verdicts: vec![margin(value)],
                },
                None,
                None,
            )
            .unwrap();
        }
        let view = view_of(&log);
        assert_eq!(view.assessments.len(), 2);
        assert!(
            view.assessments
                .values()
                .all(|entry| entry.record.revision_id == "rev-001")
        );
    }

    #[test]
    fn a_duplicate_revision_id_is_refused_before_and_under_the_lock() {
        let scratch = Scratch::new("duplicate");
        let log = scratch.join("campaign.jsonl");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        let request = revision_request(
            "rev-001",
            candidate,
            &digest("manifest"),
            &digest("snapshot"),
        );
        create_revision(&log, &request, None, None).unwrap();
        let error = create_revision(&log, &request, None, None).unwrap_err();
        assert!(error.to_string().contains("already exists"), "{error}");
    }

    #[test]
    fn a_revision_naming_a_missing_parent_is_refused() {
        let scratch = Scratch::new("missing-parent");
        let log = scratch.join("campaign.jsonl");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        let mut request = revision_request(
            "rev-002",
            candidate,
            &digest("manifest"),
            &digest("snapshot"),
        );
        request.parent_revision_id = Some("never-existed".into());
        let error = create_revision(&log, &request, None, None).unwrap_err();
        assert!(error.to_string().contains("never-existed"), "{error}");
    }

    #[test]
    fn a_child_revision_derives_its_changes_and_binds_the_exact_parent_line() {
        let scratch = Scratch::new("child-revision");
        let log = scratch.join("campaign.jsonl");
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        let (candidate_a, _) = document(&scratch, "a.json", json!({"thickness":"1"}));
        let (record, parent_sha256) = create_revision(
            &log,
            &revision_request("rev-001", candidate_a, &manifest, &snapshot),
            None,
            None,
        )
        .unwrap();
        assert!(record.parent_revision_id.is_none());

        let (candidate_b, _) = document(&scratch, "b.json", json!({"thickness":"2"}));
        let mut request = revision_request("rev-002", candidate_b, &manifest, &snapshot);
        request.parent_revision_id = Some("rev-001".into());
        let (child, _) = create_revision(&log, &request, None, None).unwrap();
        assert_eq!(child.generation, 1);
        assert_eq!(child.parent_revision_id.as_deref(), Some("rev-001"));
        assert_eq!(
            child.parent_record_sha256.as_deref(),
            Some(parent_sha256.as_str())
        );
        assert_eq!(
            child.changes,
            vec![AttemptChange::Replaced {
                pointer: "/thickness".into(),
                before: json!("1"),
                after: json!("2"),
            }]
        );

        // A child crossing the fixed identity is refused.
        let (candidate_c, _) = document(&scratch, "c.json", json!({"thickness":"3"}));
        let mut request = revision_request("rev-003", candidate_c, &digest("other"), &snapshot);
        request.parent_revision_id = Some("rev-002".into());
        let error = create_revision(&log, &request, None, None).unwrap_err();
        assert!(
            error.to_string().contains("fixed question identity"),
            "{error}"
        );
    }

    #[test]
    fn a_run_whose_candidate_disagrees_with_its_cited_revision_is_refused() {
        let scratch = Scratch::new("edge-mismatch");
        let log = scratch.join("campaign.jsonl");
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        create_revision(
            &log,
            &revision_request("rev-001", candidate, &manifest, &snapshot),
            None,
            None,
        )
        .unwrap();

        // The run's candidate state is not the revision's.
        let (line, request, attempt) = run_row(
            "run-001",
            None,
            &json!({"thickness":"9"}),
            &manifest,
            &snapshot,
            Some("rev-001"),
            None,
            &[],
        );
        let error = append_run(&log, &line, &request, &attempt).unwrap_err();
        assert!(
            error.to_string().contains("proposes candidate state"),
            "{error}"
        );

        // And a row carrying a citation whose assessment id is not the
        // attempt's own id — something `append_log` never writes, so it
        // can only arrive out of band — fails validation on the next read.
        let (line, request, attempt) = run_row(
            "run-002",
            None,
            &json!({"thickness":"1"}),
            &manifest,
            &snapshot,
            Some("rev-001"),
            None,
            &[],
        );
        let mut row: Value = serde_json::from_str(&line).unwrap();
        row["assessment_id"] = json!("someone-else");
        let line = row.to_string();
        crate::case_run::log::append_log_line(&log, &line, |_| Ok(())).unwrap();
        let _ = (request, attempt);
        let view = parse_log(&fs::read_to_string(&log).unwrap(), &log).unwrap();
        let error = validate_log(&view, None).unwrap_err();
        assert!(
            error.contains("an assessment id is always its run's attempt id"),
            "{error}"
        );
    }

    #[test]
    fn a_tampered_assessment_verdict_copy_fails_the_whole_log() {
        let scratch = Scratch::new("tampered-assessment");
        let log = scratch.join("campaign.jsonl");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        let candidate_state: Value =
            serde_json::from_slice(&fs::read(&candidate).unwrap()).unwrap();
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        create_revision(
            &log,
            &revision_request("rev-001", candidate, &manifest, &snapshot),
            None,
            None,
        )
        .unwrap();
        let (line, request, attempt) = run_row(
            "run-001",
            None,
            &candidate_state,
            &manifest,
            &snapshot,
            Some("rev-001"),
            None,
            &[margin("1/3")],
        );
        let run_sha256 = digest(line.as_bytes());
        append_run(&log, &line, &request, &attempt).unwrap();
        append_assessment(
            &log,
            AssessmentBinding {
                attempt,
                revision_id: "rev-001".into(),
                run_record_sha256: run_sha256,
                manifest_sha256: manifest,
                compiled_snapshot_sha256: snapshot,
                campaign_sha256: None,
                requirement_set_id: None,
                requirement_set_sha256: None,
                verdicts: vec![margin("1/3")],
            },
            None,
            None,
        )
        .unwrap();

        // Rewrite the assessment row's recorded verdict copy after the
        // fact — the run row keeps its own `"1/3"`.
        let content = fs::read_to_string(&log).unwrap();
        let mut lines: Vec<String> = content.split('\n').map(str::to_owned).collect();
        let last = lines.len() - 2; // trailing newline leaves a final empty split
        lines[last] = lines[last].replacen("\"1/3\"", "\"9/9\"", 1);
        let tampered = lines.join("\n");
        assert_ne!(tampered, content);
        let view = parse_log(&tampered, &log).unwrap();
        let error = validate_log(&view, None).unwrap_err();
        assert!(error.contains("verbatim copy"), "{error}");
    }

    #[test]
    fn a_revision_less_attempt_reads_as_a_derived_revision_and_assessment() {
        let scratch = Scratch::new("legacy");
        let log = scratch.join("campaign.jsonl");
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        let (line, request, attempt) = run_row(
            "legacy-001",
            None,
            &json!({"thickness":"1"}),
            &manifest,
            &snapshot,
            None,
            None,
            &[margin("1/3")],
        );
        append_run(&log, &line, &request, &attempt).unwrap();

        let view = view_of(&log);
        let revision = resolve_revision(&view, "legacy-001").unwrap();
        let RevisionRef::Derived(derived) = revision else {
            panic!("a revision-less attempt must project a derived revision")
        };
        assert_eq!(derived.record.attempt_id, "legacy-001");
        assert_eq!(attempt_revision_id(derived), "legacy-001");
    }

    #[test]
    fn a_named_reference_moves_and_keeps_its_history() {
        let scratch = Scratch::new("reference");
        let log = scratch.join("campaign.jsonl");
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        let (candidate_a, _) = document(&scratch, "a.json", json!({"thickness":"1"}));
        create_revision(
            &log,
            &revision_request("rev-001", candidate_a, &manifest, &snapshot),
            None,
            None,
        )
        .unwrap();
        let (candidate_b, _) = document(&scratch, "b.json", json!({"thickness":"2"}));
        let mut request = revision_request("rev-002", candidate_b, &manifest, &snapshot);
        request.parent_revision_id = Some("rev-001".into());
        create_revision(&log, &request, None, None).unwrap();

        let (first, _) = set_reference(
            &log,
            "baseline",
            "rev-001",
            None,
            "operator",
            "initial baseline",
            None,
            None,
        )
        .unwrap();
        assert!(first.superseded_revision_id.is_none());
        let (moved, _) = set_reference(
            &log,
            "baseline",
            "rev-002",
            None,
            "operator",
            "promote the child",
            None,
            None,
        )
        .unwrap();
        assert_eq!(moved.superseded_revision_id.as_deref(), Some("rev-001"));

        // A move back to the earlier target is a legal move; every move
        // keeps its record.
        let result = set_reference(
            &log, "baseline", "rev-001", None, "operator", "go back", None, None,
        );
        assert!(result.is_ok(), "moving back is a legal move: {result:?}");
        let view = view_of(&log);
        assert_eq!(view.references["baseline"].len(), 3);
    }

    #[test]
    fn a_reference_citing_a_foreign_assessment_is_refused() {
        let scratch = Scratch::new("foreign-assessment");
        let log = scratch.join("campaign.jsonl");
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        let candidate_state: Value =
            serde_json::from_slice(&fs::read(&candidate).unwrap()).unwrap();
        create_revision(
            &log,
            &revision_request("rev-001", candidate.clone(), &manifest, &snapshot),
            None,
            None,
        )
        .unwrap();
        create_revision(
            &log,
            &revision_request("rev-002", candidate, &manifest, &snapshot),
            None,
            None,
        )
        .unwrap();
        let (line, request, attempt) = run_row(
            "run-001",
            None,
            &candidate_state,
            &manifest,
            &snapshot,
            Some("rev-001"),
            None,
            &[margin("1/3")],
        );
        let run_sha256 = digest(line.as_bytes());
        append_run(&log, &line, &request, &attempt).unwrap();
        append_assessment(
            &log,
            AssessmentBinding {
                attempt,
                revision_id: "rev-001".into(),
                run_record_sha256: run_sha256,
                manifest_sha256: manifest,
                compiled_snapshot_sha256: snapshot,
                campaign_sha256: None,
                requirement_set_id: None,
                requirement_set_sha256: None,
                verdicts: vec![margin("1/3")],
            },
            None,
            None,
        )
        .unwrap();

        let error = set_reference(
            &log,
            "review-target",
            "rev-002",
            Some("run-001"),
            "operator",
            "wrong revision's evidence",
            None,
            None,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("of revision `rev-001`"),
            "{error}"
        );
    }

    #[test]
    fn an_amendment_admits_a_new_root_across_changed_fixed_identities() {
        let scratch = Scratch::new("amendment");
        let log = scratch.join("campaign.jsonl");
        let (prior_manifest, prior_sha256) =
            document(&scratch, "manifest-v1.json", json!({"question":"v1"}));
        let (new_manifest, new_sha256) =
            document(&scratch, "manifest-v2.json", json!({"question":"v2"}));
        let snapshot_old = digest("snapshot-old");
        let snapshot_new = digest("snapshot-new");
        let (candidate, _) = document(&scratch, "candidate.json", json!({"thickness":"1"}));
        let candidate_state: Value =
            serde_json::from_slice(&fs::read(&candidate).unwrap()).unwrap();

        create_revision(
            &log,
            &revision_request("rev-old", candidate.clone(), &prior_sha256, &snapshot_old),
            None,
            None,
        )
        .unwrap();
        let (amendment, _) = record_amendment(
            &log,
            "amend-001",
            "rev-old",
            &prior_manifest,
            &new_manifest,
            &snapshot_new,
            "operator",
            "the question changed deliberately",
            None,
            None,
        )
        .unwrap();
        assert_eq!(amendment.superseded_root_id, "rev-old");
        assert_eq!(amendment.new_manifest_sha256, new_sha256);
        assert_eq!(amendment.changed_elements.len(), 1);

        // The new root under the changed identities must cite the amendment.
        let mut request = revision_request("rev-new", candidate, &new_sha256, &snapshot_new);
        request.amendment_id = Some("amend-001".into());
        create_revision(&log, &request, None, None).unwrap();
        let (line, run_request, attempt) = run_row(
            "run-new",
            None,
            &candidate_state,
            &new_sha256,
            &snapshot_new,
            Some("rev-new"),
            Some("amend-001"),
            &[margin("1/3")],
        );
        append_run(&log, &line, &run_request, &attempt).unwrap();
        view_of(&log);
    }

    #[test]
    fn a_changed_root_without_an_amendment_is_refused() {
        let scratch = Scratch::new("silent-amendment");
        let log = scratch.join("campaign.jsonl");
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        let (line, request, attempt) = run_row(
            "run-001",
            None,
            &json!({"thickness":"1"}),
            &manifest,
            &snapshot,
            None,
            None,
            &[],
        );
        append_run(&log, &line, &request, &attempt).unwrap();

        // A second root for the same case under a different manifest and
        // no amendment is refused; the boundary stays deliberate.
        let (line, request, attempt) = run_row(
            "run-002",
            None,
            &json!({"thickness":"1"}),
            &digest("changed-manifest"),
            &snapshot,
            None,
            None,
            &[],
        );
        let error = append_run(&log, &line, &request, &attempt).unwrap_err();
        assert!(error.to_string().contains("needs an amendment"), "{error}");
    }

    #[test]
    fn a_child_citing_an_amendment_is_refused() {
        let scratch = Scratch::new("child-amendment");
        let log = scratch.join("campaign.jsonl");
        let manifest = digest("manifest");
        let snapshot = digest("snapshot");
        let (line, request, attempt) = run_row(
            "run-001",
            None,
            &json!({"thickness":"1"}),
            &manifest,
            &snapshot,
            None,
            None,
            &[],
        );
        append_run(&log, &line, &request, &attempt).unwrap();
        let (line, mut request, child) = run_row(
            "run-002",
            Some((&attempt, &line)),
            &json!({"thickness":"2"}),
            &manifest,
            &snapshot,
            None,
            Some("amend-001"),
            &[],
        );
        request.amendment_id = Some("amend-001".into());
        let error = append_run(&log, &line, &request, &child).unwrap_err();
        assert!(
            error.to_string().contains("amendments link lineage roots"),
            "{error}"
        );
    }

    // ---- ADR-0021: attestations and state transitions ----

    fn test_seed() -> [u8; 32] {
        [7u8; 32]
    }

    fn attestation_request(id: &str, role: KeyRole) -> crate::transitions::AttestationRequest {
        crate::transitions::AttestationRequest {
            attestation_id: id.into(),
            subject: crate::transitions::AttestationSubject {
                kind: crate::transitions::AttestationSubjectKind::Campaign,
                identity: "camp-1".into(),
                sha256: digest("campaign dossier"),
            },
            statement: crate::transitions::AttestationStatement::Approves,
            detail: "test attestation".into(),
            target: None,
            actor: crate::transitions::ActorRef {
                actor_id: "operator-1".into(),
                actor_kind: crate::transitions::ActorKind::Person,
            },
            role,
        }
    }

    fn transition_request(
        id: &str,
        kind: crate::transitions::TransitionSubjectKind,
        identity: &str,
        to: &str,
        attestation_id: &str,
    ) -> crate::transitions::TransitionRequest {
        crate::transitions::TransitionRequest {
            transition_id: id.into(),
            subject: crate::transitions::TransitionSubject {
                kind,
                identity: identity.into(),
            },
            to_state: to.into(),
            attestation_id: attestation_id.into(),
            at: "2026-09-19T00:00:00Z".into(),
            rationale: "test".into(),
            step_effects: Vec::new(),
        }
    }

    #[test]
    fn an_attestation_and_transition_derive_the_campaign_state() {
        let scratch = Scratch::new("transition-happy");
        let log = scratch.join("campaign.jsonl");
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-001", KeyRole::Requester),
            &test_seed(),
            None,
            None,
        )
        .unwrap();
        let (record, _) = crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-001",
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1",
                "running",
                "att-001",
            ),
            None,
            None,
        )
        .unwrap();
        assert_eq!(record.from_state, "planned");
        assert_eq!(record.to_state, "running");
        let view = view_of(&log);
        assert_eq!(view.attestations.len(), 1);
        assert_eq!(view.transitions.len(), 1);
        assert_eq!(
            crate::transitions::derived_state(
                &view,
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1"
            ),
            "running"
        );
    }

    #[test]
    fn an_illegal_transition_edge_is_refused() {
        let scratch = Scratch::new("transition-illegal");
        let log = scratch.join("campaign.jsonl");
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-001", KeyRole::Requester),
            &test_seed(),
            None,
            None,
        )
        .unwrap();
        // `planned` cannot move straight to `completed`.
        let error = crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-001",
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1",
                "completed",
                "att-001",
            ),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("CORE-X6402"), "{error}");
        // Nor a self-transition once running.
        crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-001",
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1",
                "running",
                "att-001",
            ),
            None,
            None,
        )
        .unwrap();
        let error = crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-002",
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1",
                "running",
                "att-001",
            ),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("CORE-X6402"), "{error}");
    }

    #[test]
    fn a_transition_without_its_attestation_is_refused() {
        let scratch = Scratch::new("transition-no-att");
        let log = scratch.join("campaign.jsonl");
        let error = crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-001",
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1",
                "running",
                "att-missing",
            ),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not exist"), "{error}");
    }

    #[test]
    fn a_transition_under_the_wrong_role_is_refused() {
        let scratch = Scratch::new("transition-wrong-role");
        let log = scratch.join("campaign.jsonl");
        // Campaign `running -> superseded` requires a requester attestation;
        // a runner attestation does not satisfy it.
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-runner", KeyRole::Runner),
            &test_seed(),
            None,
            None,
        )
        .unwrap();
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-requester", KeyRole::Requester),
            &test_seed(),
            None,
            None,
        )
        .unwrap();
        crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-001",
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1",
                "running",
                "att-requester",
            ),
            None,
            None,
        )
        .unwrap();
        let error = crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-002",
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1",
                "superseded",
                "att-runner",
            ),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("CORE-X6403"), "{error}");
    }

    #[test]
    fn a_hand_written_line_claiming_the_wrong_from_state_fails_closed() {
        let scratch = Scratch::new("transition-bad-from");
        let log = scratch.join("campaign.jsonl");
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-001", KeyRole::Requester),
            &test_seed(),
            None,
            None,
        )
        .unwrap();
        let (_, att_sha) = crate::transitions::append_attestation(
            &log,
            &attestation_request("att-002", KeyRole::Requester),
            &test_seed(),
            None,
            None,
        )
        .unwrap();
        // Forge a transition whose from_state does not equal the derived
        // `planned`: the fold must refuse it.
        let forged = crate::transitions::StateTransition {
            schema_version: crate::transitions::STATE_TRANSITION_SCHEMA_VERSION.into(),
            transition_id: "tr-forged".into(),
            subject: crate::transitions::TransitionSubject {
                kind: crate::transitions::TransitionSubjectKind::Campaign,
                identity: "camp-1".into(),
            },
            from_state: "running".into(),
            to_state: "completed".into(),
            actor_attestation: crate::transitions::AttestationRef {
                attestation_id: "att-002".into(),
                sha256: att_sha,
            },
            at: "2026-09-19T00:00:00Z".into(),
            rationale: String::new(),
            step_effects: Vec::new(),
        };
        let line = crate::case_run::log::record_line("state_transition", &forged, None).unwrap();
        use std::io::Write;
        let mut file = fs::OpenOptions::new().append(true).open(&log).unwrap();
        writeln!(file, "{line}").unwrap();
        drop(file);
        let view = parse_log(&fs::read_to_string(&log).unwrap(), &log).unwrap();
        let error = validate_log(&view, None).unwrap_err();
        assert!(error.contains("CORE-X6402"), "{error}");
    }

    #[test]
    fn step_states_use_their_own_vocabulary() {
        let scratch = Scratch::new("transition-step");
        let log = scratch.join("campaign.jsonl");
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-001", KeyRole::Runner),
            &test_seed(),
            None,
            None,
        )
        .unwrap();
        // The only recorded step move is cancellation of an in-flight
        // step; a step with no transitions derives `pending`.
        let (record, _) = crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-001",
                crate::transitions::TransitionSubjectKind::Step,
                "camp-1/transport",
                "cancelled",
                "att-001",
            ),
            None,
            None,
        )
        .unwrap();
        assert_eq!(record.from_state, "pending");
        assert_eq!(record.to_state, "cancelled");
        // `completed` is not a step state.
        let error = crate::transitions::append_transition(
            &log,
            &transition_request(
                "tr-002",
                crate::transitions::TransitionSubjectKind::Step,
                "camp-1/other",
                "completed",
                "att-001",
            ),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("CORE-X6402"), "{error}");
    }

    #[test]
    fn a_policy_owner_attestation_authorizes_invalidation_under_a_trust_root() {
        let scratch = Scratch::new("transition-policy-owner");
        let log = scratch.join("campaign.jsonl");
        let seed = test_seed();
        let (key_id, _) = avila_core_evidence::signature::sign_digest(&seed, &[0u8; 32]);
        let trust_root = avila_core_evidence::signature::TrustRoot {
            schema_version: avila_core_evidence::signature::TRUST_ROOT_SCHEMA_VERSION.into(),
            keys: vec![
                avila_core_evidence::signature::TrustRootEntry {
                    key_id: key_id.clone(),
                    public_key_hex: avila_core_evidence::signature::public_key_hex_from_seed(&seed),
                    role: KeyRole::Requester,
                },
                avila_core_evidence::signature::TrustRootEntry {
                    key_id,
                    public_key_hex: avila_core_evidence::signature::public_key_hex_from_seed(&seed),
                    role: KeyRole::PolicyOwner,
                },
            ],
        };
        // One key listed under two roles: the requester attestation runs the
        // campaign, but only the policy_owner listing authorizes invalidation.
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-001", KeyRole::Requester),
            &seed,
            None,
            Some(&trust_root),
        )
        .unwrap();
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-002", KeyRole::PolicyOwner),
            &seed,
            None,
            Some(&trust_root),
        )
        .unwrap();
        for (id, to, att) in [
            ("tr-1", "running", "att-001"),
            ("tr-2", "completed", "att-001"),
            ("tr-3", "invalidated", "att-002"),
        ] {
            crate::transitions::append_transition(
                &log,
                &transition_request(
                    id,
                    crate::transitions::TransitionSubjectKind::Campaign,
                    "camp-1",
                    to,
                    att,
                ),
                None,
                Some(&trust_root),
            )
            .unwrap();
        }
        let view = view_of(&log);
        assert_eq!(
            crate::transitions::derived_state(
                &view,
                crate::transitions::TransitionSubjectKind::Campaign,
                "camp-1"
            ),
            "invalidated"
        );
        // The same key under only the requester role could not have
        // authorized that move: a root without the policy_owner listing
        // makes the log fail validation.
        let requester_only = avila_core_evidence::signature::TrustRoot {
            schema_version: trust_root.schema_version.clone(),
            keys: vec![trust_root.keys[0].clone()],
        };
        let error = validate_log(&view, Some(&requester_only)).unwrap_err();
        assert!(error.contains("CORE-X6403"), "{error}");
    }

    #[test]
    fn a_terminal_subject_rejects_every_move() {
        let scratch = Scratch::new("transition-terminal");
        let log = scratch.join("campaign.jsonl");
        crate::transitions::append_attestation(
            &log,
            &attestation_request("att-001", KeyRole::Requester),
            &test_seed(),
            None,
            None,
        )
        .unwrap();
        for (id, to) in [("tr-1", "running"), ("tr-2", "completed")] {
            crate::transitions::append_transition(
                &log,
                &transition_request(
                    id,
                    crate::transitions::TransitionSubjectKind::Campaign,
                    "camp-1",
                    to,
                    "att-001",
                ),
                None,
                None,
            )
            .unwrap();
        }
        // `completed` is terminal: nothing moves it again.
        for to in ["running", "blocked", "superseded", "cancelled"] {
            let error = crate::transitions::append_transition(
                &log,
                &transition_request(
                    "tr-x",
                    crate::transitions::TransitionSubjectKind::Campaign,
                    "camp-1",
                    to,
                    "att-001",
                ),
                None,
                None,
            )
            .unwrap_err();
            assert!(error.to_string().contains("CORE-X6402"), "{error}");
        }
    }
}
