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

/// Writers use xz preset 9; readers accept any valid xz stream.
const XZ_PRESET: u32 = 9;
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
        let blobs = self.root.join(BLOBS_DIR);
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
        Ok(self.root.join(blob_relative(sha256)))
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
        }
    }

    /// The store directory contains exactly `store.json` and `blobs/`, and
    /// `blobs/` holds exactly the referenced `<h0h1>/<h>.xz` regular files.
    fn check_layout(&self, referenced: &BTreeMap<&str, u64>, findings: &mut Collector) {
        let top = match read_dir_names(&self.root) {
            Ok(names) => names,
            Err(error) => {
                findings.push("layout", PathBuf::new(), error.to_string());
                return;
            }
        };
        for name in &top {
            if name != INDEX_FILE && name != BLOBS_DIR {
                findings.push(
                    "unexpected_entry",
                    PathBuf::from(name),
                    "only store.json and blobs/ may appear in a store".into(),
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
        },
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

fn write_blob(source: &Path, entry: &StoreFile, destination: &Path) -> Result<u64, StoreError> {
    let mut input = File::open(source).map_err(|error| io_error(source, error))?;
    let output = File::create_new(destination).map_err(|error| io_error(destination, error))?;
    let mut encoder = XzEncoder::new(BufWriter::new(output), XZ_PRESET);
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
    let stored = fs::metadata(destination)
        .map_err(|error| io_error(destination, error))?
        .len();
    Ok(stored)
}

/// Create a new store at `out` from named directory trees. `trees` pairs a
/// tree name with its source directory; their order does not matter.
/// Blobs are written first and `store.json` last, so a directory without an
/// index is an unfinished store, never a valid one. On failure the output
/// directory is removed.
pub fn pack_store(out: &Path, trees: &[(String, PathBuf)]) -> Result<StorePackReport, StoreError> {
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
    if fs::symlink_metadata(out).is_ok() {
        return Err(StoreError::AlreadyExists(out.display().to_string()));
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
    let index = StoreIndex {
        schema_version: EVIDENCE_STORE_SCHEMA_VERSION.into(),
        codec: EVIDENCE_STORE_CODEC.into(),
        trees: index_trees,
    };
    validate_index(&index).map_err(|error| StoreError::InvalidInput(error.to_string()))?;

    create_new_directory(out)?;
    match write_store(out, &index, &scanned) {
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
) -> Result<StorePackReport, StoreError> {
    let blobs = out.join(BLOBS_DIR);
    fs::create_dir(&blobs).map_err(|error| io_error(&blobs, error))?;
    let mut done: BTreeSet<&str> = BTreeSet::new();
    let mut files = 0_usize;
    let mut uncompressed_bytes = 0_u64;
    let mut distinct_bytes = 0_u64;
    let mut stored_bytes = 0_u64;
    for file in scanned.iter().flatten() {
        files += 1;
        uncompressed_bytes += file.entry.bytes;
        if !done.insert(&file.entry.sha256) {
            continue;
        }
        let relative = blob_relative(&file.entry.sha256);
        let destination = out.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
        }
        stored_bytes += write_blob(&file.source, &file.entry, &destination)?;
        distinct_bytes += file.entry.bytes;
    }
    let index_path = out.join(INDEX_FILE);
    let mut index_file =
        File::create_new(&index_path).map_err(|error| io_error(&index_path, error))?;
    index_file
        .write_all(&serialize_index(index)?)
        .and_then(|()| index_file.sync_all())
        .map_err(|error| io_error(&index_path, error))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

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
}
