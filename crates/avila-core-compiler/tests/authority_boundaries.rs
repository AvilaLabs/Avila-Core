//! ADR-0006 `authority.frontend-cannot-construct-verdict.build`: verdict
//! and admission construction is a compiler/kernel boundary — no runner,
//! evidence, CLI, or frontend code builds a `VerdictOutput` or
//! `AdmissionRecord` literal. This is the repository-level check the
//! acceptance plan describes as a CI grep, run here as a test so it
//! cannot silently drift.

use std::fs;
use std::path::{Path, PathBuf};

/// Directories where verdict/admission construction is the authoritative
/// boundary. Struct definitions and impl blocks are matched by the grep
/// too, so the check looks at the line's context, not just the pattern.
const ALLOWED_PREFIXES: &[&str] = &["avila-core-kernel", "avila-core-compiler"];

const PINNED: &[&str] = &["VerdictOutput {", "AdmissionRecord {"];

fn rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().unwrap() != "target" {
                rust_files(&path, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn verdict_and_admission_construction_stays_inside_the_boundary() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let crates = workspace.join("crates");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(!files.is_empty());

    let mut violations = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(&crates)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let crate_dir = relative.split('/').next().unwrap_or_default();
        if ALLOWED_PREFIXES.contains(&crate_dir) {
            continue;
        }
        let source = fs::read_to_string(&file).unwrap();
        for (number, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            for pattern in PINNED {
                if line.contains(pattern)
                    && !trimmed.starts_with("pub struct")
                    && !trimmed.starts_with("struct ")
                    && !trimmed.starts_with("impl ")
                    && !trimmed.starts_with("//")
                {
                    violations.push(format!("{relative}:{}: {}", number + 1, trimmed));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "verdict/admission construction outside the kernel/compiler boundary:\n{}",
        violations.join("\n")
    );
}
