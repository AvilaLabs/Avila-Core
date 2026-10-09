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

    // `--report-only` prints what `--out` prints and writes nothing, from a
    // directory and from a store tree.
    let report_only = |case: &str, case_root: &str, tool_root: &str| {
        Command::new(BIN)
            .args(["export", case])
            .arg("--source-root")
            .arg(format!("case={case_root}"))
            .arg("--source-root")
            .arg(format!("matmul={tool_root}"))
            .arg("--report-only")
            .current_dir(&dir.0)
            .output()
            .unwrap()
    };
    let listing = |path: &Path| {
        let mut names: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    let before = listing(&dir.0);
    let only_disk = report_only(text(&case), text(&case), text(&tool));
    assert!(only_disk.status.success(), "{only_disk:?}");
    let written = fs::read(out_disk.join("export-report.json")).unwrap();
    // stdout is the written report's bytes; --out's own stdout adds a newline.
    assert_eq!(only_disk.stdout, written);
    assert_eq!([&written[..], b"\n"].concat(), a.stdout);
    let only_store = report_only(&address("case"), &address("case"), &address("matmul"));
    assert!(only_store.status.success(), "{only_store:?}");
    assert_eq!(only_store.stdout, written);
    assert_eq!(listing(&dir.0), before);

    // Exactly one of --out and --report-only.
    let neither = Command::new(BIN)
        .args(["export", text(&case)])
        .output()
        .unwrap();
    assert_eq!(neither.status.code(), Some(2));
    let both = Command::new(BIN)
        .args(["export", text(&case), "--report-only", "--out"])
        .arg(dir.0.join("both"))
        .output()
        .unwrap();
    assert_eq!(both.status.code(), Some(2));
    assert!(!dir.0.join("both").exists());

    // A store root that does not name a tree is an error naming the store.
    let bad = run(
        &format!("store:{}#nope", store_dir.display()),
        text(&case),
        text(&tool),
    );
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("tree `nope`"));
}

/// The pinned interpreter of the bundled matmul case, when this machine has
/// it; the run test is skipped otherwise.
fn matmul_python() -> Option<PathBuf> {
    let python = PathBuf::from("/usr/bin/python3");
    let (digest, _) = avila_core_evidence::sha256_file(&python).ok()?;
    (digest == "sha256:52e0a13e60a981d8c4b6478be2ba5176f69da07948a056bf49cf6f077e30cb41")
        .then_some(python)
}

fn run_matmul(dir: &TestDir, python: &Path, extra: &[&str]) -> Output {
    run_matmul_at(python, Some(&dir.0.join("ws")), &dir.0, extra)
}

/// Run the matmul case from `cwd`; `workspace` is `--workspace`, or the
/// default `workspaces/<case>/<run>` under `cwd` when `None`.
fn run_matmul_at(python: &Path, workspace: Option<&Path>, cwd: &Path, extra: &[&str]) -> Output {
    let case =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-010-matmul-rank");
    let mut command = Command::new(BIN);
    command
        .arg("run")
        .arg(&case)
        .arg("--source-root")
        .arg(format!("matmul={}", case.join("capability").display()))
        .arg("--source-root")
        .arg(format!("case={}", case.display()))
        .arg("--capability")
        .arg(format!("python3={}", python.display()))
        .args(["--no-reuse", "--json"])
        .current_dir(cwd);
    if let Some(workspace) = workspace {
        command.arg("--workspace").arg(workspace);
    }
    command.args(extra).output().unwrap()
}

#[test]
fn run_store_persists_the_run_and_removes_the_workspace() {
    let Some(python) = matmul_python() else {
        eprintln!("skipped: the pinned python3 is not at /usr/bin/python3");
        return;
    };
    let dir = TestDir::new("run-store");
    let store_dir = dir.0.join("runs.store");
    let ran = run_matmul(&dir, &python, &["--store", text(&store_dir)]);
    assert!(ran.status.success(), "{ran:?}");
    let report = json(&ran);
    let tree = report["store"]["tree"].as_str().unwrap();
    assert_eq!(
        report["store"]["address"],
        format!("store:{}#{tree}", store_dir.display())
    );
    assert!(tree.starts_with("CASE-010."), "{tree}");
    assert!(!dir.0.join("ws").exists());

    let verified = store(&["verify", text(&store_dir)]);
    assert!(verified.status.success(), "{verified:?}");
    let listing = store(&["ls", text(&store_dir), tree]);
    let listing = json(&listing);
    let paths: Vec<&str> = listing["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    for expected in [
        "run-report.json",
        "claims.json",
        "campaign-report.json",
        "verify/receipt.json",
    ] {
        assert!(paths.contains(&expected), "{expected} in {paths:?}");
    }
    // run-report.json does not carry the store field.
    let written = store(&["cat", text(&store_dir), tree, "run-report.json"]);
    assert!(json(&written).get("store").is_none());
}

const NO_PYTHON: &str = "skipped: the pinned python3 is not at /usr/bin/python3";

fn names(path: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

#[test]
fn run_defaults_to_a_store_beside_the_workspace_and_siblings_share_it() {
    let Some(python) = matmul_python() else {
        eprintln!("{NO_PYTHON}");
        return;
    };
    let dir = TestDir::new("run-default");
    let runs = dir.0.join("runs");
    let first = run_matmul_at(&python, Some(&runs.join("a")), &dir.0, &[]);
    assert!(first.status.success(), "{first:?}");
    let store_dir = runs.join("evidence-store");
    let a = json(&first);
    assert_eq!(a["store"]["store"], text(&store_dir));
    assert_eq!(
        a["store"]["address"],
        format!(
            "store:{}#{}",
            store_dir.display(),
            a["store"]["tree"].as_str().unwrap()
        )
    );
    // Only the store is left beside the workspaces.
    assert_eq!(names(&runs), ["evidence-store"]);

    let second = run_matmul_at(&python, Some(&runs.join("b")), &dir.0, &[]);
    assert!(second.status.success(), "{second:?}");
    let b = json(&second);
    assert_ne!(a["store"]["tree"], b["store"]["tree"]);
    assert!(
        b["store"]["stored_bytes"].as_u64().unwrap() < a["store"]["stored_bytes"].as_u64().unwrap(),
        "{} vs {}",
        b["store"]["stored_bytes"],
        a["store"]["stored_bytes"]
    );
    assert_eq!(names(&runs), ["evidence-store"]);
    let verified = store(&["verify", text(&store_dir)]);
    assert!(verified.status.success(), "{verified:?}");
    assert_eq!(json(&verified)["trees"], 2);

    // The human-readable output prints the address too.
    let third = run_matmul_at(&python, Some(&runs.join("c")), &dir.0, &[]);
    assert!(third.status.success());
    let human = Command::new(BIN)
        .arg("run")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-010-matmul-rank"),
        )
        .args(["--no-reuse", "--workspace"])
        .arg(runs.join("d"))
        .arg("--source-root")
        .arg(format!(
            "matmul={}",
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/cases/case-010-matmul-rank/capability")
                .display()
        ))
        .arg("--source-root")
        .arg(format!(
            "case={}",
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/cases/case-010-matmul-rank")
                .display()
        ))
        .arg("--capability")
        .arg(format!("python3={}", python.display()))
        .output()
        .unwrap();
    assert!(human.status.success(), "{human:?}");
    assert!(
        String::from_utf8_lossy(&human.stdout)
            .contains(&format!("store:{}#CASE-010.", store_dir.display()))
    );
}

#[test]
fn the_default_workspace_puts_the_store_in_its_case_folder() {
    let Some(python) = matmul_python() else {
        eprintln!("{NO_PYTHON}");
        return;
    };
    let dir = TestDir::new("run-default-ws");
    let ran = run_matmul_at(&python, None, &dir.0, &[]);
    assert!(ran.status.success(), "{ran:?}");
    let case_folder = dir.0.join("workspaces/CASE-010");
    assert_eq!(names(&case_folder), ["evidence-store"]);
    let report = json(&ran);
    let canonical = fs::canonicalize(&case_folder)
        .unwrap()
        .join("evidence-store");
    assert_eq!(report["store"]["store"], text(&canonical));
}

#[test]
fn directory_restores_the_plain_workspace_and_writes_no_store() {
    let Some(python) = matmul_python() else {
        eprintln!("{NO_PYTHON}");
        return;
    };
    let dir = TestDir::new("run-directory");
    let runs = dir.0.join("runs");
    let ran = run_matmul_at(&python, Some(&runs.join("a")), &dir.0, &["--directory"]);
    assert!(ran.status.success(), "{ran:?}");
    assert!(json(&ran).get("store").is_none());
    assert!(runs.join("a/verify/receipt.json").is_file());
    assert!(runs.join("a/run-report.json").is_file());
    assert_eq!(names(&runs), ["a"]);
}

#[test]
fn directory_conflicts_with_store_and_keep_scratch_and_runs_nothing() {
    let dir = TestDir::new("run-conflicts");
    let case =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-010-matmul-rank");
    for other in [&["--store", "s.store"][..], &["--keep-scratch"][..]] {
        let refused = Command::new(BIN)
            .arg("run")
            .arg(&case)
            .arg("--directory")
            .args(other)
            .current_dir(&dir.0)
            .output()
            .unwrap();
        assert_eq!(refused.status.code(), Some(2), "{refused:?}");
        assert!(String::from_utf8_lossy(&refused.stderr).contains("cannot be used with"));
    }
    assert!(names(&dir.0).is_empty());
}

#[test]
fn keep_scratch_works_with_the_default_store() {
    let Some(python) = matmul_python() else {
        eprintln!("{NO_PYTHON}");
        return;
    };
    let dir = TestDir::new("run-keep");
    let runs = dir.0.join("runs");
    let ran = run_matmul_at(&python, Some(&runs.join("a")), &dir.0, &["--keep-scratch"]);
    assert!(ran.status.success(), "{ran:?}");
    assert!(json(&ran)["store"]["address"].is_string());
    assert!(runs.join("a/verify/receipt.json").is_file());
    assert!(runs.join("evidence-store/store.json").is_file());
}

#[test]
fn a_run_that_executes_nothing_creates_no_workspace_and_no_store() {
    let dir = TestDir::new("run-nothing");
    let case = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/cases/case-003-thermal-spreader");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Reuse of the committed receipts executes no step.
    let ran = Command::new(BIN)
        .arg("run")
        .arg(&case)
        .arg("--source-root")
        .arg(format!("case={}", case.display()))
        .arg("--source-root")
        .arg(format!(
            "thermal={}",
            root.join("examples/capabilities/thermal").display()
        ))
        .arg("--trust-root")
        .arg(root.join("examples/keys/trust-root.json"))
        .arg("--json")
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert!(json(&ran).get("store").is_none());
    assert!(names(&dir.0).is_empty(), "{:?}", names(&dir.0));
}
