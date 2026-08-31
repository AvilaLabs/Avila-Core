#![forbid(unsafe_code)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use avila_core_model::{CapabilityManifest, EvidenceContract};
use avila_core_runtime::CampaignPlanner;
use clap::{Parser, Subcommand};
use serde::de::DeserializeOwned;

#[derive(Debug, Parser)]
#[command(name = "avila-core", version, about = "Avila Core local scaffold")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate the structure of an evidence contract.
    ValidateContract { contract: PathBuf },
    /// Produce a deterministic campaign plan. This does not execute it.
    Plan {
        #[arg(long)]
        contract: PathBuf,
        #[arg(long = "capability", required = true)]
        capabilities: Vec<PathBuf>,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::ValidateContract { contract } => {
            let contract: EvidenceContract = read_json(&contract)?;
            contract.validate()?;
            println!(
                "{}",
                serde_json::json!({
                    "contract_id": contract.contract_id,
                    "status": "structurally_valid",
                    "notice": "No scientific validity or requirement verdict was evaluated."
                })
            );
        }
        Command::Plan {
            contract,
            capabilities,
        } => {
            let contract: EvidenceContract = read_json(&contract)?;
            let manifests: Vec<CapabilityManifest> = capabilities
                .iter()
                .map(|path| read_json(path))
                .collect::<Result<_, _>>()?;
            let plan = CampaignPlanner.plan(&contract, &manifests)?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}
