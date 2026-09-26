//! `language::execute` — the real-process half of EL-03: spawn the
//! declared `synthetic/…` executables under a cleared environment with a
//! timeout, collect the declared output, and emit the observation set
//! `evaluate` binds. These tests run actual child processes; they require
//! `python3` on PATH (the synthetic executables' shebang).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use avila_core_compiler::language::{AnalysisOptions, evaluate_program, execution_plan};
use avila_core_runner::{LanguageExecuteOptions, execute_plan};

fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/language")
}

fn program_bytes(name: &str) -> Vec<u8> {
    fs::read(
        examples_root()
            .join("programs")
            .join(format!("{name}.program.json")),
    )
    .unwrap()
}

fn library_bytes() -> Vec<u8> {
    fs::read(examples_root().join("libraries/thermal-expansion.v1.json")).unwrap()
}

fn executables() -> BTreeMap<String, PathBuf> {
    let root = examples_root().join("executables");
    BTreeMap::from([
        (
            "synthetic/linear-expansion@1".to_string(),
            root.join("linear-expansion.py"),
        ),
        (
            "synthetic/linear-expansion-table@1".to_string(),
            root.join("linear-expansion-table.py"),
        ),
        (
            "synthetic/clearance-heuristic@1".to_string(),
            root.join("clearance-heuristic.py"),
        ),
    ])
}

fn have_python() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok()
}

/// A fresh per-test scratch directory under the OS temp dir, removed on
/// drop.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("avila-runner-test-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn execution_produces_bound_observations() {
    if !have_python() {
        return;
    }
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    let workdir = ScratchDir::new("execute-pass");
    let observations = execute_plan(
        &plan,
        workdir.path(),
        &LanguageExecuteOptions {
            executables: executables(),
            timeout: Duration::from_secs(30),
        },
    )
    .unwrap();
    assert_eq!(observations.observations.len(), 1);
    let record = &observations.observations[0];
    assert_eq!(record.at, "body[0]");
    assert_eq!(record.receipt.status, "completed");
    assert!(record.output_sha256.is_some());
    // The observation answers the same plan — the staged digest fields are
    // the identity the verifier re-derives.
    assert_eq!(observations.plan_sha256, plan.plan_sha256.unwrap());
    for (slot, digest) in &record.inputs {
        assert_eq!(
            Some(digest),
            plan.invocations[0].inputs[slot].sha256.as_ref()
        );
    }

    // The full loop: the observation evaluates to the pinned verdict.
    let evaluation = evaluate_program(
        &program,
        &library_bytes(),
        &AnalysisOptions::default(),
        &serde_json::to_vec(&observations).unwrap(),
    );
    assert_eq!(evaluation.requirements["EL-R1"].status, "pass");
}

#[test]
fn a_timeout_is_evidence_of_no_output() {
    if !have_python() {
        return;
    }
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    // A stub executable that sleeps — the invocation times out, the
    // observation carries the failed receipt, the obligation stays open.
    let workdir = ScratchDir::new("execute-timeout");
    let sleeper = workdir.join("sleeper.py");
    fs::write(
        &sleeper,
        "#!/usr/bin/env python3\nimport time\ntime.sleep(60)\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&sleeper, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let observations = execute_plan(
        &plan,
        workdir.path(),
        &LanguageExecuteOptions {
            executables: BTreeMap::from([("synthetic/linear-expansion@1".to_string(), sleeper)]),
            timeout: Duration::from_millis(400),
        },
    )
    .unwrap();
    let record = &observations.observations[0];
    assert_eq!(record.receipt.status, "timed_out");
    assert!(record.receipt.process.timed_out);
    assert!(record.output.is_none());
    let evaluation = evaluate_program(
        &program,
        &library_bytes(),
        &AnalysisOptions::default(),
        &serde_json::to_vec(&observations).unwrap(),
    );
    assert_eq!(evaluation.requirements["EL-R1"].status, "not_evaluated");
}

#[test]
fn a_missing_output_leaves_the_obligation_open() {
    if !have_python() {
        return;
    }
    let program = program_bytes("clearance-pass");
    let plan = execution_plan(&program, &library_bytes(), &AnalysisOptions::default());
    let workdir = ScratchDir::new("execute-missing");
    // A stub that exits 0 but writes nothing — the receipt fails the
    // output collection; the observation is rejected at binding.
    let noop = workdir.join("noop.py");
    fs::write(&noop, "#!/usr/bin/env python3\nimport sys\nsys.exit(0)\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&noop, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let observations = execute_plan(
        &plan,
        workdir.path(),
        &LanguageExecuteOptions {
            executables: BTreeMap::from([("synthetic/linear-expansion@1".to_string(), noop)]),
            timeout: Duration::from_secs(30),
        },
    )
    .unwrap();
    let record = &observations.observations[0];
    assert_eq!(record.receipt.status, "failed");
    assert_eq!(record.receipt.outputs[0].state, "missing");
    let evaluation = evaluate_program(
        &program,
        &library_bytes(),
        &AnalysisOptions::default(),
        &serde_json::to_vec(&observations).unwrap(),
    );
    assert_eq!(evaluation.requirements["EL-R1"].status, "not_evaluated");
    assert!(
        evaluation
            .observations
            .iter()
            .any(|o| o.state == "rejected")
    );
}
