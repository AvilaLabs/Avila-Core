//! Identity-bound lineage for candidate-search attempts.
//!
//! Core remains the oracle rather than the optimizer. A caller nominates one
//! supplied JSON input as the candidate and names an attempt plus an optional
//! parent. Core stores the canonical candidate state, derives the typed delta,
//! and binds a child to the exact parent log line and fixed question identity.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use avila_core_evidence::{sha256_file, sha256_hex};
use avila_core_kernel::canonicalize_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ATTEMPT_LINEAGE_SCHEMA_VERSION: &str = "avila.core/attempt-lineage/v0.1-draft";
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

#[derive(Debug)]
struct PriorAttempt {
    record: AttemptRecord,
    record_sha256: String,
    line: usize,
    top_level_manifest_sha256: Option<String>,
    top_level_compiled_snapshot_sha256: Option<String>,
}

pub(crate) fn prepare_attempt(
    request: &AttemptLineageRequest,
    log_path: Option<&Path>,
    candidate_path: Option<&Path>,
    supplied_candidate_sha256: Option<&str>,
    manifest_sha256: &str,
    compiled_snapshot_sha256: &str,
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
    validate_history(&attempts)?;
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
) -> Result<(), String> {
    let attempts = read_attempts(log_path)?;
    validate_history(&attempts)?;
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
        }
        (None, None) => {}
        _ => {
            return Err("attempt parent id and parent record identity must appear together".into());
        }
    }
    Ok(())
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

fn validate_history(attempts: &BTreeMap<String, PriorAttempt>) -> Result<(), String> {
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
