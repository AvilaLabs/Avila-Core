//! The immutable evaluation context and the checked values produced under
//! it (ADR-0026).
//!
//! `EvaluationContext` binds one compiled snapshot, one registry, one
//! claims document, the compiled policy, the qualification material the
//! claims carry, and the artifact observations actually performed. Every
//! identity in the record is computed by `bind` from the supplied bytes —
//! the context is the single place where "these documents under these
//! observations" becomes a named thing with an identity (`context_sha256`).
//!
//! `CheckedAdmissions` and `DerivedVerdicts` can only be produced by
//! `admit`/`derive_verdicts` on a context, and verdict derivation refuses
//! admissions minted under a different context identity — mixing contexts
//! is an explicit refusal, not a silent composition.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use avila_core_kernel::{SEMANTIC_PROFILE, canonicalize_json};

use super::document::ClaimsDocument;
use super::{AdmissionRecord, VerdictBoundary, VerdictRecord};
use crate::compile::registry::RegistryIndex;
use crate::compile::schema::SchemaDocument;
use crate::compile::source::read_document;
use crate::compile::{CompiledContract, prefixed_sha256};
use crate::diagnostic::CoreDiagnostic;
use crate::document::{ExecutionPolicy, RegistrySnapshot};

/// The schema the context record declares inside a derivation.
pub const CONTEXT_SCHEMA_VERSION: &str = "avila.core/evaluation-context/v0.1-draft";

/// Artifact and receipt bytes actually supplied and re-hashed for this
/// evaluation.
///
/// The only way a digest enters either set is `check_bytes`/`check_file`
/// (artifacts) and `check_receipt`/`check_receipt_file` (receipts), which
/// hash the supplied bytes and record what they computed. A caller cannot
/// assert a digest it did not produce — a bare string is not a byte-check
/// witness.
///
/// An empty artifact set is a digest-only evaluation: no `artifact` check
/// field is emitted and the records are unchanged from the historical
/// form. An empty receipt set binds no receipt identities.
#[derive(Debug, Clone, Default)]
pub struct ArtifactObservations {
    observed: BTreeSet<String>,
    receipts: BTreeSet<String>,
}

impl ArtifactObservations {
    /// No supplied artifacts or receipts — the digest-only evaluation.
    pub fn none() -> Self {
        Self::default()
    }

    /// Hash the supplied bytes and record the digest they produced.
    /// Returns the recorded `sha256:…` identity.
    pub fn check_bytes(&mut self, bytes: &[u8]) -> String {
        let digest = prefixed_sha256(bytes);
        self.observed.insert(digest.clone());
        digest
    }

    /// Read `path` and check its bytes. An unreadable file records
    /// nothing and reports the I/O error.
    pub fn check_file(&mut self, path: &Path) -> std::io::Result<String> {
        let bytes = std::fs::read(path)?;
        Ok(self.check_bytes(&bytes))
    }

    /// Hash the supplied receipt bytes and record the digest they
    /// produced. Returns the recorded `sha256:…` identity.
    pub fn check_receipt(&mut self, bytes: &[u8]) -> String {
        let digest = prefixed_sha256(bytes);
        self.receipts.insert(digest.clone());
        digest
    }

    /// Read `path` and check its bytes as a receipt. An unreadable file
    /// records nothing and reports the I/O error.
    pub fn check_receipt_file(&mut self, path: &Path) -> std::io::Result<String> {
        let bytes = std::fs::read(path)?;
        Ok(self.check_receipt(&bytes))
    }

    /// Whether a supplied artifact hashed to exactly this identity.
    pub fn contains(&self, digest: &str) -> bool {
        self.observed.contains(digest)
    }

    /// Whether a supplied receipt hashed to exactly this identity.
    pub fn contains_receipt(&self, digest: &str) -> bool {
        self.receipts.contains(digest)
    }

    pub fn is_empty(&self) -> bool {
        self.observed.is_empty() && self.receipts.is_empty()
    }

    /// The artifact digests this set observed, sorted.
    pub fn observed(&self) -> impl Iterator<Item = &String> {
        self.observed.iter()
    }

    /// The receipt digests this set observed, sorted.
    pub fn receipts(&self) -> impl Iterator<Item = &String> {
        self.receipts.iter()
    }
}

/// The bound-material record a derivation embeds — every identity the
/// rule applications were checked under. The qualifications list names
/// each `id@revision:digest` envelope the bound claims carry; the artifact
/// observations list names each digest that was byte-checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRecord {
    pub schema_version: String,
    pub semantic_profile: String,
    pub evaluator: String,
    pub compiled_snapshot_sha256: String,
    pub registry_id: String,
    pub registry_revision: u64,
    pub registry_sha256: String,
    pub claims_sha256: String,
    pub execution_policy: ExecutionPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qualifications: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_observations: Vec<String>,
    /// Digests of the receipt documents byte-checked under this context.
    /// Absent when the evaluation checked none (digest-only callers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipts: Vec<String>,
}

/// Why `bind` refused to create a context. Each failure is a bounded,
/// explained outcome — never a silently weaker context.
#[derive(Debug)]
pub enum ContextError {
    /// The supplied registry bytes do not hash to the snapshot the
    /// contract was compiled against — a mixed context.
    RegistryMismatch { expected: String, found: String },
    /// The registry bytes could not be canonicalized or decoded.
    RegistryUnreadable(String),
    /// The claims bytes could not be read authoritatively; the compile
    /// findings explain why.
    ClaimsUnreadable { findings: Vec<CoreDiagnostic> },
    /// The claims document is readable but was produced for a different
    /// compiled snapshot — binding it would mint a context claiming
    /// provenance the claims do not have.
    SnapshotMismatch {
        expected: String,
        found: String,
        claims_sha256: String,
    },
}

/// Why an authoritative use refused.
#[derive(Debug)]
pub enum EvaluationError {
    /// The admissions were minted under a different context identity.
    ContextMismatch { expected: String, found: String },
}

/// An immutable evaluation context: the checked contract, the registry
/// and claims it was bound to, the compiled policy, the qualification
/// identities the claims carry, and the artifact observations supplied.
/// Constructed only by `bind`; context equality is `context_sha256`, not
/// an address or a lifetime.
#[derive(Debug)]
pub struct EvaluationContext {
    compiled: CompiledContract,
    registry: RegistrySnapshot,
    claims: ClaimsDocument,
    observations: ArtifactObservations,
    record: ContextRecord,
    context_sha256: String,
}

impl EvaluationContext {
    /// Bind the checked contract, registry bytes, claims bytes, and
    /// supplied observations into one context. The claims bytes are parsed
    /// and hashed here; the registry must hash to the digest the contract
    /// was compiled against.
    pub fn bind(
        compiled: &CompiledContract,
        registry_bytes: &[u8],
        claims_bytes: &[u8],
        observations: ArtifactObservations,
    ) -> Result<Self, ContextError> {
        let registry_canonical = canonicalize_json(registry_bytes)
            .map_err(|error| ContextError::RegistryUnreadable(error.to_string()))?;
        let registry_sha256 = prefixed_sha256(registry_canonical);
        if registry_sha256 != compiled.registry_sha256() {
            return Err(ContextError::RegistryMismatch {
                expected: compiled.registry_sha256().to_string(),
                found: registry_sha256,
            });
        }
        let registry: RegistrySnapshot = serde_json::from_slice(registry_bytes)
            .map_err(|error| ContextError::RegistryUnreadable(error.to_string()))?;

        let mut identities = Vec::new();
        let mut findings = Vec::new();
        let Some(claims) = read_document::<ClaimsDocument>(
            "claims",
            SchemaDocument::Claims,
            claims_bytes,
            &mut identities,
            &mut findings,
        ) else {
            return Err(ContextError::ClaimsUnreadable { findings });
        };
        let claims_sha256 = identities
            .iter()
            .find(|identity| identity.document == "claims")
            .map(|identity| identity.sha256.clone())
            .expect("a parsed claims document has an identity");
        if claims.compiled_snapshot_sha256 != compiled.snapshot_sha256() {
            return Err(ContextError::SnapshotMismatch {
                expected: compiled.snapshot_sha256().to_string(),
                found: claims.compiled_snapshot_sha256.clone(),
                claims_sha256,
            });
        }

        let mut qualifications: BTreeSet<String> = BTreeSet::new();
        for claim in &claims.claims {
            if let Some(qualification) = &claim.qualification {
                qualifications.insert(format!(
                    "{}@{}:{}",
                    qualification.qualification_id, qualification.revision, qualification.sha256
                ));
            }
        }

        let record = ContextRecord {
            schema_version: CONTEXT_SCHEMA_VERSION.into(),
            semantic_profile: SEMANTIC_PROFILE.into(),
            evaluator: super::EVALUATOR_ID.into(),
            compiled_snapshot_sha256: compiled.snapshot_sha256().to_string(),
            registry_id: registry.registry_id.clone(),
            registry_revision: registry.revision,
            registry_sha256,
            claims_sha256,
            execution_policy: compiled.execution_policy().clone(),
            qualifications: qualifications.into_iter().collect(),
            artifact_observations: observations.observed().cloned().collect(),
            receipts: observations.receipts().cloned().collect(),
        };
        let bytes = serde_json::to_vec(&record).expect("context record serializes");
        let context_sha256 =
            prefixed_sha256(canonicalize_json(&bytes).expect("context record canonicalizes"));

        Ok(Self {
            compiled: compiled.clone(),
            registry,
            claims,
            observations,
            record,
            context_sha256,
        })
    }

    /// The context identity — the canonical hash of the bound record.
    pub fn context_sha256(&self) -> &str {
        &self.context_sha256
    }

    /// The bound-material record embedded in derivations.
    pub fn record(&self) -> &ContextRecord {
        &self.record
    }

    pub fn compiled(&self) -> &CompiledContract {
        &self.compiled
    }

    pub fn claims(&self) -> &ClaimsDocument {
        &self.claims
    }

    pub fn observations(&self) -> &ArtifactObservations {
        &self.observations
    }

    /// The registry as a kind/type index for the evaluation passes.
    /// Rebuildable on demand — the index is an in-session view, never an
    /// identity.
    pub(crate) fn registry_index(&self, findings: &mut Vec<CoreDiagnostic>) -> RegistryIndex<'_> {
        RegistryIndex::build(&self.registry, findings)
    }

    /// The verdict boundary record for this context — the identities the
    /// report's `boundary` block has always carried.
    pub(crate) fn verdict_boundary(&self) -> VerdictBoundary {
        VerdictBoundary {
            semantic_profile: SEMANTIC_PROFILE.into(),
            compiler: self.compiled.compiler().to_string(),
            evaluator: super::EVALUATOR_ID.into(),
            compiled_snapshot_sha256: self.compiled.snapshot_sha256().to_string(),
            claims_sha256: self.record.claims_sha256.clone(),
        }
    }

    /// Run the SC-11 admission rules over the bound claims. Only this
    /// method produces `CheckedAdmissions`, and only under this context's
    /// identity.
    pub fn admit(&self) -> (CheckedAdmissions, Vec<CoreDiagnostic>) {
        let mut findings = Vec::new();
        let registry = self.registry_index(&mut findings);
        debug_assert!(
            findings.is_empty(),
            "a compiled contract has a valid registry"
        );
        let (records, applications) = super::admission::admit(self, &registry, &mut findings);
        (
            CheckedAdmissions {
                context_sha256: self.context_sha256.clone(),
                records,
                applications,
            },
            findings,
        )
    }

    /// Derive verdicts from admissions minted under this context.
    /// Admissions from any other context are refused.
    pub fn derive_verdicts(
        &self,
        admissions: &CheckedAdmissions,
    ) -> Result<DerivedVerdicts, EvaluationError> {
        if admissions.context_sha256 != self.context_sha256 {
            return Err(EvaluationError::ContextMismatch {
                expected: self.context_sha256.clone(),
                found: admissions.context_sha256.clone(),
            });
        }
        let mut findings = Vec::new();
        let registry = self.registry_index(&mut findings);
        debug_assert!(
            findings.is_empty(),
            "a compiled contract has a valid registry"
        );
        let boundary = self.verdict_boundary();
        Ok(super::verdicts::evaluate(
            self, &registry, admissions, &boundary,
        ))
    }
}

/// The admission records the checker produced under one context — plus
/// the rule applications that explain them. Constructed only by
/// `EvaluationContext::admit`.
///
/// ```compile_fail
/// // A checked admission cannot be fabricated downstream:
/// let admissions = avila_core_compiler::CheckedAdmissions::default();
/// ```
#[derive(Debug)]
pub struct CheckedAdmissions {
    context_sha256: String,
    records: Vec<AdmissionRecord>,
    applications: Vec<super::derivation::RuleApplication>,
}

impl CheckedAdmissions {
    /// The context identity these admissions were derived under.
    pub fn context_sha256(&self) -> &str {
        &self.context_sha256
    }

    /// The admission records, in the checker's deterministic order.
    pub fn records(&self) -> &[AdmissionRecord] {
        &self.records
    }

    /// The rule applications that produced the records.
    pub fn applications(&self) -> &[super::derivation::RuleApplication] {
        &self.applications
    }
}

/// The verdict records derived under one context, plus the rule
/// applications that explain them. Constructed only by
/// `EvaluationContext::derive_verdicts`, which refuses admissions minted
/// under a different context identity.
#[derive(Debug)]
pub struct DerivedVerdicts {
    pub(crate) context_sha256: String,
    pub(crate) records: Vec<VerdictRecord>,
    pub(crate) applications: Vec<super::derivation::RuleApplication>,
}

impl DerivedVerdicts {
    /// The context identity these verdicts were derived under.
    pub fn context_sha256(&self) -> &str {
        &self.context_sha256
    }

    /// The verdict records, one per requirement in canonical order.
    pub fn records(&self) -> &[VerdictRecord] {
        &self.records
    }

    /// The rule applications that produced the records.
    pub fn applications(&self) -> &[super::derivation::RuleApplication] {
        &self.applications
    }
}
