use std::error::Error;
use std::path::PathBuf;

use avila_core_runner::query::{INSTRUCTIONS, QueryContext, call_tool, human_query, tool_catalog};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum View {
    Summary,
    Requirements,
    Findings,
    Artifacts,
    Workflow,
    Evidence,
    Steps,
}

impl View {
    fn tool(self) -> &'static str {
        match self {
            Self::Summary => "core_inspect",
            Self::Requirements => "core_requirements",
            Self::Findings => "core_findings",
            Self::Artifacts => "core_artifacts",
            Self::Workflow => "core_workflow",
            Self::Evidence => "core_evidence",
            Self::Steps => "core_steps",
        }
    }
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    /// Saved run-report.json, not a case directory.
    report: PathBuf,
    #[arg(long, value_enum, default_value = "summary")]
    view: View,
    /// Select a requirement, evidence record, or execution step by exact id.
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    offset: Option<usize>,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub struct HistoryArgs {
    log: PathBuf,
    #[arg(long)]
    case_id: Option<String>,
    /// Exact sha256: invocation identity. Includes planned/failed matches with their states.
    #[arg(long)]
    invocation: Option<String>,
    #[arg(long)]
    offset: Option<usize>,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub struct AttemptArgs {
    log: PathBuf,
    id: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
pub enum ToolsCommand {
    /// List the same named tools and argument schemas exposed over MCP.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Call a shared query operation. Arguments are a JSON object.
    Call {
        name: String,
        #[arg(long, default_value = "{}")]
        arguments: String,
        #[arg(long)]
        json: bool,
    },
    /// Print concise workflow instructions for a human or agent integration.
    Instructions,
}

fn invoke(name: &str, args: Value, json: bool) -> Result<(), Box<dyn Error>> {
    let result = call_tool(&QueryContext::unrestricted(), name, args)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print!("{}", human_query(&result));
    }
    Ok(())
}

fn put<T: serde::Serialize>(args: &mut Value, name: &str, value: Option<T>) {
    if let Some(value) = value {
        args[name] = serde_json::to_value(value).expect("query argument serialization");
    }
}

pub fn inspect(args: InspectArgs) -> Result<(), Box<dyn Error>> {
    let mut values = json!({"path":args.report});
    put(&mut values, "id", args.id);
    put(&mut values, "offset", args.offset);
    put(&mut values, "limit", args.limit);
    invoke(args.view.tool(), values, args.json)
}

pub fn history(args: HistoryArgs) -> Result<(), Box<dyn Error>> {
    let mut values = json!({"path":args.log});
    put(&mut values, "case_id", args.case_id);
    put(&mut values, "invocation", args.invocation);
    put(&mut values, "offset", args.offset);
    put(&mut values, "limit", args.limit);
    invoke("core_history", values, args.json)
}

pub fn attempt(args: AttemptArgs) -> Result<(), Box<dyn Error>> {
    invoke(
        "core_attempt",
        json!({"path":args.log,"id":args.id}),
        args.json,
    )
}

pub fn tools(command: ToolsCommand) -> Result<(), Box<dyn Error>> {
    match command {
        ToolsCommand::Instructions => println!("{INSTRUCTIONS}"),
        ToolsCommand::List { json: true } => {
            println!("{}", serde_json::to_string_pretty(&tool_catalog())?)
        }
        ToolsCommand::List { json: false } => {
            for tool in tool_catalog() {
                println!(
                    "{} — {}",
                    tool["name"].as_str().unwrap(),
                    tool["description"].as_str().unwrap()
                );
            }
        }
        ToolsCommand::Call {
            name,
            arguments,
            json,
        } => {
            let canonical = avila_core_kernel::canonicalize_json(arguments.as_bytes())?;
            invoke(&name, serde_json::from_slice(&canonical)?, json)?;
        }
    }
    Ok(())
}
