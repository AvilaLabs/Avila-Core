//! Read-only evidence roots (ADR-0028, Amendment 1, A3).
//!
//! A root is where a case package, a source root or a workspace is read from:
//! a directory, or one tree of an evidence store written as
//! `store:<STORE_DIR>#<TREE>`. Commands that read or verify evidence take the
//! same text in either form, so verification of a package, a receipt or a
//! campaign gives the same answer from a store tree as from the same files on
//! disk.
//!
//! Directory roots keep every check they had: the path is confined beneath
//! the canonical root, symlinks that escape are refused, and the target must
//! be a regular file. Store roots need no confinement checks because index
//! paths cannot name anything outside the tree; a path that is not in the
//! index is absent, and a file is returned only through the verifying reader
//! (bounded decompression, length and SHA-256 checked), so unverified bytes
//! never reach a caller. A blob that fails verification is reported as
//! `Fetched::Corrupt`, which callers treat as a digest mismatch, never as
//! content.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::package::{
    PackageError, canonical_directory, hash_confined_file, resolve_confined, validate_relative_path,
};
use crate::store::{EvidenceStore, StoreError};

/// The prefix that marks a store-tree root.
pub const STORE_ROOT_PREFIX: &str = "store:";

/// Verified file bytes up to this size are kept for repeated reads within one
/// root; larger files are decoded again each time.
const MEMO_FILE_LIMIT: u64 = 8 * 1024 * 1024;
/// The memo never holds more than this many bytes.
const MEMO_TOTAL_LIMIT: usize = 64 * 1024 * 1024;

/// The outcome of fetching one file from a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched<T> {
    Found(T),
    /// The root has no such file.
    Missing,
    /// The root lists the file but its bytes cannot be produced verified; the
    /// text says why. Only store roots produce this.
    Corrupt(String),
}

/// Split `store:<STORE_DIR>#<TREE>` into its parts. `None` when the text is
/// not a store address at all; an error when it is one but malformed.
pub fn parse_store_address(text: &str) -> Option<Result<(PathBuf, String), String>> {
    let rest = text.strip_prefix(STORE_ROOT_PREFIX)?;
    Some(match rest.rsplit_once('#') {
        Some((store, tree)) if !store.is_empty() && !tree.is_empty() => {
            Ok((PathBuf::from(store), tree.to_owned()))
        }
        _ => Err(format!(
            "`{text}` is not a store root; write `store:<STORE_DIR>#<TREE>`"
        )),
    })
}

/// Whether a path argument names a store tree rather than a directory.
pub fn is_store_address(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|text| text.starts_with(STORE_ROOT_PREFIX))
}

/// Read a case package's manifest, given a case argument: a directory
/// holding `package.json`, the manifest file itself, or a store tree
/// (`store:<STORE_DIR>#<TREE>`, whose `package.json` is read verified).
/// Returns the manifest bytes and the package root to hand to
/// [`crate::verify_case_package`]. I/O errors are the file system's own.
pub fn read_case_manifest(case_or_manifest: &Path) -> io::Result<(Vec<u8>, PathBuf)> {
    if is_store_address(case_or_manifest) {
        let root = EvidenceRoot::open(case_or_manifest).map_err(io::Error::other)?;
        return match root.fetch_bytes("package.json").map_err(io::Error::other)? {
            Fetched::Found(bytes) => Ok((bytes, case_or_manifest.to_path_buf())),
            Fetched::Missing => Err(io::Error::other(format!(
                "{} has no package.json",
                root.display()
            ))),
            Fetched::Corrupt(detail) => Err(io::Error::other(detail)),
        };
    }
    let manifest_path = if case_or_manifest.is_dir() {
        case_or_manifest.join("package.json")
    } else {
        case_or_manifest.to_path_buf()
    };
    let bytes = fs::read(&manifest_path)?;
    let root = manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    Ok((bytes, root))
}

/// One resolved file location inside a root, ready to be hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    /// A confined regular file on disk.
    File(PathBuf),
    /// A store-tree path (always present in the index).
    Stored(String),
}

#[derive(Default)]
struct Memo {
    verified: HashSet<String>,
    bytes: HashMap<String, Arc<Vec<u8>>>,
    bytes_total: usize,
}

/// A store tree opened for reading.
pub struct StoreRoot {
    store: EvidenceStore,
    tree: String,
    /// A directory inside the tree this root is anchored at, as a store path
    /// prefix ending in `/`, or empty for the whole tree.
    prefix: String,
    label: String,
    memo: Mutex<Memo>,
}

/// A read-only evidence root: a directory or a store tree.
pub enum EvidenceRoot {
    Directory(PathBuf),
    Store(Box<StoreRoot>),
}

fn store_error(label: &str, error: impl std::fmt::Display) -> PackageError {
    PackageError::Store(format!("{label}: {error}"))
}

impl EvidenceRoot {
    /// Open a root from a command-line path: `store:<STORE_DIR>#<TREE>`, or a
    /// directory (resolved to its canonical form).
    pub fn open(path: &Path) -> Result<Self, PackageError> {
        let Some(text) = path.to_str() else {
            return canonical_directory(path).map(Self::Directory);
        };
        let Some(parsed) = parse_store_address(text) else {
            return canonical_directory(path).map(Self::Directory);
        };
        let (store_dir, address) = parsed.map_err(PackageError::Store)?;
        // `TREE/DIR` anchors the root at a directory inside the tree, so one
        // step of a stored run is a workspace of its own. Tree names cannot
        // contain `/`, which keeps the split unambiguous.
        let (tree, directory) = match address.split_once('/') {
            Some((tree, directory)) => (tree.to_owned(), Some(directory.to_owned())),
            None => (address.clone(), None),
        };
        let label = format!("store `{}` tree `{address}`", store_dir.display());
        let store = EvidenceStore::open(&store_dir).map_err(|error| store_error(&label, error))?;
        let Some(stored) = store.tree(&tree) else {
            return Err(store_error(&label, "the store has no such tree"));
        };
        let prefix = match directory {
            Some(directory) => {
                let prefix = format!(
                    "{}/",
                    StoreRoot::relative_text(directory.trim_end_matches('/'))?
                );
                if !stored
                    .files
                    .iter()
                    .any(|file| file.path.starts_with(&prefix))
                {
                    return Err(store_error(&label, "the tree has no such directory"));
                }
                prefix
            }
            None => String::new(),
        };
        Ok(Self::Store(Box::new(StoreRoot {
            store,
            tree,
            prefix,
            label,
            memo: Mutex::new(Memo::default()),
        })))
    }

    /// The root as an operator wrote it: the canonical directory, or the
    /// store address.
    pub fn display(&self) -> String {
        match self {
            Self::Directory(path) => path.display().to_string(),
            Self::Store(root) => format!(
                "{STORE_ROOT_PREFIX}{}#{}{}{}",
                root.store.root().display(),
                root.tree,
                if root.prefix.is_empty() { "" } else { "/" },
                root.prefix.trim_end_matches('/')
            ),
        }
    }

    /// The directory behind this root, if it is one. The hash cache and
    /// anything that needs real files apply to directories only.
    pub fn directory(&self) -> Option<&Path> {
        match self {
            Self::Directory(path) => Some(path),
            Self::Store(_) => None,
        }
    }

    /// Resolve `relative` to a file without reading it. `Ok(None)` when it is
    /// absent.
    pub fn locate(&self, relative: &str) -> Result<Option<Located>, PackageError> {
        match self {
            Self::Directory(root) => Ok(resolve_confined(root, relative)?.map(Located::File)),
            Self::Store(root) => root.locate(relative),
        }
    }

    /// Read a whole file. Directory files are read as before; store files only
    /// after their length and digest verify.
    pub fn fetch_bytes(&self, relative: &str) -> Result<Fetched<Vec<u8>>, PackageError> {
        match self {
            Self::Directory(root) => {
                let Some(canonical) = resolve_confined(root, relative)? else {
                    return Ok(Fetched::Missing);
                };
                fs::read(&canonical)
                    .map(Fetched::Found)
                    .map_err(|source| PackageError::Io {
                        path: canonical.display().to_string(),
                        source,
                    })
            }
            Self::Store(root) => root.fetch_bytes(relative),
        }
    }

    /// The `sha256:`-prefixed digest and length of a file. A store file is
    /// decoded and checked first, so the digest is of verified bytes.
    pub fn fetch_hash(&self, relative: &str) -> Result<Fetched<(String, u64)>, PackageError> {
        match self {
            Self::Directory(root) => Ok(match hash_confined_file(root, relative)? {
                Some(found) => Fetched::Found(found),
                None => Fetched::Missing,
            }),
            Self::Store(root) => root.fetch_hash(relative),
        }
    }

    /// Hash an already-located file.
    pub(crate) fn hash_located(
        &self,
        located: &Located,
    ) -> Result<Fetched<(String, u64)>, PackageError> {
        match (self, located) {
            (_, Located::File(path)) => crate::package::hash_file(path).map(Fetched::Found),
            (Self::Store(root), Located::Stored(text)) => root.hash_stored(text),
            (Self::Directory(_), Located::Stored(_)) => Ok(Fetched::Missing),
        }
    }

    /// Copy a file to `destination` (which must not exist), returning the
    /// bytes written only after they verified. A store file is streamed
    /// through the verifying reader to a staging name beside the destination
    /// and renamed into place on success, so a failed read leaves nothing.
    pub fn copy_to(
        &self,
        relative: &str,
        destination: &Path,
    ) -> Result<Fetched<u64>, PackageError> {
        match self {
            Self::Directory(root) => {
                let Some(source) = resolve_confined(root, relative)? else {
                    return Ok(Fetched::Missing);
                };
                fs::copy(&source, destination)
                    .map(Fetched::Found)
                    .map_err(|error| PackageError::Io {
                        path: format!("{} -> {}", source.display(), destination.display()),
                        source: error,
                    })
            }
            Self::Store(root) => root.copy_to(relative, destination),
        }
    }
}

impl StoreRoot {
    /// A relative path as a store path, normalized (no prefix applied).
    fn relative_text(relative: &str) -> Result<String, PackageError> {
        let path = validate_relative_path(relative)?;
        Ok(path
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"))
    }

    fn locate(&self, relative: &str) -> Result<Option<Located>, PackageError> {
        let text = format!("{}{}", self.prefix, Self::relative_text(relative)?);
        if self.store.entry(&self.tree, &text).is_some() {
            return Ok(Some(Located::Stored(text)));
        }
        // A directory prefix is present but is not a regular file, exactly as
        // a directory is on disk.
        let tree = self
            .store
            .tree(&self.tree)
            .expect("the tree was checked when the root was opened");
        let prefix = format!("{text}/");
        let position = tree
            .files
            .partition_point(|file| file.path.as_bytes() < prefix.as_bytes());
        if tree
            .files
            .get(position)
            .is_some_and(|file| file.path.starts_with(&prefix))
        {
            return Err(PackageError::InvalidManifest(format!(
                "package entry `{}` in {} is not a regular file",
                text, self.label
            )));
        }
        Ok(None)
    }

    /// Map a failed verified read: an absent blob file is a missing file, any
    /// other failure is corruption.
    fn failure<T>(&self, relative: &str, error: StoreError) -> Result<Fetched<T>, PackageError> {
        match error {
            StoreError::Blob { detail, .. } if detail.contains("is missing") => {
                Ok(Fetched::Missing)
            }
            StoreError::Blob { detail, .. } => Ok(Fetched::Corrupt(format!(
                "{}: `{relative}`: {detail}",
                self.label
            ))),
            StoreError::Io { path, source } => Ok(Fetched::Corrupt(format!(
                "{}: `{relative}`: cannot read `{path}`: {source}",
                self.label
            ))),
            other => Err(store_error(&self.label, other)),
        }
    }

    fn fetch_bytes(&self, relative: &str) -> Result<Fetched<Vec<u8>>, PackageError> {
        let Some(Located::Stored(text)) = self.locate(relative)? else {
            return Ok(Fetched::Missing);
        };
        let entry = self
            .store
            .entry(&self.tree, &text)
            .expect("a located store path is in the index");
        if let Some(cached) = self
            .memo
            .lock()
            .expect("memo lock")
            .bytes
            .get(&entry.sha256)
        {
            return Ok(Fetched::Found(cached.as_ref().clone()));
        }
        match self.store.read_file(&self.tree, &text) {
            Ok(bytes) => {
                let mut memo = self.memo.lock().expect("memo lock");
                memo.verified.insert(entry.sha256.clone());
                if entry.bytes <= MEMO_FILE_LIMIT
                    && memo.bytes_total + bytes.len() <= MEMO_TOTAL_LIMIT
                {
                    memo.bytes_total += bytes.len();
                    memo.bytes
                        .insert(entry.sha256.clone(), Arc::new(bytes.clone()));
                }
                Ok(Fetched::Found(bytes))
            }
            Err(error) => self.failure(&text, error),
        }
    }

    fn fetch_hash(&self, relative: &str) -> Result<Fetched<(String, u64)>, PackageError> {
        let Some(Located::Stored(text)) = self.locate(relative)? else {
            return Ok(Fetched::Missing);
        };
        self.hash_stored(&text)
    }

    /// Hash a store path already located in this root (prefix included).
    fn hash_stored(&self, text: &str) -> Result<Fetched<(String, u64)>, PackageError> {
        let text = text.to_owned();
        let entry = self
            .store
            .entry(&self.tree, &text)
            .expect("a located store path is in the index");
        let known = self
            .memo
            .lock()
            .expect("memo lock")
            .verified
            .contains(&entry.sha256);
        if !known {
            if let Err(error) = self.store.verify_file(&self.tree, &text) {
                return self.failure(&text, error);
            }
            self.memo
                .lock()
                .expect("memo lock")
                .verified
                .insert(entry.sha256.clone());
        }
        Ok(Fetched::Found((
            format!("sha256:{}", entry.sha256),
            entry.bytes,
        )))
    }

    fn copy_to(&self, relative: &str, destination: &Path) -> Result<Fetched<u64>, PackageError> {
        let Some(Located::Stored(text)) = self.locate(relative)? else {
            return Ok(Fetched::Missing);
        };
        let io_error = |path: &Path, source: io::Error| PackageError::Io {
            path: path.display().to_string(),
            source,
        };
        let mut staging = destination.as_os_str().to_owned();
        staging.push(".part");
        let staging = PathBuf::from(staging);
        let outcome = (|| -> Result<Fetched<u64>, PackageError> {
            let mut reader = match self.store.open_file(&self.tree, &text) {
                Ok(reader) => reader,
                Err(error) => return self.failure(&text, error),
            };
            let mut file = fs::File::options()
                .write(true)
                .create_new(true)
                .open(&staging)
                .map_err(|error| io_error(&staging, error))?;
            let copied = match io::copy(&mut reader, &mut file) {
                Ok(copied) => copied,
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    return Ok(Fetched::Corrupt(format!(
                        "{}: `{text}`: {error}",
                        self.label
                    )));
                }
                Err(error) => return Err(io_error(&staging, error)),
            };
            file.flush().map_err(|error| io_error(&staging, error))?;
            fs::rename(&staging, destination).map_err(|error| io_error(destination, error))?;
            Ok(Fetched::Found(copied))
        })();
        if !matches!(outcome, Ok(Fetched::Found(_))) {
            let _ = fs::remove_file(&staging);
        }
        outcome
    }
}

/// Source roots as execution needs them: real files. A directory root
/// resolves in place; a store root's file is first copied, verified, into a
/// scratch directory (and only if it is read), which is removed when this
/// value is dropped. Verification of the same roots needs no scratch: it goes
/// through [`EvidenceRoot`].
pub struct StagedRoots {
    roots: std::collections::BTreeMap<String, EvidenceRoot>,
    scratch: PathBuf,
}

static NEXT_SCRATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl StagedRoots {
    /// Open each supplied root. A root that does not open is left out, so
    /// files under it do not resolve. `scratch_base` is the directory under
    /// which store files are staged; pick one on the filesystem the run
    /// writes to.
    pub fn new(
        source_roots: &std::collections::BTreeMap<String, PathBuf>,
        scratch_base: &Path,
    ) -> Self {
        let roots = source_roots
            .iter()
            .filter_map(|(name, path)| {
                EvidenceRoot::open(path)
                    .ok()
                    .map(|root| (name.clone(), root))
            })
            .collect();
        let sequence = NEXT_SCRATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            roots,
            scratch: scratch_base.join(format!(
                ".avila-core-stage-{}-{sequence}",
                std::process::id()
            )),
        }
    }

    /// Whether the named root resolved.
    pub fn contains(&self, root: &str) -> bool {
        self.roots.contains_key(root)
    }

    /// The path of `relative` under the named root as a real file, staging a
    /// store file first. `None` when the root is unknown or a store file
    /// cannot be produced verified. The file is not required to exist for a
    /// directory root, exactly as before.
    pub fn file_path(&self, root: &str, relative: &str) -> Option<PathBuf> {
        match self.roots.get(root)? {
            EvidenceRoot::Directory(path) => Some(path.join(relative)),
            store => {
                let relative_path = validate_relative_path(relative).ok()?;
                let destination = self.scratch.join(root).join(relative_path);
                if destination.exists() {
                    return Some(destination);
                }
                fs::create_dir_all(destination.parent()?).ok()?;
                match store.copy_to(relative, &destination) {
                    Ok(Fetched::Found(_)) => Some(destination),
                    _ => None,
                }
            }
        }
    }
}

impl Drop for StagedRoots {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.scratch);
    }
}

impl std::fmt::Debug for EvidenceRoot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("EvidenceRoot")
            .field(&self.display())
            .finish()
    }
}

/// Helpers shared by the tests of the modules that read through a root.
#[cfg(test)]
pub(crate) mod testing {
    use std::io::Write as _;

    use super::*;
    use crate::store::pack_store;

    /// Pack directories into a new store (a fast preset: tests do not need
    /// dense archives) and return the `store:` address of one tree.
    pub(crate) fn pack(store: &Path, trees: &[(&str, &Path)]) {
        let trees: Vec<(String, PathBuf)> = trees
            .iter()
            .map(|(name, dir)| ((*name).to_owned(), dir.to_path_buf()))
            .collect();
        pack_store(store, &trees, 1).unwrap();
    }

    pub(crate) fn address(store: &Path, tree: &str) -> PathBuf {
        PathBuf::from(format!("{STORE_ROOT_PREFIX}{}#{tree}", store.display()))
    }

    /// Replace a file's blob with a valid xz stream of different bytes, as a
    /// tamperer who also fixes up the container would.
    pub(crate) fn tamper(store: &Path, content: &[u8], replacement: &[u8]) {
        let sha = crate::sha256_hex(content);
        let blob = store
            .join("blobs")
            .join(&sha[..2])
            .join(format!("{sha}.xz"));
        let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 1);
        encoder.write_all(replacement).unwrap();
        fs::write(blob, encoder.finish().unwrap()).unwrap();
    }

    pub(crate) fn blob_path(store: &Path, content: &[u8]) -> PathBuf {
        let sha = crate::sha256_hex(content);
        store
            .join("blobs")
            .join(&sha[..2])
            .join(format!("{sha}.xz"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_addresses_parse() {
        assert!(parse_store_address("/some/dir").is_none());
        assert_eq!(
            parse_store_address("store:/a/b.store#case")
                .unwrap()
                .unwrap(),
            (PathBuf::from("/a/b.store"), "case".to_owned())
        );
        // The tree is what follows the last `#`.
        assert_eq!(
            parse_store_address("store:/a#b/s#t").unwrap().unwrap(),
            (PathBuf::from("/a#b/s"), "t".to_owned())
        );
        for bad in ["store:", "store:/a", "store:#t", "store:/a#"] {
            assert!(parse_store_address(bad).unwrap().is_err(), "{bad}");
        }
        assert!(is_store_address(Path::new("store:/a#t")));
        assert!(!is_store_address(Path::new("./store:/a#t")));
    }

    #[test]
    fn a_tree_directory_is_a_root_of_its_own() {
        let base = std::env::temp_dir().join(format!("avila-core-subroot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let work = base.join("work");
        fs::create_dir_all(work.join("step-a/inputs")).unwrap();
        fs::write(work.join("step-a/receipt.json"), b"receipt").unwrap();
        fs::write(work.join("step-a/inputs/x.json"), b"input").unwrap();
        fs::write(work.join("top.json"), b"top").unwrap();
        let store = base.join("s.store");
        testing::pack(&store, &[("run", work.as_path())]);

        let whole = EvidenceRoot::open(&testing::address(&store, "run")).unwrap();
        assert!(matches!(
            whole.fetch_bytes("top.json"),
            Ok(Fetched::Found(_))
        ));
        assert_eq!(whole.fetch_bytes("receipt.json").unwrap(), Fetched::Missing);

        let step = EvidenceRoot::open(&testing::address(&store, "run/step-a")).unwrap();
        assert_eq!(
            step.fetch_bytes("receipt.json").unwrap(),
            Fetched::Found(b"receipt".to_vec())
        );
        assert_eq!(
            step.fetch_bytes("inputs/x.json").unwrap(),
            Fetched::Found(b"input".to_vec())
        );
        assert_eq!(step.fetch_bytes("top.json").unwrap(), Fetched::Missing);
        assert!(matches!(
            step.fetch_hash("inputs/x.json"),
            Ok(Fetched::Found((_, 5)))
        ));
        assert!(
            step.display().ends_with("#run/step-a"),
            "{}",
            step.display()
        );
        assert!(EvidenceRoot::open(&testing::address(&store, "run/nope")).is_err());
        assert!(EvidenceRoot::open(&testing::address(&store, "run/step-a/../x")).is_err());
        let _ = fs::remove_dir_all(&base);
    }
}
