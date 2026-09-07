//! Identity-bound lineage for candidate-search attempts.
//!
//! Core remains the oracle rather than the optimizer. A caller nominates one
//! supplied JSON input as the candidate and names an attempt plus an optional
//! parent. Core stores the canonical candidate state, derives the typed delta,
//! and binds a child to the exact parent log line and fixed question identity.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use avila_core_evidence::signature::{
    KeyRole, SignatureDocument, TrustRoot, check_internal_consistency, verify_signature_document,
};
use avila_core_evidence::{sha256_file, sha256_hex};
use avila_core_kernel::{VerdictStatus, canonicalize_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const ATTEMPT_LINEAGE_SCHEMA_VERSION: &str = "avila.core/attempt-lineage/v0.1-draft";
pub const ATTEMPT_COMPARISON_SCHEMA_VERSION: &str = "avila.core/attempt-comparison/v0.1-draft";
const MAX_CANDIDATE_BYTES: u64 = 1024 * 1024;

/// The caller's request to place one run in an attempt lineage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptLineageRequest {
    pub attempt_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_attempt_id: Option<String>,
    pub candidate_input: String,
}

/// A typed change derived by Core from the parent's and child's canonical
/// candidate JSON. It is observation, not a designer-authored explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttemptChange {
    Added {
        pointer: String,
        value: Value,
    },
    Removed {
        pointer: String,
        value: Value,
    },
    Replaced {
        pointer: String,
        before: Value,
        after: Value,
    },
}

impl AttemptChange {
    #[must_use]
    pub fn pointer(&self) -> &str {
        match self {
            Self::Added { pointer, .. }
            | Self::Removed { pointer, .. }
            | Self::Replaced { pointer, .. } => pointer,
        }
    }
}

/// The lineage portion of a case report and its corresponding JSONL record.
/// The surrounding record supplies the findings, verdicts, receipts, and
/// output artifacts produced by this exact candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub schema_version: String,
    pub attempt_id: String,
    pub generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_record_sha256: Option<String>,
    pub fixed_manifest_sha256: String,
    pub fixed_compiled_snapshot_sha256: String,
    pub candidate_input: String,
    pub candidate_artifact_sha256: String,
    pub candidate_state_sha256: String,
    pub candidate_state: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<AttemptChange>,
}

/// A child run's Core-derived comparison with the exact parent JSONL record
/// its lineage already binds. This is an observation over two Core results,
/// not an optimizer-authored assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptComparison {
    pub schema_version: String,
    pub parent_attempt_id: String,
    pub parent_record_sha256: String,
    pub verdicts_compared: usize,
    pub unchanged_verdicts: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verdict_transitions: Vec<AttemptVerdictTransition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verdict_comparison_unavailable: Vec<AttemptVerdictUnavailable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exact_margin_comparisons: Vec<AttemptMarginComparison>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub margin_comparison_unavailable: Vec<AttemptMarginUnavailable>,
}

/// One requirement whose Core verdict state changed from parent to child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptVerdictTransition {
    pub requirement_id: String,
    pub parent_status: VerdictStatus,
    pub child_status: VerdictStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptVerdictUnavailableReason {
    ParentMissing,
    ChildMissing,
}

/// A requirement that appeared in only one of the two result surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptVerdictUnavailable {
    pub requirement_id: String,
    pub reason: AttemptVerdictUnavailableReason,
}

/// An exact arithmetic comparison of two safety margins. Since every Core
/// numeric margin is positive inside its bound and negative outside, a
/// positive delta means the child has more margin than its parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptMarginComparison {
    pub requirement_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub parent_margin: String,
    pub child_margin: String,
    pub delta: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptMarginUnavailableReason {
    ParentMissing,
    ChildMissing,
    BothMissing,
    NotNumeric,
    UnitMismatch,
    LimitMismatch,
    InvalidNumber,
}

/// Why Core did not claim an exact numeric margin delta for a requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptMarginUnavailable {
    pub requirement_id: String,
    pub reason: AttemptMarginUnavailableReason,
}

#[derive(Debug)]
struct PriorAttempt {
    record: AttemptRecord,
    record_sha256: String,
    line: usize,
    top_level_manifest_sha256: Option<String>,
    top_level_compiled_snapshot_sha256: Option<String>,
    verdicts: Option<Value>,
    /// The complete raw JSON of this line, retained so a parent's own
    /// `signature` member can be verified (ADR-0015 clause 6) without
    /// re-reading the log file.
    full_line: Value,
}

pub(crate) fn prepare_attempt(
    request: &AttemptLineageRequest,
    log_path: Option<&Path>,
    candidate_path: Option<&Path>,
    supplied_candidate_sha256: Option<&str>,
    manifest_sha256: &str,
    compiled_snapshot_sha256: &str,
    trust_root: Option<&TrustRoot>,
) -> Result<AttemptRecord, String> {
    validate_identifier("attempt", &request.attempt_id)?;
    validate_identifier("candidate input", &request.candidate_input)?;
    if let Some(parent) = &request.parent_attempt_id {
        validate_identifier("parent attempt", parent)?;
        if parent == &request.attempt_id {
            return Err("an attempt cannot name itself as its parent".into());
        }
    }
    let log_path = log_path.ok_or("attempt lineage requires `--log FILE`")?;
    let candidate_path = candidate_path.ok_or_else(|| {
        format!(
            "candidate input `{}` was not supplied with `--input {}=PATH`",
            request.candidate_input, request.candidate_input
        )
    })?;
    let supplied_candidate_sha256 = supplied_candidate_sha256.ok_or_else(|| {
        format!(
            "candidate input `{}` did not become a verified supplied input",
            request.candidate_input
        )
    })?;

    let (candidate_artifact_sha256, bytes) = sha256_file(candidate_path)
        .map_err(|error| format!("candidate `{}`: {error}", candidate_path.display()))?;
    if candidate_artifact_sha256 != supplied_candidate_sha256 {
        return Err(format!(
            "candidate `{}` changed while the attempt was being planned: supplied identity {supplied_candidate_sha256}, observed {candidate_artifact_sha256}",
            candidate_path.display()
        ));
    }
    if bytes > MAX_CANDIDATE_BYTES {
        return Err(format!(
            "candidate `{}` is {bytes} bytes; lineage candidates are limited to {MAX_CANDIDATE_BYTES} bytes",
            candidate_path.display()
        ));
    }
    let candidate_bytes = fs::read(candidate_path)
        .map_err(|error| format!("candidate `{}`: {error}", candidate_path.display()))?;
    let canonical = canonicalize_json(&candidate_bytes).map_err(|error| {
        format!(
            "candidate `{}` is not canonical-profile JSON: {error}",
            candidate_path.display()
        )
    })?;
    let candidate_state: Value = serde_json::from_slice(&canonical).map_err(|error| {
        format!(
            "candidate `{}` could not be represented as JSON: {error}",
            candidate_path.display()
        )
    })?;
    let candidate_state_sha256 = digest(&canonical);

    let attempts = read_attempts(log_path)?;
    validate_history(&attempts, trust_root)?;
    if attempts.contains_key(&request.attempt_id) {
        return Err(format!(
            "attempt id `{}` already exists in `{}`",
            request.attempt_id,
            log_path.display()
        ));
    }

    let (generation, parent_record_sha256, changes) = if let Some(parent_id) =
        &request.parent_attempt_id
    {
        let parent = attempts.get(parent_id).ok_or_else(|| {
            format!(
                "parent attempt `{parent_id}` does not exist in `{}`",
                log_path.display()
            )
        })?;
        if parent.record.fixed_manifest_sha256 != manifest_sha256 {
            return Err(format!(
                "parent attempt `{parent_id}` fixes manifest {}, but this run uses {manifest_sha256}; start a new root attempt for changed goalposts",
                parent.record.fixed_manifest_sha256
            ));
        }
        if parent.record.fixed_compiled_snapshot_sha256 != compiled_snapshot_sha256 {
            return Err(format!(
                "parent attempt `{parent_id}` fixes compiled snapshot {}, but this run compiled {compiled_snapshot_sha256}; start a new root attempt for changed semantics",
                parent.record.fixed_compiled_snapshot_sha256
            ));
        }
        if parent.record.candidate_input != request.candidate_input {
            return Err(format!(
                "parent attempt `{parent_id}` tracks candidate input `{}`, not `{}`",
                parent.record.candidate_input, request.candidate_input
            ));
        }
        if let Some(trust_root) = trust_root {
            verify_log_line_signature(&parent.full_line, trust_root).map_err(|issue| {
                format!("parent attempt `{parent_id}` log line does not verify: {issue}")
            })?;
        }
        let generation = parent
            .record
            .generation
            .checked_add(1)
            .ok_or("attempt generation overflowed u64")?;
        (
            generation,
            Some(parent.record_sha256.clone()),
            diff_candidate_states(&parent.record.candidate_state, &candidate_state),
        )
    } else {
        (0, None, Vec::new())
    };

    Ok(AttemptRecord {
        schema_version: ATTEMPT_LINEAGE_SCHEMA_VERSION.into(),
        attempt_id: request.attempt_id.clone(),
        generation,
        parent_attempt_id: request.parent_attempt_id.clone(),
        parent_record_sha256,
        fixed_manifest_sha256: manifest_sha256.into(),
        fixed_compiled_snapshot_sha256: compiled_snapshot_sha256.into(),
        candidate_input: request.candidate_input.clone(),
        candidate_artifact_sha256,
        candidate_state_sha256,
        candidate_state,
        changes,
    })
}

/// Re-read lineage immediately before append so a search cannot quietly
/// attach a completed run to a parent that changed while the capability ran.
pub(crate) fn revalidate_before_append(
    log_path: &Path,
    attempt: &AttemptRecord,
    trust_root: Option<&TrustRoot>,
) -> Result<(), String> {
    let attempts = read_attempts(log_path)?;
    validate_history(&attempts, trust_root)?;
    if attempts.contains_key(&attempt.attempt_id) {
        return Err(format!(
            "attempt id `{}` appeared in `{}` while this run was in progress",
            attempt.attempt_id,
            log_path.display()
        ));
    }
    match (&attempt.parent_attempt_id, &attempt.parent_record_sha256) {
        (Some(parent_id), Some(expected_sha256)) => {
            let parent = attempts.get(parent_id).ok_or_else(|| {
                format!(
                    "parent attempt `{parent_id}` disappeared from `{}` while this run was in progress",
                    log_path.display()
                )
            })?;
            if &parent.record_sha256 != expected_sha256 {
                return Err(format!(
                    "parent attempt `{parent_id}` changed while this run was in progress: expected {expected_sha256}, observed {}",
                    parent.record_sha256
                ));
            }
            if let Some(trust_root) = trust_root {
                verify_log_line_signature(&parent.full_line, trust_root).map_err(|issue| {
                    format!("parent attempt `{parent_id}` log line does not verify: {issue}")
                })?;
            }
        }
        (None, None) => {}
        _ => {
            return Err("attempt parent id and parent record identity must appear together".into());
        }
    }
    Ok(())
}

/// Return the verdict-margin surface from the exact parent line already bound
/// by `attempt`. Old run-log envelope versions remain readable because lineage
/// validation intentionally depends on fields, not the envelope's version.
pub(crate) fn parent_verdict_values(
    log_path: &Path,
    attempt: &AttemptRecord,
) -> Result<Option<Vec<Value>>, String> {
    let Some(parent_id) = &attempt.parent_attempt_id else {
        return Ok(None);
    };
    let expected_sha256 = attempt
        .parent_record_sha256
        .as_deref()
        .ok_or("a child attempt is missing its parent record identity")?;
    let attempts = read_attempts(log_path)?;
    // Read-only comparison rendering, not an admission gate: signatures are
    // not re-verified here (they already were, when this lineage was
    // admitted or last revalidated under a trust root).
    validate_history(&attempts, None)?;
    let parent = attempts.get(parent_id).ok_or_else(|| {
        format!(
            "parent attempt `{parent_id}` disappeared from `{}` before comparison",
            log_path.display()
        )
    })?;
    if parent.record_sha256 != expected_sha256 {
        return Err(format!(
            "parent attempt `{parent_id}` changed before comparison: expected {expected_sha256}, observed {}",
            parent.record_sha256
        ));
    }
    let verdicts = parent.verdicts.as_ref().ok_or_else(|| {
        format!(
            "parent attempt `{parent_id}` on line {} has no verdict collection",
            parent.line
        )
    })?;
    let verdicts = verdicts.as_array().ok_or_else(|| {
        format!(
            "parent attempt `{parent_id}` on line {} has a non-array verdict collection",
            parent.line
        )
    })?;
    Ok(Some(verdicts.clone()))
}

fn read_attempts(path: &Path) -> Result<BTreeMap<String, PriorAttempt>, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => {
            return Err(format!(
                "cannot read lineage log `{}`: {error}",
                path.display()
            ));
        }
    };
    parse_attempts(&content, path)
}

fn parse_attempts(content: &str, path: &Path) -> Result<BTreeMap<String, PriorAttempt>, String> {
    let mut attempts = BTreeMap::new();
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
        let Some(attempt_value) = entry.get("attempt") else {
            continue;
        };
        let record: AttemptRecord =
            serde_json::from_value(attempt_value.clone()).map_err(|error| {
                format!(
                    "line {line} of lineage log `{}` has an invalid attempt record: {error}",
                    path.display()
                )
            })?;
        let attempt_id = record.attempt_id.clone();
        let prior = PriorAttempt {
            record,
            record_sha256: digest(raw_line.as_bytes()),
            line,
            top_level_manifest_sha256: entry
                .get("manifest_sha256")
                .and_then(Value::as_str)
                .map(str::to_owned),
            top_level_compiled_snapshot_sha256: entry
                .get("compiled_snapshot_sha256")
                .and_then(Value::as_str)
                .map(str::to_owned),
            verdicts: entry.get("verdicts").cloned(),
            full_line: entry,
        };
        if let Some(first) = attempts.insert(attempt_id.clone(), prior) {
            return Err(format!(
                "attempt id `{attempt_id}` occurs on both lines {} and {line} of `{}`",
                first.line,
                path.display()
            ));
        }
    }
    Ok(attempts)
}

/// Query one immutable in-memory log snapshot using the runner's existing
/// lineage validator and exact comparison implementation. No signatures or
/// external artifacts are verified by this read-only view.
pub(crate) fn query_attempt(bytes: &[u8], id: &str) -> Result<Value, String> {
    let content = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    for line in content.split('\n').filter(|line| !line.trim().is_empty()) {
        canonicalize_json(line.as_bytes()).map_err(|e| e.to_string())?;
        let record: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        crate::query::validate_history_record(&record)?;
    }
    let attempts = parse_attempts(content, Path::new("query snapshot"))?;
    validate_history(&attempts, None)?;
    let Some(attempt) = attempts.get(id) else {
        return Ok(serde_json::json!({"match_status":"no_match_in_record"}));
    };
    let comparison = if let Some(parent_id) = &attempt.record.parent_attempt_id {
        let parent = &attempts[parent_id];
        let parse_margins = |prior: &PriorAttempt| -> Result<Vec<crate::VerdictMargin>, String> {
            serde_json::from_value(
                prior
                    .verdicts
                    .clone()
                    .ok_or("attempt has no recorded verdict collection")?,
            )
            .map_err(|e| format!("invalid attempt verdict collection: {e}"))
        };
        Some(crate::case_run::compare_attempt_results(
            &attempt.record,
            &parse_margins(parent)?,
            &parse_margins(attempt)?,
        )?)
    } else {
        None
    };
    Ok(
        serde_json::json!({"match_status":"found","line":attempt.line,
        "record_sha256":attempt.record_sha256,"attempt":attempt.record,
        "comparison":comparison,"lineage_validation":"consistent",
        "signature_verification":"not_checked"}),
    )
}

fn validate_history(
    attempts: &BTreeMap<String, PriorAttempt>,
    trust_root: Option<&TrustRoot>,
) -> Result<(), String> {
    for (attempt_id, attempt) in attempts {
        validate_identifier("attempt", attempt_id)?;
        let record = &attempt.record;
        if record.schema_version != ATTEMPT_LINEAGE_SCHEMA_VERSION {
            return Err(format!(
                "attempt `{attempt_id}` uses unsupported lineage schema `{}`",
                record.schema_version
            ));
        }
        validate_identifier("candidate input", &record.candidate_input)?;
        if record.fixed_manifest_sha256
            != attempt.top_level_manifest_sha256.as_deref().unwrap_or("")
        {
            return Err(format!(
                "attempt `{attempt_id}` does not match its enclosing manifest identity"
            ));
        }
        if record.fixed_compiled_snapshot_sha256
            != attempt
                .top_level_compiled_snapshot_sha256
                .as_deref()
                .unwrap_or("")
        {
            return Err(format!(
                "attempt `{attempt_id}` does not match its enclosing compiled snapshot identity"
            ));
        }
        let observed_state_sha256 = candidate_state_identity(&record.candidate_state)?;
        if observed_state_sha256 != record.candidate_state_sha256 {
            return Err(format!(
                "attempt `{attempt_id}` candidate state hashes to {observed_state_sha256}, not the recorded {}",
                record.candidate_state_sha256
            ));
        }

        match (&record.parent_attempt_id, &record.parent_record_sha256) {
            (None, None) => {
                if record.generation != 0 {
                    return Err(format!(
                        "root attempt `{attempt_id}` has generation {}, not 0",
                        record.generation
                    ));
                }
                if !record.changes.is_empty() {
                    return Err(format!(
                        "root attempt `{attempt_id}` records changes without a parent"
                    ));
                }
            }
            (Some(parent_id), Some(parent_sha256)) => {
                validate_identifier("parent attempt", parent_id)?;
                let parent = attempts.get(parent_id).ok_or_else(|| {
                    format!("attempt `{attempt_id}` names missing parent `{parent_id}`")
                })?;
                if parent.line >= attempt.line {
                    return Err(format!(
                        "attempt `{attempt_id}` on line {} must follow parent `{parent_id}` on line {}",
                        attempt.line, parent.line
                    ));
                }
                if &parent.record_sha256 != parent_sha256 {
                    return Err(format!(
                        "attempt `{attempt_id}` binds parent `{parent_id}` as {parent_sha256}, but its log record hashes to {}",
                        parent.record_sha256
                    ));
                }
                if let Some(trust_root) = trust_root {
                    verify_log_line_signature(&parent.full_line, trust_root).map_err(|issue| {
                        format!(
                            "attempt `{attempt_id}` binds parent `{parent_id}` on line {}, whose log line does not verify: {issue}",
                            parent.line
                        )
                    })?;
                }
                let expected_generation =
                    parent.record.generation.checked_add(1).ok_or_else(|| {
                        format!("parent attempt `{parent_id}` generation overflows u64")
                    })?;
                if record.generation != expected_generation {
                    return Err(format!(
                        "attempt `{attempt_id}` generation {} does not follow parent `{parent_id}` generation {}",
                        record.generation, parent.record.generation
                    ));
                }
                if record.fixed_manifest_sha256 != parent.record.fixed_manifest_sha256
                    || record.fixed_compiled_snapshot_sha256
                        != parent.record.fixed_compiled_snapshot_sha256
                {
                    return Err(format!(
                        "attempt `{attempt_id}` crosses the fixed question identity of parent `{parent_id}`"
                    ));
                }
                if record.candidate_input != parent.record.candidate_input {
                    return Err(format!(
                        "attempt `{attempt_id}` changes the candidate input tracked by parent `{parent_id}`"
                    ));
                }
                let expected_changes =
                    diff_candidate_states(&parent.record.candidate_state, &record.candidate_state);
                if record.changes != expected_changes {
                    return Err(format!(
                        "attempt `{attempt_id}` changes do not match its parent and candidate states"
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "attempt `{attempt_id}` must record both its parent id and parent record identity, or neither"
                ));
            }
        }
    }
    Ok(())
}

fn candidate_state_identity(state: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(state)
        .map_err(|error| format!("candidate state cannot be serialized: {error}"))?;
    let canonical = canonicalize_json(&bytes)
        .map_err(|error| format!("candidate state is outside the canonical profile: {error}"))?;
    Ok(digest(&canonical))
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// Verify one campaign log line's own runner signature against a trust
/// root (ADR-0015 clause 6): the canonical form of the line with its
/// `signature` member removed must reproduce the digest that member names,
/// and the signature must verify against a listed runner key.
fn verify_log_line_signature(line: &Value, trust_root: &TrustRoot) -> Result<String, String> {
    let mut without_signature = line.clone();
    let object = without_signature
        .as_object_mut()
        .ok_or("log line is not a JSON object")?;
    let signature_value = object
        .remove("signature")
        .ok_or("log line carries no signature")?;
    let document: SignatureDocument = serde_json::from_value(signature_value)
        .map_err(|error| format!("log line signature is malformed: {error}"))?;
    let bytes = serde_json::to_vec(&without_signature)
        .map_err(|error| format!("log line could not be re-serialized: {error}"))?;
    let canonical = canonicalize_json(&bytes)
        .map_err(|error| format!("log line is outside the canonical profile: {error}"))?;
    let expected_digest: [u8; 32] = Sha256::digest(&canonical).into();
    check_internal_consistency(&document, &expected_digest)
        .map_err(|error| format!("log line signature is inconsistent: {error}"))?;
    verify_signature_document(&document, trust_root, KeyRole::Runner)
        .map_err(|error| format!("log line signature does not verify: {error}"))
}

fn validate_identifier(kind: &str, value: &str) -> Result<(), String> {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return Err(format!("{kind} id must not be empty"));
    };
    if value.len() > 128 {
        return Err(format!("{kind} id `{value}` exceeds 128 bytes"));
    }
    if !first.is_ascii_alphanumeric()
        || !characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return Err(format!(
            "{kind} id `{value}` must start with an ASCII letter or digit and contain only ASCII letters, digits, `.`, `_`, or `-`"
        ));
    }
    Ok(())
}

fn diff_candidate_states(before: &Value, after: &Value) -> Vec<AttemptChange> {
    let mut changes = Vec::new();
    diff_value(before, after, "", &mut changes);
    changes
}

fn diff_value(before: &Value, after: &Value, pointer: &str, changes: &mut Vec<AttemptChange>) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            let keys: BTreeSet<&str> = before
                .keys()
                .map(String::as_str)
                .chain(after.keys().map(String::as_str))
                .collect();
            for key in keys {
                let child = child_pointer(pointer, &escape_pointer_segment(key));
                match (before.get(key), after.get(key)) {
                    (Some(before), Some(after)) => diff_value(before, after, &child, changes),
                    (None, Some(value)) => changes.push(AttemptChange::Added {
                        pointer: child,
                        value: value.clone(),
                    }),
                    (Some(value), None) => changes.push(AttemptChange::Removed {
                        pointer: child,
                        value: value.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(before), Value::Array(after)) if before.len() == after.len() => {
            for (index, (before, after)) in before.iter().zip(after).enumerate() {
                diff_value(
                    before,
                    after,
                    &child_pointer(pointer, &index.to_string()),
                    changes,
                );
            }
        }
        _ => changes.push(AttemptChange::Replaced {
            pointer: pointer.into(),
            before: before.clone(),
            after: after.clone(),
        }),
    }
}

fn child_pointer(parent: &str, segment: &str) -> String {
    format!("{parent}/{segment}")
}

fn escape_pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_recomputes_comparison_and_rejects_rewritten_parent() {
        let candidate = json!({"thickness":"1"});
        let root = AttemptRecord {
            schema_version: ATTEMPT_LINEAGE_SCHEMA_VERSION.into(),
            attempt_id: "parent".into(),
            generation: 0,
            parent_attempt_id: None,
            parent_record_sha256: None,
            fixed_manifest_sha256: digest("manifest"),
            fixed_compiled_snapshot_sha256: digest("question"),
            candidate_input: "candidate".into(),
            candidate_artifact_sha256: digest("candidate bytes"),
            candidate_state_sha256: candidate_state_identity(&candidate).unwrap(),
            candidate_state: candidate,
            changes: Vec::new(),
        };
        let margin = |value: &str| json!({"requirement_id":"r","status":"pass","rule":"test","unit":"m","margin":value});
        let row = |record: &AttemptRecord, value: &str| {
            json!({
            "schema_version":"avila.core/run-attempt/v0.3-draft","status":"evaluated","case_id":"case","steps":[],
            "manifest_sha256":record.fixed_manifest_sha256,
            "compiled_snapshot_sha256":record.fixed_compiled_snapshot_sha256,
            "attempt":record,"verdicts":[margin(value)],
            "attempt_comparison":{"untrusted":"this stored comparison is deliberately ignored"}})
        };
        let parent = row(&root, "1/3").to_string();
        let mut child = root.clone();
        child.attempt_id = "child".into();
        child.generation = 1;
        child.parent_attempt_id = Some("parent".into());
        child.parent_record_sha256 = Some(digest(parent.as_bytes()));
        child.candidate_state = json!({"thickness":"2"});
        child.candidate_state_sha256 = candidate_state_identity(&child.candidate_state).unwrap();
        child.candidate_artifact_sha256 = digest("new candidate bytes");
        child.changes = diff_candidate_states(&root.candidate_state, &child.candidate_state);
        let text = format!("{parent}\n{}\n", row(&child, "2/3"));
        let result = query_attempt(text.as_bytes(), "child").unwrap();
        assert_eq!(result["lineage_validation"], "consistent");
        assert_eq!(result["signature_verification"], "not_checked");
        assert_eq!(
            result["comparison"]["exact_margin_comparisons"][0]["delta"],
            "1/3"
        );
        let root_result = query_attempt(text.as_bytes(), "parent").unwrap();
        assert!(root_result["comparison"].is_null());
        let changed = text.replacen("1/3", "1/4", 1);
        assert!(
            query_attempt(changed.as_bytes(), "child")
                .unwrap_err()
                .contains("hashes to")
        );
        assert!(query_attempt(format!("{text}{parent}\n").as_bytes(), "child").is_err());
    }

    #[test]
    fn published_schema_names_the_runtime_lineage_version() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../schemas/attempt-lineage.v0.1-draft.schema.json"
        ))
        .unwrap();
        assert_eq!(
            schema["properties"]["schema_version"]["const"],
            ATTEMPT_LINEAGE_SCHEMA_VERSION
        );
    }

    #[test]
    fn published_schema_names_the_runtime_comparison_version() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../schemas/attempt-comparison.v0.1-draft.schema.json"
        ))
        .unwrap();
        assert_eq!(
            schema["properties"]["schema_version"]["const"],
            ATTEMPT_COMPARISON_SCHEMA_VERSION
        );
    }

    #[test]
    fn candidate_diff_is_typed_recursive_and_pointer_safe() {
        let before = json!({
            "layers": [{"material": "steel", "thickness": "4"}],
            "remove/me": true,
            "same": 1
        });
        let after = json!({
            "layers": [{"material": "steel", "thickness": "6"}],
            "add~me": [1, 2],
            "same": 1
        });
        assert_eq!(
            diff_candidate_states(&before, &after),
            vec![
                AttemptChange::Added {
                    pointer: "/add~0me".into(),
                    value: json!([1, 2]),
                },
                AttemptChange::Replaced {
                    pointer: "/layers/0/thickness".into(),
                    before: json!("4"),
                    after: json!("6"),
                },
                AttemptChange::Removed {
                    pointer: "/remove~1me".into(),
                    value: json!(true),
                },
            ]
        );
    }

    #[test]
    fn an_array_shape_change_is_one_unambiguous_replacement() {
        let before = json!({"layers": [1, 2]});
        let after = json!({"layers": [1, 2, 3]});
        assert_eq!(
            diff_candidate_states(&before, &after),
            vec![AttemptChange::Replaced {
                pointer: "/layers".into(),
                before: json!([1, 2]),
                after: json!([1, 2, 3]),
            }]
        );
    }
}
