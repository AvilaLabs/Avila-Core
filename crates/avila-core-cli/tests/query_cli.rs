//! Exercise the shipped executable, including stdio framing and CLI/MCP parity.
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_avila-core");

#[test]
fn real_report_has_identical_cli_and_mcp_query_results() {
    let dir = std::env::temp_dir().join(format!("core-query-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let case = root.join("examples/cases/case-003-thermal-spreader");
    let output = Command::new(BIN)
        .arg("run")
        .arg(&case)
        .args([
            "--source-root",
            &format!("case={}", case.display()),
            "--source-root",
            &format!(
                "thermal={}",
                root.join("examples/capabilities/thermal").display()
            ),
            "--trust-root",
            &root.join("examples/keys/trust-root.json").display().to_string(),
        ])
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = dir.join("report.json");
    fs::write(&path, &output.stdout).unwrap();
    let cli = Command::new(BIN)
        .arg("inspect")
        .arg(&path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(cli.status.success());
    let expected: Value = serde_json::from_slice(&cli.stdout).unwrap();
    let generic = Command::new(BIN)
        .args(["tools", "call", "core_inspect", "--arguments"])
        .arg(json!({"path":path}).to_string())
        .arg("--json")
        .output()
        .unwrap();
    assert!(generic.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&generic.stdout).unwrap(),
        expected
    );
    let mut server = Command::new(BIN)
        .args(["mcp", "serve", "--root"])
        .arg(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = server.stdin.take().unwrap();
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"integration-test","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"core_inspect","arguments":{"path":"report.json"}}}),
        ] {
            writeln!(input, "{message}").unwrap();
        }
    }
    let output = server.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let messages: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        messages.len(),
        2,
        "stdout must contain only JSON-RPC responses"
    );
    assert_eq!(messages[1]["result"]["isError"], false);
    assert_eq!(messages[1]["result"]["structuredContent"], expected);
    assert_eq!(
        serde_json::from_str::<Value>(
            messages[1]["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
        )
        .unwrap(),
        expected
    );
    fs::remove_dir_all(dir).unwrap();
}
