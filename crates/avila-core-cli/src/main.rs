#![forbid(unsafe_code)]

use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use avila_core_evidence::sha256_hex;
use avila_core_kernel::{SEMANTIC_PROFILE, canonicalize_json};
use avila_core_model::{CapabilityManifest, EvidenceContract};
use avila_core_runtime::CampaignPlanner;
use clap::{Parser, Subcommand};
use serde::de::DeserializeOwned;

#[derive(Debug, Parser)]
#[command(
    name = "avila-core",
    version,
    about = "Avila Core local semantic and planning interface"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Report the draft semantic profile and exact embedded vector identities.
    SemanticProfile,
    /// Read authoritative JSON and emit its deterministic canonical bytes.
    Canonicalize { document: PathBuf },
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
        Command::SemanticProfile => {
            println!(
                "{}",
                serde_json::to_string_pretty(&semantic_profile_report()?)?
            );
        }
        Command::Canonicalize { document } => {
            let source = fs::read(document)?;
            let canonical = canonicalize_json(&source)?;
            let mut stdout = io::stdout().lock();
            stdout.write_all(&canonical)?;
            stdout.write_all(b"\n")?;
        }
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

#[derive(Debug, serde::Serialize)]
struct SemanticProfileReport {
    semantic_profile: &'static str,
    kernel_version: &'static str,
    status: &'static str,
    implemented_vector_sets: Vec<VectorSetReport>,
    total_vectors: usize,
    notice: &'static str,
}

#[derive(Debug, serde::Serialize)]
struct VectorSetReport {
    vector_set: String,
    version: u64,
    vectors: usize,
    sha256: String,
}

const EMBEDDED_VECTOR_SETS: [&[u8]; 4] = [
    include_bytes!("../../../fixtures/semantic-core/vectors/canon.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/unit-scaling.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/scope-predicates.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/verdict-calculus.v1.json"),
];

fn semantic_profile_report() -> Result<SemanticProfileReport, Box<dyn Error>> {
    let mut implemented_vector_sets = Vec::with_capacity(EMBEDDED_VECTOR_SETS.len());
    let mut total_vectors = 0;
    for bytes in EMBEDDED_VECTOR_SETS {
        let document: serde_json::Value = serde_json::from_slice(bytes)?;
        let vector_set = required_string(&document, "vector_set")?;
        let version = document
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or("embedded vector set is missing an integer version")?;
        let primary = document
            .get("vectors")
            .and_then(serde_json::Value::as_array)
            .ok_or("embedded vector set is missing vectors")?
            .len();
        let secondary = document
            .get("aggregation_vectors")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        let vectors = primary + secondary;
        total_vectors += vectors;
        implemented_vector_sets.push(VectorSetReport {
            vector_set: vector_set.into(),
            version,
            vectors,
            sha256: format!("sha256:{}", sha256_hex(bytes)),
        });
    }

    Ok(SemanticProfileReport {
        semantic_profile: SEMANTIC_PROFILE,
        kernel_version: env!("CARGO_PKG_VERSION"),
        status: "draft",
        implemented_vector_sets,
        total_vectors,
        notice: "Conformance to these software vectors is not scientific qualification, evidence admission, or certification.",
    })
}

fn required_string<'a>(
    document: &'a serde_json::Value,
    field: &str,
) -> Result<&'a str, Box<dyn Error>> {
    document
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("embedded vector set is missing `{field}`").into())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_profile_report_identifies_every_executable_vector() {
        let report = semantic_profile_report().unwrap();
        assert_eq!(report.semantic_profile, SEMANTIC_PROFILE);
        assert_eq!(report.status, "draft");
        assert_eq!(report.total_vectors, 90);
        assert_eq!(report.implemented_vector_sets.len(), 4);
        assert!(
            report
                .implemented_vector_sets
                .iter()
                .all(|set| set.sha256.starts_with("sha256:") && set.sha256.len() == 71)
        );
    }

    #[test]
    fn cli_canonicalization_uses_the_authoritative_reader() {
        assert_eq!(
            canonicalize_json(br#"{"z":1,"a":2}"#).unwrap(),
            br#"{"a":2,"z":1}"#
        );
        assert!(canonicalize_json(br#"{"value":25.0}"#).is_err());
    }
}
