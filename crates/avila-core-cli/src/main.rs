#![forbid(unsafe_code)]

mod mcp;
mod queries;

use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use avila_core_compiler::{
    CampaignStatus, CompilationStatus, DIAGNOSTIC_CATALOG, compile_documents, evaluate_campaign,
    explain, render_campaign_report, render_compile_report,
};
use avila_core_evidence::sha256_hex;
use avila_core_evidence::signature::{self, KeyRole};
use avila_core_kernel::{SEMANTIC_PROFILE, canonicalize_json};
use avila_core_runner::{RUNTIME_DIAGNOSTIC_CATALOG, explain_runtime};
use clap::{Args, Parser, Subcommand, ValueEnum};

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
    /// Query a saved run report without executing or freshly verifying artifacts.
    Inspect(queries::InspectArgs),
    /// Search recorded runs in one explicit campaign JSONL log.
    History(queries::HistoryArgs),
    /// Inspect an attempt and compare it with its identity-bound parent.
    Attempt(queries::AttemptArgs),
    /// Discover and call shared Core query tools, or print integration instructions.
    Tools {
        #[command(subcommand)]
        command: queries::ToolsCommand,
    },
    /// Serve shared read-only query tools to local MCP clients.
    Mcp {
        #[command(subcommand)]
        command: mcp::McpCommand,
    },
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
        /// Print findings as readable text with source locations instead of
        /// the JSON report.
        #[arg(long)]
        text: bool,
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
        /// Print findings and verdicts as readable text with source locations
        /// instead of the JSON report.
        #[arg(long)]
        text: bool,
    },
    /// Run a composed case package through integrity checks, compilation,
    /// controlled execution with receipts, claim generation, evidence
    /// binding, campaign evaluation, and deterministic replay.
    ///
    /// Boxed only to keep this enum's variants close in size; `RunArgs`
    /// carries the actual argument set.
    Run(Box<RunArgs>),
    /// Explain a stable finding code from the diagnostic catalog.
    Explain {
        /// A code such as `CORE-R3102`. Omit it and pass `--all` for the whole catalog.
        code: Option<String>,
        /// Print every catalog entry.
        #[arg(long)]
        all: bool,
    },
    /// Generate or inspect Ed25519 signing keys (ADR-0015).
    Keys {
        #[command(subcommand)]
        command: KeysCommand,
    },
    /// Sign a case package document with a requester or runner key.
    Sign {
        #[command(subcommand)]
        command: SignCommand,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum KeyRoleArg {
    Requester,
    Runner,
}

impl From<KeyRoleArg> for KeyRole {
    fn from(value: KeyRoleArg) -> Self {
        match value {
            KeyRoleArg::Requester => KeyRole::Requester,
            KeyRoleArg::Runner => KeyRole::Runner,
        }
    }
}

#[derive(Debug, Subcommand)]
enum KeysCommand {
    /// Write a fresh 32-byte seed (mode 0600) and hex public key file, and
    /// print the key id. The seed is never printed.
    Generate {
        #[arg(long, value_enum)]
        role: KeyRoleArg,
        /// Directory to write `<role>.seed` and `<role>.pub` into. Defaults
        /// to `$XDG_CONFIG_HOME/avila-core/keys` (or `~/.config/avila-core/keys`).
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
    },
    /// Print a key file's id and public key hex. Accepts either a seed file
    /// or a public key file; a seed's bytes are never printed.
    Show { file: PathBuf },
}

#[derive(Debug, Subcommand)]
enum SignCommand {
    /// Sign CASE's package manifest with a requester (or other) seed key,
    /// writing `signatures/manifest.sig.json` and binding it into
    /// `package.json` as a `signature` document. Re-run after any other
    /// edit to the manifest; running it again replaces the prior signature.
    Manifest {
        /// Case directory containing package.json, or the manifest path itself.
        case: PathBuf,
        #[arg(long, value_name = "FILE")]
        key: PathBuf,
    },
    /// Sign CASE's already-committed receipt for STEP with a runner seed
    /// key, writing `signatures/<step>-receipt.sig.json` and binding it
    /// into `package.json` as a `signature` document. Used to sign a
    /// receipt that already exists (blessed by an earlier run) without
    /// re-executing it; a fresh run's own receipt is instead signed inline
    /// when `run` is given `--runner-key`.
    Receipt {
        /// Case directory containing package.json, or the manifest path itself.
        case: PathBuf,
        #[arg(long)]
        step: String,
        #[arg(long, value_name = "FILE")]
        key: PathBuf,
    },
}

/// `$XDG_CONFIG_HOME/avila-core/keys`, or `$HOME/.config/avila-core/keys`
/// when `XDG_CONFIG_HOME` is unset, per ADR-0015 clause 4.
fn default_key_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));
    base.join("avila-core").join("keys")
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Args)]
struct RunArgs {
    /// Case directory containing package.json, or the manifest path itself.
    case: PathBuf,
    /// Resolve an external artifact root as NAME=PATH. Repeat as needed.
    #[arg(long = "source-root", value_name = "NAME=PATH")]
    source_roots: Vec<String>,
    /// Supply the executable for a package capability as NAME=PATH. Its
    /// bytes must hash to the identity the package binds. Repeat as needed.
    #[arg(long = "capability", value_name = "NAME=PATH")]
    capabilities: Vec<String>,
    /// Fresh directory for staged inputs, outputs, logs, receipts, and the
    /// generated documents. Defaults to workspaces/<case>/<run> under the
    /// current directory when something is executed.
    #[arg(long, value_name = "DIR")]
    workspace: Option<PathBuf>,
    /// Execute every declared step afresh instead of reusing a step whose
    /// committed receipt matches the planned invocation and whose outputs
    /// still verify.
    #[arg(long = "no-reuse")]
    no_reuse: bool,
    /// Report what would be reused or rerun, and why, without executing.
    #[arg(long)]
    plan: bool,
    /// Supply a free contract input for this run as NAME=PATH. The bytes
    /// are hashed and attested; committed expectations are not replayed.
    #[arg(long = "input", value_name = "NAME=PATH")]
    inputs: Vec<String>,
    /// Supply a value for an environment key an execution declares.
    #[arg(long = "env", value_name = "KEY=VALUE")]
    environment: Vec<String>,
    /// Refuse the run unless the package manifest's sha256 equals this
    /// pinned value, so a campaign cannot evaluate a rewritten package.
    #[arg(long = "expect-manifest", value_name = "SHA256")]
    expect_manifest: Option<String>,
    /// Append one JSON line describing this run to FILE.
    #[arg(long, value_name = "FILE")]
    log: Option<PathBuf>,
    /// Cache verified digests of large, unchanging artifacts resolved
    /// under a --source-root in this operator-owned JSON file, keyed by
    /// exact path, size, and modification time. Off unless supplied;
    /// package documents and anything inside the case directory are
    /// always re-hashed. See SECURITY.md for the trust this accepts.
    #[arg(long = "hash-cache", value_name = "FILE")]
    hash_cache: Option<PathBuf>,
    /// Place this run in an identity-bound candidate lineage. Requires
    /// --log and a supplied canonical-profile JSON candidate input.
    #[arg(long = "attempt", value_name = "ID")]
    attempt_id: Option<String>,
    /// Name an earlier attempt in the same log as this attempt's parent.
    #[arg(long = "parent-attempt", value_name = "ID")]
    parent_attempt_id: Option<String>,
    /// The supplied input Core should snapshot and diff for lineage.
    /// Defaults to `candidate` when --attempt is present.
    #[arg(long = "candidate-input", value_name = "NAME")]
    candidate_input: Option<String>,
    /// The requester and runner public keys this run accepts (ADR-0015).
    /// With it, the manifest signature must verify against a listed
    /// requester key or the run is refused before compilation, and a
    /// committed receipt is reused under SC-12 only when its signature
    /// verifies against a listed runner key. Without it, every signature is
    /// reported `unsigned` or `signature not checked`, never `verified`.
    #[arg(long = "trust-root", value_name = "FILE")]
    trust_root: Option<PathBuf>,
    /// A runner seed key (32 raw bytes, as written by `avila-core keys
    /// generate`). When supplied, a freshly executed step's receipt is
    /// signed in the workspace, and campaign log lines this run appends are
    /// signed the same way. Never printed or logged.
    #[arg(long = "runner-key", value_name = "FILE")]
    runner_key: Option<PathBuf>,
    /// Emit the complete machine-readable run report instead of the concise view.
    #[arg(long)]
    json: bool,
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
        Command::Inspect(args) => queries::inspect(args)?,
        Command::History(args) => queries::history(args)?,
        Command::Attempt(args) => queries::attempt(args)?,
        Command::Tools { command } => queries::tools(command)?,
        Command::Mcp { command } => mcp::run(command)?,
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
        Command::Compile {
            contract,
            registry,
            text,
        } => {
            let contract = fs::read(contract)?;
            let registry = fs::read(registry)?;
            let report = compile_documents(&contract, &registry)?;
            if text {
                print!(
                    "{}",
                    render_compile_report(
                        &report,
                        &[("contract", &contract), ("registry", &registry)]
                    )
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            if report.status == CompilationStatus::Rejected {
                return Ok(ExitCode::from(1));
            }
        }
        Command::Evaluate {
            contract,
            registry,
            claims,
            text,
        } => {
            let contract = fs::read(contract)?;
            let registry = fs::read(registry)?;
            let claims = fs::read(claims)?;
            let report = evaluate_campaign(&contract, &registry, &claims)?;
            if text {
                print!(
                    "{}",
                    render_campaign_report(
                        &report,
                        &[
                            ("contract", &contract),
                            ("registry", &registry),
                            ("claims", &claims)
                        ]
                    )
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            if report.status == CampaignStatus::Rejected {
                return Ok(ExitCode::from(1));
            }
        }
        Command::Run(args) => {
            let RunArgs {
                case,
                source_roots,
                capabilities,
                workspace,
                no_reuse,
                expect_manifest,
                plan,
                inputs,
                environment,
                log,
                hash_cache,
                attempt_id,
                parent_attempt_id,
                candidate_input,
                trust_root,
                runner_key,
                json,
            } = *args;
            let attempt = attempt_request(attempt_id, parent_attempt_id, candidate_input)?;
            let options = avila_core_runner::CaseRunOptions {
                source_roots: avila_core_runner::parse_source_roots(&source_roots)?,
                capabilities: avila_core_runner::parse_capabilities(&capabilities)?,
                workspace,
                reuse: !no_reuse,
                plan_only: plan,
                inputs: avila_core_runner::parse_inputs(&inputs)?,
                environment: avila_core_runner::parse_environment(&environment)?,
                log,
                expected_manifest_sha256: expect_manifest,
                attempt,
                hash_cache,
                trust_root,
                runner_key,
            };
            let report = avila_core_runner::execute_case(&case, &options)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", avila_core_runner::human_summary(&report));
            }
            if !report.succeeded() {
                return Ok(ExitCode::from(1));
            }
        }
        Command::Explain { code, all } => match (code, all) {
            (None, true) => {
                let mut entries: Vec<_> = DIAGNOSTIC_CATALOG
                    .iter()
                    .chain(RUNTIME_DIAGNOSTIC_CATALOG)
                    .collect();
                entries.sort_by_key(|entry| entry.code);
                println!("{}", serde_json::to_string_pretty(&entries)?);
            }
            (Some(code), false) => match explain(&code).or_else(|| explain_runtime(&code)) {
                Some(entry) => println!("{}", serde_json::to_string_pretty(entry)?),
                None => {
                    return Err(format!(
                        "`{code}` is not a finding code Core emits; run `avila-core explain --all` for the catalog"
                    )
                    .into());
                }
            },
            _ => return Err("pass exactly one code, or `--all` for the whole catalog".into()),
        },
        Command::Keys { command } => {
            println!("{}", serde_json::to_string_pretty(&run_keys(command)?)?);
        }
        Command::Sign { command } => {
            println!("{}", serde_json::to_string_pretty(&run_sign(command)?)?);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Performs a `keys` subcommand and returns exactly the JSON document the
/// CLI prints, so a test can assert on its fields without capturing stdout.
fn run_keys(command: KeysCommand) -> Result<serde_json::Value, Box<dyn Error>> {
    match command {
        KeysCommand::Generate { role, out } => {
            let role: KeyRole = role.into();
            let dir = out.unwrap_or_else(default_key_dir);
            fs::create_dir_all(&dir)?;
            let seed_path = dir.join(format!("{role}.seed"));
            let public_key_path = dir.join(format!("{role}.pub"));
            if seed_path.exists() {
                return Err(format!(
                    "refusing to overwrite existing seed file `{}`; move or delete it first",
                    seed_path.display()
                )
                .into());
            }
            let pair = signature::generate_keypair(role)?;
            fs::write(&seed_path, pair.seed)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&seed_path, fs::Permissions::from_mode(0o600))?;
            }
            fs::write(&public_key_path, format!("{}\n", pair.public_key_hex))?;
            Ok(serde_json::json!({
                "role": role.to_string(),
                "key_id": pair.key_id,
                "public_key_hex": pair.public_key_hex,
                "seed_path": seed_path.display().to_string(),
                "public_key_path": public_key_path.display().to_string(),
            }))
        }
        KeysCommand::Show { file } => {
            let bytes = fs::read(&file)?;
            let public_key_hex = if bytes.len() == 32 {
                let seed: [u8; 32] = bytes
                    .try_into()
                    .expect("length checked above to be exactly 32");
                signature::public_key_hex_from_seed(&seed)
            } else {
                String::from_utf8(bytes)
                    .map_err(|_| {
                        format!(
                            "`{}` is neither a 32-byte seed nor a UTF-8 hex public key file",
                            file.display()
                        )
                    })?
                    .trim()
                    .to_string()
            };
            let key_id = signature::key_id_from_public_hex(&public_key_hex)?;
            Ok(serde_json::json!({
                "public_key_hex": public_key_hex,
                "key_id": key_id,
            }))
        }
    }
}

/// The document id `sign manifest` uses for the requester signature it
/// binds. Running the command again replaces this exact entry, so signing
/// is idempotent under repeated invocation on an otherwise unchanged
/// manifest.
const MANIFEST_SIGNATURE_DOCUMENT_ID: &str = "signature-manifest";

/// Performs a `sign` subcommand and returns exactly the JSON document the
/// CLI prints, so a test can assert on its fields without capturing stdout.
fn run_sign(command: SignCommand) -> Result<serde_json::Value, Box<dyn Error>> {
    match command {
        SignCommand::Manifest { case, key } => {
            let manifest_path = if case.is_dir() {
                case.join("package.json")
            } else {
                case.clone()
            };
            let case_dir = manifest_path
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            let manifest_bytes = fs::read(&manifest_path)?;
            let mut manifest: avila_core_evidence::CasePackageManifest =
                serde_json::from_slice(&manifest_bytes)?;
            let seed = signature::parse_seed_bytes(&fs::read(&key)?)?;

            // The digest is computed over the manifest as `write_manifest`'s
            // struct-based serialization will actually render it (not the
            // raw file bytes, which may predate any struct round-trip and
            // so omit fields the struct always writes, such as an empty
            // `free_inputs`), with the not-yet-added (or, on a re-sign, the
            // already-bound) signature entry removed. This is what makes
            // signing idempotent and later verification, which always reads
            // the struct-normalized bytes back from disk, reproduce the
            // identical digest (ADR-0015 clause 3).
            let normalized_bytes = serde_json::to_vec(&manifest)?;
            let digest = signature::manifest_signing_digest(
                &normalized_bytes,
                MANIFEST_SIGNATURE_DOCUMENT_ID,
            )?;
            let signed_document_sha256 = format!("sha256:{}", hex_encode(&digest));
            let document = signature::build_signature_document(
                &seed,
                "manifest",
                manifest.case_id.clone(),
                signed_document_sha256,
                &digest,
            );
            let mut document_bytes = serde_json::to_vec_pretty(&document)?;
            document_bytes.push(b'\n');

            let signatures_dir = case_dir.join("signatures");
            fs::create_dir_all(&signatures_dir)?;
            let signature_path = signatures_dir.join("manifest.sig.json");
            fs::write(&signature_path, &document_bytes)?;
            let signature_document_sha256 = format!("sha256:{}", sha256_hex(&document_bytes));

            manifest
                .documents
                .retain(|document| document.document_id != MANIFEST_SIGNATURE_DOCUMENT_ID);
            manifest
                .documents
                .push(avila_core_evidence::PackageDocument {
                    document_id: MANIFEST_SIGNATURE_DOCUMENT_ID.into(),
                    role: "signature".into(),
                    path: "signatures/manifest.sig.json".into(),
                    sha256: signature_document_sha256,
                    step_id: None,
                });
            let mut manifest_bytes_out = serde_json::to_vec_pretty(&manifest)?;
            manifest_bytes_out.push(b'\n');
            fs::write(&manifest_path, &manifest_bytes_out)?;

            Ok(serde_json::json!({
                "case_id": manifest.case_id,
                "signed_document_sha256": document.signed_document.sha256,
                "key_id": document.key_id,
                "signature_path": signature_path.display().to_string(),
            }))
        }
        SignCommand::Receipt { case, step, key } => {
            let manifest_path = if case.is_dir() {
                case.join("package.json")
            } else {
                case.clone()
            };
            let case_dir = manifest_path
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            let manifest_bytes = fs::read(&manifest_path)?;
            let mut manifest: avila_core_evidence::CasePackageManifest =
                serde_json::from_slice(&manifest_bytes)?;
            let seed = signature::parse_seed_bytes(&fs::read(&key)?)?;

            let receipt_document = manifest
                .documents
                .iter()
                .find(|document| {
                    document.role == "execution_receipt"
                        && document.step_id.as_deref() == Some(step.as_str())
                })
                .ok_or_else(|| {
                    format!("no committed execution_receipt document names step `{step}`")
                })?
                .clone();
            let receipt_bytes = fs::read(case_dir.join(&receipt_document.path))?;
            let actual_sha256 = format!("sha256:{}", sha256_hex(&receipt_bytes));
            if actual_sha256 != receipt_document.sha256 {
                return Err(format!(
                    "receipt `{}` on disk hashes to {actual_sha256}, but the manifest binds {}; rehash before signing",
                    receipt_document.path, receipt_document.sha256
                )
                .into());
            }
            let digest = signature::digest_from_prefixed(&actual_sha256)?;
            let document = signature::build_signature_document(
                &seed,
                "execution_receipt",
                step.clone(),
                actual_sha256,
                &digest,
            );
            let mut document_bytes = serde_json::to_vec_pretty(&document)?;
            document_bytes.push(b'\n');

            let signatures_dir = case_dir.join("signatures");
            fs::create_dir_all(&signatures_dir)?;
            let signature_relative_path = format!("signatures/{step}-receipt.sig.json");
            let signature_path = case_dir.join(&signature_relative_path);
            fs::write(&signature_path, &document_bytes)?;
            let signature_document_sha256 = format!("sha256:{}", sha256_hex(&document_bytes));

            let signature_document_id = format!("signature-receipt-{step}");
            manifest
                .documents
                .retain(|document| document.document_id != signature_document_id);
            manifest
                .documents
                .push(avila_core_evidence::PackageDocument {
                    document_id: signature_document_id,
                    role: "signature".into(),
                    path: signature_relative_path,
                    sha256: signature_document_sha256,
                    step_id: None,
                });
            let mut manifest_bytes_out = serde_json::to_vec_pretty(&manifest)?;
            manifest_bytes_out.push(b'\n');
            fs::write(&manifest_path, &manifest_bytes_out)?;

            Ok(serde_json::json!({
                "case_id": manifest.case_id,
                "step_id": step,
                "signed_document_sha256": document.signed_document.sha256,
                "key_id": document.key_id,
                "signature_path": signature_path.display().to_string(),
            }))
        }
    }
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

fn attempt_request(
    attempt_id: Option<String>,
    parent_attempt_id: Option<String>,
    candidate_input: Option<String>,
) -> Result<Option<avila_core_runner::AttemptLineageRequest>, Box<dyn Error>> {
    match attempt_id {
        Some(attempt_id) => Ok(Some(avila_core_runner::AttemptLineageRequest {
            attempt_id,
            parent_attempt_id,
            candidate_input: candidate_input.unwrap_or_else(|| "candidate".into()),
        })),
        None if parent_attempt_id.is_some() || candidate_input.is_some() => {
            Err("`--parent-attempt` and `--candidate-input` require `--attempt ID`".into())
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_profile_report_identifies_every_executable_vector() {
        let report = semantic_profile_report().unwrap();
        assert_eq!(report.semantic_profile, SEMANTIC_PROFILE);
        assert_eq!(report.status, "draft");
        assert_eq!(report.total_vectors, 91);
        assert_eq!(report.implemented_vector_sets.len(), 4);
        assert_eq!(report.total_compiler_fixtures, 68);
        assert_eq!(report.implemented_compiler_fixture_sets.len(), 5);
        assert_eq!(report.total_campaign_fixtures, 15);
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
        assert_eq!(explain_runtime("CORE-X2601").unwrap().code, "CORE-X2601");
        assert!(explain("CORE-X9999").is_none());
        assert!(!DIAGNOSTIC_CATALOG.is_empty());
        assert!(!RUNTIME_DIAGNOSTIC_CATALOG.is_empty());
    }

    #[test]
    fn cli_canonicalization_uses_the_authoritative_reader() {
        assert_eq!(
            canonicalize_json(br#"{"z":1,"a":2}"#).unwrap(),
            br#"{"a":2,"z":1}"#
        );
        assert!(canonicalize_json(br#"{"value":25.0}"#).is_err());
    }

    #[test]
    fn attempt_flags_form_one_explicit_lineage_request() {
        let request = attempt_request(Some("try-002".into()), Some("try-001".into()), None)
            .unwrap()
            .unwrap();
        assert_eq!(request.attempt_id, "try-002");
        assert_eq!(request.parent_attempt_id.as_deref(), Some("try-001"));
        assert_eq!(request.candidate_input, "candidate");
        assert!(attempt_request(None, Some("try-001".into()), None).is_err());
        assert!(attempt_request(None, None, Some("design".into())).is_err());
    }

    // --- ADR-0015: `keys` and `sign` (S-043) --------------------------

    /// A fresh, per-test scratch directory under the OS temp dir, removed
    /// when the guard drops so a failed assertion never leaks files into a
    /// later test run.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "avila-core-cli-test-{label}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
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

    fn is_lowercase_hex(text: &str, expected_len: usize) -> bool {
        text.len() == expected_len && text.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    #[test]
    fn keys_generate_writes_a_seed_and_public_key_with_a_well_formed_id() {
        let scratch = ScratchDir::new("keys-generate");
        let generated = run_keys(KeysCommand::Generate {
            role: KeyRoleArg::Requester,
            out: Some(scratch.0.clone()),
        })
        .unwrap();

        assert_eq!(generated["role"], "requester");
        let key_id = generated["key_id"].as_str().unwrap();
        let public_key_hex = generated["public_key_hex"].as_str().unwrap();
        // Key id: SHA-256 of the public key, hex encoded (64 chars). Public
        // key: 32 raw bytes, hex encoded (64 chars). Both lowercase, neither
        // carrying the `sha256:` content-digest prefix used elsewhere.
        assert!(is_lowercase_hex(key_id, 64), "key_id: {key_id}");
        assert!(
            is_lowercase_hex(public_key_hex, 64),
            "public_key_hex: {public_key_hex}"
        );
        assert!(!key_id.starts_with("sha256:"));

        let seed_path = scratch.join("requester.seed");
        let public_key_path = scratch.join("requester.pub");
        assert_eq!(generated["seed_path"], seed_path.display().to_string());
        assert_eq!(
            generated["public_key_path"],
            public_key_path.display().to_string()
        );
        let seed_bytes = fs::read(&seed_path).unwrap();
        assert_eq!(seed_bytes.len(), 32, "a seed file is exactly 32 raw bytes");
        assert_eq!(
            fs::read_to_string(&public_key_path).unwrap(),
            format!("{public_key_hex}\n")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&seed_path).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "a private seed must be readable only by its owner"
            );
        }

        // The key id is deterministically the SHA-256 of the public key
        // bytes: the CLI's own derivation must reproduce what it just wrote.
        assert_eq!(
            signature::key_id_from_public_hex(public_key_hex).unwrap(),
            key_id
        );

        // A second `generate` into the same, now-populated directory must
        // never silently overwrite a private key.
        let error = run_keys(KeysCommand::Generate {
            role: KeyRoleArg::Requester,
            out: Some(scratch.0.clone()),
        })
        .unwrap_err();
        assert!(error.to_string().contains("refusing to overwrite"));
    }

    #[test]
    fn keys_show_reports_the_same_key_id_from_either_the_seed_or_the_public_file() {
        let scratch = ScratchDir::new("keys-show");
        let generated = run_keys(KeysCommand::Generate {
            role: KeyRoleArg::Runner,
            out: Some(scratch.0.clone()),
        })
        .unwrap();
        let key_id = generated["key_id"].as_str().unwrap().to_string();
        let public_key_hex = generated["public_key_hex"].as_str().unwrap().to_string();

        let from_seed = run_keys(KeysCommand::Show {
            file: scratch.join("runner.seed"),
        })
        .unwrap();
        assert_eq!(from_seed["key_id"], key_id);
        assert_eq!(from_seed["public_key_hex"], public_key_hex);

        let from_public = run_keys(KeysCommand::Show {
            file: scratch.join("runner.pub"),
        })
        .unwrap();
        assert_eq!(from_public["key_id"], key_id);
        assert_eq!(from_public["public_key_hex"], public_key_hex);

        // Neither a seed's nor a key id's raw bytes are ever printed: only
        // the derived public key and key id appear in either report.
        assert_eq!(from_seed.as_object().unwrap().len(), 2, "{from_seed:?}");
    }

    #[test]
    fn keys_show_refuses_a_file_that_is_neither_a_seed_nor_hex_text() {
        let scratch = ScratchDir::new("keys-show-malformed");
        // Not 32 bytes (so not a seed) and not valid UTF-8 (so not hex text
        // either), unlike a too-short or too-long ASCII string, which would
        // instead reach the hex decoder and fail there with a different,
        // still-honest message.
        let path = scratch.join("not-a-key");
        fs::write(&path, [0xff_u8, 0xfe, 0x00, 0x01]).unwrap();
        let error = run_keys(KeysCommand::Show { file: path }).unwrap_err();
        assert!(
            error.to_string().contains("neither a 32-byte seed"),
            "{error}"
        );

        // Valid UTF-8 that is merely not valid hex is refused too, just by
        // the hex decoder further along instead of this message.
        let non_hex_ascii = scratch.join("not-hex-text");
        fs::write(&non_hex_ascii, b"not hexadecimal at all").unwrap();
        assert!(
            run_keys(KeysCommand::Show {
                file: non_hex_ascii
            })
            .is_err()
        );
    }

    /// A minimal on-disk case package: just enough for `sign manifest` and
    /// `sign receipt` to have a `package.json` and, when requested, one
    /// committed `execution_receipt` document to sign.
    fn write_minimal_case(
        scratch: &ScratchDir,
        case_id: &str,
        receipt_step: Option<&str>,
    ) -> PathBuf {
        let case_dir = scratch.join(case_id);
        fs::create_dir_all(&case_dir).unwrap();
        let mut documents = Vec::new();
        if let Some(step) = receipt_step {
            let receipt_bytes = format!("{{\"receipt for\":\"{step}\"}}").into_bytes();
            fs::create_dir_all(case_dir.join("receipts")).unwrap();
            fs::write(
                case_dir.join("receipts").join(format!("{step}.json")),
                &receipt_bytes,
            )
            .unwrap();
            documents.push(avila_core_evidence::PackageDocument {
                document_id: format!("{case_id}-{step}-receipt"),
                role: "execution_receipt".into(),
                path: format!("receipts/{step}.json"),
                sha256: format!("sha256:{}", sha256_hex(&receipt_bytes)),
                step_id: Some(step.into()),
            });
        }
        let manifest = avila_core_evidence::CasePackageManifest {
            schema_version: avila_core_evidence::CASE_PACKAGE_SCHEMA_VERSION.into(),
            case_id: case_id.into(),
            title: "CLI signing test fixture".into(),
            documents,
            artifacts: Vec::new(),
            capabilities: Vec::new(),
            executions: Vec::new(),
            free_inputs: Vec::new(),
            coverage: None,
            limitations: Vec::new(),
        };
        let mut bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        bytes.push(b'\n');
        fs::write(case_dir.join("package.json"), bytes).unwrap();
        case_dir
    }

    fn generate_key(scratch: &ScratchDir, name: &str, role: KeyRoleArg) -> PathBuf {
        run_keys(KeysCommand::Generate {
            role,
            out: Some(scratch.join(name)),
        })
        .unwrap();
        let role: KeyRole = role.into();
        scratch.join(name).join(format!("{role}.seed"))
    }

    #[test]
    fn sign_manifest_writes_a_schema_valid_signature_and_is_idempotent() {
        let scratch = ScratchDir::new("sign-manifest");
        let case_dir = write_minimal_case(&scratch, "CLI-SIGN-TEST", None);
        let key_path = generate_key(&scratch, "keys", KeyRoleArg::Requester);

        let result = run_sign(SignCommand::Manifest {
            case: case_dir.clone(),
            key: key_path.clone(),
        })
        .unwrap();
        assert_eq!(result["case_id"], "CLI-SIGN-TEST");
        let key_id = result["key_id"].as_str().unwrap().to_string();
        assert!(is_lowercase_hex(&key_id, 64), "key_id: {key_id}");
        let signed_document_sha256 = result["signed_document_sha256"].as_str().unwrap();
        assert!(signed_document_sha256.starts_with("sha256:"));

        let signature_path = case_dir.join("signatures").join("manifest.sig.json");
        assert_eq!(
            result["signature_path"],
            signature_path.display().to_string()
        );
        let document: signature::SignatureDocument =
            serde_json::from_slice(&fs::read(&signature_path).unwrap()).unwrap();
        assert_eq!(document.schema_version, signature::SIGNATURE_SCHEMA_VERSION);
        assert_eq!(document.algorithm, signature::ALGORITHM_ED25519);
        assert_eq!(document.key_id, key_id);
        assert_eq!(document.signed_document.role, "manifest");
        assert_eq!(document.signed_document.document_id, "CLI-SIGN-TEST");
        assert_eq!(document.signed_document.sha256, signed_document_sha256);
        assert!(
            is_lowercase_hex(&document.signature_hex, 128),
            "an Ed25519 signature is 64 raw bytes, hex encoded: {}",
            document.signature_hex
        );
        assert!(!document.notice.is_empty());

        // The manifest now binds the signature document as `documents[]`.
        let manifest: avila_core_evidence::CasePackageManifest =
            serde_json::from_slice(&fs::read(case_dir.join("package.json")).unwrap()).unwrap();
        let bound = manifest
            .documents
            .iter()
            .find(|document| document.document_id == "signature-manifest")
            .unwrap();
        assert_eq!(bound.role, "signature");
        assert_eq!(bound.path, "signatures/manifest.sig.json");

        // Re-running against the now-signed manifest is idempotent: Ed25519
        // is deterministic, and the digest rule excludes the entry it is
        // about to write, so the signature bytes reproduce exactly.
        let second = run_sign(SignCommand::Manifest {
            case: case_dir.clone(),
            key: key_path,
        })
        .unwrap();
        assert_eq!(second, result);
        let document_again: signature::SignatureDocument =
            serde_json::from_slice(&fs::read(&signature_path).unwrap()).unwrap();
        assert_eq!(document_again, document);
    }

    #[test]
    fn a_manifest_signed_by_the_wrong_role_key_is_refused_at_verification() {
        // The CLI itself signs with whatever seed it is given: `sign
        // manifest` never asks whether a key is a requester's. The role
        // check that matters happens when a trust root verifies the
        // resulting signature document against an expected role — exactly
        // what `run --trust-root` does before compiling a package (S-030,
        // ADR-0015 clause 3).
        let scratch = ScratchDir::new("sign-manifest-wrong-role");
        let case_dir = write_minimal_case(&scratch, "CLI-WRONG-ROLE", None);
        let runner_key_path = generate_key(&scratch, "keys", KeyRoleArg::Runner);
        let runner_public_hex = fs::read_to_string(scratch.join("keys").join("runner.pub"))
            .unwrap()
            .trim()
            .to_string();
        let runner_key_id = signature::key_id_from_public_hex(&runner_public_hex).unwrap();

        run_sign(SignCommand::Manifest {
            case: case_dir.clone(),
            key: runner_key_path,
        })
        .unwrap();
        let document: signature::SignatureDocument = serde_json::from_slice(
            &fs::read(case_dir.join("signatures").join("manifest.sig.json")).unwrap(),
        )
        .unwrap();

        // A trust root that lists this exact key, but only under `runner`.
        let trust_root = signature::TrustRoot {
            schema_version: signature::TRUST_ROOT_SCHEMA_VERSION.into(),
            keys: vec![signature::TrustRootEntry {
                key_id: runner_key_id.clone(),
                public_key_hex: runner_public_hex,
                role: KeyRole::Runner,
            }],
        };

        // Verifying under its real role succeeds: the signature itself is
        // genuine.
        assert_eq!(
            signature::verify_signature_document(&document, &trust_root, KeyRole::Runner).unwrap(),
            runner_key_id
        );
        // Verifying under the role a manifest signature must carry does
        // not: the key is well-formed and the signature is genuine, but it
        // is not listed as a requester key.
        let error =
            signature::verify_signature_document(&document, &trust_root, KeyRole::Requester)
                .unwrap_err();
        assert!(
            matches!(error, signature::SignatureError::KeyNotListed { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn sign_receipt_writes_a_signature_document_bound_to_its_step() {
        let scratch = ScratchDir::new("sign-receipt");
        let case_dir = write_minimal_case(&scratch, "CLI-SIGN-RECEIPT", Some("screen"));
        let key_path = generate_key(&scratch, "keys", KeyRoleArg::Runner);

        let result = run_sign(SignCommand::Receipt {
            case: case_dir.clone(),
            step: "screen".into(),
            key: key_path,
        })
        .unwrap();
        assert_eq!(result["case_id"], "CLI-SIGN-RECEIPT");
        assert_eq!(result["step_id"], "screen");

        let signature_path = case_dir.join("signatures").join("screen-receipt.sig.json");
        let document: signature::SignatureDocument =
            serde_json::from_slice(&fs::read(&signature_path).unwrap()).unwrap();
        assert_eq!(document.signed_document.role, "execution_receipt");
        assert_eq!(document.signed_document.document_id, "screen");
        let manifest: avila_core_evidence::CasePackageManifest =
            serde_json::from_slice(&fs::read(case_dir.join("package.json")).unwrap()).unwrap();
        let receipt_document = manifest
            .documents
            .iter()
            .find(|document| document.step_id.as_deref() == Some("screen"))
            .unwrap();
        assert_eq!(document.signed_document.sha256, receipt_document.sha256);
    }

    #[test]
    fn sign_receipt_refuses_a_step_the_manifest_does_not_name() {
        let scratch = ScratchDir::new("sign-receipt-unknown-step");
        let case_dir = write_minimal_case(&scratch, "CLI-NO-SUCH-STEP", Some("screen"));
        let key_path = generate_key(&scratch, "keys", KeyRoleArg::Runner);
        let error = run_sign(SignCommand::Receipt {
            case: case_dir,
            step: "transport".into(),
            key: key_path,
        })
        .unwrap_err();
        assert!(error.to_string().contains("no committed execution_receipt"));
    }

    #[test]
    fn sign_receipt_refuses_a_receipt_whose_bytes_no_longer_match_the_manifest() {
        let scratch = ScratchDir::new("sign-receipt-drifted");
        let case_dir = write_minimal_case(&scratch, "CLI-DRIFTED-RECEIPT", Some("screen"));
        let key_path = generate_key(&scratch, "keys", KeyRoleArg::Runner);
        fs::write(
            case_dir.join("receipts").join("screen.json"),
            b"{\"edited after binding\":true}",
        )
        .unwrap();
        let error = run_sign(SignCommand::Receipt {
            case: case_dir,
            step: "screen".into(),
            key: key_path,
        })
        .unwrap_err();
        assert!(error.to_string().contains("rehash before signing"));
    }
}
