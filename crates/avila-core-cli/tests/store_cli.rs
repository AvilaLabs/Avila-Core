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

#[test]
fn add_extends_a_store_and_refuses_a_duplicate_name_or_a_held_lock() {
    let dir = TestDir::new("add");
    let first = dir.0.join("first");
    let second = dir.0.join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("a.json"), b"{\"a\":1}\n").unwrap();
    fs::write(second.join("b.json"), b"{\"a\":1}\n").unwrap();
    fs::write(second.join("c.json"), b"{\"c\":3}\n").unwrap();
    let store_dir = dir.0.join("s.store");
    let packed = store(&[
        "pack",
        "--out",
        text(&store_dir),
        &format!("one={}", first.display()),
    ]);
    assert!(packed.status.success(), "{packed:?}");

    let added = store(&[
        "add",
        text(&store_dir),
        "--preset",
        "6",
        &format!("two={}", second.display()),
    ]);
    assert!(added.status.success(), "{added:?}");
    let report = json(&added);
    assert_eq!(report["added_trees"][0], "two");
    assert_eq!(
        (
            report["new_blobs"].as_u64(),
            report["reused_blobs"].as_u64()
        ),
        (Some(1), Some(1))
    );
    let verified = store(&["verify", text(&store_dir)]);
    assert!(verified.status.success());
    assert_eq!(json(&verified)["status"], "verified");
    assert_eq!(json(&verified)["trees"], 2);
    assert_eq!(
        store(&["cat", text(&store_dir), "two", "b.json"]).stdout,
        b"{\"a\":1}\n"
    );

    // A name already in the store is refused.
    let again = store(&[
        "add",
        text(&store_dir),
        &format!("one={}", second.display()),
    ]);
    assert_eq!(again.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&again.stderr).contains("already has a tree `one`"));

    // A held lock refuses the write; verify and reads carry on, and report
    // the lock as information.
    fs::write(store_dir.join("store.lock"), "pid 1 host other\n").unwrap();
    let locked = store(&[
        "add",
        text(&store_dir),
        &format!("three={}", second.display()),
    ]);
    assert_eq!(locked.status.code(), Some(2));
    let message = String::from_utf8_lossy(&locked.stderr).into_owned();
    assert!(
        message.contains("store.lock") && message.contains("no writer is running"),
        "{message}"
    );
    let verified = store(&["verify", text(&store_dir)]);
    assert!(verified.status.success());
    assert!(
        json(&verified)["writer_state"][0]
            .as_str()
            .unwrap()
            .contains("store.lock")
    );
    assert!(store(&["ls", text(&store_dir)]).status.success());
}

#[test]
fn a_case_is_planned_and_exported_identically_from_a_store_tree() {
    let dir = TestDir::new("roots");
    let case =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-010-matmul-rank");
    let tool = case.join("capability");
    let store_dir = dir.0.join("case.store");
    let packed = store(&[
        "pack",
        "--out",
        text(&store_dir),
        "--preset",
        "1",
        &format!("case={}", case.display()),
        &format!("matmul={}", tool.display()),
    ]);
    assert!(packed.status.success(), "{packed:?}");
    let address = |tree: &str| format!("store:{}#{tree}", store_dir.display());
    let run = |case: &str, case_root: &str, tool_root: &str| {
        Command::new(BIN)
            .args(["run", case, "--plan", "--json"])
            .arg("--source-root")
            .arg(format!("case={case_root}"))
            .arg("--source-root")
            .arg(format!("matmul={tool_root}"))
            .output()
            .unwrap()
    };
    let on_disk = run(text(&case), text(&case), text(&tool));
    assert!(on_disk.status.success(), "{on_disk:?}");
    let from_store = run(&address("case"), &address("case"), &address("matmul"));
    assert!(from_store.status.success(), "{from_store:?}");
    assert_eq!(json(&from_store), json(&on_disk));

    // `capabilities` reads the manifest from the store tree too.
    let caps = |case: &str| {
        Command::new(BIN)
            .args(["capabilities", case])
            .output()
            .unwrap()
    };
    // (The report's `case` field names the root, so it is excluded.)
    assert_eq!(
        json(&caps(&address("case")))["capabilities"],
        json(&caps(text(&case)))["capabilities"]
    );

    // Export from a store tree writes an ordinary directory.
    let export = |case: &str, case_root: &str, tool_root: &str, out: &Path| {
        Command::new(BIN)
            .args(["export", case])
            .arg("--source-root")
            .arg(format!("case={case_root}"))
            .arg("--source-root")
            .arg(format!("matmul={tool_root}"))
            .arg("--out")
            .arg(out)
            .output()
            .unwrap()
    };
    let out_disk = dir.0.join("export-disk");
    let out_store = dir.0.join("export-store");
    let a = export(text(&case), text(&case), text(&tool), &out_disk);
    assert!(a.status.success(), "{a:?}");
    let b = export(
        &address("case"),
        &address("case"),
        &address("matmul"),
        &out_store,
    );
    assert!(b.status.success(), "{b:?}");
    assert_eq!(json(&a)["export_sha256"], json(&b)["export_sha256"]);
    assert_eq!(
        fs::read(out_disk.join("export-report.json")).unwrap(),
        fs::read(out_store.join("export-report.json")).unwrap()
    );
    assert!(out_store.join("roots/matmul/brent_verify.py").is_file());

    // A store root that does not name a tree is an error naming the store.
    let bad = run(
        &format!("store:{}#nope", store_dir.display()),
        text(&case),
        text(&tool),
    );
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("tree `nope`"));
}
