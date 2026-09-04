//! An opt-in, operator-owned cache of previously verified artifact digests.
//!
//! This cache exists to avoid re-hashing large, unchanging operator-supplied
//! artifacts (bulk nuclear-data libraries, vendored release binaries) on
//! every `avila-core run` invocation. It is off unless an operator names a
//! cache file with `--hash-cache PATH`, and it never applies to package
//! documents (always small, always re-hashed) or to any artifact resolved
//! from inside the case package directory itself.
//!
//! **Trust statement.** A cache entry is keyed by a file's canonical
//! absolute path plus a stamp of its size, modification time (nanoseconds),
//! and device/inode where the platform exposes them. A "hit" means the
//! stamp on disk still equals the stamp recorded when the file was last
//! hashed; Core then reuses the recorded digest instead of reading the
//! bytes again. This is an assumption, not a defence: an actor with write
//! access to the artifact root who can reproduce the original size and
//! modification time defeats it, because nothing about the cache inspects
//! file content on a hit. It trusts that operator-owned artifact roots are
//! not modified while preserving size and mtime; it is not a substitute for
//! the requester's manifest pin (S-030), which is unaffected by this cache
//! and still anchors the package identity a campaign is allowed to
//! evaluate. See `SECURITY.md` and
//! `docs/architecture/CAPABILITY_PROTOCOL.md`.
//!
//! A cache hit is reported as the distinct `verified_cached` integrity
//! state, never as plain `verified`: the report must say when an identity
//! came from a prior measurement rather than from these bytes just now.
//! Whatever the cache says, the digest is still compared against the
//! manifest's bound identity exactly as an uncached digest would be, so a
//! stale or malicious cache entry that disagrees with the manifest still
//! fails closed.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const HASH_CACHE_SCHEMA_VERSION: &str = "avila.core/hash-cache/v1-draft";

/// Everything about a file's identity this cache reads without hashing its
/// bytes. Two equal stamps are treated as the same bytes; this equality is
/// the cache's entire trust assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStamp {
    pub size: u64,
    /// Nanoseconds since the Unix epoch. `u64` (not the `u128` a duration
    /// natively yields) because `serde_json` cannot represent a 128-bit
    /// integer; a `u64` nanosecond count does not overflow until the year
    /// 2554, which is not this cache's problem.
    pub mtime_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ino: Option<u64>,
}

impl FileStamp {
    pub fn read(metadata: &fs::Metadata) -> io::Result<Self> {
        let modified = metadata.modified()?;
        let mtime_ns = modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        Ok(Self {
            size: metadata.len(),
            mtime_ns,
            dev: platform_dev(metadata),
            ino: platform_ino(metadata),
        })
    }
}

#[cfg(unix)]
fn platform_dev(metadata: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt as _;
    Some(metadata.dev())
}

#[cfg(unix)]
fn platform_ino(metadata: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt as _;
    Some(metadata.ino())
}

#[cfg(not(unix))]
fn platform_dev(_metadata: &fs::Metadata) -> Option<u64> {
    None
}

#[cfg(not(unix))]
fn platform_ino(_metadata: &fs::Metadata) -> Option<u64> {
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashCacheEntry {
    #[serde(flatten)]
    pub stamp: FileStamp,
    pub sha256: String,
    /// When this entry was last derived by hashing bytes, RFC 3339. Never
    /// used to decide a hit; recorded only so an operator can inspect the
    /// cache's age.
    pub verified_at: String,
}

/// The on-disk cache: one entry per canonical absolute artifact path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashCache {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    #[serde(default)]
    pub entries: BTreeMap<String, HashCacheEntry>,
}

fn schema_version() -> String {
    HASH_CACHE_SCHEMA_VERSION.into()
}

impl Default for HashCache {
    fn default() -> Self {
        Self::new()
    }
}

impl HashCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema_version: HASH_CACHE_SCHEMA_VERSION.into(),
            entries: BTreeMap::new(),
        }
    }

    /// The cached digest for this exact path and stamp, or `None` on a miss
    /// (an unknown path, or a path whose recorded stamp differs).
    #[must_use]
    pub fn hit(&self, canonical_path: &str, stamp: &FileStamp) -> Option<&str> {
        self.entries
            .get(canonical_path)
            .filter(|entry| &entry.stamp == stamp)
            .map(|entry| entry.sha256.as_str())
    }

    /// Record (or overwrite) the entry for a path after hashing it fresh.
    pub fn record(
        &mut self,
        canonical_path: String,
        stamp: FileStamp,
        sha256: String,
        verified_at: String,
    ) {
        self.entries.insert(
            canonical_path,
            HashCacheEntry {
                stamp,
                sha256,
                verified_at,
            },
        );
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HashCacheError {
    #[error("cannot read hash cache `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("hash cache `{path}` is not valid JSON: {source}")]
    Invalid {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "hash cache `{path}` has unsupported schema_version `{found}`; expected `{HASH_CACHE_SCHEMA_VERSION}`"
    )]
    UnsupportedSchema { path: String, found: String },
}

/// Load a hash cache file. A missing file is a cold cache, not an error. Any
/// other read or parse failure is returned rather than silently treated as
/// empty, so the caller can decide how to report it; this module never
/// itself decides that a corrupt cache is invisible.
pub fn load_hash_cache(path: &Path) -> Result<HashCache, HashCacheError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HashCache::new()),
        Err(source) => {
            return Err(HashCacheError::Io {
                path: path.display().to_string(),
                source,
            });
        }
    };
    let cache: HashCache =
        serde_json::from_slice(&bytes).map_err(|source| HashCacheError::Invalid {
            path: path.display().to_string(),
            source,
        })?;
    if cache.schema_version != HASH_CACHE_SCHEMA_VERSION {
        return Err(HashCacheError::UnsupportedSchema {
            path: path.display().to_string(),
            found: cache.schema_version,
        });
    }
    Ok(cache)
}

/// Write the cache atomically: a temporary file in the same directory, then
/// a rename, so a crash mid-write never leaves a half-written cache file.
pub fn save_hash_cache(path: &Path, cache: &HashCache) -> io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(cache).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("hash-cache.json");
    let temp_path = path.with_file_name(format!("{file_name}.tmp-{}", std::process::id()));
    fs::write(&temp_path, &bytes)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestDir(std::path::PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "avila-core-hash-cache-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_file_is_a_cold_empty_cache() {
        let dir = TestDir::new();
        let cache = load_hash_cache(&dir.0.join("hash-cache.json")).unwrap();
        assert!(cache.entries.is_empty());
        assert_eq!(cache.schema_version, HASH_CACHE_SCHEMA_VERSION);
    }

    #[test]
    fn corrupt_file_is_reported_not_silently_emptied() {
        let dir = TestDir::new();
        let path = dir.0.join("hash-cache.json");
        fs::write(&path, b"{ not json").unwrap();
        assert!(matches!(
            load_hash_cache(&path),
            Err(HashCacheError::Invalid { .. })
        ));
    }

    #[test]
    fn wrong_schema_version_is_reported() {
        let dir = TestDir::new();
        let path = dir.0.join("hash-cache.json");
        fs::write(&path, br#"{"schema_version":"other","entries":{}}"#).unwrap();
        assert!(matches!(
            load_hash_cache(&path),
            Err(HashCacheError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn save_then_load_round_trips_atomically() {
        let dir = TestDir::new();
        let path = dir.0.join("nested").join("hash-cache.json");
        let mut cache = HashCache::new();
        cache.record(
            "/abs/path".into(),
            FileStamp {
                size: 10,
                mtime_ns: 1,
                dev: Some(1),
                ino: Some(2),
            },
            "sha256:abc".into(),
            "2026-09-04T00:00:00Z".into(),
        );
        save_hash_cache(&path, &cache).unwrap();
        let loaded = load_hash_cache(&path).unwrap();
        assert_eq!(loaded, cache);
        // No leftover temp file.
        let siblings: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(siblings, vec![path.file_name().unwrap().to_owned()]);
    }

    #[test]
    fn hit_requires_the_full_stamp_to_match() {
        let mut cache = HashCache::new();
        let stamp = FileStamp {
            size: 10,
            mtime_ns: 1,
            dev: Some(1),
            ino: Some(2),
        };
        cache.record(
            "/abs/path".into(),
            stamp,
            "sha256:abc".into(),
            "2026-09-04T00:00:00Z".into(),
        );
        assert_eq!(cache.hit("/abs/path", &stamp), Some("sha256:abc"));
        assert_eq!(cache.hit("/abs/other", &stamp), None);
        let different_mtime = FileStamp {
            mtime_ns: 2,
            ..stamp
        };
        assert_eq!(cache.hit("/abs/path", &different_mtime), None);
        let different_size = FileStamp { size: 11, ..stamp };
        assert_eq!(cache.hit("/abs/path", &different_size), None);
    }
}
