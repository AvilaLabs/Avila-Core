//! Cases read from store trees give the same results as the same files on
//! disk (ADR-0028, Amendment 1, A3). Each test packs a bundled example case
//! and its source roots into a store, then compares complete reports.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_evidence::pack_store;
use avila_core_runner::{CaseRunOptions, CaseRunReport, execute_case};
use serde_json::Value;

struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("core-store-roots-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn address(store: &Path, tree: &str) -> PathBuf {
    PathBuf::from(format!("store:{}#{tree}", store.display()))
}

/// Pack the case directory as tree `case` and every other root as a tree of
/// its own name; return both ways of naming the same roots.
fn pack_case(
    store: &Path,
    case: &Path,
    roots: &[(&str, PathBuf)],
) -> (BTreeMap<String, PathBuf>, BTreeMap<String, PathBuf>) {
    let mut trees = vec![("case".to_owned(), case.to_path_buf())];
    for (name, dir) in roots {
        trees.push(((*name).to_owned(), dir.clone()));
    }
    pack_store(store, &trees, 1).unwrap();
    let on_disk = trees
        .iter()
        .map(|(name, dir)| (name.clone(), dir.clone()))
        .collect();
    let in_store = trees
        .iter()
        .map(|(name, _)| (name.clone(), address(store, name)))
        .collect();
    (on_disk, in_store)
}

fn plan(case: &Path, roots: &BTreeMap<String, PathBuf>) -> CaseRunReport {
    let options = CaseRunOptions {
        source_roots: roots.clone(),
        plan_only: true,
        ..CaseRunOptions::default()
    };
    execute_case(case, &options).unwrap()
}

/// The report as JSON with the fields that only name where a root lives
/// removed. Today none do; the function is the one place to extend.
fn comparable(report: &CaseRunReport) -> Value {
    serde_json::to_value(report).unwrap()
}

fn assert_same_everywhere(name: &str, case_dir: &str, roots: Vec<(&str, PathBuf)>) {
    let dir = TestDir::new(name);
    let case = examples().join("cases").join(case_dir);
    let store = dir.0.join("case.store");
    let (disk, stored) = pack_case(&store, &case, &roots);

    let baseline = plan(&case, &disk);
    assert!(
        baseline
            .integrity
            .artifacts
            .iter()
            .any(|check| check.source_root == "case"),
        "the case reads its own artifacts"
    );
    let expected = comparable(&baseline);
    // The case package from the store, source roots on disk or in the store.
    for (package, sources) in [
        (address(&store, "case"), &disk),
        (address(&store, "case"), &stored),
        (case.clone(), &stored),
    ] {
        let report = plan(&package, sources);
        assert_eq!(
            comparable(&report),
            expected,
            "{name}: package {} with roots {:?}",
            package.display(),
            sources.values().collect::<Vec<_>>()
        );
        assert_eq!(report.status, baseline.status);
    }
    // Verification reaches the same integrity states without any root.
    let bare = plan(&case, &BTreeMap::new());
    assert_eq!(
        comparable(&plan(&address(&store, "case"), &BTreeMap::new())),
        comparable(&bare)
    );
}

#[test]
fn case_001_plans_identically_from_a_store() {
    assert_same_everywhere(
        "c001",
        "case-001-shield-search",
        vec![
            ("shielding", examples().join("capabilities/shielding")),
            ("agents", examples().join("agents")),
        ],
    );
}

#[test]
fn case_003_plans_identically_from_a_store() {
    assert_same_everywhere(
        "c003",
        "case-003-thermal-spreader",
        vec![("thermal", examples().join("capabilities/thermal"))],
    );
}

#[test]
fn case_010_plans_identically_from_a_store() {
    assert_same_everywhere(
        "c010",
        "case-010-matmul-rank",
        vec![(
            "matmul",
            examples().join("cases/case-010-matmul-rank/capability"),
        )],
    );
}

#[test]
fn a_tampered_blob_rejects_the_run_instead_of_crashing() {
    let dir = TestDir::new("tamper");
    let case = examples().join("cases/case-010-matmul-rank");
    let store = dir.0.join("case.store");
    let (_, stored) = pack_case(&store, &case, &[("matmul", case.join("capability"))]);
    let victim = fs::read(case.join("expected/npz-333-report.json")).unwrap();
    let digest = format!("{:x}", sha2_digest(&victim));
    let blob = store
        .join("blobs")
        .join(&digest[..2])
        .join(format!("{digest}.xz"));
    // A different, valid xz stream: the container checks pass, the digest
    // does not.
    fs::write(&blob, tiny_xz()).unwrap();

    let report = plan(&address(&store, "case"), &stored);
    assert_eq!(format!("{:?}", report.status), "Rejected", "{report:?}");
    let mismatched: Vec<_> = report
        .integrity
        .artifacts
        .iter()
        .filter(|check| format!("{:?}", check.state) == "Mismatch")
        .collect();
    assert!(!mismatched.is_empty(), "{:?}", report.integrity);
}

fn sha2_digest(bytes: &[u8]) -> impl std::fmt::LowerHex {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
}

fn tiny_xz() -> Vec<u8> {
    // The xz encoding of the empty stream, a valid file of other content.
    vec![
        0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00, 0x00, 0x04, 0xe6, 0xd6, 0xb4, 0x46, 0x00, 0x00, 0x00,
        0x00, 0x1c, 0xdf, 0x44, 0x21, 0x1f, 0xb6, 0xf3, 0x7d, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04,
        0x59, 0x5a,
    ]
}
