//! Run persistence for `run --store` (ADR-0028 A4).
//!
//! Steps still execute in a scratch step directory under the workspace,
//! because programs need real files. Once a step's receipt verifies, its
//! receipt, declared inputs, declared outputs and logs are put into the
//! store as blobs and the directory is deleted, undeclared scratch included.
//! When the run ends, one tree holding exactly those files, laid out as the
//! workspace would be, plus the run-level reports, is added under the
//! store's writer lock. Nothing here changes what a receipt, a report or a
//! verdict says: the persisted paths and bytes are the ones a directory run
//! leaves.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_evidence::{BlobRef, ExecutionReceipt, OutputState, StoreFile, StoreWriter};
use serde::Serialize;

use super::CaseRunOptions;
use crate::execute::rfc3339_now;

/// Runs trade a little density for speed: blobs are written as steps finish.
pub(super) const RUN_STORE_PRESET: u32 = 6;

const MAX_TREE_NAME_CHARS: usize = 128;

/// Where a run's evidence went, reported beside (never inside) the run
/// report that the workspace holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoreRunReport {
    /// `store:<STORE_DIR>#<TREE>`, accepted wherever a workspace is read.
    pub address: String,
    pub store: String,
    pub tree: String,
    pub files: usize,
    pub distinct_blobs: usize,
    /// Blobs this run wrote; the rest were already in the store.
    pub new_blobs: usize,
    pub reused_blobs: usize,
    /// Compressed bytes this run added to the store.
    pub stored_bytes: u64,
    /// Directories left on disk: a failed step's directory, or every step
    /// directory under `--keep-scratch`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept_directories: Vec<String>,
}

#[derive(Default)]
struct PersistState {
    case_id: String,
    workspace: Option<PathBuf>,
    /// Tree path to entry, for everything put so far.
    files: BTreeMap<String, StoreFile>,
    new_blobs: BTreeSet<String>,
    seen_blobs: BTreeSet<String>,
    stored_bytes: u64,
    finished: bool,
}

pub(super) struct RunStore {
    writer: StoreWriter,
    store_dir: PathBuf,
    keep_scratch: bool,
    stamp: String,
    state: RefCell<PersistState>,
}

impl RunStore {
    /// The store a run writes to, created empty if it does not exist. A
    /// `--plan` run executes nothing and so never touches the store.
    pub(super) fn open(options: &CaseRunOptions) -> Result<Option<Self>, Box<dyn Error>> {
        let Some(path) = options.store.as_deref() else {
            return Ok(None);
        };
        if options.plan_only {
            return Ok(None);
        }
        let store_dir = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let writer = StoreWriter::open_or_create(&store_dir, RUN_STORE_PRESET)?;
        let stamp = rfc3339_now()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();
        Ok(Some(Self {
            writer,
            store_dir,
            keep_scratch: options.keep_scratch,
            stamp,
            state: RefCell::new(PersistState::default()),
        }))
    }

    /// Record the workspace the run's steps execute under.
    pub(super) fn begin(&self, case_id: &str, workspace: &Path) {
        let mut state = self.state.borrow_mut();
        state.case_id = case_id.to_string();
        state.workspace = Some(workspace.to_path_buf());
    }

    fn put(&self, source: &Path, tree_path: String) -> Result<StoreFile, Box<dyn Error>> {
        let put = self.writer.put_file(source, &tree_path)?;
        let mut state = self.state.borrow_mut();
        if put.new {
            state.new_blobs.insert(put.file.sha256.clone());
            state.stored_bytes += put.stored_bytes;
        }
        state.seen_blobs.insert(put.file.sha256.clone());
        state.files.insert(tree_path, put.file.clone());
        Ok(put.file)
    }

    /// Put a step's receipt (and signature), declared inputs, declared
    /// outputs and logs into the store. Returns the outputs' blobs by output
    /// id, which later steps read instead of the step directory.
    pub(super) fn persist_step(
        &self,
        step_id: &str,
        step_dir: &Path,
        receipt: &ExecutionReceipt,
    ) -> Result<BTreeMap<String, BlobRef>, Box<dyn Error>> {
        let mut relatives: Vec<&str> = vec!["receipt.json", "receipt.sig.json"];
        relatives.extend(receipt.inputs.iter().map(|i| i.workspace_path.as_str()));
        relatives.extend(receipt.logs.iter().map(|l| l.workspace_path.as_str()));
        let outputs: Vec<_> = receipt
            .outputs
            .iter()
            .filter(|o| matches!(o.state, OutputState::Collected | OutputState::Partial))
            .collect();
        relatives.extend(outputs.iter().map(|o| o.workspace_path.as_str()));
        let mut seen = BTreeSet::new();
        for relative in relatives {
            if !seen.insert(relative) {
                continue;
            }
            let source = step_dir.join(relative);
            // The signature is optional, and a failed step may lack files.
            if !fs::symlink_metadata(&source).is_ok_and(|m| m.is_file()) {
                continue;
            }
            self.put(&source, format!("{step_id}/{relative}"))?;
        }
        let mut blobs = BTreeMap::new();
        for output in outputs {
            let path = format!("{step_id}/{}", output.workspace_path);
            let file = self.state.borrow().files.get(&path).cloned();
            let Some(file) = file else {
                continue;
            };
            let expected = output.sha256.as_deref().unwrap_or_default();
            if expected.strip_prefix("sha256:") != Some(file.sha256.as_str()) {
                return Err(format!(
                    "output `{}` of step `{step_id}` changed after its receipt was verified",
                    output.output_id
                )
                .into());
            }
            blobs.insert(
                output.output_id.clone(),
                self.writer.blob(&file.sha256, file.bytes),
            );
        }
        Ok(blobs)
    }

    /// The verified bytes of a file already in the run's tree.
    pub(super) fn read_tree_file(&self, tree_path: &str) -> Option<Vec<u8>> {
        let file = self.state.borrow().files.get(tree_path).cloned()?;
        self.writer.blob(&file.sha256, file.bytes).read().ok()
    }

    /// Delete a persisted step's directory, undeclared scratch included.
    pub(super) fn discard_step_dir(&self, step_dir: &Path) -> Result<(), Box<dyn Error>> {
        if self.keep_scratch {
            return Ok(());
        }
        fs::remove_dir_all(step_dir)?;
        Ok(())
    }

    /// Add the run's tree and remove what the store now holds. The tree is
    /// the persisted step files plus the regular files at the workspace
    /// root, with the paths a directory run would have. Returns `None` when
    /// no workspace was ever needed (nothing executed).
    pub(super) fn finish(&self) -> Result<Option<StoreRunReport>, Box<dyn Error>> {
        let (case_id, workspace) = {
            let mut state = self.state.borrow_mut();
            if state.finished {
                return Ok(None);
            }
            state.finished = true;
            let Some(workspace) = state.workspace.clone() else {
                return Ok(None);
            };
            (state.case_id.clone(), workspace)
        };
        let mut root_files = Vec::new();
        for entry in fs::read_dir(&workspace)? {
            let entry = entry?;
            let path = entry.path();
            if !fs::symlink_metadata(&path)?.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            self.put(&path, name)?;
            root_files.push(path);
        }
        let files: Vec<StoreFile> = self.state.borrow().files.values().cloned().collect();
        let report = self
            .writer
            .add_tree(&tree_name(&case_id, &self.stamp), files)?;
        let tree = report.added_trees[0].clone();

        let mut kept = Vec::new();
        if self.keep_scratch {
            kept.push(workspace.display().to_string());
        } else {
            for path in &root_files {
                let _ = fs::remove_file(path);
            }
            if fs::remove_dir(&workspace).is_err() {
                // Failed step directories (and anything unexpected) remain.
                match fs::read_dir(&workspace) {
                    Ok(entries) => {
                        let mut left: Vec<String> = entries
                            .filter_map(Result::ok)
                            .map(|e| e.path().display().to_string())
                            .collect();
                        left.sort();
                        kept.extend(left);
                    }
                    Err(_) => kept.push(workspace.display().to_string()),
                }
            }
        }
        let state = self.state.borrow();
        Ok(Some(StoreRunReport {
            address: format!("store:{}#{tree}", self.store_dir.display()),
            store: self.store_dir.display().to_string(),
            tree,
            files: report.files,
            distinct_blobs: report.distinct_blobs,
            new_blobs: state.new_blobs.len(),
            reused_blobs: state.seen_blobs.len() - state.new_blobs.len(),
            stored_bytes: state.stored_bytes,
            kept_directories: kept,
        }))
    }
}

/// `<case_id>.<run-stamp>-<pid>`, reduced to the characters a tree name may
/// hold and to at most 128 of them; the stamp and pid are never cut.
fn tree_name(case_id: &str, stamp: &str) -> String {
    let suffix = format!(".{stamp}-{}", std::process::id());
    let clean: String = case_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(MAX_TREE_NAME_CHARS.saturating_sub(suffix.len()))
        .collect();
    format!("{clean}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_names_are_reduced_and_bounded() {
        let name = tree_name("case/one two", "20261008T010203Z");
        assert!(name.starts_with("case_one_two.20261008T010203Z-"), "{name}");
        let long = tree_name(&"x".repeat(300), "20261008T010203Z");
        assert_eq!(long.chars().count(), MAX_TREE_NAME_CHARS);
        assert!(long.contains(".20261008T010203Z-"));
        assert!(
            long.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        );
    }
}
