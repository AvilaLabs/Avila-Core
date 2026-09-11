//! Bounded, redacted tails of a failed step's stderr for run findings.
//!
//! Execution diagnostics are deliberately capped (`run_step` only ever embeds a
//! few lines): control characters are replaced, empty lines dropped, and any
//! step-supplied environment value is redacted before bytes are truncated to a
//! bounded excerpt. When truncation is required *and* sensitive values are
//! present, the excerpt is withheld entirely rather than risk leaking a partial
//! secret split by the truncation boundary.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub(super) const DIAGNOSTIC_STDERR_READ_BYTES: u64 = 16 * 1024;
pub(super) const DIAGNOSTIC_STDERR_MAX_LINES: usize = 8;
pub(super) const DIAGNOSTIC_STDERR_MAX_CHARS: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiagnosticLogExcerpt {
    pub(super) text: String,
    pub(super) truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DiagnosticStderrFeedback {
    Empty,
    Excerpt(DiagnosticLogExcerpt),
    WithheldForRedaction,
}

pub(super) fn read_diagnostic_stderr(
    path: &Path,
    sensitive_environment: &BTreeMap<String, String>,
) -> io::Result<DiagnosticStderrFeedback> {
    let mut file = fs::File::open(path)?;
    let bytes = file.metadata()?.len();
    let captured = bytes.min(DIAGNOSTIC_STDERR_READ_BYTES);
    if bytes > captured
        && sensitive_environment
            .values()
            .any(|value| !value.is_empty())
    {
        return Ok(DiagnosticStderrFeedback::WithheldForRedaction);
    }
    if bytes > captured {
        file.seek(SeekFrom::Start(bytes - captured))?;
    }
    let mut tail = Vec::with_capacity(usize::try_from(captured).unwrap_or(0));
    file.take(captured).read_to_end(&mut tail)?;
    Ok(
        match sanitize_diagnostic_stderr(&tail, bytes > captured, sensitive_environment) {
            Some(excerpt) => DiagnosticStderrFeedback::Excerpt(excerpt),
            None => DiagnosticStderrFeedback::Empty,
        },
    )
}

pub(super) fn sanitize_diagnostic_stderr(
    tail: &[u8],
    prefix_truncated: bool,
    sensitive_environment: &BTreeMap<String, String>,
) -> Option<DiagnosticLogExcerpt> {
    let decoded = String::from_utf8_lossy(tail);
    let mut sensitive_values: Vec<&str> = sensitive_environment
        .values()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    sensitive_values.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
    let redacted = redact_sensitive_values(&decoded, &sensitive_values);
    let lines: Vec<String> = redacted
        .lines()
        .map(|line| {
            line.chars()
                .map(|character| {
                    if character == '\t' || !character.is_control() {
                        character
                    } else {
                        '�'
                    }
                })
                .collect::<String>()
        })
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }

    let first_line = lines.len().saturating_sub(DIAGNOSTIC_STDERR_MAX_LINES);
    let mut text = lines[first_line..].join("\n");
    let character_count = text.chars().count();
    let character_truncated = character_count > DIAGNOSTIC_STDERR_MAX_CHARS;
    if character_truncated {
        text = text
            .chars()
            .skip(character_count - DIAGNOSTIC_STDERR_MAX_CHARS)
            .collect();
    }
    let truncated = prefix_truncated || first_line > 0 || character_truncated;
    if truncated {
        text.insert(0, '…');
    }
    Some(DiagnosticLogExcerpt { text, truncated })
}

fn redact_sensitive_values(text: &str, sensitive_values: &[&str]) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        if let Some(value) = sensitive_values
            .iter()
            .find(|value| text[offset..].starts_with(**value))
        {
            redacted.push_str("[REDACTED]");
            offset += value.len();
        } else {
            let Some(character) = text[offset..].chars().next() else {
                break;
            };
            redacted.push(character);
            offset += character.len_utf8();
        }
    }
    redacted
}
