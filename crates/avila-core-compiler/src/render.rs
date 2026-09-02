//! Human-readable rendering of findings, in the shape a compiler user
//! expects: class and code, message, `document:line:column` with the JSON
//! Pointer, the source line with the value underlined, owner, related
//! locations, repair alternatives, and the catalog's next action.
//!
//! Rendering is presentation only. Nothing here changes a finding, and
//! consumers keep matching codes, classes, owners, locations, and repair
//! applicability, never this text.

use std::fmt::Write as _;

use crate::campaign::CampaignReport;
use crate::catalog::explain;
use crate::compile::{CompilationStatus, CompileReport};
use crate::diagnostic::{CoreDiagnostic, FindingClass, RepairApplicability, SourceLocation};

/// Where a JSON Pointer lands in a source document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    /// 1-based line of the located value.
    pub line: usize,
    /// 1-based column, in characters, of the located value.
    pub column: usize,
    /// Byte offset of the located value.
    pub start: usize,
    /// Byte offset one past the located value.
    pub end: usize,
    /// The pointer the span was resolved for. Equals the requested pointer
    /// when it exists in the document; otherwise the nearest existing
    /// ancestor, which is where a missing member would be added.
    pub resolved_pointer: String,
}

impl SourceSpan {
    #[must_use]
    pub fn is_exact(&self, pointer: &str) -> bool {
        self.resolved_pointer == pointer
    }
}

/// Locate a JSON Pointer in source bytes. Returns `None` only when the bytes
/// are not a JSON document the locator can walk; a pointer that does not
/// exist resolves to its nearest existing ancestor.
#[must_use]
pub fn locate(source: &[u8], pointer: &str) -> Option<SourceSpan> {
    let text = std::str::from_utf8(source).ok()?;
    let tokens: Vec<String> = if pointer.is_empty() {
        Vec::new()
    } else {
        pointer
            .strip_prefix('/')?
            .split('/')
            .map(|token| token.replace("~1", "/").replace("~0", "~"))
            .collect()
    };
    let mut scanner = Scanner {
        text,
        bytes: text.as_bytes(),
        position: 0,
    };
    scanner.skip_whitespace();
    let mut best: Option<(usize, usize, usize)> = None;
    scanner.walk(&tokens, 0, &mut best)?;
    scanner.skip_whitespace();
    if scanner.position != scanner.bytes.len() {
        return None;
    }
    let (start, end, depth) = best?;
    let resolved_pointer = tokens[..depth]
        .iter()
        .map(|token| format!("/{}", token.replace('~', "~0").replace('/', "~1")))
        .collect::<String>();
    let (line, column) = line_and_column(text, start);
    Some(SourceSpan {
        line,
        column,
        start,
        end,
        resolved_pointer,
    })
}

fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = text[line_start..offset].chars().count() + 1;
    (line, column)
}

struct Scanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
    position: usize,
}

impl Scanner<'_> {
    fn skip_whitespace(&mut self) {
        while self.position < self.bytes.len()
            && matches!(self.bytes[self.position], b' ' | b'\t' | b'\n' | b'\r')
        {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn expect(&mut self, byte: u8) -> Option<()> {
        if self.peek() == Some(byte) {
            self.position += 1;
            Some(())
        } else {
            None
        }
    }

    /// Walk the value at the current position. `depth` tokens of the
    /// pointer have been matched so far; `best` keeps the deepest matched
    /// value's span and depth.
    fn walk(
        &mut self,
        tokens: &[String],
        depth: usize,
        best: &mut Option<(usize, usize, usize)>,
    ) -> Option<()> {
        self.skip_whitespace();
        let start = self.position;
        match self.peek()? {
            b'{' => {
                self.position += 1;
                self.skip_whitespace();
                if self.peek() == Some(b'}') {
                    self.position += 1;
                } else {
                    loop {
                        self.skip_whitespace();
                        let key = self.string()?;
                        self.skip_whitespace();
                        self.expect(b':')?;
                        let wanted = depth < tokens.len() && tokens[depth] == key;
                        if wanted {
                            self.walk(tokens, depth + 1, best)?;
                        } else {
                            self.skip_value()?;
                        }
                        self.skip_whitespace();
                        match self.peek()? {
                            b',' => self.position += 1,
                            b'}' => {
                                self.position += 1;
                                break;
                            }
                            _ => return None,
                        }
                    }
                }
            }
            b'[' => {
                self.position += 1;
                self.skip_whitespace();
                if self.peek() == Some(b']') {
                    self.position += 1;
                } else {
                    let mut index = 0usize;
                    loop {
                        let wanted = depth < tokens.len() && tokens[depth] == index.to_string();
                        if wanted {
                            self.walk(tokens, depth + 1, best)?;
                        } else {
                            self.skip_value()?;
                        }
                        self.skip_whitespace();
                        match self.peek()? {
                            b',' => {
                                self.position += 1;
                                index += 1;
                            }
                            b']' => {
                                self.position += 1;
                                break;
                            }
                            _ => return None,
                        }
                    }
                }
            }
            _ => self.skip_value()?,
        }
        let end = self.position;
        if best.is_none_or(|(_, _, best_depth)| depth >= best_depth) {
            *best = Some((start, end, depth));
        }
        Some(())
    }

    fn skip_value(&mut self) -> Option<()> {
        self.skip_whitespace();
        match self.peek()? {
            b'{' => {
                self.position += 1;
                self.skip_whitespace();
                if self.peek() == Some(b'}') {
                    self.position += 1;
                    return Some(());
                }
                loop {
                    self.skip_whitespace();
                    self.string()?;
                    self.skip_whitespace();
                    self.expect(b':')?;
                    self.skip_value()?;
                    self.skip_whitespace();
                    match self.peek()? {
                        b',' => self.position += 1,
                        b'}' => {
                            self.position += 1;
                            return Some(());
                        }
                        _ => return None,
                    }
                }
            }
            b'[' => {
                self.position += 1;
                self.skip_whitespace();
                if self.peek() == Some(b']') {
                    self.position += 1;
                    return Some(());
                }
                loop {
                    self.skip_value()?;
                    self.skip_whitespace();
                    match self.peek()? {
                        b',' => self.position += 1,
                        b']' => {
                            self.position += 1;
                            return Some(());
                        }
                        _ => return None,
                    }
                }
            }
            b'"' => self.string().map(|_| ()),
            _ => {
                let start = self.position;
                while self.position < self.bytes.len()
                    && !matches!(
                        self.bytes[self.position],
                        b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'
                    )
                {
                    self.position += 1;
                }
                let token = &self.text[start..self.position];
                let is_literal = matches!(token, "true" | "false" | "null");
                let is_number = !token.is_empty()
                    && token.bytes().all(|byte| {
                        byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E')
                    });
                (is_literal || is_number).then_some(())
            }
        }
    }

    /// Read a JSON string at the current position and return its decoded
    /// value.
    fn string(&mut self) -> Option<String> {
        self.expect(b'"')?;
        let mut decoded = String::new();
        loop {
            let rest = &self.text[self.position..];
            let mut characters = rest.char_indices();
            let (_, character) = characters.next()?;
            self.position += character.len_utf8();
            match character {
                '"' => return Some(decoded),
                '\\' => {
                    let (_, escaped) = characters.next()?;
                    self.position += escaped.len_utf8();
                    match escaped {
                        '"' => decoded.push('"'),
                        '\\' => decoded.push('\\'),
                        '/' => decoded.push('/'),
                        'b' => decoded.push('\u{8}'),
                        'f' => decoded.push('\u{c}'),
                        'n' => decoded.push('\n'),
                        'r' => decoded.push('\r'),
                        't' => decoded.push('\t'),
                        'u' => {
                            let hex = self.text.get(self.position..self.position + 4)?;
                            let code = u32::from_str_radix(hex, 16).ok()?;
                            self.position += 4;
                            decoded.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return None,
                    }
                }
                other => decoded.push(other),
            }
        }
    }
}

/// Render findings against their source documents. A document whose bytes
/// are not supplied is rendered by pointer alone.
#[must_use]
pub fn render_findings(findings: &[CoreDiagnostic], sources: &[(&str, &[u8])]) -> String {
    let mut out = String::new();
    for finding in findings {
        render_finding(&mut out, finding, sources);
    }
    out
}

/// Render a compile report: every finding, then one summary line.
#[must_use]
pub fn render_compile_report(report: &CompileReport, sources: &[(&str, &[u8])]) -> String {
    let mut out = render_findings(&report.findings, sources);
    let (blocking, notices) = counts(&report.findings);
    match (&report.status, &report.compiled) {
        (CompilationStatus::Compiled, Some(compiled)) => {
            let _ = writeln!(
                out,
                "compiled: {} revision {} → snapshot {}{}",
                compiled.contract_id,
                compiled.contract_revision,
                compiled.snapshot_sha256,
                plural(notices, " notice")
            );
        }
        _ => {
            let _ = writeln!(
                out,
                "rejected: {} blocking finding{}{}",
                blocking,
                if blocking == 1 { "" } else { "s" },
                plural(notices, " notice")
            );
        }
    }
    out
}

/// Render a campaign report's findings and its outcome line.
#[must_use]
pub fn render_campaign_report(report: &CampaignReport, sources: &[(&str, &[u8])]) -> String {
    let mut out = render_findings(&report.findings, sources);
    match report.status {
        crate::campaign::CampaignStatus::Evaluated => {
            for verdict in &report.verdicts {
                let _ = writeln!(
                    out,
                    "{}: {:?} — {}",
                    verdict.requirement_id, verdict.verdict.status, verdict.verdict.rule
                );
            }
            if let Some(identity) = &report.campaign_sha256 {
                let _ = writeln!(out, "evaluated: campaign {identity}");
            }
        }
        crate::campaign::CampaignStatus::Rejected => {
            let (blocking, _) = counts(&report.findings);
            let _ = writeln!(
                out,
                "rejected: {} finding{}",
                blocking,
                if blocking == 1 { "" } else { "s" }
            );
        }
    }
    out
}

fn counts(findings: &[CoreDiagnostic]) -> (usize, usize) {
    let notices = findings
        .iter()
        .filter(|finding| finding.class == FindingClass::Notice)
        .count();
    (findings.len() - notices, notices)
}

fn plural(count: usize, noun: &str) -> String {
    match count {
        0 => String::new(),
        1 => format!("; 1{noun}"),
        _ => format!("; {count}{noun}s"),
    }
}

fn render_finding(out: &mut String, finding: &CoreDiagnostic, sources: &[(&str, &[u8])]) {
    let class = match finding.class {
        FindingClass::Missing => "missing",
        FindingClass::Invalid => "invalid",
        FindingClass::Unsatisfied => "unsatisfied",
        FindingClass::Inadmissible => "inadmissible",
        FindingClass::Notice => "notice",
    };
    let _ = writeln!(out, "{class}[{}]: {}", finding.code, finding.message);
    render_location(out, &finding.primary, sources, true);
    let _ = writeln!(out, "   = owner: {}", finding.owner);
    for related in &finding.related {
        render_location(out, related, sources, false);
    }
    for repair in &finding.repairs {
        let applicability = match repair.applicability {
            RepairApplicability::MechanicallySafe => "mechanically safe",
            RepairApplicability::ConstrainedChoice => "constrained choice",
            RepairApplicability::MethodOwnerJudgment => "method owner judgment",
        };
        if repair.candidates.is_empty() {
            let _ = writeln!(out, "   = repair ({applicability})");
        } else {
            let _ = writeln!(
                out,
                "   = repair ({applicability}): {}",
                repair
                    .candidates
                    .iter()
                    .map(|candidate| format!("`{candidate}`"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
    }
    if let Some(entry) = explain(&finding.code) {
        let _ = writeln!(out, "   = next: {}", entry.next_action);
    }
    out.push('\n');
}

fn render_location(
    out: &mut String,
    location: &SourceLocation,
    sources: &[(&str, &[u8])],
    primary: bool,
) {
    let arrow = if primary { "  -->" } else { "   = related:" };
    let source = sources
        .iter()
        .find(|(document, _)| *document == location.document)
        .map(|(_, bytes)| *bytes);
    let span = source.and_then(|bytes| locate(bytes, &location.pointer));
    match (source, span) {
        (Some(bytes), Some(span)) => {
            let exact = span.is_exact(&location.pointer);
            let _ = writeln!(
                out,
                "{arrow} {}:{}:{}  {}{}",
                location.document,
                span.line,
                span.column,
                location.pointer,
                if exact {
                    String::new()
                } else {
                    format!("  (not present; nearest is {})", span.resolved_pointer)
                }
            );
            if primary {
                render_snippet(out, bytes, &span);
            }
        }
        _ => {
            let _ = writeln!(out, "{arrow} {}  {}", location.document, location.pointer);
        }
    }
}

fn render_snippet(out: &mut String, bytes: &[u8], span: &SourceSpan) {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    let line_start = text[..span.start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[span.start..]
        .find('\n')
        .map_or(text.len(), |index| span.start + index);
    let line_text = &text[line_start..line_end];
    let underline_start = text[line_start..span.start].chars().count();
    let underline_end = text[line_start..span.end.min(line_end)].chars().count();
    let width = span.line.to_string().len();
    let _ = writeln!(out, "{:width$} |", "", width = width);
    let _ = writeln!(out, "{:width$} | {}", span.line, line_text, width = width);
    let _ = writeln!(
        out,
        "{:width$} | {}{}{}",
        "",
        " ".repeat(underline_start),
        "^".repeat((underline_end - underline_start).max(1)),
        if span.end > line_end { " ..." } else { "" },
        width = width
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_documents;

    const DOCUMENT: &str = r#"{
  "schema_version": "x",
  "workflow": [
    { "step_id": "a", "parameters": { "histories": "not_defined" } },
    { "step_id": "b", "bindings": [] }
  ],
  "we~ird/key": [1, 2, 3]
}"#;

    #[test]
    fn pointers_resolve_to_lines_and_columns() {
        let span = locate(DOCUMENT.as_bytes(), "/workflow/0/parameters/histories").unwrap();
        assert_eq!((span.line, span.column), (4, 52));
        assert!(span.is_exact("/workflow/0/parameters/histories"));
        assert_eq!(&DOCUMENT[span.start..span.end], "\"not_defined\"");

        let span = locate(DOCUMENT.as_bytes(), "/workflow/1").unwrap();
        assert_eq!((span.line, span.column), (5, 5));

        let span = locate(DOCUMENT.as_bytes(), "/we~0ird~1key/2").unwrap();
        assert_eq!(&DOCUMENT[span.start..span.end], "3");

        let span = locate(DOCUMENT.as_bytes(), "/workflow/0/parameters/absent").unwrap();
        assert!(!span.is_exact("/workflow/0/parameters/absent"));
        assert_eq!(span.resolved_pointer, "/workflow/0/parameters");

        let root = locate(DOCUMENT.as_bytes(), "").unwrap();
        assert_eq!((root.line, root.column), (1, 1));
        assert!(locate(b"not json", "/a").is_none());
    }

    #[test]
    fn specimen_findings_render_with_locations() {
        let contract = include_bytes!("../../../examples/contracts/shutdown-dose-specimen.json");
        let registry =
            include_bytes!("../../../examples/registry/shutdown-dose-specimen.registry.json");
        let report = compile_documents(contract, registry).unwrap();
        let text =
            render_compile_report(&report, &[("contract", contract), ("registry", registry)]);
        assert!(text.starts_with("missing[CORE-S1301]"), "{text}");
        assert!(text.contains("--> contract:"), "{text}");
        assert!(text.contains("= owner: requester"), "{text}");
        assert!(text.contains("= next:"), "{text}");
        assert!(text.contains("^"), "{text}");
        assert!(text.trim_end().ends_with("blocking findings"), "{text}");
        for finding in &report.findings {
            let span = locate(contract, &finding.primary.pointer).unwrap();
            assert!(span.line > 1, "{}", finding.primary.pointer);
        }
    }
}
