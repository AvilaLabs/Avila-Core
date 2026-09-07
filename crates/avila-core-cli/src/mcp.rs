//! Minimal synchronous MCP stdio transport; all tool behavior lives in runner.
//! Protocol: https://modelcontextprotocol.io/specification/2025-11-25

use std::error::Error;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;

use avila_core_runner::query::{INSTRUCTIONS, QueryContext, call_tool, tool_catalog};
use clap::Subcommand;
use serde_json::{Value, json};

const PROTOCOL: &str = "2025-11-25";
const MAX_MESSAGE: u64 = 1024 * 1024;

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Serve read-only recorded-result queries over stdin/stdout; no network listener.
    Serve {
        /// Directory containing the reports and campaign logs clients may query.
        #[arg(long)]
        root: PathBuf,
    },
}

pub fn run(command: McpCommand) -> Result<(), Box<dyn Error>> {
    let McpCommand::Serve { root } = command;
    let context = QueryContext::rooted(&root)?;
    serve(io::stdin().lock(), io::stdout().lock(), &context)?;
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

#[derive(Default)]
struct Session {
    initialized: bool,
    ready: bool,
}

impl Session {
    fn handle(&mut self, message: Value, context: &QueryContext) -> Option<Value> {
        let id = message.get("id").cloned();
        if !message.is_object()
            || message["jsonrpc"] != "2.0"
            || !message["method"].is_string()
            || id
                .as_ref()
                .is_some_and(|v| !v.is_string() && !v.is_i64() && !v.is_u64())
        {
            return Some(error(Value::Null, -32600, "Invalid JSON-RPC request"));
        }
        let method = message["method"].as_str().unwrap();
        let Some(id) = id else {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            return None;
        };
        let result = match method {
            "initialize" => {
                if self.initialized {
                    return Some(error(id, -32600, "Already initialized"));
                }
                let params = &message["params"];
                if !params["protocolVersion"].is_string()
                    || !params["capabilities"].is_object()
                    || !params["clientInfo"]["name"].is_string()
                    || !params["clientInfo"]["version"].is_string()
                {
                    return Some(error(id, -32602, "Invalid initialize parameters"));
                }
                self.initialized = true;
                json!({"protocolVersion":PROTOCOL,"capabilities":{"tools":{"listChanged":false}},
                    "serverInfo":{"name":"avila-core","version":env!("CARGO_PKG_VERSION")},"instructions":INSTRUCTIONS})
            }
            "ping" => json!({}),
            _ if !self.ready => {
                return Some(error(
                    id,
                    -32000,
                    "Initialize the MCP session before using tools",
                ));
            }
            "tools/list" => {
                if message
                    .get("params")
                    .is_some_and(|p| !p.is_object() || p.get("cursor").is_some())
                {
                    return Some(error(
                        id,
                        -32602,
                        "This fixed tool catalog does not accept a cursor",
                    ));
                }
                json!({"tools":tool_catalog()})
            }
            "tools/call" => {
                let Some(name) = message["params"]["name"].as_str() else {
                    return Some(error(id, -32602, "Tool name is required"));
                };
                if !tool_catalog().iter().any(|tool| tool["name"] == name) {
                    return Some(error(id, -32602, "Unknown Core tool"));
                }
                let args = message["params"]
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match call_tool(context, name, args) {
                    Ok(value) => {
                        json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                    }
                    Err(message) => {
                        json!({"content":[{"type":"text","text":message}],"isError":true})
                    }
                }
            }
            _ => return Some(error(id, -32601, "Method not found")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
}

fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    context: &QueryContext,
) -> io::Result<()> {
    let mut session = Session::default();
    loop {
        let mut bytes = Vec::new();
        let count = (&mut input)
            .take(MAX_MESSAGE + 1)
            .read_until(b'\n', &mut bytes)?;
        if count == 0 {
            return Ok(());
        }
        if count as u64 > MAX_MESSAGE {
            // Terminate instead of interpreting a truncated message as a new request.
            writeln!(
                output,
                "{}",
                error(Value::Null, -32700, "MCP message exceeds 1 MiB")
            )?;
            output.flush()?;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized MCP message",
            ));
        }
        let response = match serde_json::from_slice::<Value>(&bytes) {
            Ok(message) => session.handle(message, context),
            Err(_) => Some(error(Value::Null, -32700, "Invalid JSON")),
        };
        if let Some(response) = response {
            writeln!(output, "{response}")?;
            output.flush()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(messages: &[Value]) -> Vec<Value> {
        let input = messages
            .iter()
            .map(|m| format!("{m}\n"))
            .collect::<String>();
        let mut output = Vec::new();
        serve(
            io::Cursor::new(input),
            &mut output,
            &QueryContext::unrestricted(),
        )
        .unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn initialize() -> Value {
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
    }

    #[test]
    fn handshake_catalog_and_shared_tool_call() {
        let replies = exchange(&[
            initialize(),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":"explain","method":"tools/call","params":{"name":"core_explain","arguments":{"id":"CORE-R3102"}}}),
        ]);
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[0]["result"]["protocolVersion"], PROTOCOL);
        assert_eq!(replies[1]["result"]["tools"], json!(tool_catalog()));
        assert_eq!(replies[2]["id"], "explain");
        assert_eq!(replies[2]["result"]["isError"], false);
        assert_eq!(
            replies[2]["result"]["structuredContent"],
            call_tool(
                &QueryContext::unrestricted(),
                "core_explain",
                json!({"id":"CORE-R3102"})
            )
            .unwrap()
        );
    }

    #[test]
    fn lifecycle_and_error_boundaries() {
        let replies = exchange(&[
            json!({"jsonrpc":"2.0","id":0,"method":"tools/list"}),
            initialize(),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"core_inspect","arguments":{"path":"missing-run-report.json"}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"unknown"}}),
            json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":0}}),
            json!([]),
        ]);
        assert_eq!(replies.len(), 5);
        assert_eq!(replies[0]["error"]["code"], -32000);
        assert_eq!(replies[2]["result"]["isError"], true);
        assert_eq!(replies[3]["error"]["code"], -32602);
        assert_eq!(replies[4]["error"]["code"], -32600);
    }

    #[test]
    fn malformed_json_does_not_pollute_or_end_the_stream() {
        let mut output = Vec::new();
        serve(
            io::Cursor::new(b"oops\n{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"ping\"}\n"),
            &mut output,
            &QueryContext::unrestricted(),
        )
        .unwrap();
        let replies: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(replies[0]["error"]["code"], -32700);
        assert_eq!(replies[1]["result"], json!({}));
    }
}
