//! Content-addressed evidence store (ADR-0028).
//!
//! A store holds any number of named file trees and keeps each distinct file
//! content once, as one xz stream under `blobs/<h0h1>/<h>.xz`. A file's
//! identity is the SHA-256 of its exact uncompressed bytes, the digest
//! receipts and packages already record; this module only adds a format and
//! the operations on it. It does not change receipts, invocation identities,
//! manifests, or signatures, and a store is not a security boundary: anyone
//! can write one, and integrity comes from checking digests against the
//! records that name them.
//!
//! Nothing read from a store is believed. The index is validated against the
//! format rules before use, every blob is decompressed with its output
//! bounded to the recorded length plus one byte, and content is accepted only
//! when its length and digest equal the index entry. [`StoreFileReader`]
//! checks both at end of stream; bytes it has returned are provisional until a
//! read returns end-of-file without error.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use xz2::stream::{Action, Status, Stream};
use xz2::write::XzEncoder;

pub const EVIDENCE_STORE_SCHEMA_VERSION: &str = "avila.core/evidence-store/v0.1";
pub const EVIDENCE_STORE_CODEC: &str = "xz";

/// The default writer preset (`store pack`, `store add`). The preset is a
/// writer parameter; readers accept any valid xz stream.
pub const DEFAULT_XZ_PRESET: u32 = 9;
/// Decoder memory ceiling. Preset 9 needs about 65 MiB; the limit only stops a
/// hostile stream from declaring an enormous dictionary.
const DECODER_MEMORY_LIMIT: u64 = 1 << 30;

const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TREES: usize = 1_024;
const MAX_FILES_PER_TREE: usize = 65_536;
const MAX_FILES_TOTAL: usize = 262_144;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_TREE_NAME_CHARS: usize = 128;
const MAX_PATH_COMPONENTS: usize = 64;
/// A verification report lists at most this many findings; the count is exact.
const MAX_REPORTED_FINDINGS: usize = 1_000;

const INDEX_FILE: &str = "store.json";
const BLOBS_DIR: &str = "blobs";
/// Writer state: an exclusive lock file and a staging directory. Readers
/// ignore both.
const LOCK_FILE: &str = "store.lock";
const TMP_DIR: &str = "tmp";
const COPY_BUFFER: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("store I/O error at `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("invalid store index: {0}")]
    InvalidIndex(String),
    #[error("cannot pack: {0}")]
    InvalidInput(String),
    #[error("`{0}` already exists; a store operation only writes to a new location")]
    AlreadyExists(String),
    #[error("store has no tree `{0}`")]
    UnknownTree(String),
    #[error("tree `{tree}` has no file `{path}`")]
    UnknownFile { tree: String, path: String },
    #[error("blob {sha256} is not valid: {detail}")]
    Blob { sha256: String, detail: String },
    #[error("store layout is not valid: {0}")]
    Layout(String),
    #[error(
        "store `{store}` is locked by `{lock}` ({holder}); if you have confirmed that no writer is running on that host, delete that file and retry"
    )]
    Locked {
        store: String,
        lock: String,
        holder: String,
    },
    #[error("store already has a tree `{0}`; adding never changes an existing tree")]
    TreeExists(String),
    #[error("xz preset {0} is not valid; use 0 to 9")]
    InvalidPreset(u32),
}

fn io_error(path: &Path, source: io::Error) -> StoreError {
    StoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// One file in a tree: where it lives and the identity of its content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreTree {
    pub name: String,
    pub files: Vec<StoreFile>,
}

/// The parsed `store.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreIndex {
    pub schema_version: String,
    pub codec: String,
    pub trees: Vec<StoreTree>,
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_tree_name(name: &str) -> Result<(), String> {
    let count = name.chars().count();
    if count == 0 || count > MAX_TREE_NAME_CHARS {
        return Err(format!(
            "tree name `{name}` must be 1 to {MAX_TREE_NAME_CHARS} characters"
        ));
    }
    if name == "." || name == ".." {
        return Err(format!("tree name `{name}` is not allowed"));
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "tree name `{name}` may contain only A-Z a-z 0-9 . _ -"
        ));
    }
    Ok(())
}

/// Rule 2 for one path, before any filesystem is consulted.
fn validate_store_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path is empty".into());
    }
    if path.starts_with('/') {
        return Err(format!("path `{path}` is absolute"));
    }
    if let Some(bad) = path.chars().find(|c| *c == '\\' || c.is_control()) {
        return Err(format!(
            "path `{}` contains a backslash or control character (U+{:04X})",
            path.escape_debug(),
            u32::from(bad)
        ));
    }
    let components: Vec<&str> = path.split('/').collect();
    if components.len() > MAX_PATH_COMPONENTS {
        return Err(format!(
            "path `{path}` has more than {MAX_PATH_COMPONENTS} components"
        ));
    }
    for component in components {
        if component.is_empty() || component == "." || component == ".." {
            return Err(format!("path `{path}` has an empty, `.` or `..` component"));
        }
    }
    Ok(())
}

/// Rules 1 to 3 and 6. The caller has already checked the schema and codec.
fn validate_index(index: &StoreIndex) -> Result<(), StoreError> {
    let invalid = |message: String| Err(StoreError::InvalidIndex(message));
    if index.schema_version != EVIDENCE_STORE_SCHEMA_VERSION {
        return invalid(format!(
            "schema_version must be `{EVIDENCE_STORE_SCHEMA_VERSION}`, found `{}`",
            index.schema_version.escape_debug()
        ));
    }
    if index.codec != EVIDENCE_STORE_CODEC {
        return invalid(format!(
            "codec must be `{EVIDENCE_STORE_CODEC}`, found `{}`",
            index.codec.escape_debug()
        ));
    }
    if index.trees.len() > MAX_TREES {
        return invalid(format!("more than {MAX_TREES} trees"));
    }
    let mut total_files = 0_usize;
    let mut lengths: BTreeMap<&str, u64> = BTreeMap::new();
    let mut previous_name: Option<&str> = None;
    for tree in &index.trees {
        if let Err(message) = validate_tree_name(&tree.name) {
            return invalid(message);
        }
        if let Some(previous) = previous_name {
            if previous == tree.name {
                return invalid(format!("duplicate tree name `{}`", tree.name));
            }
            if previous > tree.name.as_str() {
                return invalid(format!(
                    "trees are not sorted by name (`{}` after `{previous}`)",
                    tree.name
                ));
            }
        }
        previous_name = Some(&tree.name);
        if tree.files.len() > MAX_FILES_PER_TREE {
            return invalid(format!(
                "tree `{}` has more than {MAX_FILES_PER_TREE} files",
                tree.name
            ));
        }
        total_files += tree.files.len();
        if total_files > MAX_FILES_TOTAL {
            return invalid(format!("more than {MAX_FILES_TOTAL} files in total"));
        }
        let mut paths: BTreeSet<&str> = BTreeSet::new();
        let mut previous_path: Option<&str> = None;
        for file in &tree.files {
            if let Err(message) = validate_store_path(&file.path) {
                return invalid(format!("tree `{}`: {message}", tree.name));
            }
            if let Some(previous) = previous_path {
                if previous == file.path {
                    return invalid(format!(
                        "tree `{}` lists `{}` more than once",
                        tree.name, file.path
                    ));
                }
                if previous.as_bytes() > file.path.as_bytes() {
                    return invalid(format!(
                        "tree `{}` files are not sorted by path (`{}` after `{previous}`)",
                        tree.name, file.path
                    ));
                }
            }
            previous_path = Some(&file.path);
            paths.insert(&file.path);
            if !is_lower_hex_sha256(&file.sha256) {
                return invalid(format!(
                    "tree `{}` file `{}`: sha256 must be 64 lowercase hexadecimal characters",
                    tree.name, file.path
                ));
            }
            if file.bytes > MAX_FILE_BYTES {
                return invalid(format!(
                    "tree `{}` file `{}`: {} bytes exceeds the {MAX_FILE_BYTES}-byte limit",
                    tree.name, file.path, file.bytes
                ));
            }
            match lengths.get(file.sha256.as_str()) {
                Some(known) if *known != file.bytes => {
                    return invalid(format!(
                        "digest {} is recorded with two different lengths ({known} and {})",
                        file.sha256, file.bytes
                    ));
                }
                Some(_) => {}
                None => {
                    lengths.insert(&file.sha256, file.bytes);
                }
            }
        }
        // A file may not also be a directory of the same tree. Sorted order
        // does not make such pairs adjacent ("a", "a-x", "a/b"), so look up
        // every proper ancestor.
        for path in &paths {
            let mut end = 0;
            for component in path.split('/') {
                end += component.len();
                if end >= path.len() {
                    break;
                }
                if paths.contains(&path[..end]) {
                    return invalid(format!(
                        "tree `{}`: `{}` is a file and also a directory prefix of `{path}`",
                        tree.name,
                        &path[..end]
                    ));
                }
                end += 1;
            }
        }
    }
    Ok(())
}

fn blob_relative(sha256: &str) -> PathBuf {
    Path::new(BLOBS_DIR)
        .join(&sha256[..2])
        .join(format!("{sha256}.xz"))
}

fn serialize_index(index: &StoreIndex) -> Result<Vec<u8>, StoreError> {
    let mut bytes = serde_json::to_vec_pretty(index)
        .map_err(|error| StoreError::InvalidIndex(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// The blob's path under `root`, after checking that `blobs/` and its
/// fan-out directory are real directories, so a symlinked directory cannot
/// lead a read outside the store.
fn blob_path_in(root: &Path, sha256: &str) -> Result<PathBuf, StoreError> {
    let blobs = root.join(BLOBS_DIR);
    let fanout = blobs.join(&sha256[..2]);
    for directory in [&blobs, &fanout] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(StoreError::Blob {
                    sha256: sha256.into(),
                    detail: format!("`{}` is not a directory", directory.display()),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(StoreError::Blob {
                    sha256: sha256.into(),
                    detail: "the blob file is missing".into(),
                });
            }
            Err(error) => return Err(io_error(directory, error)),
        }
    }
    Ok(root.join(blob_relative(sha256)))
}

// ---------------------------------------------------------------------------
// Reading one blob
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReaderState {
    Reading,
    StreamEnded,
    Finished,
    Failed,
}

/// A streaming, verifying reader over one blob.
///
/// Output is bounded to the recorded length plus one byte, so a stream that
/// decompresses longer than recorded fails instead of filling memory or disk.
/// The length and the SHA-256 are checked when the stream ends: a caller that
/// hashes or copies the content must treat it as provisional until `read`
/// has returned `Ok(0)`, and discard it on any error.
pub struct StoreFileReader {
    file: File,
    label: String,
    stream: Stream,
    input: Vec<u8>,
    pos: usize,
    len: usize,
    eof: bool,
    hasher: Sha256,
    produced: u64,
    expected_bytes: u64,
    expected_sha256: String,
    state: ReaderState,
}

fn bad_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

impl StoreFileReader {
    fn open(path: &Path, sha256: &str, bytes: u64) -> Result<Self, StoreError> {
        let blob_error = |detail: &str| StoreError::Blob {
            sha256: sha256.into(),
            detail: detail.into(),
        };
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                blob_error("the blob file is missing")
            } else {
                io_error(path, error)
            }
        })?;
        if !metadata.is_file() {
            return Err(blob_error("the blob is not a regular file"));
        }
        let file = File::open(path).map_err(|error| io_error(path, error))?;
        let stream = Stream::new_stream_decoder(DECODER_MEMORY_LIMIT, 0)
            .map_err(|error| blob_error(&format!("cannot start the xz decoder: {error}")))?;
        Ok(Self {
            file,
            label: sha256.into(),
            stream,
            input: vec![0; COPY_BUFFER],
            pos: 0,
            len: 0,
            eof: false,
            hasher: Sha256::new(),
            produced: 0,
            expected_bytes: bytes,
            expected_sha256: sha256.into(),
            state: ReaderState::Reading,
        })
    }

    fn finish(&mut self) -> io::Result<()> {
        if self.pos < self.len {
            return Err(bad_data("data follows the end of the xz stream"));
        }
        if !self.eof {
            let mut probe = [0_u8; 1];
            if self.file.read(&mut probe)? != 0 {
                return Err(bad_data("data follows the end of the xz stream"));
            }
        }
        if self.produced != self.expected_bytes {
            return Err(bad_data(format!(
                "decompressed to {} bytes, index records {}",
                self.produced, self.expected_bytes
            )));
        }
        let actual = format!("{:x}", self.hasher.clone().finalize());
        if actual != self.expected_sha256 {
            return Err(bad_data(format!("decompressed content hashes to {actual}")));
        }
        Ok(())
    }

    fn read_inner(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.state == ReaderState::StreamEnded {
            self.finish()?;
            self.state = ReaderState::Finished;
            return Ok(0);
        }
        loop {
            if self.pos == self.len && !self.eof {
                self.len = self.file.read(&mut self.input)?;
                self.pos = 0;
                if self.len == 0 {
                    self.eof = true;
                }
            }
            // At most one byte past the recorded length is ever produced.
            let allowance =
                (self.expected_bytes + 1 - self.produced).min(buf.len() as u64) as usize;
            let before_in = self.stream.total_in();
            let before_out = self.stream.total_out();
            let status = self
                .stream
                .process(
                    &self.input[self.pos..self.len],
                    &mut buf[..allowance],
                    Action::Run,
                )
                .map_err(|error| bad_data(format!("xz decoding failed: {error}")))?;
            let consumed = (self.stream.total_in() - before_in) as usize;
            let out = (self.stream.total_out() - before_out) as usize;
            self.pos += consumed;
            if out > 0 {
                self.hasher.update(&buf[..out]);
                self.produced += out as u64;
                if self.produced > self.expected_bytes {
                    return Err(bad_data(format!(
                        "decompresses to more than the {} bytes the index records",
                        self.expected_bytes
                    )));
                }
            }
            match status {
                Status::StreamEnd => {
                    if out > 0 {
                        self.state = ReaderState::StreamEnded;
                        return Ok(out);
                    }
                    self.finish()?;
                    self.state = ReaderState::Finished;
                    return Ok(0);
                }
                Status::MemNeeded => return Err(bad_data("xz stream exceeds the memory limit")),
                Status::Ok | Status::GetCheck => {}
            }
            if out > 0 {
                return Ok(out);
            }
            if consumed == 0 {
                if self.eof {
                    return Err(bad_data("the xz stream is truncated"));
                }
                if self.pos < self.len {
                    return Err(bad_data("the xz decoder made no progress"));
                }
            }
        }
    }
}

impl Read for StoreFileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.state {
            ReaderState::Finished => return Ok(0),
            ReaderState::Failed => {
                return Err(bad_data(format!(
                    "blob {} already failed verification",
                    self.label
                )));
            }
            _ => {}
        }
        let result = self.read_inner(buf);
        if result.is_err() {
            self.state = ReaderState::Failed;
        }
        result
    }
}

/// Drain a reader to its end, discarding the bytes: a pure verification.
fn drain(reader: &mut impl Read) -> io::Result<u64> {
    let mut buffer = vec![0_u8; COPY_BUFFER];
    let mut total = 0_u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(total);
        }
        total += count as u64;
    }
}

// ---------------------------------------------------------------------------
// The opened store
// ---------------------------------------------------------------------------

/// A store whose index has been read and validated. Opening does not touch
/// any blob.
#[derive(Debug)]
pub struct EvidenceStore {
    root: PathBuf,
    index: StoreIndex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoreUnpackReport {
    pub out: String,
    pub trees: usize,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreVerifyStatus {
    Verified,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoreFinding {
    pub kind: &'static str,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoreVerifyReport {
    pub store: String,
    pub status: StoreVerifyStatus,
    pub trees: usize,
    pub files: usize,
    pub distinct_blobs: usize,
    pub uncompressed_bytes: u64,
    pub stored_bytes: u64,
    pub finding_count: usize,
    pub findings: Vec<StoreFinding>,
    /// Writer state that readers ignore (`store.lock`, `tmp/`), reported as
    /// information; it never fails verification.
    pub writer_state: Vec<String>,
}

impl EvidenceStore {
    /// Read and validate `store.json`. A store whose index breaks any of
    /// rules 1 to 3 or 6 does not open.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        let root_metadata = fs::metadata(root).map_err(|error| io_error(root, error))?;
        if !root_metadata.is_dir() {
            return Err(StoreError::Layout(format!(
                "`{}` is not a directory",
                root.display()
            )));
        }
        let index_path = root.join(INDEX_FILE);
        let metadata =
            fs::symlink_metadata(&index_path).map_err(|error| io_error(&index_path, error))?;
        if !metadata.is_file() {
            return Err(StoreError::Layout(format!(
                "`{}` is not a regular file",
                index_path.display()
            )));
        }
        let mut raw = Vec::new();
        File::open(&index_path)
            .and_then(|file| file.take(MAX_INDEX_BYTES + 1).read_to_end(&mut raw))
            .map_err(|error| io_error(&index_path, error))?;
        if raw.len() as u64 > MAX_INDEX_BYTES {
            return Err(StoreError::InvalidIndex(format!(
                "{INDEX_FILE} is larger than {MAX_INDEX_BYTES} bytes"
            )));
        }
        let index: StoreIndex = serde_json::from_slice(&raw)
            .map_err(|error| StoreError::InvalidIndex(error.to_string()))?;
        validate_index(&index)?;
        Ok(Self {
            root: root.to_path_buf(),
            index,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn index(&self) -> &StoreIndex {
        &self.index
    }

    pub fn tree(&self, name: &str) -> Option<&StoreTree> {
        self.index
            .trees
            .binary_search_by(|tree| tree.name.as_str().cmp(name))
            .ok()
            .map(|position| &self.index.trees[position])
    }

    pub fn entry(&self, tree: &str, path: &str) -> Option<&StoreFile> {
        let tree = self.tree(tree)?;
        tree.files
            .binary_search_by(|file| file.path.as_bytes().cmp(path.as_bytes()))
            .ok()
            .map(|position| &tree.files[position])
    }

    fn lookup(&self, tree: &str, path: &str) -> Result<&StoreFile, StoreError> {
        if self.tree(tree).is_none() {
            return Err(StoreError::UnknownTree(tree.into()));
        }
        self.entry(tree, path)
            .ok_or_else(|| StoreError::UnknownFile {
                tree: tree.into(),
                path: path.into(),
            })
    }

    /// The blob's path, after checking that `blobs/` and its fan-out
    /// directory are real directories, so a symlinked directory cannot lead
    /// a read outside the store.
    fn blob_path(&self, sha256: &str) -> Result<PathBuf, StoreError> {
        blob_path_in(&self.root, sha256)
    }

    /// Open one file for streaming. The returned reader verifies length and
    /// digest at end of stream; see [`StoreFileReader`].
    pub fn open_file(&self, tree: &str, path: &str) -> Result<StoreFileReader, StoreError> {
        let entry = self.lookup(tree, path)?;
        self.open_blob(entry)
    }

    fn open_blob(&self, entry: &StoreFile) -> Result<StoreFileReader, StoreError> {
        let blob = self.blob_path(&entry.sha256)?;
        StoreFileReader::open(&blob, &entry.sha256, entry.bytes)
    }

    /// Read one whole file, returned only after its length and digest match
    /// the index.
    pub fn read_file(&self, tree: &str, path: &str) -> Result<Vec<u8>, StoreError> {
        let entry = self.lookup(tree, path)?;
        let mut reader = self.open_blob(entry)?;
        let mut content = Vec::with_capacity(entry.bytes.min(64 * 1024 * 1024) as usize);
        reader
            .read_to_end(&mut content)
            .map_err(|error| StoreError::Blob {
                sha256: entry.sha256.clone(),
                detail: error.to_string(),
            })?;
        Ok(content)
    }

    /// Fully verify a single file (decode, length, digest) without keeping it.
    pub fn verify_file(&self, tree: &str, path: &str) -> Result<(), StoreError> {
        let entry = self.lookup(tree, path)?;
        let mut reader = self.open_blob(entry)?;
        drain(&mut reader).map_err(|error| StoreError::Blob {
            sha256: entry.sha256.clone(),
            detail: error.to_string(),
        })?;
        Ok(())
    }

    /// Verify the whole store: every blob the index references exists and
    /// decodes to its digest and length, and `blobs/` holds nothing else.
    pub fn verify(&self) -> StoreVerifyReport {
        let mut findings = Collector::default();
        let mut referenced: BTreeMap<&str, u64> = BTreeMap::new();
        let mut files = 0_usize;
        let mut uncompressed_bytes = 0_u64;
        for tree in &self.index.trees {
            files += tree.files.len();
            for file in &tree.files {
                referenced.insert(&file.sha256, file.bytes);
                uncompressed_bytes += file.bytes;
            }
        }

        self.check_layout(&referenced, &mut findings);

        let mut stored_bytes = 0_u64;
        for (sha256, bytes) in &referenced {
            let entry = StoreFile {
                path: String::new(),
                sha256: (*sha256).into(),
                bytes: *bytes,
            };
            let outcome = self.open_blob(&entry).and_then(|mut reader| {
                drain(&mut reader).map_err(|error| StoreError::Blob {
                    sha256: (*sha256).into(),
                    detail: error.to_string(),
                })
            });
            match outcome {
                Ok(_) => {
                    if let Ok(metadata) = fs::metadata(self.root.join(blob_relative(sha256))) {
                        stored_bytes += metadata.len();
                    }
                }
                Err(StoreError::Blob { detail, .. }) if detail.contains("is missing") => {
                    findings.push("missing_blob", blob_relative(sha256), detail);
                }
                Err(error) => {
                    findings.push("bad_blob", blob_relative(sha256), error.to_string());
                }
            }
        }

        StoreVerifyReport {
            store: self.root.display().to_string(),
            status: if findings.count == 0 {
                StoreVerifyStatus::Verified
            } else {
                StoreVerifyStatus::Failed
            },
            trees: self.index.trees.len(),
            files,
            distinct_blobs: referenced.len(),
            uncompressed_bytes,
            stored_bytes,
            finding_count: findings.count,
            findings: findings.items,
            writer_state: writer_state(&self.root),
        }
    }

    /// The store directory contains exactly `store.json` and `blobs/`, and
    /// `blobs/` holds exactly the referenced `<h0h1>/<h>.xz` regular files.
    /// Writer state (`store.lock`, `tmp/`) is ignored.
    fn check_layout(&self, referenced: &BTreeMap<&str, u64>, findings: &mut Collector) {
        let top = match read_dir_names(&self.root) {
            Ok(names) => names,
            Err(error) => {
                findings.push("layout", PathBuf::new(), error.to_string());
                return;
            }
        };
        for name in &top {
            if ![INDEX_FILE, BLOBS_DIR, LOCK_FILE, TMP_DIR].contains(&name.as_str()) {
                findings.push(
                    "unexpected_entry",
                    PathBuf::from(name),
                    "only store.json and blobs/ (and writer state: store.lock, tmp/) may appear in a store".into(),
                );
            }
        }
        let blobs = self.root.join(BLOBS_DIR);
        match fs::symlink_metadata(&blobs) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                findings.push(
                    "layout",
                    PathBuf::from(BLOBS_DIR),
                    "blobs is not a directory".into(),
                );
                return;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // An empty store has no blobs directory only if it references
                // none; missing referenced blobs are reported separately.
                return;
            }
            Err(error) => {
                findings.push("layout", PathBuf::from(BLOBS_DIR), error.to_string());
                return;
            }
        }
        let fanouts = match read_dir_names(&blobs) {
            Ok(names) => names,
            Err(error) => {
                findings.push("layout", PathBuf::from(BLOBS_DIR), error.to_string());
                return;
            }
        };
        for fanout in fanouts {
            let relative = Path::new(BLOBS_DIR).join(&fanout);
            let fanout_path = blobs.join(&fanout);
            let is_fanout_name = fanout.len() == 2
                && fanout
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'));
            let metadata = match fs::symlink_metadata(&fanout_path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    findings.push("layout", relative, error.to_string());
                    continue;
                }
            };
            if !metadata.is_dir() || !is_fanout_name {
                findings.push(
                    "misnamed_blob",
                    relative,
                    "blobs/ may contain only two-character lowercase hex directories".into(),
                );
                continue;
            }
            let names = match read_dir_names(&fanout_path) {
                Ok(names) => names,
                Err(error) => {
                    findings.push("layout", relative, error.to_string());
                    continue;
                }
            };
            for name in names {
                let blob_relative_path = relative.join(&name);
                let digest = name
                    .strip_suffix(".xz")
                    .filter(|digest| is_lower_hex_sha256(digest) && digest[..2] == fanout);
                let Some(digest) = digest else {
                    findings.push(
                        "misnamed_blob",
                        blob_relative_path,
                        "a blob must be named <h0h1>/<sha256>.xz".into(),
                    );
                    continue;
                };
                if !referenced.contains_key(digest) {
                    findings.push(
                        "unreferenced_blob",
                        blob_relative_path,
                        "no entry in the index references this blob".into(),
                    );
                    continue;
                }
                match fs::symlink_metadata(fanout_path.join(&name)) {
                    Ok(metadata) if metadata.is_file() => {}
                    Ok(_) => findings.push(
                        "not_regular_blob",
                        blob_relative_path,
                        "the blob is not a regular file".into(),
                    ),
                    Err(error) => findings.push("layout", blob_relative_path, error.to_string()),
                }
            }
        }
    }

    /// Materialize trees as `out/<name>/<path>`, byte-identical to what was
    /// packed, verifying every file as it is written. `out` must not exist.
    /// An empty `trees` selects every tree. On failure the partly written
    /// `out` is removed.
    pub fn unpack(&self, out: &Path, trees: &[String]) -> Result<StoreUnpackReport, StoreError> {
        let selected: Vec<&StoreTree> = if trees.is_empty() {
            self.index.trees.iter().collect()
        } else {
            let mut names = BTreeSet::new();
            let mut selected = Vec::new();
            for name in trees {
                let tree = self
                    .tree(name)
                    .ok_or_else(|| StoreError::UnknownTree(name.clone()))?;
                if names.insert(name.as_str()) {
                    selected.push(tree);
                }
            }
            selected
        };
        create_new_directory(out)?;
        match self.unpack_into(out, &selected) {
            Ok(report) => Ok(report),
            Err(error) => {
                let _ = fs::remove_dir_all(out);
                Err(error)
            }
        }
    }

    fn unpack_into(
        &self,
        out: &Path,
        selected: &[&StoreTree],
    ) -> Result<StoreUnpackReport, StoreError> {
        let mut files = 0_usize;
        let mut bytes = 0_u64;
        // The first written copy of each content, so a repeated blob is
        // decoded once and the later files are copied from verified bytes.
        let mut written: BTreeMap<&str, PathBuf> = BTreeMap::new();
        for tree in selected {
            let tree_root = out.join(&tree.name);
            fs::create_dir(&tree_root).map_err(|error| io_error(&tree_root, error))?;
            for file in &tree.files {
                let destination = confined_destination(&tree_root, &file.path)?;
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
                }
                if let Some(source) = written.get(file.sha256.as_str()) {
                    fs::copy(source, &destination)
                        .map_err(|error| io_error(&destination, error))?;
                } else {
                    let mut reader = self.open_blob(file)?;
                    let mut target = File::create_new(&destination)
                        .map_err(|error| io_error(&destination, error))?;
                    io::copy(&mut reader, &mut target).map_err(|error| StoreError::Blob {
                        sha256: file.sha256.clone(),
                        detail: error.to_string(),
                    })?;
                    target
                        .flush()
                        .map_err(|error| io_error(&destination, error))?;
                    written.insert(&file.sha256, destination);
                }
                files += 1;
                bytes += file.bytes;
            }
        }
        Ok(StoreUnpackReport {
            out: out.display().to_string(),
            trees: selected.len(),
            files,
            bytes,
        })
    }
}

/// Verify a store from its path, reporting an index that does not open as a
/// failed verification rather than an error.
pub fn verify_store(root: &Path) -> StoreVerifyReport {
    match EvidenceStore::open(root) {
        Ok(store) => store.verify(),
        Err(error) => StoreVerifyReport {
            store: root.display().to_string(),
            status: StoreVerifyStatus::Failed,
            trees: 0,
            files: 0,
            distinct_blobs: 0,
            uncompressed_bytes: 0,
            stored_bytes: 0,
            finding_count: 1,
            findings: vec![StoreFinding {
                kind: "invalid_index",
                path: INDEX_FILE.into(),
                detail: error.to_string(),
            }],
            writer_state: Vec::new(),
        },
    }
}

/// Describe the writer state a reader ignores: a lock file and a `tmp/`
/// directory.
fn writer_state(root: &Path) -> Vec<String> {
    let mut notes = Vec::new();
    let lock = root.join(LOCK_FILE);
    if fs::symlink_metadata(&lock).is_ok() {
        notes.push(format!(
            "{LOCK_FILE} is present ({}); a writer may be running or may have crashed",
            lock_holder(&lock)
        ));
    }
    let tmp = root.join(TMP_DIR);
    if fs::symlink_metadata(&tmp).is_ok() {
        let entries =
            read_dir_names(&tmp).map_or_else(|_| "unreadable".into(), |n| n.len().to_string());
        notes.push(format!(
            "{TMP_DIR}/ is present ({entries} entries); leftover from a writer, ignored"
        ));
    }
    notes
}

fn lock_holder(lock: &Path) -> String {
    let mut raw = Vec::new();
    let read = File::open(lock).and_then(|file| file.take(256).read_to_end(&mut raw));
    match read {
        Ok(_) => String::from_utf8_lossy(&raw)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        Err(_) => "holder unreadable".into(),
    }
}

#[derive(Default)]
struct Collector {
    count: usize,
    items: Vec<StoreFinding>,
}

impl Collector {
    fn push(&mut self, kind: &'static str, path: PathBuf, detail: String) {
        self.count += 1;
        if self.items.len() < MAX_REPORTED_FINDINGS {
            self.items.push(StoreFinding {
                kind,
                path: path.display().to_string(),
                detail,
            });
        }
    }
}

fn read_dir_names(directory: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(directory)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

fn create_new_directory(path: &Path) -> Result<(), StoreError> {
    if fs::symlink_metadata(path).is_ok() {
        return Err(StoreError::AlreadyExists(path.display().to_string()));
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    fs::create_dir(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            StoreError::AlreadyExists(path.display().to_string())
        } else {
            io_error(path, error)
        }
    })
}

/// Join a validated store path under `root`, refusing any component the
/// platform would read as anything but one plain name (for example a drive
/// prefix such as `C:` on Windows).
fn confined_destination(root: &Path, relative: &str) -> Result<PathBuf, StoreError> {
    let mut destination = root.to_path_buf();
    for component in relative.split('/') {
        let mut parsed = Path::new(component).components();
        match (parsed.next(), parsed.next()) {
            (Some(Component::Normal(name)), None) => destination.push(name),
            _ => {
                return Err(StoreError::InvalidIndex(format!(
                    "path `{relative}` has a component this platform cannot confine: `{component}`"
                )));
            }
        }
    }
    Ok(destination)
}

// ---------------------------------------------------------------------------
// Packing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StorePackReport {
    pub out: String,
    pub trees: usize,
    pub files: usize,
    pub distinct_blobs: usize,
    pub uncompressed_bytes: u64,
    pub distinct_bytes: u64,
    pub stored_bytes: u64,
}

struct ScannedFile {
    source: PathBuf,
    entry: StoreFile,
}

fn hash_stream(path: &Path) -> Result<(String, u64), StoreError> {
    let mut file = File::open(path).map_err(|error| io_error(path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; COPY_BUFFER];
    let mut length = 0_u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| io_error(path, error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        length += count as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), length))
}

/// Collect the regular files under `root`. Symlinks, special files and names
/// that are not valid store paths are refused, naming the offending path.
fn scan_tree(tree: &str, root: &Path) -> Result<Vec<ScannedFile>, StoreError> {
    let refuse = |path: &Path, why: &str| {
        Err(StoreError::InvalidInput(format!(
            "tree `{tree}`: `{}` {why}",
            path.display()
        )))
    };
    let mut found = Vec::new();
    let mut pending = vec![(root.to_path_buf(), Vec::<String>::new())];
    while let Some((directory, prefix)) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| io_error(&directory, error))? {
            let entry = entry.map_err(|error| io_error(&directory, error))?;
            let path = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                return refuse(&path, "has a name that is not valid UTF-8");
            };
            let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                return refuse(&path, "is a symbolic link");
            }
            let mut components = prefix.clone();
            components.push(name);
            if file_type.is_dir() {
                if components.len() >= MAX_PATH_COMPONENTS {
                    return refuse(&path, "is nested deeper than the format allows");
                }
                pending.push((path, components));
            } else if file_type.is_file() {
                let relative = components.join("/");
                if let Err(message) = validate_store_path(&relative) {
                    return refuse(&path, &format!("is not a valid store path: {message}"));
                }
                let (sha256, bytes) = hash_stream(&path)?;
                if bytes != metadata.len() {
                    return refuse(&path, "changed size while it was being read");
                }
                found.push(ScannedFile {
                    source: path,
                    entry: StoreFile {
                        path: relative,
                        sha256,
                        bytes,
                    },
                });
            } else {
                return refuse(&path, "is not a regular file or directory");
            }
        }
    }
    found.sort_by(|a, b| a.entry.path.as_bytes().cmp(b.entry.path.as_bytes()));
    Ok(found)
}

/// Compress `source` into `destination` (a staging file under `tmp/`),
/// checking that the bytes read match the scanned identity. Returns the
/// compressed size.
fn write_blob(
    source: &Path,
    entry: &StoreFile,
    destination: &Path,
    preset: u32,
) -> Result<u64, StoreError> {
    let mut input = File::open(source).map_err(|error| io_error(source, error))?;
    let output = File::create_new(destination).map_err(|error| io_error(destination, error))?;
    let mut encoder = XzEncoder::new(BufWriter::new(output), preset);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; COPY_BUFFER];
    let mut length = 0_u64;
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| io_error(source, error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        length += count as u64;
        encoder
            .write_all(&buffer[..count])
            .map_err(|error| io_error(destination, error))?;
    }
    let mut writer = encoder
        .finish()
        .map_err(|error| io_error(destination, error))?;
    writer
        .flush()
        .map_err(|error| io_error(destination, error))?;
    if length != entry.bytes || format!("{:x}", hasher.finalize()) != entry.sha256 {
        return Err(StoreError::InvalidInput(format!(
            "`{}` changed while the store was being written",
            source.display()
        )));
    }
    let file = writer
        .into_inner()
        .map_err(|error| io_error(destination, error.into_error()))?;
    file.sync_all()
        .map_err(|error| io_error(destination, error))?;
    let stored = fs::metadata(destination)
        .map_err(|error| io_error(destination, error))?
        .len();
    Ok(stored)
}

fn check_preset(preset: u32) -> Result<(), StoreError> {
    if preset > 9 {
        return Err(StoreError::InvalidPreset(preset));
    }
    Ok(())
}

/// Make sure `root/tmp` is a real directory (not a symlink) and return it.
fn ensure_tmp_dir(root: &Path) -> Result<PathBuf, StoreError> {
    let tmp = root.join(TMP_DIR);
    match fs::symlink_metadata(&tmp) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(StoreError::Layout(format!(
                "`{}` is not a directory",
                tmp.display()
            )));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // Another writer may create it first; that is fine.
            match fs::create_dir(&tmp) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error(&tmp, error)),
            }
        }
        Err(error) => return Err(io_error(&tmp, error)),
    }
    Ok(tmp)
}

/// What placing one blob did.
enum Placed {
    /// Written now; carries the compressed size.
    New(u64),
    /// Already in the store and verified; left untouched.
    Kept,
}

/// Put one blob into `root/blobs/` by writing it under `tmp/` and renaming it
/// into place. A blob that already exists is verified and kept; one that
/// fails verification is an error, never overwritten. The caller holds the
/// writer lock (or owns a store nobody else can see yet), so any staging
/// file of the same name is a crashed writer's and is replaced.
fn place_blob(
    root: &Path,
    tmp: &Path,
    source: &Path,
    entry: &StoreFile,
    preset: u32,
    created: &mut Vec<PathBuf>,
) -> Result<Placed, StoreError> {
    let destination = root.join(blob_relative(&entry.sha256));
    if fs::symlink_metadata(&destination).is_ok() {
        let path = blob_path_in(root, &entry.sha256)?;
        let mut reader = StoreFileReader::open(&path, &entry.sha256, entry.bytes)?;
        drain(&mut reader).map_err(|error| StoreError::Blob {
            sha256: entry.sha256.clone(),
            detail: format!("the existing blob does not verify: {error}"),
        })?;
        return Ok(Placed::Kept);
    }
    let staging = tmp.join(format!("{}.part", entry.sha256));
    if fs::symlink_metadata(&staging).is_ok() {
        fs::remove_file(&staging).map_err(|error| io_error(&staging, error))?;
    }
    let stored = match write_blob(source, entry, &staging, preset) {
        Ok(stored) => stored,
        Err(error) => {
            let _ = fs::remove_file(&staging);
            return Err(error);
        }
    };
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    fs::rename(&staging, &destination).map_err(|error| io_error(&destination, error))?;
    created.push(destination);
    Ok(Placed::New(stored))
}

/// Write `index` to `tmp/` and rename it over `store.json`, so a reader sees
/// the old index or the new one, never a partial one.
fn replace_index(root: &Path, tmp: &Path, index: &StoreIndex) -> Result<(), StoreError> {
    let staging = tmp.join("store.json.part");
    if fs::symlink_metadata(&staging).is_ok() {
        fs::remove_file(&staging).map_err(|error| io_error(&staging, error))?;
    }
    let mut file = File::create_new(&staging).map_err(|error| io_error(&staging, error))?;
    file.write_all(&serialize_index(index)?)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_error(&staging, error))?;
    let index_path = root.join(INDEX_FILE);
    fs::rename(&staging, &index_path).map_err(|error| io_error(&index_path, error))
}

/// Scan the named directories into index trees plus the files to store.
fn scan_sources(
    trees: &[(String, PathBuf)],
) -> Result<(Vec<StoreTree>, Vec<Vec<ScannedFile>>), StoreError> {
    let mut sources: Vec<(&String, &PathBuf)> = trees.iter().map(|(n, p)| (n, p)).collect();
    sources.sort_by(|a, b| a.0.cmp(b.0));
    for pair in sources.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(StoreError::InvalidInput(format!(
                "tree name `{}` is given more than once",
                pair[0].0
            )));
        }
    }
    for (name, _) in &sources {
        validate_tree_name(name).map_err(StoreError::InvalidInput)?;
    }
    let mut index_trees = Vec::new();
    let mut scanned = Vec::new();
    for (name, directory) in &sources {
        let metadata = fs::metadata(directory).map_err(|error| io_error(directory, error))?;
        if !metadata.is_dir() {
            return Err(StoreError::InvalidInput(format!(
                "tree `{name}`: `{}` is not a directory",
                directory.display()
            )));
        }
        let files = scan_tree(name, directory)?;
        index_trees.push(StoreTree {
            name: (*name).clone(),
            files: files.iter().map(|file| file.entry.clone()).collect(),
        });
        scanned.push(files);
    }
    Ok((index_trees, scanned))
}

/// Create a new store at `out` from named directory trees. `trees` pairs a
/// tree name with its source directory; their order does not matter.
/// `preset` is the xz preset (0 to 9; [`DEFAULT_XZ_PRESET`] for archival
/// packs). Blobs are staged under `tmp/` and renamed into place, then
/// `store.json` is renamed in last, so a directory without an index is an
/// unfinished store, never a valid one. On failure the output directory is
/// removed.
pub fn pack_store(
    out: &Path,
    trees: &[(String, PathBuf)],
    preset: u32,
) -> Result<StorePackReport, StoreError> {
    check_preset(preset)?;
    if fs::symlink_metadata(out).is_ok() {
        return Err(StoreError::AlreadyExists(out.display().to_string()));
    }
    let (index_trees, scanned) = scan_sources(trees)?;
    let index = StoreIndex {
        schema_version: EVIDENCE_STORE_SCHEMA_VERSION.into(),
        codec: EVIDENCE_STORE_CODEC.into(),
        trees: index_trees,
    };
    validate_index(&index).map_err(|error| StoreError::InvalidInput(error.to_string()))?;

    create_new_directory(out)?;
    match write_store(out, &index, &scanned, preset) {
        Ok(report) => Ok(report),
        Err(error) => {
            let _ = fs::remove_dir_all(out);
            Err(error)
        }
    }
}

fn write_store(
    out: &Path,
    index: &StoreIndex,
    scanned: &[Vec<ScannedFile>],
    preset: u32,
) -> Result<StorePackReport, StoreError> {
    let blobs = out.join(BLOBS_DIR);
    fs::create_dir(&blobs).map_err(|error| io_error(&blobs, error))?;
    let tmp = ensure_tmp_dir(out)?;
    let mut done: BTreeSet<&str> = BTreeSet::new();
    let mut files = 0_usize;
    let mut uncompressed_bytes = 0_u64;
    let mut distinct_bytes = 0_u64;
    let mut stored_bytes = 0_u64;
    let mut created = Vec::new();
    for file in scanned.iter().flatten() {
        files += 1;
        uncompressed_bytes += file.entry.bytes;
        if !done.insert(&file.entry.sha256) {
            continue;
        }
        if let Placed::New(stored) =
            place_blob(out, &tmp, &file.source, &file.entry, preset, &mut created)?
        {
            stored_bytes += stored;
        }
        distinct_bytes += file.entry.bytes;
    }
    replace_index(out, &tmp, index)?;
    fs::remove_dir(&tmp).map_err(|error| io_error(&tmp, error))?;
    Ok(StorePackReport {
        out: out.display().to_string(),
        trees: index.trees.len(),
        files,
        distinct_blobs: done.len(),
        uncompressed_bytes,
        distinct_bytes,
        stored_bytes,
    })
}

// ---------------------------------------------------------------------------
// Adding trees to an existing store
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoreAddReport {
    pub store: String,
    pub added_trees: Vec<String>,
    pub files: usize,
    pub distinct_blobs: usize,
    pub new_blobs: usize,
    pub reused_blobs: usize,
    pub uncompressed_bytes: u64,
    /// Compressed bytes of the blobs written now.
    pub stored_bytes: u64,
}

/// The writer lock: `store.lock`, created exclusively and removed on drop.
struct StoreLock {
    path: PathBuf,
}

impl StoreLock {
    fn acquire(root: &Path) -> Result<Self, StoreError> {
        let path = root.join(LOCK_FILE);
        let mut file = match File::create_new(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(StoreError::Locked {
                    store: root.display().to_string(),
                    lock: path.display().to_string(),
                    holder: lock_holder(&path),
                });
            }
            Err(error) => return Err(io_error(&path, error)),
        };
        let held = format!("pid {} host {}\n", std::process::id(), host_name());
        if let Err(error) = file
            .write_all(held.as_bytes())
            .and_then(|()| file.sync_all())
        {
            let _ = fs::remove_file(&path);
            return Err(io_error(&path, error));
        }
        Ok(Self { path })
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn host_name() -> String {
    let name = fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_default();
    let name: String = name
        .trim()
        .chars()
        .filter(|c| c.is_ascii_graphic())
        .collect();
    if name.is_empty() {
        "unknown".into()
    } else {
        name
    }
}

/// Add named directory trees to an existing store. The writer holds
/// `store.lock` (created exclusively; an existing lock refuses the write),
/// stages new blobs and the new index under `tmp/` and renames them into
/// place, so readers see the old index or the new one. A blob already in the
/// store is verified and kept. Existing trees are never changed, and a name
/// that already exists is refused. On failure the blobs this call wrote are
/// removed and the index is untouched.
pub fn add_trees(
    store: &Path,
    trees: &[(String, PathBuf)],
    preset: u32,
) -> Result<StoreAddReport, StoreError> {
    check_preset(preset)?;
    if trees.is_empty() {
        return Err(StoreError::InvalidInput("no tree to add".into()));
    }
    let mut names: Vec<&String> = trees.iter().map(|(name, _)| name).collect();
    names.sort();
    for pair in names.windows(2) {
        if pair[0] == pair[1] {
            return Err(StoreError::InvalidInput(format!(
                "tree name `{}` is given more than once",
                pair[0]
            )));
        }
    }
    for name in &names {
        validate_tree_name(name).map_err(StoreError::InvalidInput)?;
    }
    // Fail on a store that does not open before taking the lock.
    EvidenceStore::open(store)?;
    let _lock = StoreLock::acquire(store)?;
    // The index is read again under the lock: it is the one we extend.
    let existing = EvidenceStore::open(store)?;
    for name in &names {
        if existing.tree(name).is_some() {
            return Err(StoreError::TreeExists((*name).clone()));
        }
    }
    let (added, scanned) = scan_sources(trees)?;
    let mut index = existing.index.clone();
    index.trees.extend(added.iter().cloned());
    index.trees.sort_by(|a, b| a.name.cmp(&b.name));
    validate_index(&index).map_err(|error| StoreError::InvalidInput(error.to_string()))?;

    let tmp = ensure_tmp_dir(store)?;
    let mut created = Vec::new();
    let outcome = add_blobs_and_index(store, &tmp, &index, &scanned, preset, &mut created);
    match outcome {
        Ok((new_blobs, reused_blobs, stored_bytes, files, uncompressed_bytes)) => {
            // Remove the staging directory only if it is empty: another
            // writer's crashed leftovers stay for the operator to inspect.
            let _ = fs::remove_dir(&tmp);
            Ok(StoreAddReport {
                store: store.display().to_string(),
                added_trees: added.iter().map(|tree| tree.name.clone()).collect(),
                files,
                distinct_blobs: new_blobs + reused_blobs,
                new_blobs,
                reused_blobs,
                uncompressed_bytes,
                stored_bytes,
            })
        }
        Err(error) => {
            for blob in created {
                let _ = fs::remove_file(blob);
            }
            Err(error)
        }
    }
}

type AddCounts = (usize, usize, u64, usize, u64);

fn add_blobs_and_index(
    store: &Path,
    tmp: &Path,
    index: &StoreIndex,
    scanned: &[Vec<ScannedFile>],
    preset: u32,
    created: &mut Vec<PathBuf>,
) -> Result<AddCounts, StoreError> {
    let blobs = store.join(BLOBS_DIR);
    match fs::symlink_metadata(&blobs) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(StoreError::Layout(format!(
                "`{}` is not a directory",
                blobs.display()
            )));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(&blobs).map_err(|error| io_error(&blobs, error))?;
        }
        Err(error) => return Err(io_error(&blobs, error)),
    }
    let mut done: BTreeSet<&str> = BTreeSet::new();
    let (mut new_blobs, mut reused_blobs, mut stored_bytes) = (0_usize, 0_usize, 0_u64);
    let (mut files, mut uncompressed_bytes) = (0_usize, 0_u64);
    for file in scanned.iter().flatten() {
        files += 1;
        uncompressed_bytes += file.entry.bytes;
        if !done.insert(&file.entry.sha256) {
            continue;
        }
        match place_blob(store, tmp, &file.source, &file.entry, preset, created)? {
            Placed::New(stored) => {
                new_blobs += 1;
                stored_bytes += stored;
            }
            Placed::Kept => reused_blobs += 1,
        }
    }
    replace_index(store, tmp, index)?;
    Ok((
        new_blobs,
        reused_blobs,
        stored_bytes,
        files,
        uncompressed_bytes,
    ))
}

// ---------------------------------------------------------------------------
// Run persistence (ADR-0028 A4): lock-free blob puts, one locked tree add
// ---------------------------------------------------------------------------

/// How long adding a tree waits for another writer's lock before giving up.
/// A run holds the lock only while it extends the index, so contention is
/// brief; a lock that outlasts this wait is reported as the stale lock it
/// most likely is.
const ADD_LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

static PUT_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A reference to one blob in a store, by content address. It carries no
/// trust: every read goes through the verifying [`StoreFileReader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRef {
    root: PathBuf,
    sha256: String,
    bytes: u64,
}

impl BlobRef {
    /// The lowercase hex SHA-256 of the uncompressed content.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// The uncompressed length.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn open(&self) -> Result<StoreFileReader, StoreError> {
        let path = blob_path_in(&self.root, &self.sha256)?;
        StoreFileReader::open(&path, &self.sha256, self.bytes)
    }

    /// The whole content, returned only after length and digest match.
    pub fn read(&self) -> Result<Vec<u8>, StoreError> {
        let mut reader = self.open()?;
        let mut content = Vec::with_capacity(self.bytes.min(64 * 1024 * 1024) as usize);
        reader
            .read_to_end(&mut content)
            .map_err(|error| StoreError::Blob {
                sha256: self.sha256.clone(),
                detail: error.to_string(),
            })?;
        Ok(content)
    }

    /// Write the content to a new file at `destination`. The file is removed
    /// again if the content does not verify, so a verified path never holds
    /// unverified bytes.
    pub fn copy_to(&self, destination: &Path) -> Result<(), StoreError> {
        let mut reader = self.open()?;
        let mut file =
            File::create_new(destination).map_err(|error| io_error(destination, error))?;
        let copied = io::copy(&mut reader, &mut file).and_then(|_| file.sync_all());
        drop(file);
        if let Err(error) = copied {
            let _ = fs::remove_file(destination);
            return Err(StoreError::Blob {
                sha256: self.sha256.clone(),
                detail: error.to_string(),
            });
        }
        Ok(())
    }
}

/// What putting one file did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobPut {
    /// The tree entry for the file (the path given, the digest and length
    /// measured now).
    pub file: StoreFile,
    /// False when the blob was already in the store (and verified).
    pub new: bool,
    /// Compressed bytes written; zero for an existing blob.
    pub stored_bytes: u64,
}

/// A handle for writers that put blobs as they go and add one tree at the
/// end. Putting needs no lock: a blob is written under `tmp/` and renamed to
/// its content address, so concurrent writers of the same content produce the
/// same file. Only [`StoreWriter::add_tree`] takes `store.lock`.
#[derive(Debug, Clone)]
pub struct StoreWriter {
    root: PathBuf,
    preset: u32,
}

impl StoreWriter {
    /// Open the store at `root`, creating an empty one (a valid index with
    /// zero trees) when `root` does not exist or is an empty directory.
    /// Creation builds the store beside `root` and renames it into place, so
    /// concurrent creators cannot observe or leave a half-made store.
    pub fn open_or_create(root: &Path, preset: u32) -> Result<Self, StoreError> {
        check_preset(preset)?;
        let writer = Self {
            root: root.to_path_buf(),
            preset,
        };
        match fs::symlink_metadata(root) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                EvidenceStore::open(root)?;
                return Ok(writer);
            }
            Ok(metadata) if metadata.is_dir() => {
                let empty = fs::read_dir(root)
                    .map_err(|error| io_error(root, error))?
                    .next()
                    .is_none();
                if !empty {
                    EvidenceStore::open(root)?;
                    return Ok(writer);
                }
            }
            Ok(_) => {
                return Err(StoreError::Layout(format!(
                    "`{}` is not a directory",
                    root.display()
                )));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(root, error)),
        }
        writer.create_empty()?;
        Ok(writer)
    }

    fn create_empty(&self) -> Result<(), StoreError> {
        let root = &self.root;
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "store".into());
        let parent = root
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
        let sequence = PUT_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let building = parent.join(format!(
            ".{name}.creating-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&building).map_err(|error| io_error(&building, error))?;
        let built = (|| {
            fs::create_dir(building.join(BLOBS_DIR))
                .map_err(|error| io_error(&building.join(BLOBS_DIR), error))?;
            let index = StoreIndex {
                schema_version: EVIDENCE_STORE_SCHEMA_VERSION.into(),
                codec: EVIDENCE_STORE_CODEC.into(),
                trees: Vec::new(),
            };
            let index_path = building.join(INDEX_FILE);
            fs::write(&index_path, serialize_index(&index)?)
                .map_err(|error| io_error(&index_path, error))
        })();
        if let Err(error) = built {
            let _ = fs::remove_dir_all(&building);
            return Err(error);
        }
        match fs::rename(&building, root) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_dir_all(&building);
                // Another creator won the rename: use theirs if it is a store.
                if EvidenceStore::open(root).is_ok() {
                    Ok(())
                } else {
                    Err(io_error(root, error))
                }
            }
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A reference to a blob by content address, without checking it exists.
    pub fn blob(&self, sha256: &str, bytes: u64) -> BlobRef {
        BlobRef {
            root: self.root.clone(),
            sha256: sha256.into(),
            bytes,
        }
    }

    /// Put one file's content into the store as a blob, recording it under
    /// `path` (a store path inside the tree it will join). A blob already in
    /// the store is verified and kept; one that fails verification is an
    /// error, never overwritten.
    pub fn put_file(&self, source: &Path, path: &str) -> Result<BlobPut, StoreError> {
        validate_store_path(path).map_err(StoreError::InvalidInput)?;
        let (sha256, bytes) = hash_stream(source)?;
        if bytes > MAX_FILE_BYTES {
            return Err(StoreError::InvalidInput(format!(
                "`{}` is larger than the format allows",
                source.display()
            )));
        }
        let file = StoreFile {
            path: path.into(),
            sha256,
            bytes,
        };
        let destination = self.root.join(blob_relative(&file.sha256));
        if fs::symlink_metadata(&destination).is_ok() {
            let blob = blob_path_in(&self.root, &file.sha256)?;
            let mut reader = StoreFileReader::open(&blob, &file.sha256, file.bytes)?;
            drain(&mut reader).map_err(|error| StoreError::Blob {
                sha256: file.sha256.clone(),
                detail: format!("the existing blob does not verify: {error}"),
            })?;
            return Ok(BlobPut {
                file,
                new: false,
                stored_bytes: 0,
            });
        }
        // The staging name is unique to this call, so concurrent puts of the
        // same content never share or remove each other's staging file.
        let sequence = PUT_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut attempts = 0;
        let (staging, stored) = loop {
            let tmp = ensure_tmp_dir(&self.root)?;
            let staging = tmp.join(format!(
                "{}.{}.{sequence}.part",
                file.sha256,
                std::process::id()
            ));
            match write_blob(source, &file, &staging, self.preset) {
                Ok(stored) => break (staging, stored),
                Err(StoreError::Io { source, .. })
                    if source.kind() == io::ErrorKind::NotFound && attempts < 3 =>
                {
                    // A finishing writer removed an empty `tmp/` between our
                    // check and our create; make it again.
                    attempts += 1;
                }
                Err(error) => {
                    let _ = fs::remove_file(&staging);
                    return Err(error);
                }
            }
        };
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
        }
        if let Err(error) = fs::rename(&staging, &destination) {
            let _ = fs::remove_file(&staging);
            return Err(io_error(&destination, error));
        }
        Ok(BlobPut {
            file,
            new: true,
            stored_bytes: stored,
        })
    }

    /// Add one tree whose files are already blobs in the store. The writer
    /// takes `store.lock` (waiting briefly for another writer's), re-reads
    /// the index, checks that every blob exists, and renames the new index
    /// into place. A `preferred` name that exists is adjusted
    /// deterministically (`-2`, `-3`, …, within 128 characters). The report's
    /// `added_trees` names the tree as added; its blob counts say how many
    /// distinct blobs the tree references (all `reused_blobs`, because the
    /// puts already placed them), and `stored_bytes` is zero.
    pub fn add_tree(
        &self,
        preferred: &str,
        mut files: Vec<StoreFile>,
    ) -> Result<StoreAddReport, StoreError> {
        validate_tree_name(preferred).map_err(StoreError::InvalidInput)?;
        files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        let started = std::time::Instant::now();
        let _lock = loop {
            match StoreLock::acquire(&self.root) {
                Ok(lock) => break lock,
                Err(StoreError::Locked { .. }) if started.elapsed() < ADD_LOCK_WAIT => {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(error) => return Err(error),
            }
        };
        let existing = EvidenceStore::open(&self.root)?;
        let mut name = preferred.to_string();
        let mut suffix = 1_u32;
        while existing.tree(&name).is_some() {
            suffix += 1;
            let tail = format!("-{suffix}");
            let keep = MAX_TREE_NAME_CHARS - tail.len();
            name = format!("{}{tail}", preferred.chars().take(keep).collect::<String>());
        }
        let mut distinct: BTreeMap<&str, u64> = BTreeMap::new();
        let mut uncompressed_bytes = 0_u64;
        for file in &files {
            uncompressed_bytes += file.bytes;
            distinct.insert(&file.sha256, file.bytes);
            let blob = blob_path_in(&self.root, &file.sha256)?;
            match fs::symlink_metadata(&blob) {
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => {
                    return Err(StoreError::Blob {
                        sha256: file.sha256.clone(),
                        detail: "the blob is not a regular file".into(),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Err(StoreError::Blob {
                        sha256: file.sha256.clone(),
                        detail: "the blob file is missing".into(),
                    });
                }
                Err(error) => return Err(io_error(&blob, error)),
            }
        }
        let distinct_blobs = distinct.len();
        let mut index = existing.index.clone();
        let file_count = files.len();
        index.trees.push(StoreTree {
            name: name.clone(),
            files,
        });
        index.trees.sort_by(|a, b| a.name.cmp(&b.name));
        validate_index(&index).map_err(|error| StoreError::InvalidInput(error.to_string()))?;
        let tmp = ensure_tmp_dir(&self.root)?;
        replace_index(&self.root, &tmp, &index)?;
        // Remove the staging directory only if it is empty: another
        // writer's in-flight or crashed staging files stay.
        let _ = fs::remove_dir(&tmp);
        Ok(StoreAddReport {
            store: self.root.display().to_string(),
            added_trees: vec![name],
            files: file_count,
            distinct_blobs,
            new_blobs: 0,
            reused_blobs: distinct_blobs,
            uncompressed_bytes,
            stored_bytes: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    /// Most tests pack with the default preset; the shadowing keeps them short.
    fn pack_store(out: &Path, trees: &[(String, PathBuf)]) -> Result<StorePackReport, StoreError> {
        super::pack_store(out, trees, DEFAULT_XZ_PRESET)
    }

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "avila-core-store-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn xz(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = XzEncoder::new(Vec::new(), 1);
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn blob_file(store: &Path, sha256: &str) -> PathBuf {
        store.join(blob_relative(sha256))
    }

    /// Two trees sharing one file, one tree repeating a file under two paths.
    fn sample_sources(dir: &TestDir) -> Vec<(String, PathBuf)> {
        let case = dir.path("src-case");
        write(&case.join("package.json"), b"{\"package\":true}\n");
        write(&case.join("expected/history.json"), b"shared content");
        write(&case.join("nested/deep/file.txt"), b"deep");
        write(&case.join("empty.bin"), b"");
        let work = dir.path("src-work");
        write(
            &work.join("steps/s1/outputs/history.json"),
            b"shared content",
        );
        write(
            &work.join("steps/s2/inputs/history.json"),
            b"shared content",
        );
        write(&work.join("log.txt"), b"log line\n");
        vec![("workspace".into(), work), ("case".into(), case)]
    }

    fn packed(dir: &TestDir) -> PathBuf {
        let out = dir.path("store");
        pack_store(&out, &sample_sources(dir)).unwrap();
        out
    }

    fn tree_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut found = BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    let relative = path.strip_prefix(root).unwrap();
                    found.insert(
                        relative.to_string_lossy().replace('\\', "/"),
                        fs::read(&path).unwrap(),
                    );
                }
            }
        }
        found
    }

    fn read_index(store: &Path) -> Value {
        serde_json::from_slice(&fs::read(store.join("store.json")).unwrap()).unwrap()
    }

    fn write_index(store: &Path, index: &Value) {
        fs::write(
            store.join("store.json"),
            serde_json::to_vec_pretty(index).unwrap(),
        )
        .unwrap();
    }

    fn index_with(trees: Value) -> Value {
        json!({
            "schema_version": EVIDENCE_STORE_SCHEMA_VERSION,
            "codec": "xz",
            "trees": trees,
        })
    }

    fn open_error(index: &Value) -> String {
        let dir = TestDir::new();
        let store = dir.path("s");
        fs::create_dir_all(&store).unwrap();
        write_index(&store, index);
        EvidenceStore::open(&store).unwrap_err().to_string()
    }

    fn file(path: &str, content: &[u8]) -> Value {
        json!({ "path": path, "sha256": sha(content), "bytes": content.len() })
    }

    fn findings(report: &StoreVerifyReport) -> Vec<&'static str> {
        report.findings.iter().map(|finding| finding.kind).collect()
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let dir = TestDir::new();
        let sources = sample_sources(&dir);
        let store = dir.path("store");
        let report = pack_store(&store, &sources).unwrap();
        assert_eq!(report.trees, 2);
        assert_eq!(report.files, 7);

        let opened = EvidenceStore::open(&store).unwrap();
        assert_eq!(opened.verify().status, StoreVerifyStatus::Verified);
        let out = dir.path("out");
        let unpacked = opened.unpack(&out, &[]).unwrap();
        assert_eq!(unpacked.files, 7);
        for (name, source) in &sources {
            assert_eq!(tree_files(&out.join(name)), tree_files(source), "{name}");
        }
        assert_eq!(
            opened.read_file("case", "nested/deep/file.txt").unwrap(),
            b"deep"
        );
        assert_eq!(opened.read_file("case", "empty.bin").unwrap(), b"");
    }

    #[test]
    fn identical_contents_share_one_blob() {
        let dir = TestDir::new();
        let store = dir.path("store");
        let report = pack_store(&store, &sample_sources(&dir)).unwrap();
        // "shared content" appears in three files of two trees; the other
        // distinct contents are the package, "deep", the empty file, the log.
        assert_eq!(report.distinct_blobs, 5);
        let blobs: usize = fs::read_dir(store.join("blobs"))
            .unwrap()
            .map(|fanout| fs::read_dir(fanout.unwrap().path()).unwrap().count())
            .sum();
        assert_eq!(blobs, 5);
        assert_eq!(report.uncompressed_bytes, 17 + 14 + 4 + 14 + 14 + 9);
        assert_eq!(report.distinct_bytes, 17 + 14 + 4 + 9);
    }

    #[test]
    fn same_file_twice_in_one_tree_is_one_blob() {
        let dir = TestDir::new();
        let source = dir.path("src");
        write(&source.join("a.txt"), b"same");
        write(&source.join("b/a.txt"), b"same");
        let report = pack_store(&dir.path("store"), &[("t".into(), source)]).unwrap();
        assert_eq!((report.files, report.distinct_blobs), (2, 1));
    }

    #[test]
    fn index_is_deterministic_and_sorted() {
        let dir = TestDir::new();
        let sources = sample_sources(&dir);
        pack_store(&dir.path("one"), &sources).unwrap();
        let mut reversed = sources.clone();
        reversed.reverse();
        pack_store(&dir.path("two"), &reversed).unwrap();
        assert_eq!(
            fs::read(dir.path("one/store.json")).unwrap(),
            fs::read(dir.path("two/store.json")).unwrap()
        );
        let index = read_index(&dir.path("one"));
        assert_eq!(index["trees"][0]["name"], "case");
        assert_eq!(index["schema_version"], EVIDENCE_STORE_SCHEMA_VERSION);
    }

    #[test]
    fn pack_refuses_an_existing_output() {
        let dir = TestDir::new();
        let sources = sample_sources(&dir);
        fs::create_dir_all(dir.path("store")).unwrap();
        assert!(matches!(
            pack_store(&dir.path("store"), &sources),
            Err(StoreError::AlreadyExists(_))
        ));
    }

    #[test]
    fn pack_refuses_duplicate_and_bad_tree_names() {
        let dir = TestDir::new();
        let source = dir.path("src");
        write(&source.join("a"), b"a");
        for names in [["x", "x"], ["x", "bad name"], ["x", ".."]] {
            let trees: Vec<_> = names
                .iter()
                .map(|n| ((*n).into(), source.clone()))
                .collect();
            let error = pack_store(&dir.path("store"), &trees).unwrap_err();
            assert!(matches!(error, StoreError::InvalidInput(_)), "{names:?}");
            assert!(!dir.path("store").exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn pack_refuses_symlinks_and_special_files() {
        let dir = TestDir::new();
        let source = dir.path("src");
        write(&source.join("real.txt"), b"real");
        std::os::unix::fs::symlink(source.join("real.txt"), source.join("link.txt")).unwrap();
        let error = pack_store(&dir.path("store"), &[("t".into(), source.clone())])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("link.txt") && error.contains("symbolic link"),
            "{error}"
        );
        assert!(
            !dir.path("store").exists(),
            "a failed pack leaves no output"
        );

        fs::remove_file(source.join("link.txt")).unwrap();
        let _socket = std::os::unix::net::UnixListener::bind(source.join("sock")).unwrap();
        let error = pack_store(&dir.path("store"), &[("t".into(), source.clone())])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("sock") && error.contains("not a regular file"),
            "{error}"
        );

        // A symlinked directory is also refused rather than followed.
        fs::remove_file(source.join("sock")).unwrap();
        std::os::unix::fs::symlink(dir.path("src"), source.join("loop")).unwrap();
        let error = pack_store(&dir.path("store"), &[("t".into(), source)])
            .unwrap_err()
            .to_string();
        assert!(error.contains("loop"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn pack_refuses_non_utf8_and_control_character_names() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let dir = TestDir::new();
        let source = dir.path("src");
        write(&source.join("ok.txt"), b"ok");
        let odd = source.join(OsString::from_vec(vec![b'b', 0xff, b'd']));
        // Some file systems (APFS on macOS) refuse non-UTF-8 names outright;
        // there the case cannot arise, so only the other forms are checked.
        if fs::write(&odd, b"x").is_ok() {
            let error = pack_store(&dir.path("store"), &[("t".into(), source.clone())])
                .unwrap_err()
                .to_string();
            assert!(error.contains("UTF-8"), "{error}");
            fs::remove_file(&odd).unwrap();
        }
        fs::write(source.join("tab\there"), b"x").unwrap();
        let error = pack_store(&dir.path("store"), &[("t".into(), source.clone())])
            .unwrap_err()
            .to_string();
        assert!(error.contains("control"), "{error}");
        fs::remove_file(source.join("tab\there")).unwrap();
        fs::write(source.join("back\\slash"), b"x").unwrap();
        let error = pack_store(&dir.path("store"), &[("t".into(), source)])
            .unwrap_err()
            .to_string();
        assert!(error.contains("backslash"), "{error}");
    }

    #[test]
    fn index_refuses_bad_path_forms() {
        let long = vec!["a"; 65].join("/");
        let bad = [
            "../x",
            "a/../b",
            "a/./b",
            "a//b",
            "a/",
            "/abs",
            "a\\b",
            "a\u{1}b",
            "a\u{7f}b",
            "a\0b",
            "",
            ".",
            long.as_str(),
        ];
        for path in bad {
            let index = index_with(json!([{ "name": "t", "files": [file(path, b"x")] }]));
            let error = open_error(&index);
            assert!(error.contains("invalid store index"), "{path:?}: {error}");
        }
        // 64 components is the largest accepted depth.
        let deepest = vec!["a"; 64].join("/");
        let index = index_with(json!([{ "name": "t", "files": [file(&deepest, b"x")] }]));
        let dir = TestDir::new();
        fs::create_dir_all(dir.path("s")).unwrap();
        write_index(&dir.path("s"), &index);
        EvidenceStore::open(&dir.path("s")).unwrap();
    }

    #[test]
    fn index_refuses_a_file_that_is_also_a_directory() {
        // "a-x" sorts between "a" and "a/b", so the pair is not adjacent.
        let index = index_with(json!([{ "name": "t", "files": [
            file("a", b"1"), file("a-x", b"2"), file("a/b", b"3"),
        ] }]));
        assert!(open_error(&index).contains("directory prefix"));
    }

    #[test]
    fn index_refuses_unsorted_and_duplicate_entries() {
        let unsorted_files = index_with(json!([{ "name": "t", "files": [
            file("b", b"1"), file("a", b"2"),
        ] }]));
        assert!(open_error(&unsorted_files).contains("not sorted"));
        // Byte order, not locale or code-point-insensitive order: "B" < "a".
        let byte_order = index_with(json!([{ "name": "t", "files": [
            file("a", b"1"), file("B", b"2"),
        ] }]));
        assert!(open_error(&byte_order).contains("not sorted"));
        let duplicate_path = index_with(json!([{ "name": "t", "files": [
            file("a", b"1"), file("a", b"1"),
        ] }]));
        assert!(open_error(&duplicate_path).contains("more than once"));
        let unsorted_trees = index_with(json!([
            { "name": "b", "files": [] }, { "name": "a", "files": [] },
        ]));
        assert!(open_error(&unsorted_trees).contains("not sorted"));
        let duplicate_trees = index_with(json!([
            { "name": "a", "files": [] }, { "name": "a", "files": [] },
        ]));
        assert!(open_error(&duplicate_trees).contains("duplicate tree"));
    }

    #[test]
    fn index_refuses_bad_names_digests_and_lengths() {
        for name in ["", ".", "..", "has space", "slash/name", &"n".repeat(129)] {
            let index = index_with(json!([{ "name": name, "files": [] }]));
            assert!(open_error(&index).contains("tree name"), "{name:?}");
        }
        let mut entry = file("a", b"x");
        entry["sha256"] = json!(sha(b"x").to_uppercase());
        assert!(
            open_error(&index_with(json!([{ "name": "t", "files": [entry] }]))).contains("sha256")
        );
        let mut entry = file("a", b"x");
        entry["sha256"] = json!(format!("sha256:{}", sha(b"x")));
        assert!(
            open_error(&index_with(json!([{ "name": "t", "files": [entry] }]))).contains("sha256")
        );
        let mut entry = file("a", b"x");
        entry["bytes"] = json!(MAX_FILE_BYTES + 1);
        assert!(
            open_error(&index_with(json!([{ "name": "t", "files": [entry] }]))).contains("limit")
        );
    }

    #[test]
    fn index_refuses_one_digest_with_two_lengths() {
        let mut other = file("b", b"x");
        other["bytes"] = json!(2);
        let index = index_with(json!([{ "name": "t", "files": [file("a", b"x"), other] }]));
        assert!(open_error(&index).contains("two different lengths"));
    }

    #[test]
    fn index_refuses_wrong_schema_codec_and_unknown_fields() {
        let mut index = index_with(json!([]));
        index["schema_version"] = json!("avila.core/evidence-store/v0.2");
        assert!(open_error(&index).contains("schema_version"));
        let mut index = index_with(json!([]));
        index["codec"] = json!("zstd");
        assert!(open_error(&index).contains("codec"));
        let mut index = index_with(json!([]));
        index["extra"] = json!(1);
        assert!(open_error(&index).contains("unknown field"));
        let mut entry = file("a", b"x");
        entry["mode"] = json!(420);
        assert!(
            open_error(&index_with(json!([{ "name": "t", "files": [entry] }])))
                .contains("unknown field")
        );
        // Duplicate JSON keys are refused, not last-one-wins.
        let dir = TestDir::new();
        fs::create_dir_all(dir.path("s")).unwrap();
        fs::write(
            dir.path("s/store.json"),
            format!(
                "{{\"schema_version\":\"{EVIDENCE_STORE_SCHEMA_VERSION}\",\"codec\":\"xz\",\"codec\":\"xz\",\"trees\":[]}}"
            ),
        )
        .unwrap();
        assert!(EvidenceStore::open(&dir.path("s")).is_err());
    }

    #[test]
    fn index_enforces_count_bounds() {
        let trees: Vec<Value> = (0..=MAX_TREES)
            .map(|n| json!({ "name": format!("t{n:05}"), "files": [] }))
            .collect();
        assert!(open_error(&index_with(Value::Array(trees))).contains("more than 1024 trees"));
        let files: Vec<Value> = (0..=MAX_FILES_PER_TREE)
            .map(|n| json!({ "path": format!("f{n:06}"), "sha256": sha(b"x"), "bytes": 1 }))
            .collect();
        assert!(
            open_error(&index_with(json!([{ "name": "t", "files": files }])))
                .contains("65536 files")
        );
    }

    #[test]
    fn open_refuses_an_oversized_index() {
        let dir = TestDir::new();
        fs::create_dir_all(dir.path("s")).unwrap();
        let file = File::create(dir.path("s/store.json")).unwrap();
        file.set_len(MAX_INDEX_BYTES + 1).unwrap();
        let error = EvidenceStore::open(&dir.path("s")).unwrap_err().to_string();
        assert!(error.contains("larger than"), "{error}");
    }

    #[test]
    fn tampered_blob_is_detected() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let target = sha(b"deep");
        fs::write(blob_file(&store, &target), xz(b"DEEP")).unwrap();
        let opened = EvidenceStore::open(&store).unwrap();
        let error = opened
            .read_file("case", "nested/deep/file.txt")
            .unwrap_err()
            .to_string();
        assert!(error.contains("hashes to"), "{error}");
        let report = opened.verify();
        assert_eq!(report.status, StoreVerifyStatus::Failed);
        assert_eq!(findings(&report), ["bad_blob"]);
        assert!(opened.unpack(&dir.path("out"), &[]).is_err());
        assert!(!dir.path("out").exists(), "failed unpack leaves no output");
    }

    #[test]
    fn decompression_bomb_is_bounded() {
        let dir = TestDir::new();
        let store = packed(&dir);
        // 64 MiB of zeros compresses to a few KiB; the index says 4 bytes.
        let bomb = xz(&vec![0_u8; 64 * 1024 * 1024]);
        assert!(bomb.len() < 64 * 1024);
        fs::write(blob_file(&store, &sha(b"deep")), bomb).unwrap();
        let opened = EvidenceStore::open(&store).unwrap();
        let mut reader = opened.open_file("case", "nested/deep/file.txt").unwrap();
        let mut sink = Vec::new();
        let error = reader.read_to_end(&mut sink).unwrap_err().to_string();
        assert!(error.contains("more than the 4 bytes"), "{error}");
        assert!(sink.len() <= 5, "at most bytes + 1 were produced");
        assert!(opened.read_file("case", "nested/deep/file.txt").is_err());
    }

    #[test]
    fn short_truncated_and_trailing_blobs_are_detected() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let opened = EvidenceStore::open(&store).unwrap();
        let blob = blob_file(&store, &sha(b"deep"));
        let good = fs::read(&blob).unwrap();

        fs::write(&blob, xz(b"dee")).unwrap();
        let error = opened
            .read_file("case", "nested/deep/file.txt")
            .unwrap_err();
        assert!(error.to_string().contains("3 bytes"), "{error}");

        fs::write(&blob, &good[..good.len() - 4]).unwrap();
        assert!(opened.read_file("case", "nested/deep/file.txt").is_err());

        let mut trailing = good.clone();
        trailing.extend_from_slice(b"junk");
        fs::write(&blob, trailing).unwrap();
        let error = opened
            .read_file("case", "nested/deep/file.txt")
            .unwrap_err();
        assert!(error.to_string().contains("follows"), "{error}");

        let mut doubled = good.clone();
        doubled.extend_from_slice(&good);
        fs::write(&blob, doubled).unwrap();
        assert!(opened.read_file("case", "nested/deep/file.txt").is_err());

        fs::write(&blob, b"not xz at all").unwrap();
        assert!(opened.read_file("case", "nested/deep/file.txt").is_err());

        fs::write(&blob, good).unwrap();
        assert_eq!(opened.verify().status, StoreVerifyStatus::Verified);
    }

    #[test]
    fn a_failed_reader_stays_failed() {
        let dir = TestDir::new();
        let store = packed(&dir);
        fs::write(blob_file(&store, &sha(b"deep")), xz(b"DEEP")).unwrap();
        let opened = EvidenceStore::open(&store).unwrap();
        let mut reader = opened.open_file("case", "nested/deep/file.txt").unwrap();
        let mut buffer = [0_u8; 16];
        let mut failed = false;
        for _ in 0..8 {
            if reader.read(&mut buffer).is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed, "end of stream must not look clean");
        assert!(reader.read(&mut buffer).is_err());
    }

    #[test]
    fn missing_blob_is_reported() {
        let dir = TestDir::new();
        let store = packed(&dir);
        fs::remove_file(blob_file(&store, &sha(b"deep"))).unwrap();
        let opened = EvidenceStore::open(&store).unwrap();
        assert_eq!(findings(&opened.verify()), ["missing_blob"]);
        assert!(opened.read_file("case", "nested/deep/file.txt").is_err());
    }

    #[test]
    fn unreferenced_and_misnamed_files_are_refused() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let opened = EvidenceStore::open(&store).unwrap();
        let extra = sha(b"not in the index");
        write(&blob_file(&store, &extra), &xz(b"not in the index"));
        assert_eq!(findings(&opened.verify()), ["unreferenced_blob"]);
        fs::remove_file(blob_file(&store, &extra)).unwrap();
        fs::remove_dir(store.join("blobs").join(&extra[..2])).ok();

        // A real blob copied under a wrong name.
        let real = blob_file(&store, &sha(b"deep"));
        write(&real.with_file_name("notes.txt"), b"x");
        write(&store.join("blobs/zz/x.xz"), b"x");
        let wrong_fanout = store.join("blobs/00").join(real.file_name().unwrap());
        write(&wrong_fanout, &fs::read(&real).unwrap());
        let report = opened.verify();
        assert_eq!(report.status, StoreVerifyStatus::Failed);
        assert_eq!(report.finding_count, 3);
        assert!(
            findings(&report)
                .iter()
                .all(|kind| *kind == "misnamed_blob")
        );
    }

    #[test]
    fn extra_top_level_entries_are_refused() {
        let dir = TestDir::new();
        let store = packed(&dir);
        write(&store.join("README.txt"), b"hello");
        let report = verify_store(&store);
        assert_eq!(findings(&report), ["unexpected_entry"]);
    }

    #[cfg(unix)]
    #[test]
    fn blob_that_is_a_symlink_is_refused() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let blob = blob_file(&store, &sha(b"deep"));
        let elsewhere = dir.path("elsewhere.xz");
        fs::rename(&blob, &elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &blob).unwrap();
        let opened = EvidenceStore::open(&store).unwrap();
        let error = opened
            .read_file("case", "nested/deep/file.txt")
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a regular file"), "{error}");
        let report = opened.verify();
        assert!(
            findings(&report).contains(&"not_regular_blob"),
            "{report:?}"
        );
        assert_eq!(report.status, StoreVerifyStatus::Failed);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_blob_directories_are_refused() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let fanout = store.join("blobs").join(&sha(b"deep")[..2]);
        let moved = dir.path("moved");
        fs::rename(&fanout, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &fanout).unwrap();
        let opened = EvidenceStore::open(&store).unwrap();
        assert!(opened.read_file("case", "nested/deep/file.txt").is_err());
        assert_eq!(opened.verify().status, StoreVerifyStatus::Failed);
    }

    #[test]
    fn unpack_refuses_an_existing_target_and_unknown_trees() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let opened = EvidenceStore::open(&store).unwrap();
        fs::create_dir_all(dir.path("out")).unwrap();
        assert!(matches!(
            opened.unpack(&dir.path("out"), &[]),
            Err(StoreError::AlreadyExists(_))
        ));
        assert!(
            dir.path("out").exists(),
            "an existing target is never removed"
        );
        assert!(matches!(
            opened.unpack(&dir.path("new"), &["nope".into()]),
            Err(StoreError::UnknownTree(_))
        ));
        assert!(!dir.path("new").exists());
    }

    #[test]
    fn unpack_can_select_trees() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let opened = EvidenceStore::open(&store).unwrap();
        let report = opened.unpack(&dir.path("out"), &["case".into()]).unwrap();
        assert_eq!((report.trees, report.files), (1, 4));
        assert!(dir.path("out/case/package.json").is_file());
        assert!(!dir.path("out/workspace").exists());
    }

    #[test]
    fn lookups_report_unknown_trees_and_files() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let opened = EvidenceStore::open(&store).unwrap();
        assert!(matches!(
            opened.read_file("nope", "a"),
            Err(StoreError::UnknownTree(_))
        ));
        assert!(matches!(
            opened.read_file("case", "nope"),
            Err(StoreError::UnknownFile { .. })
        ));
        assert!(opened.entry("case", "package.json").is_some());
    }

    #[test]
    fn verify_store_reports_an_invalid_index_as_a_failure() {
        let dir = TestDir::new();
        fs::create_dir_all(dir.path("s")).unwrap();
        fs::write(dir.path("s/store.json"), b"{").unwrap();
        let report = verify_store(&dir.path("s"));
        assert_eq!(report.status, StoreVerifyStatus::Failed);
        assert_eq!(findings(&report), ["invalid_index"]);
    }

    // ---- appendable stores and the writer preset (Amendment 1: A1, A2) ----

    fn extra_sources(dir: &TestDir) -> Vec<(String, PathBuf)> {
        let run = dir.path("src-run");
        // One file repeats content already in the store; one is new.
        write(&run.join("steps/s1/history.json"), b"shared content");
        write(&run.join("steps/s1/new.json"), b"only in the added tree");
        vec![("run-2".into(), run)]
    }

    fn all_files(store: &Path) -> BTreeMap<String, Vec<u8>> {
        tree_files(store)
    }

    #[test]
    fn add_extends_a_store_without_touching_existing_trees() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let before = EvidenceStore::open(&store).unwrap();
        let blobs_before = all_files(&store.join("blobs"));
        let index_before = before.index().clone();

        let report = add_trees(&store, &extra_sources(&dir), 1).unwrap();
        assert_eq!(report.added_trees, ["run-2"]);
        assert_eq!(report.files, 2);
        assert_eq!((report.new_blobs, report.reused_blobs), (1, 1));

        let after = EvidenceStore::open(&store).unwrap();
        assert_eq!(after.verify().status, StoreVerifyStatus::Verified);
        assert!(after.verify().writer_state.is_empty());
        let names: Vec<_> = after
            .index()
            .trees
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(names, ["case", "run-2", "workspace"]);
        // Trees that were there are unchanged, entry for entry.
        for tree in &index_before.trees {
            assert_eq!(after.tree(&tree.name), Some(tree));
        }
        // Existing blobs are byte-identical; exactly one blob is new.
        let blobs_after = all_files(&store.join("blobs"));
        for (path, bytes) in &blobs_before {
            assert_eq!(blobs_after.get(path), Some(bytes), "{path}");
        }
        assert_eq!(blobs_after.len(), blobs_before.len() + 1);
        assert_eq!(
            after.read_file("run-2", "steps/s1/history.json").unwrap(),
            b"shared content"
        );
        // The writer leaves neither its lock nor its staging directory.
        assert!(!store.join("store.lock").exists());
        assert!(!store.join("tmp").exists());
        // Unpacking old trees still gives their original bytes.
        let out = dir.path("out");
        after.unpack(&out, &["case".into()]).unwrap();
        assert_eq!(
            tree_files(&out.join("case")),
            tree_files(&dir.path("src-case"))
        );
    }

    #[test]
    fn add_refuses_an_existing_tree_name_and_changes_nothing() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let index = fs::read(store.join("store.json")).unwrap();
        let blobs = all_files(&store.join("blobs"));
        let error = add_trees(&store, &[("case".into(), dir.path("src-work"))], 1).unwrap_err();
        assert!(matches!(error, StoreError::TreeExists(ref name) if name == "case"));
        assert_eq!(fs::read(store.join("store.json")).unwrap(), index);
        assert_eq!(all_files(&store.join("blobs")), blobs);
        assert!(!store.join("store.lock").exists(), "the lock is released");
        // Bad names and repeats in one call are refused too.
        for trees in [
            vec![
                ("a".into(), dir.path("src-work")),
                ("a".into(), dir.path("src-work")),
            ],
            vec![("bad/name".into(), dir.path("src-work"))],
            vec![],
        ] {
            assert!(matches!(
                add_trees(&store, &trees, 1),
                Err(StoreError::InvalidInput(_))
            ));
        }
    }

    #[test]
    fn add_refuses_while_locked_and_says_how_to_clear_a_stale_lock() {
        let dir = TestDir::new();
        let store = packed(&dir);
        fs::write(store.join("store.lock"), "pid 99999 host elsewhere\n").unwrap();
        let error = add_trees(&store, &extra_sources(&dir), 1).unwrap_err();
        assert!(matches!(error, StoreError::Locked { .. }));
        let message = error.to_string();
        assert!(message.contains("store.lock"), "{message}");
        assert!(message.contains("pid 99999 host elsewhere"), "{message}");
        assert!(message.contains("no writer is running"), "{message}");
        // Refusing neither removed the foreign lock nor wrote anything.
        assert!(store.join("store.lock").exists());
        assert!(!store.join("tmp").exists());
        assert_eq!(EvidenceStore::open(&store).unwrap().index().trees.len(), 2);

        // After the operator confirms and clears it, the add proceeds.
        fs::remove_file(store.join("store.lock")).unwrap();
        add_trees(&store, &extra_sources(&dir), 1).unwrap();
        assert_eq!(verify_store(&store).status, StoreVerifyStatus::Verified);
    }

    #[test]
    fn a_crashed_writers_leftovers_do_not_break_readers_or_verify() {
        let dir = TestDir::new();
        let store = packed(&dir);
        // What a crash mid-add leaves: the lock, half-written staging files.
        fs::write(store.join("store.lock"), "pid 4242 host crashed\n").unwrap();
        fs::create_dir(store.join("tmp")).unwrap();
        fs::write(store.join("tmp").join("half.part"), b"\xfd7zXZ\x00 partial").unwrap();
        fs::write(store.join("tmp").join("store.json.part"), b"{ \"schema_ver").unwrap();

        let opened = EvidenceStore::open(&store).unwrap();
        assert_eq!(opened.read_file("case", "empty.bin").unwrap(), b"");
        let report = verify_store(&store);
        assert_eq!(report.status, StoreVerifyStatus::Verified, "{report:?}");
        assert_eq!(report.finding_count, 0);
        assert_eq!(report.writer_state.len(), 2);
        assert!(report.writer_state[0].contains("pid 4242 host crashed"));
        assert!(report.writer_state[1].contains("tmp/"));
        // Unpack ignores them as well.
        let out = dir.path("out");
        opened.unpack(&out, &[]).unwrap();
        assert!(!out.join("tmp").exists());

        // Once the lock is cleared, add replaces stale staging names and
        // leaves the operator's other leftovers alone.
        fs::remove_file(store.join("store.lock")).unwrap();
        fs::write(store.join("tmp").join("store.json.part"), b"stale").unwrap();
        add_trees(&store, &extra_sources(&dir), 1).unwrap();
        assert_eq!(verify_store(&store).status, StoreVerifyStatus::Verified);
        assert!(store.join("tmp").join("half.part").exists());
    }

    #[test]
    fn the_index_is_replaced_by_rename_so_readers_see_old_or_new() {
        let dir = TestDir::new();
        let store = packed(&dir);
        let old_bytes = fs::read(store.join("store.json")).unwrap();
        // A reader that opened the store before the add keeps a consistent
        // view of the old index and can still read every file it names.
        let reader = EvidenceStore::open(&store).unwrap();

        #[cfg(unix)]
        let inode = |path: &Path| std::os::unix::fs::MetadataExt::ino(&fs::metadata(path).unwrap());
        #[cfg(unix)]
        let inode_before = inode(&store.join("store.json"));
        add_trees(&store, &extra_sources(&dir), 1).unwrap();
        #[cfg(unix)]
        assert_ne!(
            inode(&store.join("store.json")),
            inode_before,
            "the index is a new file renamed into place, not rewritten in place"
        );

        assert_eq!(reader.index().trees.len(), 2);
        assert_eq!(
            reader.read_file("case", "package.json").unwrap(),
            b"{\"package\":true}\n"
        );
        let new_bytes = fs::read(store.join("store.json")).unwrap();
        assert_ne!(new_bytes, old_bytes);
        // Both whole documents parse; there is no partial state in between.
        assert!(serde_json::from_slice::<StoreIndex>(&old_bytes).is_ok());
        assert!(serde_json::from_slice::<StoreIndex>(&new_bytes).is_ok());
        // The index never appears under a staging name at rest.
        assert!(!store.join("tmp").exists());
    }

    #[test]
    fn add_verifies_a_blob_that_already_exists_and_refuses_a_bad_one() {
        let dir = TestDir::new();
        let store = packed(&dir);
        // Corrupt the blob that the added tree would reuse.
        fs::write(
            blob_file(&store, &sha(b"shared content")),
            xz(b"not the shared content"),
        )
        .unwrap();
        let before = fs::read(store.join("store.json")).unwrap();
        let error = add_trees(&store, &extra_sources(&dir), 1).unwrap_err();
        assert!(
            matches!(&error, StoreError::Blob { detail, .. } if detail.contains("existing blob")),
            "{error}"
        );
        assert_eq!(fs::read(store.join("store.json")).unwrap(), before);
        // The blob the failed add wrote before reaching the bad one is gone,
        // so the store has no unreferenced blob it did not have already.
        let report = verify_store(&store);
        assert!(
            findings(&report)
                .iter()
                .all(|kind| *kind != "unreferenced_blob"),
            "{report:?}"
        );
        assert!(!store.join("store.lock").exists());
    }

    #[test]
    fn add_refuses_a_symlink_in_the_new_tree_and_leaves_the_store_valid() {
        #[cfg(unix)]
        {
            let dir = TestDir::new();
            let store = packed(&dir);
            let run = dir.path("src-link");
            write(&run.join("real.txt"), b"x");
            std::os::unix::fs::symlink(run.join("real.txt"), run.join("link.txt")).unwrap();
            let error = add_trees(&store, &[("linked".into(), run)], 1).unwrap_err();
            assert!(matches!(error, StoreError::InvalidInput(_)), "{error}");
            assert_eq!(verify_store(&store).status, StoreVerifyStatus::Verified);
            assert!(!store.join("store.lock").exists());
        }
    }

    #[test]
    fn add_to_something_that_is_not_a_store_is_an_error_and_takes_no_lock() {
        let dir = TestDir::new();
        let empty = dir.path("not-a-store");
        fs::create_dir_all(&empty).unwrap();
        assert!(add_trees(&empty, &extra_sources(&dir), 1).is_err());
        assert!(!empty.join("store.lock").exists());
        assert!(add_trees(&dir.path("absent"), &extra_sources(&dir), 1).is_err());
    }

    #[test]
    fn pack_stages_through_tmp_and_leaves_no_writer_state() {
        let dir = TestDir::new();
        let store = packed(&dir);
        assert!(!store.join("tmp").exists());
        assert!(!store.join("store.lock").exists());
        let report = verify_store(&store);
        assert_eq!(report.status, StoreVerifyStatus::Verified);
        assert!(report.writer_state.is_empty());
    }

    #[test]
    fn the_preset_is_a_writer_parameter_that_never_affects_identity() {
        let dir = TestDir::new();
        // Compressible content, so presets differ in stored size.
        let source = dir.path("src");
        let text: Vec<u8> = (0..4_000_u32)
            .flat_map(|i| format!("{:x} row {i}\n", Sha256::digest(i.to_le_bytes())).into_bytes())
            .collect();
        write(&source.join("big.txt"), &text);
        let trees = vec![("t".into(), source.clone())];
        let fast = dir.path("fast");
        let dense = dir.path("dense");
        let fast_report = super::pack_store(&fast, &trees, 0).unwrap();
        let dense_report = super::pack_store(&dense, &trees, DEFAULT_XZ_PRESET).unwrap();
        assert_eq!(DEFAULT_XZ_PRESET, 9);
        assert_ne!(dense_report.stored_bytes, fast_report.stored_bytes);
        let a = EvidenceStore::open(&fast).unwrap();
        let b = EvidenceStore::open(&dense).unwrap();
        assert_eq!(a.index(), b.index());
        assert_eq!(a.read_file("t", "big.txt").unwrap(), text);
        assert_eq!(b.read_file("t", "big.txt").unwrap(), text);
        // Mixed presets in one store verify; add accepts its own preset.
        write(&dir.path("more").join("also.txt"), &text);
        add_trees(&dense, &[("u".into(), dir.path("more"))], 6).unwrap();
        assert_eq!(verify_store(&dense).status, StoreVerifyStatus::Verified);
        // Out-of-range presets are refused before anything is written.
        assert!(matches!(
            super::pack_store(&dir.path("bad"), &trees, 10),
            Err(StoreError::InvalidPreset(10))
        ));
        assert!(!dir.path("bad").exists());
        assert!(matches!(
            add_trees(&dense, &[("v".into(), dir.path("more"))], 10),
            Err(StoreError::InvalidPreset(10))
        ));
    }

    #[test]
    fn writer_creates_an_empty_store_puts_blobs_and_adds_one_tree() {
        let dir = TestDir::new();
        let root = dir.path("made/run.store");
        let writer = StoreWriter::open_or_create(&root, 6).unwrap();
        let opened = EvidenceStore::open(&root).unwrap();
        assert!(opened.index().trees.is_empty());
        assert_eq!(verify_store(&root).status, StoreVerifyStatus::Verified);
        // Opening an existing store changes nothing.
        StoreWriter::open_or_create(&root, 6).unwrap();

        write(&dir.path("a/one.txt"), b"alpha");
        write(&dir.path("a/two.txt"), b"alpha");
        write(&dir.path("a/three.txt"), b"beta");
        let one = writer
            .put_file(&dir.path("a/one.txt"), "s/one.txt")
            .unwrap();
        let two = writer
            .put_file(&dir.path("a/two.txt"), "s/two.txt")
            .unwrap();
        let three = writer.put_file(&dir.path("a/three.txt"), "z.txt").unwrap();
        assert!(one.new && !two.new && three.new);
        assert_eq!(two.stored_bytes, 0);
        // Blobs alone change no tree.
        assert!(EvidenceStore::open(&root).unwrap().index().trees.is_empty());

        let one_sha = one.file.sha256.clone();
        let report = writer
            .add_tree("run.1", vec![three.file, two.file, one.file])
            .unwrap();
        assert_eq!(report.added_trees, ["run.1"]);
        assert_eq!((report.files, report.distinct_blobs), (3, 2));
        // The same name is adjusted deterministically.
        let again = writer
            .add_tree(
                "run.1",
                vec![writer.put_file(&dir.path("a/one.txt"), "x").unwrap().file],
            )
            .unwrap();
        assert_eq!(again.added_trees, ["run.1-2"]);
        let store = EvidenceStore::open(&root).unwrap();
        assert_eq!(store.read_file("run.1", "s/two.txt").unwrap(), b"alpha");
        assert_eq!(verify_store(&root).status, StoreVerifyStatus::Verified);
        assert!(!root.join("store.lock").exists());
        assert!(!root.join("tmp").exists());

        // A blob read by reference is verified and copied.
        let blob = writer.blob(&one_sha, 5);
        assert_eq!(blob.read().unwrap(), b"alpha");
        blob.copy_to(&dir.path("out.txt")).unwrap();
        assert_eq!(fs::read(dir.path("out.txt")).unwrap(), b"alpha");
        assert!(blob.copy_to(&dir.path("out.txt")).is_err());
    }

    #[test]
    fn adding_a_tree_refuses_a_missing_blob_and_a_corrupt_blob_is_not_copied() {
        let dir = TestDir::new();
        let root = dir.path("run.store");
        let writer = StoreWriter::open_or_create(&root, 1).unwrap();
        let ghost = StoreFile {
            path: "ghost.txt".into(),
            sha256: sha(b"never stored"),
            bytes: 12,
        };
        let error = writer.add_tree("t", vec![ghost]).unwrap_err();
        assert!(matches!(error, StoreError::Blob { .. }), "{error}");
        assert!(EvidenceStore::open(&root).unwrap().index().trees.is_empty());
        assert!(!root.join("store.lock").exists());

        write(&dir.path("f.txt"), b"content");
        let put = writer.put_file(&dir.path("f.txt"), "f.txt").unwrap();
        // Corrupt the stored blob: the copy fails and leaves no file behind.
        let blob_path = blob_file(&root, &put.file.sha256);
        fs::write(&blob_path, xz(b"other bytes")).unwrap();
        let blob = writer.blob(&put.file.sha256, put.file.bytes);
        assert!(blob.copy_to(&dir.path("copy.txt")).is_err());
        assert!(!dir.path("copy.txt").exists());
        // Putting the same content again does not paper over the damage.
        assert!(writer.put_file(&dir.path("f.txt"), "f.txt").is_err());
    }

    #[test]
    fn concurrent_writers_share_blobs_and_each_add_their_tree() {
        let dir = TestDir::new();
        let root = dir.path("shared.store");
        write(&dir.path("common.txt"), &vec![b'c'; 300_000]);
        let handles: Vec<_> = (0..6)
            .map(|n| {
                let root = root.clone();
                let common = dir.path("common.txt");
                let own = dir.path(&format!("own-{n}.txt"));
                write(&own, format!("own {n}").as_bytes());
                std::thread::spawn(move || {
                    let writer = StoreWriter::open_or_create(&root, 1).unwrap();
                    let a = writer.put_file(&common, "common.txt").unwrap().file;
                    let b = writer.put_file(&own, "own.txt").unwrap().file;
                    writer.add_tree(&format!("run-{n}"), vec![a, b]).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let store = EvidenceStore::open(&root).unwrap();
        assert_eq!(store.index().trees.len(), 6);
        assert_eq!(store.verify().status, StoreVerifyStatus::Verified);
        let report = store.verify();
        assert_eq!(
            (report.trees, report.files, report.distinct_blobs),
            (6, 12, 7)
        );
        assert!(report.writer_state.is_empty(), "{:?}", report.writer_state);
    }
}
