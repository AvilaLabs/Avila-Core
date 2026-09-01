#![forbid(unsafe_code)]

use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use avila_core_compiler::{
    CampaignStatus, CompilationStatus, DIAGNOSTIC_CATALOG, compile_documents, evaluate_campaign,
    explain,
};
use avila_core_evidence::sha256_hex;
use avila_core_kernel::{SEMANTIC_PROFILE, canonicalize_json};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "avila-core",
    version,
    about = "Avila Core local semantic compiler interface"
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
    /// Compile a v0.2-draft contract against one immutable registry snapshot.
    ///
    /// Prints the compile report as JSON. Exits 0 when the contract compiled,
    /// possibly with notices; 1 when it was rejected; and 2 when the tool
    /// could not run at all.
    Compile {
        #[arg(long)]
        contract: PathBuf,
        #[arg(long)]
        registry: PathBuf,
    },
    /// Evaluate a campaign: admit the claims produced for a compiled contract
    /// and derive one four-state verdict per requirement.
    ///
    /// Prints the campaign report as JSON. Exits 0 when the campaign was
    /// evaluated, whatever the verdicts; 1 when the documents were rejected;
    /// and 2 when the tool could not run.
    Evaluate {
        #[arg(long)]
        contract: PathBuf,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        claims: PathBuf,
    },
    /// Explain a stable finding code from the diagnostic catalog.
    Explain {
        /// A code such as `CORE-R3102`. Omit it and pass `--all` for the whole catalog.
        code: Option<String>,
        /// Print every catalog entry.
        #[arg(long)]
        all: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
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
        Command::Compile { contract, registry } => {
            let contract = fs::read(contract)?;
            let registry = fs::read(registry)?;
            let report = compile_documents(&contract, &registry)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.status == CompilationStatus::Rejected {
                return Ok(ExitCode::from(1));
            }
        }
        Command::Evaluate {
            contract,
            registry,
            claims,
        } => {
            let contract = fs::read(contract)?;
            let registry = fs::read(registry)?;
            let claims = fs::read(claims)?;
            let report = evaluate_campaign(&contract, &registry, &claims)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.status == CampaignStatus::Rejected {
                return Ok(ExitCode::from(1));
            }
        }
        Command::Explain { code, all } => match (code, all) {
            (None, true) => println!("{}", serde_json::to_string_pretty(DIAGNOSTIC_CATALOG)?),
            (Some(code), false) => match explain(&code) {
                Some(entry) => println!("{}", serde_json::to_string_pretty(entry)?),
                None => {
                    return Err(format!(
                        "`{code}` is not a finding code this compiler emits; run `avila-core explain --all` for the catalog"
                    )
                    .into());
                }
            },
            _ => return Err("pass exactly one code, or `--all` for the whole catalog".into()),
        },
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Debug, serde::Serialize)]
struct SemanticProfileReport {
    semantic_profile: &'static str,
    kernel_version: &'static str,
    status: &'static str,
    implemented_vector_sets: Vec<VectorSetReport>,
    total_vectors: usize,
    implemented_compiler_fixture_sets: Vec<CompilerFixtureSetReport>,
    total_compiler_fixtures: usize,
    implemented_campaign_fixture_sets: Vec<CompilerFixtureSetReport>,
    total_campaign_fixtures: usize,
    notice: &'static str,
}

#[derive(Debug, serde::Serialize)]
struct VectorSetReport {
    vector_set: String,
    version: u64,
    vectors: usize,
    sha256: String,
}

#[derive(Debug, serde::Serialize)]
struct CompilerFixtureSetReport {
    fixture_set: String,
    version: u64,
    fixtures: usize,
    sha256: String,
}

const EMBEDDED_VECTOR_SETS: [&[u8]; 4] = [
    include_bytes!("../../../fixtures/semantic-core/vectors/canon.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/unit-scaling.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/scope-predicates.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/verdict-calculus.v1.json"),
];

const EMBEDDED_COMPILER_FIXTURE_SETS: [&[u8]; 5] = [
    include_bytes!("../../../fixtures/semantic-core/types/compiler-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/types/compiler-parameter-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/types/compiler-reproducibility-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/types/compiler-review-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/types/compiler-purpose-cases.v1.json"),
];

const EMBEDDED_CAMPAIGN_FIXTURE_SETS: [&[u8]; 1] = [include_bytes!(
    "../../../fixtures/semantic-core/campaigns/campaign-cases.v1.json"
)];

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

    let mut implemented_compiler_fixture_sets =
        Vec::with_capacity(EMBEDDED_COMPILER_FIXTURE_SETS.len());
    let mut total_compiler_fixtures = 0;
    for bytes in EMBEDDED_COMPILER_FIXTURE_SETS {
        let document: serde_json::Value = serde_json::from_slice(bytes)?;
        let fixture_set = required_string(&document, "fixture_set")?;
        let version = document
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or("embedded compiler fixture set is missing an integer version")?;
        let fixtures = document
            .get("fixtures")
            .and_then(serde_json::Value::as_array)
            .ok_or("embedded compiler fixture set is missing fixtures")?
            .len();
        total_compiler_fixtures += fixtures;
        implemented_compiler_fixture_sets.push(CompilerFixtureSetReport {
            fixture_set: fixture_set.into(),
            version,
            fixtures,
            sha256: format!("sha256:{}", sha256_hex(bytes)),
        });
    }

    let mut implemented_campaign_fixture_sets = Vec::new();
    let mut total_campaign_fixtures = 0;
    for bytes in EMBEDDED_CAMPAIGN_FIXTURE_SETS {
        let document: serde_json::Value = serde_json::from_slice(bytes)?;
        let fixture_set = required_string(&document, "fixture_set")?;
        let version = document
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or("embedded campaign fixture set is missing an integer version")?;
        let fixtures = document
            .get("fixtures")
            .and_then(serde_json::Value::as_array)
            .ok_or("embedded campaign fixture set is missing fixtures")?
            .len();
        total_campaign_fixtures += fixtures;
        implemented_campaign_fixture_sets.push(CompilerFixtureSetReport {
            fixture_set: fixture_set.into(),
            version,
            fixtures,
            sha256: format!("sha256:{}", sha256_hex(bytes)),
        });
    }

    Ok(SemanticProfileReport {
        semantic_profile: SEMANTIC_PROFILE,
        kernel_version: env!("CARGO_PKG_VERSION"),
        status: "draft",
        implemented_vector_sets,
        total_vectors,
        implemented_compiler_fixture_sets,
        total_compiler_fixtures,
        implemented_campaign_fixture_sets,
        total_campaign_fixtures,
        notice: "Conformance to these software fixtures is not scientific qualification, full package-level evidence admission, or certification.",
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
        assert_eq!(report.total_compiler_fixtures, 68);
        assert_eq!(report.implemented_compiler_fixture_sets.len(), 5);
        assert_eq!(report.total_campaign_fixtures, 12);
        assert_eq!(report.implemented_campaign_fixture_sets.len(), 1);
        assert!(
            report
                .implemented_vector_sets
                .iter()
                .all(|set| set.sha256.starts_with("sha256:") && set.sha256.len() == 71)
        );
        assert!(
            report
                .implemented_compiler_fixture_sets
                .iter()
                .all(|set| set.sha256.starts_with("sha256:") && set.sha256.len() == 71)
        );
        assert!(
            report
                .implemented_campaign_fixture_sets
                .iter()
                .all(|set| set.sha256.starts_with("sha256:") && set.sha256.len() == 71)
        );
    }

    #[test]
    fn explain_serves_the_embedded_catalog() {
        assert_eq!(explain("CORE-R3102").unwrap().code, "CORE-R3102");
        assert!(explain("CORE-X9999").is_none());
        assert!(!DIAGNOSTIC_CATALOG.is_empty());
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
