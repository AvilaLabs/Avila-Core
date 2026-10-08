//! Exercise `avila-core store` through the shipped executable (ADR-0028).
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_avila-core");

struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("core-store-cli-{name}-{}", std::process::id()));
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

fn store(args: &[&str]) -> Output {
    Command::new(BIN).arg("store").args(args).output().unwrap()
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn pack_ls_cat_verify_and_unpack_round_trip() {
    let dir = TestDir::new("round-trip");
    let source = dir.0.join("src");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("a.json"), b"{\"a\":1}\n").unwrap();
    fs::write(source.join("nested/copy.json"), b"{\"a\":1}\n").unwrap();
    let store_dir = dir.0.join("s.store");
    let out = dir.0.join("out");

    let packed = store(&[
        "pack",
        "--out",
        text(&store_dir),
        &format!("case={}", source.display()),
    ]);
    assert!(packed.status.success(), "{packed:?}");
    assert_eq!(json(&packed)["distinct_blobs"], 1);

    let listing = store(&["ls", text(&store_dir)]);
    assert_eq!(json(&listing)["trees"][0]["files"], 2);
    let files = store(&["ls", text(&store_dir), "case"]);
    assert_eq!(json(&files)["files"][1]["path"], "nested/copy.json");

    let cat = store(&["cat", text(&store_dir), "case", "a.json"]);
    assert_eq!(cat.stdout, b"{\"a\":1}\n");

    let verified = store(&["verify", text(&store_dir)]);
    assert!(verified.status.success());
    assert_eq!(json(&verified)["status"], "verified");

    let unpacked = store(&["unpack", text(&store_dir), "--out", text(&out)]);
    assert!(unpacked.status.success(), "{unpacked:?}");
    assert_eq!(
        fs::read(out.join("case/nested/copy.json")).unwrap(),
        b"{\"a\":1}\n"
    );

    // Existing targets are refused, in both directions.
    assert!(
        !store(&["unpack", text(&store_dir), "--out", text(&out)])
            .status
            .success()
    );
    assert!(
        !store(&[
            "pack",
            "--out",
            text(&store_dir),
            &format!("case={}", source.display())
        ])
        .status
        .success()
    );
}

#[test]
fn verify_exits_nonzero_and_cat_emits_nothing_for_a_tampered_store() {
    let dir = TestDir::new("tamper");
    let source = dir.0.join("src");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("a.txt"), b"original").unwrap();
    let store_dir = dir.0.join("s.store");
    let packed = store(&[
        "pack",
        "--out",
        text(&store_dir),
        &format!("t={}", source.display()),
    ]);
    assert!(packed.status.success());

    // Overwrite the only blob with a valid-looking but different file.
    let fanout = fs::read_dir(store_dir.join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let blob = fs::read_dir(fanout.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&blob, b"not xz").unwrap();

    let verified = store(&["verify", text(&store_dir)]);
    assert_eq!(verified.status.code(), Some(1));
    assert_eq!(json(&verified)["status"], "failed");

    let cat = store(&["cat", text(&store_dir), "t", "a.txt"]);
    assert!(!cat.status.success());
    assert!(cat.stdout.is_empty());
}

#[test]
fn pack_requires_name_equals_dir() {
    let dir = TestDir::new("badarg");
    let output = store(&["pack", "--out", text(&dir.0.join("s")), "no-equals-sign"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("NAME=DIR"));
}
