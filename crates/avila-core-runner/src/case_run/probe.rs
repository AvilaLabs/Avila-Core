//! Hash-only capability probing: helping an operator locate a local
//! executable that satisfies a package's pinned digest. A candidate is
//! never executed; probing only reports identities, and the run path
//! verifies the chosen bytes again at check or run time.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use avila_core_evidence::{PackageCapability, sha256_file};
use serde::Serialize;

use super::CapabilityCheckState;

/// Upper bound on files one scan directory contributes, so a huge or
/// unexpected directory cannot stall an interactive probe.
pub const SCAN_LIMIT: usize = 256;

/// One file probed against a declared capability's pinned executable
/// digest. The state vocabulary is the run path's: `verified` when the
/// file's bytes hash to the bound identity, `mismatch` when they hash
/// to something else, and `missing` when the path cannot be read as a
/// regular file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityCandidate {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub state: CapabilityCheckState,
}

/// Probe each candidate file against `declared`'s pinned executable
/// digest. Read-only: candidates are hashed, never executed. Paths are
/// canonicalized and de-duplicated; their first-seen order is kept.
/// A path that does not resolve or is not a regular file is reported
/// `missing` rather than dropped, so a named-but-wrong candidate stays
/// visible to its namer.
pub fn probe_capability(
    declared: &PackageCapability,
    candidates: &[PathBuf],
) -> Vec<CapabilityCandidate> {
    let mut seen = BTreeSet::new();
    let mut probed = Vec::new();
    for candidate in candidates {
        let path = match candidate.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                if seen.insert(candidate.clone()) {
                    probed.push(CapabilityCandidate {
                        path: candidate.clone(),
                        sha256: None,
                        state: CapabilityCheckState::Missing,
                    });
                }
                continue;
            }
        };
        if !seen.insert(path.clone()) {
            continue;
        }
        probed.push(CapabilityCandidate {
            path: path.clone(),
            ..probe_file(declared, &path)
        });
    }
    probed
}

fn probe_file(declared: &PackageCapability, path: &Path) -> CapabilityCandidate {
    let regular = path
        .metadata()
        .map(|metadata| metadata.is_file())
        .unwrap_or(false);
    let (sha256, state) = if !regular {
        (None, CapabilityCheckState::Missing)
    } else {
        match sha256_file(path) {
            Ok((digest, _)) if digest == declared.executable_sha256 => {
                (Some(digest), CapabilityCheckState::Verified)
            }
            Ok((digest, _)) => (Some(digest), CapabilityCheckState::Mismatch),
            Err(_) => (None, CapabilityCheckState::Missing),
        }
    };
    CapabilityCandidate {
        path: path.to_path_buf(),
        sha256,
        state,
    }
}

/// Regular files directly inside `dir`, canonicalized, de-duplicated,
/// and sorted for determinism, capped at `limit`. Subdirectories are
/// never entered and unreadable entries are skipped. A `dir` that is
/// missing or unreadable yields no candidates; the caller decides how
/// to report that.
pub fn scan_dir(dir: &Path, limit: usize) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = path.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if let Ok(path) = path.canonicalize() {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    files.truncate(limit);
    files
}

/// The prefixes `name` may appear under on a program search path:
/// itself and each progressively shorter `-`-separated prefix, so
/// `python3-numpy` also matches files named `python3` or `python3.14`.
/// An empty name, or a segment that shortens to empty, matches nothing.
fn name_prefixes(name: &str) -> Vec<&str> {
    let mut prefixes = Vec::new();
    let mut rest = name;
    while !rest.is_empty() {
        prefixes.push(rest);
        rest = match rest.rsplit_once('-') {
            Some((head, _)) => head,
            None => break,
        };
    }
    prefixes
}

/// Regular files under `dirs` whose file name begins with `name` or a
/// shortened `-`-separated prefix of it. Canonicalized, de-duplicated,
/// and sorted. Listing is read-only; whether a file satisfies a
/// capability is the probe's decision, not the listing's.
pub fn candidates_named(name: &str, dirs: &[PathBuf]) -> Vec<PathBuf> {
    let prefixes = name_prefixes(name);
    let mut files = BTreeSet::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if !prefixes.iter().any(|prefix| file_name.starts_with(prefix)) {
                continue;
            }
            let path = entry.path();
            let Ok(metadata) = path.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if let Ok(path) = path.canonicalize() {
                files.insert(path);
            }
        }
    }
    files.into_iter().collect()
}

/// The same name-prefix search over the directories of the process
/// PATH. Read-only; no candidate is ever executed by probing.
pub fn candidates_on_path(name: &str) -> Vec<PathBuf> {
    let dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    candidates_named(name, &dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn declared(digest: &str) -> PackageCapability {
        PackageCapability {
            capability_id: "stub".into(),
            package_id: "test/stub@1".into(),
            source_repository: None,
            source_commit: None,
            executable_sha256: digest.into(),
        }
    }

    fn digest_of(path: &Path) -> String {
        sha256_file(path).unwrap().0
    }

    fn tempdir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("avila-probe-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_matching_candidate_verifies_and_others_report_their_state() {
        let dir = tempdir("states");
        let good = dir.join("stub.exe");
        let wrong = dir.join("other.exe");
        let directory = dir.join("a-folder");
        fs::write(&good, b"the pinned executable").unwrap();
        fs::write(&wrong, b"some other program").unwrap();
        fs::create_dir(&directory).unwrap();
        let missing = dir.join("not-there.exe");
        let capability = declared(&digest_of(&good));
        let probed = probe_capability(
            &capability,
            &[good.clone(), wrong.clone(), missing.clone(), directory],
        );
        assert_eq!(probed.len(), 4);
        assert_eq!(probed[0].state, CapabilityCheckState::Verified);
        assert_eq!(
            probed[0].sha256.as_deref(),
            Some(capability.executable_sha256.as_str())
        );
        assert_eq!(probed[1].state, CapabilityCheckState::Mismatch);
        assert!(probed[1].sha256.is_some());
        assert_eq!(probed[2].state, CapabilityCheckState::Missing);
        assert_eq!(probed[2].path, missing);
        assert_eq!(probed[3].state, CapabilityCheckState::Missing);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_same_file_named_twice_is_probed_once() {
        let dir = tempdir("dedup");
        let file = dir.join("stub");
        fs::write(&file, b"bytes").unwrap();
        let capability = declared(&digest_of(&file));
        let relative = file.clone();
        let probed = probe_capability(&capability, &[relative, file.canonicalize().unwrap()]);
        assert_eq!(probed.len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_scan_lists_regular_files_sorted_and_bounded() {
        let dir = tempdir("scan");
        for name in ["b.exe", "a.exe", "c.exe"] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        fs::create_dir(dir.join("nested")).unwrap();
        fs::write(dir.join("nested").join("hidden.exe"), b"x").unwrap();
        let files = scan_dir(&dir, 256);
        assert_eq!(files.len(), 3);
        assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            files
                .iter()
                .all(|path| path.parent() == Some(dir.canonicalize().unwrap().as_path()))
        );
        assert_eq!(scan_dir(&dir, 2).len(), 2);
        assert!(
            scan_dir(&dir.join("nested"), 256)
                .iter()
                .all(|path| path.ends_with("hidden.exe"))
        );
        assert!(scan_dir(&dir.join("absent"), 256).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn name_candidates_match_shortened_prefixes_across_dirs() {
        let dir = tempdir("named");
        let bin = dir.join("bin");
        let sbin = dir.join("sbin");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&sbin).unwrap();
        for file in ["python3", "python3.14", "python3-config", "unrelated"] {
            fs::write(bin.join(file), b"x").unwrap();
        }
        fs::write(sbin.join("python3"), b"x").unwrap();
        fs::write(sbin.join("python3-numpy"), b"x").unwrap();
        let found = candidates_named("python3-numpy", &[bin.clone(), sbin.clone()]);
        let names: Vec<String> = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "python3",
                "python3-config",
                "python3.14",
                "python3",
                "python3-numpy"
            ]
        );
        assert_eq!(found.iter().filter(|p| p.ends_with("python3")).count(), 2);
        let exact = candidates_named("unrelated", &[bin.clone(), sbin]);
        assert_eq!(exact.len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_empty_name_matches_nothing_and_no_dirs_no_candidates() {
        let dir = tempdir("empty");
        fs::write(dir.join("anything"), b"x").unwrap();
        assert!(candidates_named("", std::slice::from_ref(&dir)).is_empty());
        assert!(candidates_named("-anything", std::slice::from_ref(&dir)).is_empty());
        assert!(candidates_named("anything", &[]).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
