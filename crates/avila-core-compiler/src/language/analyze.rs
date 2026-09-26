//! The shared analysis operation (spec §9, §11): one function that admits a
//! program and its pinned method library, checks them against the language
//! rules, and returns every consequence — typed bindings, residual
//! assumptions, generated obligations, open goals, and blocking findings —
//! without executing anything.
//!
//! Admission precedes identity: a document is admitted only when it decodes
//! under the strict schema *and* passes semantic well-formedness (unique
//! identifiers, sequential references, declared vocabulary, well-formed
//! domains). A document with an admission-level finding publishes no
//! semantic identity in the analysis record.

use std::collections::{BTreeMap, BTreeSet};

use avila_core_kernel::{CanonicalJsonValue, ExactNumber, read_authoritative_json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::document::{
    ANALYSIS_SCHEMA_VERSION, ImportDecl, LANGUAGE_PROFILE, LIBRARY_SCHEMA_VERSION, LibraryDocument,
    MethodDecl, ObligationDecl, PROGRAM_SCHEMA_VERSION, ProgramDocument, ProvenanceDecl, RefDecl,
    RequirementDecl, SlotDecl, ValueDecl,
};
use super::eval::{self, Expression, FailureKind, KindProducts, PrimitiveRule, RuleFailure};
use super::execution;
use super::model::{
    ClaimModel, Enclosure, NumericValue, QuantityType, RelationMap, ScopedProposition,
    SemanticValue, ValueState,
};
use super::projection::{self, LanguageDocument};
use crate::diagnostic::{
    CORE_E8001, CORE_E8002, CORE_E8003, CORE_E8004, CORE_E8005, CORE_E8010, CORE_E8011, CORE_E8012,
    CORE_E8013, CORE_E8014, CORE_E8015, CORE_E8016, CORE_E8017, CORE_E8018, CORE_E8019, CORE_E8020,
    CORE_E8021, CORE_E8022, CORE_E8023, CORE_E8024, CORE_E8025, CORE_E8026, CORE_E8027, CORE_E8028,
};

/// Finding-kind → stable diagnostic code (the catalog in `catalog.rs`
/// explains each; `diagnostic.rs` declares the constants).
pub const FINDING_CODES: &[(&str, &str)] = &[
    ("malformed", CORE_E8001),
    ("library_pin_mismatch", CORE_E8002),
    ("undeclared_proposition", CORE_E8003),
    ("undeclared_method", CORE_E8004),
    ("undeclared_entity", CORE_E8005),
    ("type_mismatch", CORE_E8010),
    ("relation_absent", CORE_E8011),
    ("coverage", CORE_E8012),
    ("contradiction", CORE_E8013),
    ("premise_conflict", CORE_E8014),
    ("cyclic_witness", CORE_E8015),
    ("unsupported", CORE_E8016),
    ("obligation_unmet", CORE_E8017),
    ("obligation_refuted", CORE_E8018),
    ("precondition_refuted", CORE_E8019),
    ("malformed_import", CORE_E8020),
    ("hole", CORE_E8021),
    ("ambiguity", CORE_E8022),
    ("lifecycle_conflict", CORE_E8023),
    ("missing_argument", CORE_E8024),
    ("undeclared_projection", CORE_E8025),
    ("lifecycle_refused", CORE_E8026),
    ("budget", CORE_E8027),
    ("observation_foreign", CORE_E8028),
];

fn code_for(kind: &str) -> &'static str {
    FINDING_CODES
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, c)| *c)
        .unwrap_or(CORE_E8001)
}

/// Findings that refuse the lowered plan. `cyclic_witness` is deliberately
/// absent: the assumption stays residual and the conditional verdict stands.
/// A binding-scoped finding blocks only when the binding is reachable from
/// the program's requirements or goals — an unreachable hole stays
/// inspectable without refusing the plan.
const BLOCKING_KINDS: &[&str] = &[
    "malformed",
    "library_pin_mismatch",
    "undeclared_proposition",
    "undeclared_method",
    "undeclared_entity",
    "type_mismatch",
    "relation_absent",
    "coverage",
    "contradiction",
    "premise_conflict",
    "unsupported",
    "obligation_unmet",
    "obligation_refuted",
    "precondition_refuted",
    "malformed_import",
    "hole",
    "ambiguity",
    "lifecycle_conflict",
    "missing_argument",
    "undeclared_projection",
    "lifecycle_refused",
    "budget",
];

/// Admission kinds: a document emitting one is not admitted and publishes no
/// semantic identity — `budget` means the document could not be fully
/// checked, so nothing about its semantics is verified.
const ADMISSION_KINDS: &[&str] = &[
    "malformed",
    "malformed_import",
    "undeclared_proposition",
    "undeclared_method",
    "undeclared_entity",
    "budget",
];

/// Checks the profile can replay at analysis time (spec §9: a certificate is
/// admissible only where the kernel can replay it).
const SUPPORTED_CHECKS: &[&str] = &["interval_arithmetic"];

/// The closed provenance vocabulary (spec §9 trust table).
const SOURCE_KINDS: &[&str] = &["assertion", "certificate", "declared"];
const REQUIRES_KINDS: &[&str] = &[
    "domain_containment",
    "scope_check",
    "independence",
    "provenance_disjoint",
];
const LIFECYCLE_STATES: &[&str] = &["active", "superseded", "expired", "withdrawn"];
const SCOPE_KINDS: &[&str] = &["steady-state", "transient", "any"];
const RELATION_NAMES: &[&str] = &["geometry", "scenario", "material"];

/// Profile bounds (spec §11) — finite elaboration and admission budgets.
const MAX_BODY_STEPS: usize = 512;
const MAX_INPUTS: usize = 512;
const MAX_PREMISES: usize = 256;
const MAX_ASSUMPTIONS: usize = 512;
const MAX_REQUIREMENTS: usize = 256;
const MAX_OVER_MEMBERS: usize = 32;
const MAX_LIFECYCLE_ENTRIES: usize = 128;
const MAX_ENTITIES: usize = 1024;
const MAX_METHODS: usize = 256;
/// A premise's witness chain may not deepen beyond the premise set itself;
/// the bound is stated explicitly so deepening cannot hide a cycle.
const MAX_WITNESS_DEPTH: usize = MAX_PREMISES;

/// One emitted finding. `at` is a program term path (`body[0]`, `body[1]
/// .arguments.displacement`, `premises[p-model]`); `document` names which
/// admitted input it belongs to; `pointer` is the JSON pointer into that
/// document when the path resolves to a single position; `binding` scopes
/// the finding to a bound name for requirement reachability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageFinding {
    pub code: String,
    pub kind: String,
    pub document: String,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
    pub detail: String,
    pub blocking: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
}

/// §10.2 lifecycle material supplied alongside the documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LifecycleEntry {
    /// `library:<name>@<revision>` or `method:<library>/<method>@<revision>`.
    pub key: String,
    /// `active` | `superseded` | `expired` | `withdrawn`.
    pub state: String,
}

/// Non-document inputs to analysis.
#[derive(Debug, Default)]
pub struct AnalysisOptions {
    pub lifecycle: Vec<LifecycleEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObligationState {
    Discharged,
    /// Awaiting a runtime check — stays an obligation in the lowered plan.
    Runtime,
    /// No recorded basis discharges it at analysis time.
    Open,
    /// The record itself denies it.
    Refuted,
}

impl ObligationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Discharged => "discharged",
            Self::Runtime => "runtime",
            Self::Open => "open",
            Self::Refuted => "refuted",
        }
    }

    /// The `not_evaluated` rule a blocked verdict reports.
    fn blocked_rule(self) -> Option<&'static str> {
        match self {
            Self::Open => Some("not_evaluated.obligation_unmet"),
            Self::Refuted => Some("not_evaluated.obligation_refuted"),
            Self::Discharged | Self::Runtime => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObligationReport {
    pub id: String,
    pub kind: String,
    /// The invocation path this obligation belongs to (`body[0]`, with the
    /// method id appended for generated obligations).
    pub at: String,
    /// The body-step index, for joining obligations to plan steps.
    pub step: usize,
    pub state: String,
    /// The check the declaration names, when it carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// The authored rule expression (postcondition / containment operands).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    /// Program identifiers the obligation ranges over, where applicable.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub operands: Vec<String>,
    pub detail: String,
}

/// A structured dependency link: an operand, a discharging premise, the
/// applied method, or the step that produced the binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyLink {
    /// `operand` | `premise` | `method` | `import`.
    pub kind: String,
    /// Binding id, premise id, method id, or `body[i]` step position.
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingReport {
    #[serde(rename = "type")]
    pub ty: BindingTypeReport,
    pub value_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub residual_assumptions: Vec<ScopedAssumptionReport>,
    /// Recorded source provenance (§8.3 edge identifiers).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_edges: Vec<String>,
    /// Structured dependency links: operand bindings, applied method,
    /// discharging premises.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DependencyLink>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingTypeReport {
    pub quantity_kind: String,
    pub claim: String,
    pub relations: RelationMap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedAssumptionReport {
    pub proposition: String,
    pub at: RelationMap,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementReport {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<VerdictReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictReport {
    pub status: String,
    pub rule: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoalReport {
    pub state: String,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleEntryReport {
    pub key: String,
    /// The entry's actual declared state — never collapsed.
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleReport {
    pub at: String,
    pub method: String,
    /// `usable` | `absent` | `refused`.
    pub state: String,
    /// Each applicable entry with its actual state.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<LifecycleEntryReport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refusal_reasons: Vec<String>,
    /// Non-refusing notices, e.g. a `superseded` marker.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStepReport {
    /// Body position of the authored step.
    pub at: String,
    pub bind: String,
    /// `apply` | `infer` | `import`.
    pub operation: String,
    /// Method id or primitive rule name — absent for `import`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Resolved argument bindings: slot name → bound program identifier
    /// (apply), or positional list under `_0`/`_1` (infer).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub arguments: BTreeMap<String, String>,
    /// Obligation ids generated by this step.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub obligations: Vec<String>,
    /// Payload unit the step produces, where declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanReport {
    pub state: String,
    /// The lowered steps serving the program's requirements and goals —
    /// requirement-reachable steps only.
    pub steps: Vec<PlanStepReport>,
    pub runtime_obligations: Vec<ObligationReport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentIdentityReport {
    /// Always published — it commits to the bytes read.
    pub document_sha256: String,
    /// Present only for an admitted document: semantic identity is defined
    /// only for admitted documents (§10.1).
    pub semantic_sha256: Option<String>,
    /// Whether the document passed schema and semantic admission.
    pub admitted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramIdentityReport {
    /// Absent when the document could not be decoded far enough to carry one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub identity: DocumentIdentityReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryIdentityReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    #[serde(flatten)]
    pub identity: DocumentIdentityReport,
    /// Whether the supplied document reproduces the pin the program declares.
    pub pin_match: bool,
}

/// The `avila.core/language-analysis/v0.1-draft` record.
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageAnalysis {
    pub schema_version: String,
    pub profile: String,
    /// Always present — the identity record commits to the bytes read; an
    /// inadmissible document publishes `admitted: false` and no semantic
    /// identity.
    pub program: ProgramIdentityReport,
    pub library: LibraryIdentityReport,
    /// `clean` when no findings were emitted, else `findings`.
    pub status: String,
    pub findings: Vec<LanguageFinding>,
    pub bindings: BTreeMap<String, BindingReport>,
    pub obligations: Vec<ObligationReport>,
    pub requirements: BTreeMap<String, RequirementReport>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub goals: BTreeMap<String, GoalReport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lifecycle: Vec<LifecycleReport>,
    pub plan: PlanReport,
}

impl LanguageAnalysis {
    /// Canonical bytes of the analysis record — the artifact callers hash.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        avila_core_kernel::canonicalize_json(&bytes).map_err(|e| e.detail().to_string())
    }

    /// The report for a document pair that could not both be checked —
    /// identity records are still published, the plan is refused.
    fn refused(
        findings: Vec<LanguageFinding>,
        program: ProgramIdentityReport,
        library: LibraryIdentityReport,
        lifecycle: Vec<LifecycleReport>,
    ) -> Self {
        Self {
            schema_version: ANALYSIS_SCHEMA_VERSION.into(),
            profile: LANGUAGE_PROFILE.into(),
            program,
            library,
            status: "findings".into(),
            findings,
            bindings: BTreeMap::new(),
            obligations: Vec::new(),
            requirements: BTreeMap::new(),
            goals: BTreeMap::new(),
            lifecycle,
            plan: PlanReport {
                state: "refused".into(),
                steps: Vec::new(),
                runtime_obligations: Vec::new(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Admission and identity
// ---------------------------------------------------------------------------

fn sha256_of(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn canonical_bytes_of(value: &CanonicalJsonValue) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    avila_core_kernel::canonicalize_json(&bytes).map_err(|e| e.detail().to_string())
}

/// Result of the canonical read + strict decode + projection pipeline. The
/// record exists even when decoding fails — `document_sha256` always commits
/// to the bytes read — while `semantic_sha256` is populated only when the
/// document projected (identity publication is still gated on admission).
struct Checked<T> {
    typed: Option<T>,
    document_sha256: String,
    semantic_sha256: Option<String>,
}

fn push_finding_scoped(
    findings: &mut Vec<LanguageFinding>,
    document: &str,
    kind: &str,
    at: &str,
    detail: String,
    binding: Option<String>,
) {
    findings.push(LanguageFinding {
        code: code_for(kind).to_string(),
        kind: kind.to_string(),
        document: document.to_string(),
        at: at.to_string(),
        pointer: None,
        binding,
        detail,
        blocking: BLOCKING_KINDS.contains(&kind),
        candidates: None,
    });
}

/// Emits a program-document finding with a resolved JSON pointer.
fn program_finding_at(
    program: &ProgramDocument,
    findings: &mut Vec<LanguageFinding>,
    kind: &str,
    at: &str,
    detail: String,
    binding: Option<String>,
) {
    findings.push(LanguageFinding {
        code: code_for(kind).to_string(),
        kind: kind.to_string(),
        document: "program".into(),
        at: at.to_string(),
        pointer: program_pointer(program, at),
        binding,
        detail,
        blocking: BLOCKING_KINDS.contains(&kind),
        candidates: None,
    });
}

/// Provenance object admission (spec §9): the closed source vocabulary, and
/// the fields each kind must carry. A certificate is admissible only with a
/// replayable payload — a `sha256:` digest plus a check the profile supports.
/// All provenance objects in the grammar live in the program document.
/// Returns whether the object is admissible.
fn provenance_admissible(
    program: &ProgramDocument,
    source: &ProvenanceDecl,
    at: &str,
    binding: Option<String>,
    findings: &mut Vec<LanguageFinding>,
) -> bool {
    let mut ok = true;
    let mut emit = |kind: &str, detail: String| {
        ok = false;
        program_finding_at(program, findings, kind, at, detail, binding.clone());
    };
    match source.kind.as_str() {
        "assertion" | "declared" => {
            if source.party.is_none() {
                emit(
                    "malformed",
                    format!("`{}` provenance requires `party` attribution", source.kind),
                );
            }
        }
        "certificate" => {
            if !source.digest.as_deref().is_some_and(valid_sha256_digest) {
                emit(
                    "malformed",
                    "certificate provenance requires a `sha256:<hex>` digest".into(),
                );
            }
            match source.check.as_deref() {
                Some(check) if SUPPORTED_CHECKS.contains(&check) => {}
                Some(check) => emit(
                    "unsupported",
                    format!(
                        "certificate check `{check}` is not a replayable check {}; it cannot discharge",
                        SUPPORTED_CHECKS.join(", ")
                    ),
                ),
                None => emit(
                    "malformed",
                    "certificate provenance requires `check`".into(),
                ),
            }
        }
        other => emit(
            "malformed",
            format!(
                "unknown provenance kind `{other}` (expected {})",
                SOURCE_KINDS.join(", ")
            ),
        ),
    }
    ok
}

/// Canonical read + strict decode + schema-directed projection. A field the
/// schema does not declare — including an annotation-named field at an
/// undeclared position — fails decoding; nothing is silently dropped.
fn admit<T: serde::de::DeserializeOwned>(
    label: &str,
    bytes: &[u8],
    document: LanguageDocument,
    findings: &mut Vec<LanguageFinding>,
) -> Checked<T> {
    let canonical = match read_authoritative_json(bytes) {
        Ok(value) => value,
        Err(refusal) => {
            push_finding_scoped(
                findings,
                label,
                "malformed",
                label,
                format!("authoritative read refused: {}", refusal.detail()),
                None,
            );
            return Checked {
                typed: None,
                document_sha256: sha256_of(bytes),
                semantic_sha256: None,
            };
        }
    };
    let document_sha256 = match canonical_bytes_of(&canonical) {
        Ok(b) => sha256_of(&b),
        Err(e) => {
            push_finding_scoped(
                findings,
                label,
                "malformed",
                label,
                format!("canonicalization: {e}"),
                None,
            );
            return Checked {
                typed: None,
                document_sha256: sha256_of(bytes),
                semantic_sha256: None,
            };
        }
    };
    let semantic_sha256 = match projection::project_document(&canonical, document) {
        Ok(projected) => match canonical_bytes_of(&projected) {
            Ok(b) => Some(sha256_of(&b)),
            Err(e) => {
                push_finding_scoped(
                    findings,
                    label,
                    "malformed",
                    label,
                    format!("projection: {e}"),
                    None,
                );
                None
            }
        },
        Err(e) => {
            push_finding_scoped(
                findings,
                label,
                "malformed",
                label,
                format!("projection: {}", e.detail),
                None,
            );
            None
        }
    };
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let typed: Option<T> = match serde_path_to_error::deserialize::<_, T>(&mut de) {
        Ok(t) => Some(t),
        Err(error) => {
            let mut at = String::new();
            for segment in error.path().iter() {
                use serde_path_to_error::Segment;
                match segment {
                    Segment::Seq { index } => {
                        at.push_str(&format!("[{index}]"));
                    }
                    Segment::Map { key } => {
                        if !at.is_empty() {
                            at.push('.');
                        }
                        at.push_str(key);
                    }
                    _ => {}
                }
            }
            push_finding_scoped(
                findings,
                label,
                "malformed",
                &format!(
                    "{label}{}",
                    if at.is_empty() { at } else { format!(".{at}") }
                ),
                format!("schema decode: {}", error.inner()),
                None,
            );
            None
        }
    };
    Checked {
        typed,
        document_sha256,
        semantic_sha256,
    }
}

/// Declared sets are iterated in canonical order — the order the
/// projection would hash their elements in — so source order cannot shape
/// finding order, obligation positions, or residual-assumption order. The
/// sort key is the projected canonical encoding of the element:
/// annotation fields do not participate, matching §10.1 identity.
fn canonical_order<'a, T: Serialize>(
    items: &'a [T],
    element: &'static projection::Role,
) -> Vec<(usize, &'a T)> {
    let mut ordered: Vec<(usize, &T)> = items.iter().enumerate().collect();
    ordered.sort_by_cached_key(|(_, item)| {
        let raw = serde_json::to_value(item)
            .map(|mut v| {
                // `Option::None` serializes as `null`; the document grammar
                // writes an absent field instead — strip nulls so the bytes
                // the canonical reader sees match the authored form.
                strip_nulls(&mut v);
                v
            })
            .ok()
            .and_then(|v| serde_json::to_vec(&v).ok())
            .unwrap_or_default();
        // Degenerate path: fall back to the raw serialization rather than an
        // empty key — a failed projection still orders by content.
        if raw.is_empty() {
            return raw;
        }
        read_authoritative_json(&raw)
            .ok()
            .and_then(|value| projection::project_element(&value, element).ok())
            .and_then(|projected| canonical_bytes_of(&projected).ok())
            .unwrap_or(raw)
    });
    ordered
}

fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(strip_nulls);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// The analyzer
// ---------------------------------------------------------------------------

struct LibraryIndex<'a> {
    doc: &'a LibraryDocument,
    methods: BTreeMap<&'a str, &'a MethodDecl>,
    products: BTreeMap<(String, String), (String, String)>,
    /// proposition name → declared relation params.
    propositions: BTreeMap<&'a str, &'a [String]>,
}

impl KindProducts for LibraryIndex<'_> {
    fn product(&self, lhs: &str, rhs: &str) -> Option<(String, String)> {
        self.products
            .get(&(lhs.to_string(), rhs.to_string()))
            .cloned()
    }
}

impl<'a> LibraryIndex<'a> {
    fn new(doc: &'a LibraryDocument) -> Self {
        Self {
            doc,
            methods: doc.methods.iter().map(|m| (m.id.as_str(), m)).collect(),
            products: doc
                .kind_products
                .iter()
                .map(|p| {
                    (
                        (p.lhs_kind.clone(), p.rhs_kind.clone()),
                        (p.result_kind.clone(), p.result_unit.clone()),
                    )
                })
                .collect(),
            propositions: doc
                .propositions
                .iter()
                .map(|(k, v)| (k.as_str(), v.params.as_slice()))
                .collect(),
        }
    }

    fn proposition_known(&self, name: &str) -> bool {
        proposition_known(self.doc, name)
    }

    /// The proposition's declared relation params. Built-ins scope over the
    /// shared relation params of their members — any relation name is legal.
    fn proposition_params(&self, name: &str) -> Vec<String> {
        proposition_params(self.doc, name)
    }

    fn canonical_unit(&self, kind: &str) -> Option<String> {
        canonical_unit(self.doc, kind)
    }
}

/// Vocabulary lookups over a library document — free functions so
/// `ProgramChecker` can consult them without holding a [`LibraryIndex`].
fn proposition_known(doc: &LibraryDocument, name: &str) -> bool {
    doc.propositions.contains_key(name) || matches!(name, "independent" | "provenance_disjoint")
}

/// The proposition's declared relation params. Built-ins scope over the
/// shared relation params of their members — any relation name is legal.
fn proposition_params(doc: &LibraryDocument, name: &str) -> Vec<String> {
    doc.propositions
        .get(name)
        .map(|p| p.params.to_vec())
        .unwrap_or_else(|| RELATION_NAMES.iter().map(|s| s.to_string()).collect())
}

fn canonical_unit(doc: &LibraryDocument, kind: &str) -> Option<String> {
    doc.quantity_kinds
        .get(kind)
        .and_then(|k| k.canonical_unit.clone())
}

enum Usability {
    Usable,
    Absent,
    Refused(Vec<String>),
    Conflict,
}

/// What a discharged witness contributes to the conclusion: its residual
/// assumptions, its provenance edge, and the premise link.
#[derive(Debug, Default)]
struct DischargeSupport {
    assumptions: Vec<ScopedProposition>,
    edges: BTreeSet<String>,
    premises: Vec<String>,
}

struct Analyzer<'a> {
    program: &'a ProgramDocument,
    library: LibraryIndex<'a>,
    options: &'a AnalysisOptions,
    findings: Vec<LanguageFinding>,
    obligations: Vec<ObligationReport>,
    lifecycle: Vec<LifecycleReport>,
    goals: BTreeMap<String, GoalReport>,
    env: BTreeMap<String, SemanticValue>,
    /// binding name → `not_evaluated` rule when a generated obligation or an
    /// operand's block forbids verdicts derived from it.
    blocked: BTreeMap<String, String>,
    /// binding id → recorded source edges — populated for inputs, imports,
    /// and *derived* bindings alike (dependency edges are first-class).
    edges: BTreeMap<String, BTreeSet<String>>,
    /// binding id → operand bindings it was computed from (reachability).
    deps: BTreeMap<String, BTreeSet<String>>,
    /// binding id → structured dependency links for the report.
    depends: BTreeMap<String, Vec<DependencyLink>>,
    /// premise indexes barred from discharging (conflicted, cyclic, or
    /// unattributed).
    inadmissible_premises: BTreeSet<usize>,
    /// `(proposition, scope)` → admissible premise indexes able to
    /// discharge it — populated once premise admissibility is complete, so
    /// witness lookup never consults a barred premise.
    admissible_witnesses: BTreeMap<(String, RelationMap), Vec<usize>>,
    /// `(proposition, scope)` pairs both asserted and denied by program
    /// assumptions — every binding carrying one is blocked.
    contradicted: BTreeSet<(String, RelationMap)>,
    /// Step index → obligation ids it generated (plan linkage).
    step_obligations: BTreeMap<usize, Vec<String>>,
    /// Requirement ids that failed admission — reported `not_evaluated`.
    malformed_requirements: BTreeSet<String>,
    /// Binding the currently executing step produces — scopes findings for
    /// requirement reachability.
    current_binding: Option<String>,
    /// Support contributed by witnesses discharged inside the running step —
    /// drained into the result binding when the application completes.
    pending_support: DischargeSupport,
    /// Evaluation material (empty under plain `analyze`): application-site
    /// index → the supplied observation, plus the plan identity the
    /// observations must belong to.
    observations: BTreeMap<usize, execution::ObservationRecord>,
    expected_plan_sha256: Option<String>,
    /// Sites whose supplied observation passed the digest binding — their
    /// runtime obligations are decidable at `evaluate`.
    observed_sites: BTreeSet<usize>,
    /// Site index → the observed output value the obligations replay.
    observed_values: BTreeMap<usize, Option<NumericValue>>,
    /// `analyze` leaves verdicts pending; `evaluate` derives them.
    evaluating: bool,
    /// Observe/discharge applications recorded during an evaluation run.
    eval_events: Vec<execution::RuleOutcome>,
}

/// The shared analysis operation. Always returns a record: an inadmissible
/// document or a refused program produces findings, not a thrown error.
pub fn analyze_program(
    program_bytes: &[u8],
    library_bytes: &[u8],
    options: &AnalysisOptions,
) -> LanguageAnalysis {
    analyze_core(program_bytes, library_bytes, options, None, false).0
}

/// The triple `analyze_core` returns: the analysis record, its lowered
/// execution plan, and — under `evaluate` — the rule applications the
/// observation binding and verdict comparison performed.
type CoreOutcome = (
    LanguageAnalysis,
    execution::ExecutionPlanDocument,
    Vec<execution::RuleOutcome>,
);

/// The runtime material `evaluate` carries into a re-run of the analysis:
/// the identity of the plan the observations must answer, plus the
/// observation records keyed by application site once admitted.
struct EvalMaterial {
    plan_sha256: String,
    /// `body[{index}]` → observation; records whose `at` is not a body site
    /// are foreign and never enter this map.
    observations: BTreeMap<usize, execution::ObservationRecord>,
}

/// The analysis pipeline shared by `analyze` (no evaluation material —
/// `external` outputs stay `declared`), `plan` (same run, emitting the
/// execution plan), and `evaluate` (the same derivation replayed against
/// supplied observations). Returns the analysis record and the execution
/// plan document it lowers to.
fn analyze_core(
    program_bytes: &[u8],
    library_bytes: &[u8],
    options: &AnalysisOptions,
    eval_material: Option<&EvalMaterial>,
    evaluating: bool,
) -> CoreOutcome {
    let mut findings = Vec::new();
    let library = admit::<LibraryDocument>(
        "library",
        library_bytes,
        LanguageDocument::Library,
        &mut findings,
    );
    let program = admit::<ProgramDocument>(
        "program",
        program_bytes,
        LanguageDocument::Program,
        &mut findings,
    );

    let (Some(program_doc), Some(library_doc)) = (&program.typed, &library.typed) else {
        // Whatever decoded still gets its admission pass — a document that
        // fails to decode does not mask the other document's own findings,
        // nor the program-external lifecycle material.
        if let Some(doc) = &library.typed {
            LibraryChecker::new(doc, &mut findings).run();
        }
        if let Some(doc) = &program.typed {
            // Vocabulary-free program checks still run; vocabulary-dependent
            // checks are undecidable without the library and are skipped.
            ProgramChecker {
                program: doc,
                library: library.typed.as_ref(),
                findings: &mut findings,
                malformed_requirements: BTreeSet::new(),
            }
            .run();
        }
        check_lifecycle_options(&options.lifecycle, &mut findings);
        dedupe_findings(&mut findings);
        let library_admitted = library.typed.is_some()
            && !findings
                .iter()
                .any(|f| f.document == "library" && ADMISSION_KINDS.contains(&f.kind.as_str()));
        // A program cannot be admitted without its bound library's
        // vocabulary, so its semantic identity stays unpublished here.
        let program_admitted = false;
        if program.typed.is_some() {
            push_finding_scoped(
                &mut findings,
                "program",
                "library_pin_mismatch",
                "library",
                "supplied library is not admitted; its pin cannot be checked".into(),
                None,
            );
        }
        return (
            LanguageAnalysis::refused(
                findings.clone(),
                ProgramIdentityReport {
                    id: program.typed.as_ref().map(|t| t.id.clone()),
                    identity: DocumentIdentityReport {
                        document_sha256: program.document_sha256.clone(),
                        semantic_sha256: if program_admitted {
                            program.semantic_sha256.clone()
                        } else {
                            None
                        },
                        admitted: program_admitted,
                    },
                },
                LibraryIdentityReport {
                    name: library.typed.as_ref().map(|t| t.library.name.clone()),
                    revision: library.typed.as_ref().map(|t| t.library.revision),
                    identity: DocumentIdentityReport {
                        document_sha256: library.document_sha256.clone(),
                        semantic_sha256: if library_admitted {
                            library.semantic_sha256.clone()
                        } else {
                            None
                        },
                        admitted: library_admitted,
                    },
                    pin_match: false,
                },
                Vec::new(),
            ),
            refused_plan(
                program.semantic_sha256.clone(),
                library.semantic_sha256.clone(),
                &findings,
            ),
            Vec::new(),
        );
    };

    let mut analyzer = Analyzer {
        program: program_doc,
        library: LibraryIndex::new(library_doc),
        options,
        findings,
        obligations: Vec::new(),
        lifecycle: Vec::new(),
        goals: BTreeMap::new(),
        env: BTreeMap::new(),
        blocked: BTreeMap::new(),
        edges: BTreeMap::new(),
        deps: BTreeMap::new(),
        depends: BTreeMap::new(),
        inadmissible_premises: BTreeSet::new(),
        admissible_witnesses: BTreeMap::new(),
        contradicted: BTreeSet::new(),
        step_obligations: BTreeMap::new(),
        malformed_requirements: BTreeSet::new(),
        current_binding: None,
        pending_support: DischargeSupport::default(),
        observations: eval_material
            .map(|m| m.observations.clone())
            .unwrap_or_default(),
        expected_plan_sha256: eval_material.map(|m| m.plan_sha256.clone()),
        observed_sites: BTreeSet::new(),
        observed_values: BTreeMap::new(),
        evaluating,
        eval_events: Vec::new(),
    };
    analyzer.run();

    // Requirement checks can emit findings — collect them into the same
    // report before the plan verdict is decided.
    let requirements = analyzer.requirement_reports();
    let mut findings = std::mem::take(&mut analyzer.findings);
    dedupe_findings(&mut findings);

    let program_admitted = !findings
        .iter()
        .any(|f| f.document == "program" && ADMISSION_KINDS.contains(&f.kind.as_str()));
    let library_admitted = !findings
        .iter()
        .any(|f| f.document == "library" && ADMISSION_KINDS.contains(&f.kind.as_str()));

    // The pin binds identity to identity; it is only checkable when the
    // supplied library is admitted.
    let pin_match = library_admitted
        && Some(&program_doc.library.semantic_sha256) == library.semantic_sha256.as_ref()
        && program_doc.library.name == library_doc.library.name
        && program_doc.library.revision == library_doc.library.revision;
    if !pin_match {
        push_finding_scoped(
            &mut findings,
            "program",
            "library_pin_mismatch",
            "library",
            if library_admitted {
                format!(
                    "program pins {}/{}@{}; supplied library projects to {} ({}/{})",
                    program_doc.library.name,
                    program_doc.library.revision,
                    program_doc.library.semantic_sha256,
                    library.semantic_sha256.as_deref().unwrap_or("<none>"),
                    library_doc.library.name,
                    library_doc.library.revision,
                )
            } else {
                "supplied library is not admitted; its pin cannot be checked".to_string()
            },
            None,
        );
    }

    // Reachability gating: a blocking finding scoped to a binding refuses the
    // plan only when some requirement or goal depends on the binding.
    let reachable = analyzer.reachable_bindings();
    for finding in &mut findings {
        if let Some(binding) = &finding.binding
            && BLOCKING_KINDS.contains(&finding.kind.as_str())
            && !reachable.contains(binding)
        {
            finding.blocking = false;
        }
    }
    let blocked_plan = findings.iter().any(|f| f.blocking);
    let runtime_obligations: Vec<ObligationReport> = analyzer
        .obligations
        .iter()
        .filter(|o| o.state == "runtime")
        .cloned()
        .collect();
    let steps = analyzer.plan_steps(&reachable);

    let bindings = analyzer
        .env
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                BindingReport {
                    ty: BindingTypeReport {
                        quantity_kind: value.ty.quantity_kind.clone(),
                        claim: value.ty.claim.as_str().to_string(),
                        relations: value.ty.relations.clone(),
                    },
                    value_state: match value.state {
                        ValueState::Established => "established",
                        ValueState::Declared => "declared",
                        ValueState::Unestablished => "unestablished",
                    }
                    .to_string(),
                    value: value.value_text(),
                    residual_assumptions: value
                        .assumptions
                        .iter()
                        .map(|a| ScopedAssumptionReport {
                            proposition: a.proposition.clone(),
                            at: a.at.clone(),
                        })
                        .collect(),
                    source_edges: value.edges.iter().cloned().collect(),
                    dependencies: analyzer.depends.get(name).cloned().unwrap_or_default(),
                },
            )
        })
        .collect();

    let analysis = LanguageAnalysis {
        schema_version: ANALYSIS_SCHEMA_VERSION.into(),
        profile: LANGUAGE_PROFILE.into(),
        program: ProgramIdentityReport {
            id: Some(program_doc.id.clone()),
            identity: DocumentIdentityReport {
                document_sha256: program.document_sha256.clone(),
                semantic_sha256: if program_admitted {
                    program.semantic_sha256.clone()
                } else {
                    None
                },
                admitted: program_admitted,
            },
        },
        library: LibraryIdentityReport {
            name: Some(library_doc.library.name.clone()),
            revision: Some(library_doc.library.revision),
            identity: DocumentIdentityReport {
                document_sha256: library.document_sha256.clone(),
                semantic_sha256: if library_admitted {
                    library.semantic_sha256.clone()
                } else {
                    None
                },
                admitted: library_admitted,
            },
            pin_match,
        },
        status: if findings.is_empty() {
            "clean"
        } else {
            "findings"
        }
        .into(),
        findings: findings.clone(),
        bindings,
        obligations: analyzer.obligations.clone(),
        requirements,
        goals: analyzer.goals.clone(),
        lifecycle: analyzer.lifecycle.clone(),
        plan: PlanReport {
            state: if blocked_plan { "refused" } else { "ready" }.into(),
            steps: steps.clone(),
            runtime_obligations,
        },
    };
    let plan = if blocked_plan {
        refused_plan(
            program.semantic_sha256.clone(),
            library.semantic_sha256.clone(),
            &findings,
        )
    } else {
        analyzer.emit_execution_plan(&analysis, &reachable)
    };
    (analysis, plan, analyzer.eval_events.clone())
}

/// Declared identifiers are interpolated into report paths
/// (`premises[a, b]`, `arguments.{slot}`), obligation `over` sets, and
/// lifecycle keys (`method:<library>/<id>@<rev>`) — delimiters would make a
/// position ambiguous or collide two distinct key components. `at`-grouping
/// uses `[` `]` `,`; relation paths use `.`; lifecycle keys use `/` and `:`;
/// whitespace terminates the path head. Everything else — including `@`,
/// which appears in declared entity names like `bracket@2` — stays legal.
fn identifier_charset_ok(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| !c.is_whitespace() && !matches!(c, '[' | ']' | ',' | '.' | '/' | ':'))
}

/// `sha256:` followed by exactly 64 lowercase hex digits.
fn valid_sha256_digest(digest: &str) -> bool {
    digest.starts_with("sha256:")
        && digest.len() == 7 + 64
        && digest[7..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn dedupe_findings(findings: &mut Vec<LanguageFinding>) {
    let mut seen = BTreeSet::new();
    findings.retain(|f| {
        seen.insert((
            f.code.clone(),
            f.kind.clone(),
            f.document.clone(),
            f.at.clone(),
            f.detail.clone(),
        ))
    });
}

/// Library semantic admission, factored to run without a program document:
/// every declaration is validated, applied or not — a checked
/// representation is built from invariants, not trusted fields. Extracted so
/// a program whose decode failed does not mask the supplied library's own
/// findings.
struct LibraryChecker<'a, 'f> {
    doc: &'a LibraryDocument,
    index: LibraryIndex<'a>,
    findings: &'f mut Vec<LanguageFinding>,
}

/// JSON pointer for a library finding `at` string — resolvable when the path
/// names a single declaration position.
fn library_pointer(doc: &LibraryDocument, at: &str) -> Option<String> {
    let head = at.split_whitespace().next().unwrap_or(at);
    // A path like `methods[x].ensures` resolves to the containing
    // declaration — the position of the defect, not a missing field.
    let head = head.split('.').next().unwrap_or(head);
    let (collection, rest) = head.split_once('[')?;
    let index = rest.strip_suffix(']').unwrap_or(rest);
    match collection {
        "methods" => doc
            .methods
            .iter()
            .position(|x| x.id == index)
            .map(|i| format!("/methods/{i}")),
        _ => None,
    }
}

/// JSON pointer for a program finding `at` string — resolvable when the path
/// names a single declaration position.
fn program_pointer(doc: &ProgramDocument, at: &str) -> Option<String> {
    let head = at.split_whitespace().next().unwrap_or(at);
    let head = head.split('.').next().unwrap_or(head);
    let (collection, rest) = head.split_once('[')?;
    let index = rest.strip_suffix(']').unwrap_or(rest);
    match collection {
        "body" => index.parse::<usize>().ok().map(|i| format!("/body/{i}")),
        "inputs" => doc
            .inputs
            .iter()
            .position(|x| x.id == index)
            .map(|i| format!("/inputs/{i}")),
        "premises" => doc
            .premises
            .iter()
            .position(|x| x.id == index.split(',').next().unwrap_or(index).trim())
            .map(|i| format!("/premises/{i}")),
        "assumptions" => doc
            .assumptions
            .iter()
            .position(|x| x.id == index.split(',').next().unwrap_or(index).trim())
            .map(|i| format!("/assumptions/{i}")),
        "requirements" => doc
            .requirements
            .iter()
            .position(|x| x.id == index)
            .map(|i| format!("/requirements/{i}")),
        _ => None,
    }
}

impl<'a, 'f> LibraryChecker<'a, 'f> {
    fn new(doc: &'a LibraryDocument, findings: &'f mut Vec<LanguageFinding>) -> Self {
        Self {
            doc,
            index: LibraryIndex::new(doc),
            findings,
        }
    }

    fn finding(&mut self, kind: &str, at: &str, detail: String) {
        self.findings.push(LanguageFinding {
            code: code_for(kind).to_string(),
            kind: kind.to_string(),
            document: "library".into(),
            at: at.to_string(),
            pointer: library_pointer(self.doc, at),
            binding: None,
            detail,
            blocking: BLOCKING_KINDS.contains(&kind),
            candidates: None,
        });
    }

    fn parse_failure(&mut self, at: &str, failure: &eval::ParseFailure) {
        let kind = match failure.kind {
            FailureKind::Budget => "budget",
            FailureKind::Unsupported => "unsupported",
            FailureKind::Malformed => "malformed",
        };
        self.finding(kind, at, failure.detail.clone());
    }

    /// Semantic admission of the library: every declaration is validated,
    /// applied or not — a checked representation is built from invariants,
    /// not trusted fields.
    fn run(&mut self) {
        let doc = self.doc;
        if doc.schema_version != LIBRARY_SCHEMA_VERSION {
            self.finding(
                "malformed",
                "schema_version",
                format!("expected `{LIBRARY_SCHEMA_VERSION}`"),
            );
        }
        if doc.profile != LANGUAGE_PROFILE {
            self.finding(
                "malformed",
                "profile",
                format!("expected `{LANGUAGE_PROFILE}`"),
            );
        }
        if doc.methods.len() > MAX_METHODS {
            self.finding(
                "budget",
                "methods",
                format!(
                    "{} methods exceeds the bound {MAX_METHODS}",
                    doc.methods.len()
                ),
            );
        }
        // Declared names enter report paths and lifecycle keys — each must
        // be a single token.
        if !identifier_charset_ok(&doc.library.name) {
            self.finding(
                "malformed",
                "library",
                format!(
                    "library name `{}` carries a path delimiter or whitespace",
                    doc.library.name
                ),
            );
        }
        for (collection, keys) in [
            (
                "quantity_kinds",
                doc.quantity_kinds
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
            (
                "propositions",
                doc.propositions
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
        ] {
            for key in keys {
                if !identifier_charset_ok(key) {
                    self.finding(
                        "malformed",
                        &format!("{collection}[{key}]"),
                        format!("identifier `{key}` carries a path delimiter or whitespace"),
                    );
                }
            }
        }
        let mut method_ids = BTreeSet::new();
        // `methods` is a declared set — iterate canonically so the findings
        // order is the same for every source ordering.
        for (_, method) in canonical_order(&doc.methods, &projection::METHOD) {
            let at = format!("methods[{}]", method.id);
            if !identifier_charset_ok(&method.id) {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "identifier `{}` carries a path delimiter or whitespace",
                        method.id
                    ),
                );
            }
            for key in method.inputs.keys().chain(method.variables.keys()) {
                if !identifier_charset_ok(key) {
                    self.finding(
                        "malformed",
                        &at,
                        format!("identifier `{key}` carries a path delimiter or whitespace"),
                    );
                }
            }
            if !method_ids.insert(method.id.clone()) {
                self.finding(
                    "malformed",
                    &at,
                    format!("duplicate method id `{}`", method.id),
                );
            }
            self.check_method_signature(method, &at);
        }
        let mut product_keys = BTreeSet::new();
        for (i, product) in canonical_order(&doc.kind_products, &projection::KIND_PRODUCT) {
            let at = format!("kind_products[{i}]");
            for kind in [&product.lhs_kind, &product.rhs_kind, &product.result_kind] {
                if !doc.quantity_kinds.contains_key(kind) {
                    self.finding(
                        "malformed",
                        &at,
                        format!("quantity kind `{kind}` is not declared by the library"),
                    );
                }
            }
            if !product_keys.insert((product.lhs_kind.clone(), product.rhs_kind.clone())) {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "duplicate product row for `{}` × `{}`",
                        product.lhs_kind, product.rhs_kind
                    ),
                );
            }
            if let Some(unit) = doc
                .quantity_kinds
                .get(&product.result_kind)
                .and_then(|k| k.canonical_unit.as_ref())
                && *unit != product.result_unit
            {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "result unit `{}` ≠ canonical unit `{unit}` of `{}`",
                        product.result_unit, product.result_kind
                    ),
                );
            }
        }
    }

    fn check_method_signature(&mut self, method: &MethodDecl, at: &str) {
        let doc = self.doc;
        let declared_vars: BTreeMap<&str, &str> = method
            .variables
            .iter()
            .map(|(v, r)| (v.as_str(), r.as_str()))
            .collect();
        // Every signature slot relation must name a declared variable mapping
        // back to that relation name.
        for (slot_name, slot) in method
            .inputs
            .iter()
            .map(|(s, d)| (Some(s.clone()), d))
            .chain(std::iter::once((None, &method.output)))
        {
            let at = format!("{at}.{}", slot_name.clone().unwrap_or("output".into()));
            if !doc.quantity_kinds.contains_key(&slot.quantity_kind) {
                self.finding(
                    "malformed",
                    &at,
                    format!("quantity kind `{}` is not declared", slot.quantity_kind),
                );
            }
            if ClaimModel::parse(&slot.claim).is_none() {
                self.finding("malformed", &at, format!("unknown claim `{}`", slot.claim));
            }
            for (relation, var) in slot_relations(slot) {
                match declared_vars.get(var.as_str()) {
                    Some(rel) if *rel == relation => {}
                    Some(rel) => self.finding(
                        "malformed",
                        &at,
                        format!("slot relation `{relation}` binds `{var}` which declares `{rel}`"),
                    ),
                    None => self.finding(
                        "malformed",
                        &at,
                        format!("slot relation `{relation}` binds undeclared variable `{var}`"),
                    ),
                }
            }
        }
        for (_, relation) in canonical_order(&method.projects, &projection::SCALAR) {
            if !RELATION_NAMES.contains(&relation.as_str()) {
                self.finding(
                    "malformed",
                    &format!("{at}.projects"),
                    format!("`{relation}` names no relation parameter"),
                );
            }
        }
        for (_, name) in canonical_order(&method.assumes, &projection::SCALAR) {
            if !self.index.proposition_known(name) {
                self.finding(
                    "undeclared_proposition",
                    &format!("{at}.assumes"),
                    format!("`{name}` names no library proposition or built-in"),
                );
                continue;
            }
            // A2: a signature assumption's params must be scoped by relation
            // names the signature can bind at application time.
            let declared_relations: BTreeSet<&str> =
                method.variables.values().map(String::as_str).collect();
            for param in self.index.proposition_params(name) {
                if !declared_relations.contains(param.as_str()) {
                    self.finding(
                        "malformed",
                        &format!("{at}.assumes"),
                        format!(
                            "assumption `{name}` declares param `{param}` absent from `{id}`'s signature relations",
                            id = method.id
                        ),
                    );
                }
            }
        }
        // `requires[{position}]`/`ensures[{position}]` address the canonical
        // position — the same position obligation ids carry at apply time.
        for (i, (_, requirement)) in canonical_order(&method.requires, &projection::REQUIRES)
            .into_iter()
            .enumerate()
        {
            self.check_requires_decl(method, requirement, &format!("{at}.requires[{i}]"));
        }
        for (i, (_, ensures)) in canonical_order(&method.ensures, &projection::ENSURES)
            .into_iter()
            .enumerate()
        {
            self.check_ensures_decl(method, ensures, &format!("{at}.ensures[{i}]"));
        }
        if let Some(implementation) = &method.implementation {
            let at = format!("{at}.implementation");
            match implementation.kind.as_str() {
                "primitive" => {
                    let Some(body) = &implementation.body else {
                        self.finding(
                            "malformed",
                            &at,
                            "primitive implementation declares no `body`".into(),
                        );
                        return;
                    };
                    self.check_body_expression(method, body, &at);
                }
                "external" => {
                    let Some(produces) = &implementation.produces else {
                        self.finding(
                            "malformed",
                            &at,
                            "external implementation declares no `produces`".into(),
                        );
                        return;
                    };
                    if !doc.quantity_kinds.contains_key(&produces.quantity_kind) {
                        self.finding(
                            "malformed",
                            &at,
                            format!(
                                "produces quantity kind `{}` is not declared",
                                produces.quantity_kind
                            ),
                        );
                    }
                }
                other => self.finding(
                    "malformed",
                    &at,
                    format!("unknown implementation kind `{other}`"),
                ),
            }
        }
    }

    fn check_requires_decl(&mut self, method: &MethodDecl, requirement: &ObligationDecl, at: &str) {
        match requirement.kind.as_str() {
            "domain_containment" => {
                for field in [&requirement.domain, &requirement.within] {
                    let Some(path) = field else {
                        self.finding(
                            "malformed",
                            at,
                            "domain_containment requires `domain` and `within`".into(),
                        );
                        return;
                    };
                    match Self::resolve_field_path(method, path) {
                        Some(_) => {}
                        None => self.finding(
                            "malformed",
                            at,
                            format!(
                                "domain path `{path}` is not `<relation>.<field>` over `{}`'s declared relations",
                                method.id
                            ),
                        ),
                    }
                }
            }
            "scope_check" => {
                let missing = requirement.subject.is_none() || requirement.scope.is_none();
                if missing {
                    self.finding(
                        "malformed",
                        at,
                        "scope_check requires `subject` (a slot) and `scope` (a scope kind)".into(),
                    );
                } else if let Some(scope) = &requirement.scope
                    && !SCOPE_KINDS.contains(&scope.as_str())
                {
                    self.finding(
                        "malformed",
                        at,
                        format!("`{scope}` is not a declared scope kind"),
                    );
                }
                if let Some(subject) = &requirement.subject
                    && !method.inputs.contains_key(subject)
                {
                    self.finding(
                        "malformed",
                        at,
                        format!("scope_check subject `{subject}` names no input slot"),
                    );
                }
            }
            "independence" | "provenance_disjoint" => {
                let Some(over) = &requirement.over else {
                    self.finding(
                        "malformed",
                        at,
                        format!(
                            "`{}` requires an `over` set of input slots",
                            requirement.kind
                        ),
                    );
                    return;
                };
                if over.len() > MAX_OVER_MEMBERS {
                    self.finding(
                        "budget",
                        at,
                        format!("`over` set exceeds the bound {MAX_OVER_MEMBERS}"),
                    );
                }
                for slot in over {
                    if !method.inputs.contains_key(slot) {
                        self.finding(
                            "malformed",
                            at,
                            format!("`over` member `{slot}` names no input slot"),
                        );
                    }
                }
            }
            other => self.finding(
                "malformed",
                at,
                format!(
                    "unknown requires kind `{other}` (expected {})",
                    REQUIRES_KINDS.join(", ")
                ),
            ),
        }
    }

    /// `scenario.operating_domain`/`material.applicability` — a `rel.field`
    /// path resolves when `rel` is a declared relation name in the signature
    /// and `field` is that relation's declared domain-bearing field.
    fn resolve_field_path(method: &MethodDecl, path: &str) -> Option<(String, String)> {
        let (relation, field) = path.split_once('.')?;
        let declared_field = match relation {
            "scenario" => "operating_domain",
            "material" => "applicability",
            _ => return None,
        };
        if field != declared_field {
            return None;
        }
        if !method.variables.values().any(|r| r == relation) {
            return None;
        }
        Some((relation.to_string(), field.to_string()))
    }

    fn check_ensures_decl(
        &mut self,
        method: &MethodDecl,
        ensures: &super::document::EnsuresDecl,
        at: &str,
    ) {
        if ensures.kind != "relation" {
            self.finding(
                "malformed",
                at,
                format!(
                    "unknown ensures kind `{}` (expected `relation`)",
                    ensures.kind
                ),
            );
        }
        if !SUPPORTED_CHECKS.contains(&ensures.check.as_str()) {
            self.finding(
                "unsupported",
                at,
                format!(
                    "check `{}` is not one of the profile's replayable checks {}",
                    ensures.check,
                    SUPPORTED_CHECKS.join(", ")
                ),
            );
        }
        match eval::parse_ensures(&ensures.expression) {
            Ok(expression) => self.check_expression_names(
                &expression,
                &method.inputs.keys().cloned().collect::<BTreeSet<_>>(),
                &[],
                &format!("{at}.expression"),
            ),
            Err(failure) => self.parse_failure(&format!("{at}.expression"), &failure),
        }
    }

    fn check_body_expression(&mut self, method: &MethodDecl, body: &str, at: &str) {
        match eval::parse(body) {
            Ok(expression) => self.check_expression_names(
                &expression,
                &method.inputs.keys().cloned().collect::<BTreeSet<_>>(),
                &[],
                &format!("{at}.body"),
            ),
            Err(failure) => self.parse_failure(&format!("{at}.body"), &failure),
        }
    }

    /// Every `Name` in the expression must resolve to a declared input slot —
    /// nothing may name a binding the signature does not carry.
    fn check_expression_names(
        &mut self,
        expression: &Expression,
        allowed: &BTreeSet<String>,
        extra: &[&str],
        at: &str,
    ) {
        let mut names = BTreeSet::new();
        collect_names(expression, &mut names);
        for name in names {
            if !allowed.contains(&name) && !extra.contains(&name.as_str()) {
                self.finding(
                    "malformed",
                    at,
                    format!("expression names `{name}` — no such input slot"),
                );
            }
        }
    }
}

/// Lifecycle material is program-external input: keys must parse as
/// `library:<name>@<rev>` or `method:<library>/<id>@<rev>`, and states
/// are drawn from the closed set — anything else is `malformed` and the
/// entry plays no part in usability. Runs without document context, so it
/// also applies when a document failed to decode.
fn check_lifecycle_options(entries: &[LifecycleEntry], findings: &mut Vec<LanguageFinding>) {
    if entries.len() > MAX_LIFECYCLE_ENTRIES {
        push_finding_scoped(
            findings,
            "options",
            "budget",
            "lifecycle",
            format!(
                "{} lifecycle entries exceeds the bound {MAX_LIFECYCLE_ENTRIES}",
                entries.len()
            ),
            None,
        );
    }
    for (i, (_, entry)) in canonical_order(entries, &projection::SCALAR)
        .into_iter()
        .enumerate()
    {
        let at = format!("lifecycle[{i}]");
        if !LIFECYCLE_STATES.contains(&entry.state.as_str()) {
            push_finding_scoped(
                findings,
                "options",
                "malformed",
                &at,
                format!(
                    "lifecycle state `{}` is not one of {}",
                    entry.state,
                    LIFECYCLE_STATES.join(", ")
                ),
                None,
            );
        }
        let valid_key = {
            let (scope, rest) = entry.key.split_once(':').unwrap_or(("", ""));
            let (name, revision) = rest.rsplit_once('@').unwrap_or(("", ""));
            let name_ok = match scope {
                "library" => !name.is_empty() && !name.contains('/'),
                "method" => name
                    .split_once('/')
                    .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty()),
                _ => false,
            };
            name_ok && revision.parse::<i64>().is_ok()
        };
        if !valid_key {
            push_finding_scoped(
                findings,
                "options",
                "malformed",
                &at,
                format!(
                    "lifecycle key `{}` is not `library:<name>@<rev>` or `method:<library>/<id>@<rev>`",
                    entry.key
                ),
                None,
            );
        }
    }
}

impl<'a> Analyzer<'a> {
    fn run(&mut self) {
        self.check_lifecycle_grammar();
        self.check_library();
        let mut program_check = ProgramChecker {
            program: self.program,
            library: Some(self.library.doc),
            findings: &mut self.findings,
            malformed_requirements: BTreeSet::new(),
        };
        program_check.run();
        self.malformed_requirements = program_check.malformed_requirements;
        self.load_inputs();
        self.compute_static_edges();
        self.check_contradictions();
        self.check_premise_admissibility();
        for index in 0..self.program.body.len() {
            self.step(index);
        }
        self.discharge_assumptions();
        // A binding whose residual assumptions contain a contradicted
        // proposition can ground no verdict — every dependent use is blocked.
        let contradicted = self.contradicted.clone();
        for (name, value) in &self.env {
            if value
                .assumptions
                .iter()
                .any(|a| contradicted.contains(&(a.proposition.clone(), a.at.clone())))
            {
                self.blocked
                    .insert(name.clone(), "not_evaluated.contradiction".into());
            }
        }
    }

    /// Emit a finding attributed to a document; `at` resolves to a pointer.
    fn finding(&mut self, document: &str, kind: &str, at: &str, detail: String) {
        let binding = self.current_binding.clone();
        let pointer = self.pointer_for(document, at);
        self.findings.push(LanguageFinding {
            code: code_for(kind).to_string(),
            kind: kind.to_string(),
            document: document.to_string(),
            at: at.to_string(),
            pointer,
            binding,
            detail,
            blocking: BLOCKING_KINDS.contains(&kind),
            candidates: None,
        });
    }

    fn program_finding(&mut self, kind: &str, at: &str, detail: String) {
        self.finding("program", kind, at, detail);
    }

    fn library_finding(&mut self, kind: &str, at: &str, detail: String) {
        self.finding("library", kind, at, detail);
    }

    /// Resolves a finding's `at` to a JSON pointer inside its document where
    /// the path names a single position; `None` when the position is
    /// aggregate (e.g. a grouped assumption list) or outside the documents.
    fn pointer_for(&self, document: &str, at: &str) -> Option<String> {
        match document {
            "program" => program_pointer(self.program, at),
            "library" => library_pointer(self.library.doc, at),
            _ => None,
        }
    }

    // -- admission: grammar -------------------------------------------------

    /// Lifecycle material is program-external input — see
    /// [`check_lifecycle_options`].
    fn check_lifecycle_grammar(&mut self) {
        check_lifecycle_options(&self.options.lifecycle, &mut self.findings);
    }

    /// Semantic admission of the library — the same pass a standalone
    /// library document receives.
    fn check_library(&mut self) {
        LibraryChecker::new(self.library.doc, &mut self.findings).run();
    }

    fn parse_failure(&mut self, document: &str, at: &str, failure: &eval::ParseFailure) {
        let kind = match failure.kind {
            FailureKind::Budget => "budget",
            FailureKind::Unsupported => "unsupported",
            FailureKind::Malformed => "malformed",
        };
        if document == "library" {
            self.library_finding(kind, at, failure.detail.clone());
        } else {
            self.program_finding(kind, at, failure.detail.clone());
        }
    }
}

/// Program-side semantic admission — mirrors [`LibraryChecker`]. Runs every
/// check that does not need the bound library's vocabulary;
/// vocabulary-dependent checks (quantity kinds, canonical units, proposition
/// vocabularies) run only when `library` is present, so a program whose
/// library is inadmissible still reports its own shape findings.
struct ProgramChecker<'a, 'f> {
    program: &'a ProgramDocument,
    library: Option<&'a LibraryDocument>,
    findings: &'f mut Vec<LanguageFinding>,
    malformed_requirements: BTreeSet<String>,
}

impl<'a, 'f> ProgramChecker<'a, 'f> {
    fn finding(&mut self, kind: &str, at: &str, detail: String) {
        program_finding_at(self.program, self.findings, kind, at, detail, None);
    }

    /// Semantic admission of the program: unique identifiers, sequential
    /// single-assignment references, requirement/premise shape, budgets.
    fn run(&mut self) {
        let program = self.program;
        if program.schema_version != PROGRAM_SCHEMA_VERSION {
            self.finding(
                "malformed",
                "schema_version",
                format!("expected `{PROGRAM_SCHEMA_VERSION}`"),
            );
        }
        if program.profile != LANGUAGE_PROFILE {
            self.finding(
                "malformed",
                "profile",
                format!("expected `{LANGUAGE_PROFILE}`"),
            );
        }
        // Budgets.
        for (count, bound, at) in [
            (program.body.len(), MAX_BODY_STEPS, "body"),
            (program.inputs.len(), MAX_INPUTS, "inputs"),
            (program.premises.len(), MAX_PREMISES, "premises"),
            (program.assumptions.len(), MAX_ASSUMPTIONS, "assumptions"),
            (program.requirements.len(), MAX_REQUIREMENTS, "requirements"),
            (
                program.entities.geometries.len()
                    + program.entities.scenarios.len()
                    + program.entities.materials.len(),
                MAX_ENTITIES,
                "entities",
            ),
        ] {
            if count > bound {
                self.finding(
                    "budget",
                    at,
                    format!("{count} entries exceeds the bound {bound}"),
                );
            }
        }

        // Unique identifiers — duplicates are refused before maps are built.
        let mut check_ids = |declared: &[String], collection: &str| {
            let mut seen = BTreeSet::new();
            let mut ordered: Vec<&String> = declared.iter().collect();
            ordered.sort();
            for id in ordered {
                if !identifier_charset_ok(id) {
                    self.finding(
                        "malformed",
                        &format!("{collection}[{id}]"),
                        format!("identifier `{id}` carries a path delimiter or whitespace"),
                    );
                }
                if !seen.insert(id.clone()) {
                    self.finding(
                        "malformed",
                        &format!("{collection}[{id}]"),
                        format!("duplicate id `{id}`"),
                    );
                }
            }
        };
        check_ids(
            &program
                .inputs
                .iter()
                .map(|i| i.id.clone())
                .collect::<Vec<_>>(),
            "inputs",
        );
        check_ids(
            &program
                .premises
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
            "premises",
        );
        check_ids(
            &program
                .assumptions
                .iter()
                .map(|a| a.id.clone())
                .collect::<Vec<_>>(),
            "assumptions",
        );
        check_ids(
            &program
                .requirements
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>(),
            "requirements",
        );
        for (i, step) in program.body.iter().enumerate() {
            if !identifier_charset_ok(&step.bind) {
                self.finding(
                    "malformed",
                    &format!("body[{i}]"),
                    format!(
                        "binding name `{}` carries a path delimiter or whitespace",
                        step.bind
                    ),
                );
            }
        }
        for (collection, keys) in [
            (
                "entities.geometries",
                program
                    .entities
                    .geometries
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
            (
                "entities.scenarios",
                program
                    .entities
                    .scenarios
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
            (
                "entities.materials",
                program
                    .entities
                    .materials
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
        ] {
            for key in keys {
                if !identifier_charset_ok(key) {
                    self.finding(
                        "malformed",
                        &format!("{collection}[{key}]"),
                        format!("identifier `{key}` carries a path delimiter or whitespace"),
                    );
                }
            }
        }
        if !identifier_charset_ok(&program.id) {
            self.finding(
                "malformed",
                "id",
                format!(
                    "identifier `{}` carries a path delimiter or whitespace",
                    program.id
                ),
            );
        }

        // Assumption shape: exactly one of asserts/denies. Assumptions are a
        // declared set — iterate canonically.
        for (_, assumption) in canonical_order(&program.assumptions, &projection::ASSUMPTION) {
            if assumption.asserts.is_some() == assumption.denies.is_some() {
                self.finding(
                    "malformed",
                    &format!("assumptions[{}]", assumption.id),
                    "an assumption must carry exactly one of `asserts` or `denies`".into(),
                );
            }
        }

        // Premise shape: `established_by` is grammar-required — an
        // unattributed premise is not established support.
        for (_, premise) in canonical_order(&program.premises, &projection::PREMISE) {
            if premise.established_by.is_none() {
                self.finding(
                    "malformed",
                    &format!("premises[{}]", premise.id),
                    format!(
                        "premise `{}` declares no `established_by` — attribution is required",
                        premise.id
                    ),
                );
            }
            if let Some(over) = premise.arguments.as_ref().map(|a| a.over.len())
                && over > MAX_OVER_MEMBERS
            {
                self.finding(
                    "budget",
                    &format!("premises[{}]", premise.id),
                    format!("`over` set exceeds the bound {MAX_OVER_MEMBERS}"),
                );
            }
        }

        // Requirement admission: comparison vocabulary, limit shape.
        for (_, requirement) in canonical_order(&program.requirements, &projection::REQUIREMENT) {
            let at = format!("requirements[{}]", requirement.id);
            let mut bad = false;
            if !matches!(requirement.comparison.as_str(), "ge" | "le") {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "comparison `{}` is not `ge` or `le`",
                        requirement.comparison
                    ),
                );
                bad = true;
            }
            if self
                .library
                .is_some_and(|d| !d.quantity_kinds.contains_key(&requirement.quantity_kind))
            {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "quantity kind `{}` is not declared by the bound library",
                        requirement.quantity_kind
                    ),
                );
                bad = true;
            }
            if requirement.limit.kind != "exact" {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "limit kind `{}` — a requirement bound must be exact",
                        requirement.limit.kind
                    ),
                );
                bad = true;
            }
            if ExactNumber::from_canonical(requirement.limit.value.as_deref().unwrap_or(""))
                .is_err()
            {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "limit value `{}` is not a canonical rational",
                        requirement.limit.value.as_deref().unwrap_or("")
                    ),
                );
                bad = true;
            }
            if let Some(unit) = self
                .library
                .and_then(|d| canonical_unit(d, &requirement.quantity_kind))
                && requirement.limit.unit != unit
            {
                self.finding(
                    "type_mismatch",
                    &at,
                    format!(
                        "limit unit `{}` is not the canonical unit `{unit}` of `{}`",
                        requirement.limit.unit, requirement.quantity_kind
                    ),
                );
                bad = true;
            }
            if let Some(scope) = &requirement.scope
                && !SCOPE_KINDS.contains(&scope.as_str())
            {
                self.finding(
                    "malformed",
                    &at,
                    format!("scope `{scope}` is not a declared scope kind"),
                );
                bad = true;
            }
            if let Some(scenario) = &requirement.scenario
                && !self.program.entities.scenarios.contains_key(scenario)
            {
                self.finding(
                    "undeclared_entity",
                    &at,
                    format!("scenario `{scenario}` is not a declared entity"),
                );
                bad = true;
            }
            if bad {
                self.malformed_requirements.insert(requirement.id.clone());
            }
        }

        // Entity declarations: scope kinds closed; domains/applicability are
        // admitted intervals over declared kinds and canonical units.
        for (id, scenario) in &self.program.entities.scenarios {
            if !SCOPE_KINDS.contains(&scenario.scope.as_str()) {
                self.finding(
                    "malformed",
                    &format!("entities.scenarios[{id}]"),
                    format!("scope `{}` is not a declared scope kind", scenario.scope),
                );
            }
            if let Some(domain) = &scenario.operating_domain {
                self.check_interval_decl(
                    &domain.unit,
                    &domain.lower,
                    &domain.upper,
                    &domain.quantity_kind,
                    &format!("entities.scenarios[{id}].operating_domain"),
                );
            }
        }
        for (id, material) in &self.program.entities.materials {
            for (kind, interval) in &material.applicability {
                self.check_interval_decl(
                    &interval.unit,
                    &interval.lower,
                    &interval.upper,
                    kind,
                    &format!("entities.materials[{id}].applicability[{kind}]"),
                );
            }
        }

        // Sequential single-assignment: every reference names an input or an
        // earlier binding; a bind may not shadow a bound name.
        let mut bound: BTreeSet<String> = program.inputs.iter().map(|i| i.id.clone()).collect();
        for (i, step) in program.body.iter().enumerate() {
            let at = format!("body[{i}]");
            for (position, reference) in step_references(step) {
                if !bound.contains(reference) {
                    self.finding(
                        "malformed",
                        &format!("{at}.{position}"),
                        format!(
                            "`{reference}` names no input or earlier binding — references are sequential"
                        ),
                    );
                }
            }
            if !bound.insert(step.bind.clone()) {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "`{}` shadows an earlier name — bindings are single-assignment",
                        step.bind
                    ),
                );
            }
        }
        // Requirement subjects resolve against the whole bound universe.
        for (_, requirement) in canonical_order(&program.requirements, &projection::REQUIREMENT) {
            let at = format!("requirements[{}]", requirement.id);
            if !bound.contains(&requirement.subject.reference) {
                self.finding(
                    "malformed",
                    &at,
                    format!(
                        "subject `{}` names no input or binding",
                        requirement.subject.reference
                    ),
                );
                self.malformed_requirements.insert(requirement.id.clone());
            }
        }

        // Premise `over` sets and argument scopes reference bound names.
        for (_, premise) in canonical_order(&program.premises, &projection::PREMISE) {
            let at = format!("premises[{}]", premise.id);
            if let Some(args) = &premise.arguments {
                for member in canonical_order(&args.over, &projection::SCALAR)
                    .into_iter()
                    .map(|(_, m)| m.as_str())
                {
                    if !bound.contains(member) {
                        self.finding(
                            "malformed",
                            &at,
                            format!("`over` member `{member}` names no input or binding"),
                        );
                    }
                }
            }
        }

        // Entity provenance objects are admitted under the same rules.
        for (id, entity) in &program.entities.geometries {
            if let Some(source) = &entity.source {
                provenance_admissible(
                    self.program,
                    source,
                    &format!("entities.geometries[{id}].source"),
                    None,
                    self.findings,
                );
            }
        }
        for (id, entity) in &program.entities.scenarios {
            if let Some(source) = &entity.source {
                provenance_admissible(
                    self.program,
                    source,
                    &format!("entities.scenarios[{id}].source"),
                    None,
                    self.findings,
                );
            }
        }
        for (id, entity) in &program.entities.materials {
            if let Some(source) = &entity.source {
                provenance_admissible(
                    self.program,
                    source,
                    &format!("entities.materials[{id}].source"),
                    None,
                    self.findings,
                );
            }
        }
        for (_, assumption) in canonical_order(&program.assumptions, &projection::ASSUMPTION) {
            if let Some(source) = &assumption.source {
                provenance_admissible(
                    self.program,
                    source,
                    &format!("assumptions[{}].source", assumption.id),
                    None,
                    self.findings,
                );
            }
        }
        self.check_proposition_refs();
    }

    /// An interval-valued declaration is admitted only when its quantity
    /// kind is declared, bounds parse, and `lower ≤ upper`.
    fn check_interval_decl(&mut self, unit: &str, lower: &str, upper: &str, kind: &str, at: &str) {
        // Vocabulary-dependent checks run only when the bound library is
        // legible; the bounds themselves are always checkable.
        if let Some(doc) = self.library {
            if !doc.quantity_kinds.contains_key(kind) {
                self.finding(
                    "malformed",
                    at,
                    format!("quantity kind `{kind}` is not declared by the bound library"),
                );
                return;
            }
            if let Some(canonical) = canonical_unit(doc, kind)
                && unit != canonical
            {
                self.finding(
                    "type_mismatch",
                    at,
                    format!("unit `{unit}` is not the canonical unit `{canonical}` of `{kind}`"),
                );
            }
        }
        let (Ok(lo), Ok(hi)) = (
            ExactNumber::from_canonical(lower),
            ExactNumber::from_canonical(upper),
        ) else {
            self.finding(
                "malformed",
                at,
                format!("bounds `{lower}`..`{upper}` are not canonical rationals"),
            );
            return;
        };
        if lo.checked_cmp(&hi).is_ok_and(|o| o.is_gt()) {
            self.finding(
                "malformed",
                at,
                format!("interval is inverted: {lower} > {upper}"),
            );
        }
    }

    fn check_proposition_refs(&mut self) {
        let Some(library) = self.library else {
            // Proposition vocabularies are library terms — undecidable
            // without it, not malformed.
            return;
        };
        // Iterate through a detached `&'a` reference so the closures can hold
        // `&mut self` for findings.
        let program: &'a ProgramDocument = self.program;
        let mut unknown = |name: &str, at: String| {
            if !proposition_known(library, name) {
                self.finding(
                    "undeclared_proposition",
                    &at,
                    format!("`{name}` names no library proposition or built-in"),
                );
            }
        };
        for (_, a) in canonical_order(&program.assumptions, &projection::ASSUMPTION) {
            if let Some(p) = &a.asserts {
                unknown(p, format!("assumptions[{}]", a.id));
            }
            if let Some(p) = &a.denies {
                unknown(p, format!("assumptions[{}]", a.id));
            }
        }
        for (_, p) in canonical_order(&program.premises, &projection::PREMISE) {
            unknown(&p.proposition, format!("premises[{}]", p.id));
            for (_, s) in canonical_order(&p.assumptions, &projection::SCOPED_ASSERTION) {
                unknown(&s.proposition, format!("premises[{}].assumptions", p.id));
            }
        }
        for (i, step) in program.body.iter().enumerate() {
            if let Some(assumptions) = step.import.as_ref().and_then(|i| i.assumptions.as_ref()) {
                for (_, s) in canonical_order(assumptions, &projection::SCOPED_ASSERTION) {
                    unknown(&s.proposition, format!("body[{i}].import.assumptions"));
                }
            }
        }
        // Scoped-proposition params: `at` keys are restricted to the params
        // the proposition declares (built-ins scope over relation names).
        let mut check_scope = |proposition: &str, at: &RelationMap, where_at: String| {
            let params = proposition_params(library, proposition);
            for key in at.keys() {
                if !params.contains(key) {
                    self.finding(
                        "malformed",
                        &where_at,
                        format!("`{key}` is not a declared param of proposition `{proposition}`"),
                    );
                }
            }
        };
        for (_, a) in canonical_order(&program.assumptions, &projection::ASSUMPTION) {
            if let Some(p) = a.asserts.as_ref().or(a.denies.as_ref()) {
                check_scope(p, &a.at, format!("assumptions[{}]", a.id));
            }
        }
        for (_, p) in canonical_order(&program.premises, &projection::PREMISE) {
            check_scope(&p.proposition, &p.at, format!("premises[{}]", p.id));
            for (_, s) in canonical_order(&p.assumptions, &projection::SCOPED_ASSERTION) {
                check_scope(
                    &s.proposition,
                    &s.at,
                    format!("premises[{}].assumptions", p.id),
                );
            }
        }
        for (i, step) in program.body.iter().enumerate() {
            if let Some(assumptions) = step.import.as_ref().and_then(|i| i.assumptions.as_ref()) {
                for (_, s) in canonical_order(assumptions, &projection::SCOPED_ASSERTION) {
                    check_scope(
                        &s.proposition,
                        &s.at,
                        format!("body[{i}].import.assumptions"),
                    );
                }
            }
        }
    }
}

impl<'a> Analyzer<'a> {
    fn load_inputs(&mut self) {
        let mut seen = BTreeSet::new();
        for (_, input) in canonical_order(&self.program.inputs, &projection::INPUT) {
            let at = format!("inputs[{}]", input.id);
            self.current_binding = Some(input.id.clone());
            if !seen.insert(input.id.clone()) {
                // The duplicate was reported at admission; the first binding
                // stays authoritative.
                self.current_binding = None;
                continue;
            }
            let ty = self.type_of(&input.ty, &format!("{at}.type"));
            let (value, state, edges) = match &input.binding {
                None => {
                    self.program_finding(
                        "hole",
                        &at,
                        format!("input `{}` declares a type but no binding", input.id),
                    );
                    (None, ValueState::Unestablished, BTreeSet::new())
                }
                Some(binding) => self.load_binding(binding, &ty, &at),
            };
            let unit = input
                .binding
                .as_ref()
                .and_then(|b| b.value.as_ref())
                .map(|v| v.unit.clone())
                .unwrap_or_default();
            if !edges.is_empty() {
                self.edges.insert(input.id.clone(), edges.clone());
            }
            self.env.insert(
                input.id.clone(),
                SemanticValue {
                    ty,
                    unit,
                    value,
                    state,
                    edges,
                    assumptions: Vec::new(),
                },
            );
            self.current_binding = None;
        }
    }

    fn load_binding(
        &mut self,
        binding: &super::document::BindingDecl,
        ty: &QuantityType,
        at: &str,
    ) -> (Option<NumericValue>, ValueState, BTreeSet<String>) {
        if let Some(source) = &binding.source {
            self.check_provenance(source, &format!("{at}.binding.source"));
        }
        match binding.state.as_str() {
            "bound" => {
                let Some(decl) = &binding.value else {
                    self.program_finding(
                        "malformed",
                        &format!("{at}.binding"),
                        "`bound` input carries no value".into(),
                    );
                    return (None, ValueState::Unestablished, BTreeSet::new());
                };
                match self.admit_value(decl, ty, &format!("{at}.binding.value")) {
                    Some(v) => {
                        let edges = binding
                            .source
                            .as_ref()
                            .and_then(|s| s.edge.clone())
                            .into_iter()
                            .collect();
                        (Some(v), ValueState::Established, edges)
                    }
                    None => (None, ValueState::Unestablished, BTreeSet::new()),
                }
            }
            "unavailable" => (None, ValueState::Unestablished, BTreeSet::new()),
            other => {
                self.program_finding(
                    "malformed",
                    &format!("{at}.binding.state"),
                    format!("unknown binding state `{other}`"),
                );
                (None, ValueState::Unestablished, BTreeSet::new())
            }
        }
    }

    /// Provenance object admission: the closed source vocabulary, and the
    /// fields each kind must carry. A certificate is admissible only with a
    /// replayable payload — digest plus a check the profile supports. All
    /// provenance objects in the grammar live in the program document.
    fn check_provenance(&mut self, source: &ProvenanceDecl, at: &str) -> bool {
        provenance_admissible(
            self.program,
            source,
            at,
            self.current_binding.clone(),
            &mut self.findings,
        )
    }

    /// Checked value admission: the payload's claim must satisfy the declared
    /// claim, its shape must match its kind, and its unit must be the
    /// declared kind's canonical unit. A nominal payload never satisfies an
    /// enclosure-declared type.
    fn admit_value(
        &mut self,
        decl: &ValueDecl,
        ty: &QuantityType,
        at: &str,
    ) -> Option<NumericValue> {
        let Some(claim) = ClaimModel::parse(&decl.kind) else {
            self.program_finding(
                "malformed",
                at,
                format!("unknown value kind `{}`", decl.kind),
            );
            return None;
        };
        if !claim.satisfies(ty.claim) {
            self.program_finding(
                "type_mismatch",
                at,
                format!(
                    "value kind `{}` does not satisfy declared claim `{}`",
                    decl.kind,
                    ty.claim.as_str()
                ),
            );
            return None;
        }
        if let Some(canonical) = self.library.canonical_unit(&ty.quantity_kind)
            && decl.unit != canonical
        {
            self.program_finding(
                "malformed",
                at,
                format!(
                    "unit `{u}` is not the canonical unit `{canonical}` of `{kind}`",
                    u = decl.unit,
                    kind = ty.quantity_kind
                ),
            );
            return None;
        }
        let value = match claim {
            ClaimModel::Exact | ClaimModel::Nominal => decl
                .value
                .as_deref()
                .and_then(|s| ExactNumber::from_canonical(s).ok())
                .map(NumericValue::Exact),
            ClaimModel::Enclosure => match (&decl.lower, &decl.upper) {
                (Some(lo), Some(hi)) => match (
                    ExactNumber::from_canonical(lo),
                    ExactNumber::from_canonical(hi),
                ) {
                    (Ok(l), Ok(u)) if l.checked_cmp(&u).is_ok_and(|o| !o.is_gt()) => {
                        Some(NumericValue::Enclosure(Enclosure { lower: l, upper: u }))
                    }
                    _ => None,
                },
                _ => None,
            },
        };
        if value.is_none() {
            self.program_finding(
                "malformed",
                at,
                format!("`{}` payload is missing, malformed, or inverted", decl.kind),
            );
        }
        value
    }

    /// Static provenance propagation: every binding's recorded source edges
    /// are the union of its operands' edges (§8.3 — dependency edges persist
    /// through derivation, including under type projection). Computed
    /// syntactically before the body runs so premise admissibility can read
    /// the complete derived graph.
    fn compute_static_edges(&mut self) {
        for step in &self.program.body {
            let mut set = BTreeSet::new();
            if let Some(import) = &step.import
                && let Some(edge) = import.source.as_ref().and_then(|s| s.edge.clone())
            {
                set.insert(edge);
            }
            for (_, reference) in step_references(step) {
                if let Some(edges) = self.edges.get(reference) {
                    set.extend(edges.iter().cloned());
                }
            }
            if !set.is_empty() {
                self.edges.insert(step.bind.clone(), set);
            }
        }
    }

    fn check_contradictions(&mut self) {
        // Group by (proposition, scope): one finding per conflicted
        // proposition, `at` listing every contributing assumption id in
        // declaration order.
        let mut groups: BTreeMap<(String, RelationMap), Vec<&str>> = BTreeMap::new();
        let mut has_assert: BTreeMap<(String, RelationMap), bool> = BTreeMap::new();
        let mut has_deny: BTreeMap<(String, RelationMap), bool> = BTreeMap::new();
        for (_, a) in canonical_order(&self.program.assumptions, &projection::ASSUMPTION) {
            let prop = a
                .asserts
                .clone()
                .or_else(|| a.denies.clone())
                .unwrap_or_default();
            let key = (prop, a.at.clone());
            groups.entry(key.clone()).or_default().push(a.id.as_str());
            if a.asserts.is_some() {
                has_assert.insert(key.clone(), true);
            } else {
                has_deny.insert(key, true);
            }
        }
        for (key, ids) in groups {
            if has_assert.contains_key(&key) && has_deny.contains_key(&key) {
                self.contradicted.insert(key.clone());
                self.program_finding(
                    "contradiction",
                    &format!("assumptions[{}]", ids.join(", ")),
                    format!(
                        "`{}` is both asserted and denied at the same scope; dependent uses blocked",
                        key.0
                    ),
                );
            }
        }
    }

    /// Premise admissibility (spec §8/A3): a premise needs attribution
    /// (`established_by`), a well-formed scope, and must not assert a
    /// proposition the record itself refutes or the program denies. Cyclic
    /// witnesses are grouped by mutual dependency — one finding per cycle.
    fn check_premise_admissibility(&mut self) {
        // Canonical iteration: the values below feed `admissible_witnesses`,
        // whose firing order lands in `dependencies` — source order must not
        // shape it. The indices remain source positions.
        let premise_map: BTreeMap<(String, RelationMap), Vec<usize>> = {
            let mut map: BTreeMap<(String, RelationMap), Vec<usize>> = BTreeMap::new();
            for (i, p) in canonical_order(&self.program.premises, &projection::PREMISE) {
                map.entry((p.proposition.clone(), p.at.clone()))
                    .or_default()
                    .push(i);
            }
            map
        };

        // Premises are a declared set — process in canonical order so source
        // order never shapes the findings.
        for (index, premise) in canonical_order(&self.program.premises, &projection::PREMISE) {
            let at = format!("premises[{}]", premise.id);
            let attributed = premise
                .established_by
                .as_ref()
                .is_some_and(|p| self.check_provenance(p, &format!("{at}.established_by")));
            if !attributed {
                self.inadmissible_premises.insert(index);
            }
            if premise.proposition == "provenance_disjoint" {
                let bound: Vec<&str> = premise
                    .arguments
                    .as_ref()
                    .map(|a| a.over.iter().map(String::as_str).collect())
                    .unwrap_or_default();
                let edge_sets: Vec<&BTreeSet<String>> =
                    bound.iter().filter_map(|id| self.edges.get(*id)).collect();
                let all_have_edges =
                    edge_sets.len() == bound.len() && edge_sets.iter().all(|s| !s.is_empty());
                let shared = edge_sets.iter().enumerate().any(|(a, s)| {
                    edge_sets[a + 1..]
                        .iter()
                        .any(|t| s.intersection(t).next().is_some())
                });
                if all_have_edges && shared {
                    self.program_finding(
                        "premise_conflict",
                        &at,
                        format!(
                            "premise `{}` asserts provenance_disjoint over inputs that share a recorded source edge",
                            premise.id
                        ),
                    );
                    self.inadmissible_premises.insert(index);
                }
            }
            let denied = self.program.assumptions.iter().any(|a| {
                a.denies.as_deref() == Some(premise.proposition.as_str()) && a.at == premise.at
            });
            if denied {
                self.program_finding(
                    "premise_conflict",
                    &at,
                    format!(
                        "premise `{}` asserts a proposition the program denies",
                        premise.id
                    ),
                );
                self.inadmissible_premises.insert(index);
            }
        }

        // Premise-dependency edges: i → j when i's support names the
        // proposition j asserts (at j's scope).
        let support: Vec<BTreeSet<ScopedProposition>> = self
            .program
            .premises
            .iter()
            .map(|p| p.assumptions.iter().map(ScopedProposition::from).collect())
            .collect();
        let edges: Vec<BTreeSet<usize>> = support
            .iter()
            .map(|set| {
                set.iter()
                    .flat_map(|prop| {
                        premise_map
                            .get(&(prop.proposition.clone(), prop.at.clone()))
                            .cloned()
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        // Reaches itself → cyclic. The witness chain is bounded by
        // MAX_WITNESS_DEPTH; a chain that does not terminate inside the bound
        // is refused as a budget finding rather than looped on.
        let n = self.program.premises.len();
        let reach = |from: usize| -> Option<BTreeSet<usize>> {
            let mut seen = BTreeSet::new();
            let mut stack: Vec<usize> = edges[from].iter().copied().collect();
            let mut steps = 0usize;
            while let Some(node) = stack.pop() {
                steps += 1;
                if steps > MAX_WITNESS_DEPTH * MAX_WITNESS_DEPTH.max(1) {
                    return None;
                }
                if seen.insert(node) {
                    stack.extend(edges[node].iter().copied());
                }
            }
            Some(seen)
        };
        let mut reaches: Vec<BTreeSet<usize>> = Vec::with_capacity(n);
        let mut budget_exceeded = false;
        for i in 0..n {
            match reach(i) {
                Some(set) => reaches.push(set),
                None => {
                    budget_exceeded = true;
                    reaches.push(BTreeSet::new());
                }
            }
        }
        if budget_exceeded {
            self.program_finding(
                "budget",
                "premises",
                "premise witness chains exceed the bound".to_string(),
            );
        }
        // Cyclic-group emission must not drift with source order: iterate in
        // canonical premise order and sort the grouped member ids before they
        // join the `premises[...]` path.
        let cyclic: Vec<usize> = canonical_order(&self.program.premises, &projection::PREMISE)
            .into_iter()
            .map(|(i, _)| i)
            .filter(|&i| reaches[i].contains(&i))
            .collect();
        let mut grouped: BTreeSet<usize> = BTreeSet::new();
        for &i in &cyclic {
            if grouped.contains(&i) {
                continue;
            }
            let members: Vec<usize> = cyclic
                .iter()
                .copied()
                .filter(|&j| reaches[i].contains(&j) && reaches[j].contains(&i))
                .collect();
            grouped.extend(&members);
            self.inadmissible_premises.extend(&members);
            let mut ids: Vec<&str> = members
                .iter()
                .map(|&j| self.program.premises[j].id.as_str())
                .collect();
            ids.sort_unstable();
            self.program_finding(
                "cyclic_witness",
                &format!("premises[{}]", ids.join(", ")),
                if members.len() > 1 {
                    format!("mutual cycle among premises {}", ids.join(", "))
                } else {
                    format!(
                        "premise `{}` is established under an assumption of the proposition it asserts",
                        ids[0]
                    )
                },
            );
        }
        // Witness lookup consults only premises admitted whole —
        // attribution, conflict, and acyclicity all settled above.
        self.admissible_witnesses = premise_map;
        self.admissible_witnesses
            .values_mut()
            .for_each(|v| v.retain(|i| !self.inadmissible_premises.contains(i)));
        self.admissible_witnesses.retain(|_, v| !v.is_empty());
    }

    // -- steps --------------------------------------------------------------

    fn step(&mut self, index: usize) {
        let at = format!("body[{index}]");
        let step = &self.program.body[index];
        let kinds = [
            step.apply.is_some(),
            step.infer.is_some(),
            step.import.is_some(),
            step.hole.is_some(),
            step.goal.is_some(),
        ];
        if kinds.iter().filter(|k| **k).count() != 1 {
            self.program_finding(
                "malformed",
                &at,
                format!("step `{}` must carry exactly one operation", step.bind),
            );
            return;
        }
        self.current_binding = Some(step.bind.clone());
        if let Some(method) = step.apply.clone() {
            self.apply(index, &step.bind, &method);
        } else if let Some(infer) = &step.infer {
            self.infer(index, &step.bind, &infer.rule, &infer.arguments);
        } else if let Some(import) = &step.import {
            self.import(index, &step.bind, import);
        } else if let Some(hole) = &step.hole {
            let ty = self.type_of(hole, &format!("{at}.hole"));
            self.program_finding(
                "hole",
                &at,
                format!(
                    "`{}` awaits an authored term of the declared type",
                    step.bind
                ),
            );
            self.poison(&step.bind, ty);
        } else if let Some(goal) = &step.goal {
            let ty = self.type_of(goal, &format!("{at}.goal"));
            let candidates = self.goal_candidates(&ty);
            let state = if candidates.is_empty() {
                self.program_finding(
                    "hole",
                    &at,
                    format!("goal `{}` admits no declared method", step.bind),
                );
                "unresolved"
            } else if candidates.len() == 1 {
                self.program_finding(
                    "hole",
                    &at,
                    format!(
                        "goal `{}` is open; sole candidate `{}` must still be applied",
                        step.bind, candidates[0]
                    ),
                );
                "open"
            } else {
                self.findings.push(LanguageFinding {
                    code: code_for("ambiguity").to_string(),
                    kind: "ambiguity".into(),
                    document: "program".into(),
                    at: at.clone(),
                    pointer: self.pointer_for("program", &at),
                    binding: Some(step.bind.clone()),
                    detail: format!(
                        "goal `{}` admits candidates {}; the choice is semantic and requires an authored apply",
                        step.bind,
                        candidates.join(", ")
                    ),
                    blocking: true,
                    candidates: Some(candidates.clone()),
                });
                "ambiguous"
            };
            self.goals.insert(
                step.bind.clone(),
                GoalReport {
                    state: state.to_string(),
                    candidates,
                },
            );
            self.poison(&step.bind, ty);
        }
        self.current_binding = None;
    }

    /// Inserts an unestablished binding so later steps still resolve — one
    /// refusal does not cascade malformed findings through dependents.
    fn poison(&mut self, bind: &str, ty: QuantityType) {
        self.env.insert(
            bind.to_string(),
            SemanticValue {
                ty,
                unit: String::new(),
                value: None,
                state: ValueState::Unestablished,
                edges: BTreeSet::new(),
                assumptions: Vec::new(),
            },
        );
    }

    fn resolve(&mut self, reference: &str, at: &str) -> Option<SemanticValue> {
        match self.env.get(reference) {
            Some(v) => Some(v.clone()),
            None => {
                self.program_finding(
                    "malformed",
                    at,
                    format!("`{reference}` names no input or earlier binding"),
                );
                None
            }
        }
    }

    /// The most specific upstream `not_evaluated` rule among operand
    /// bindings — a blocked operand blocks what consumes it.
    fn propagated_block(&self, references: impl Iterator<Item = String>) -> Option<String> {
        references
            .filter_map(|r| self.blocked.get(&r).cloned())
            .next()
    }

    fn infer(&mut self, index: usize, bind: &str, rule: &str, arguments: &[RefDecl]) {
        let at = format!("body[{index}]");
        let primitive = match rule {
            "interval.add" => PrimitiveRule::Add,
            "interval.sub" => PrimitiveRule::Sub,
            "interval.mul" => PrimitiveRule::Mul,
            other => {
                self.program_finding(
                    "unsupported",
                    &at,
                    format!("no primitive rule named `{other}`"),
                );
                return;
            }
        };
        let references: Vec<String> = arguments.iter().map(|a| a.reference.clone()).collect();
        self.deps
            .insert(bind.to_string(), references.iter().cloned().collect());
        self.depends.insert(
            bind.to_string(),
            references
                .iter()
                .map(|r| DependencyLink {
                    kind: "operand".into(),
                    target: r.clone(),
                })
                .collect(),
        );
        let inherited_block = self.propagated_block(references.iter().cloned());
        let mut operands = BTreeMap::new();
        for (i, arg) in arguments.iter().enumerate() {
            match self.resolve(&arg.reference, &format!("{at}.arguments[{i}]")) {
                Some(v) => {
                    operands.insert(arg.reference.clone(), v);
                }
                None => return,
            }
        }
        let expr = Expression::Call(
            primitive,
            arguments
                .iter()
                .map(|a| Expression::Name(a.reference.clone()))
                .collect(),
        );
        match eval::eval(&expr, &operands, &self.library) {
            Ok(value) => {
                self.edges.insert(bind.to_string(), value.edges.clone());
                if let Some(rule) = inherited_block {
                    self.blocked.insert(bind.to_string(), rule);
                }
                self.env.insert(bind.to_string(), value);
            }
            Err(failure) => self.rule_failure(&at, failure),
        }
    }

    fn apply(&mut self, index: usize, bind: &str, method_id: &str) {
        let at = format!("body[{index}]");
        let step = &self.program.body[index];
        let Some(method) = self.library.methods.get(method_id).copied() else {
            self.program_finding(
                "undeclared_method",
                &at,
                format!("method `{method_id}` is not declared by the bound library"),
            );
            return;
        };

        // Resolve arguments and unify signature variables.
        let mut slot_values: BTreeMap<String, SemanticValue> = BTreeMap::new();
        let mut variables: BTreeMap<String, String> = BTreeMap::new(); // var → entity
        let mut bad = false;
        let mut operand_refs = Vec::new();
        for (slot, decl) in &method.inputs {
            let arg = step.arguments.as_ref().and_then(|args| args.get(slot));
            let Some(arg) = arg else {
                self.program_finding(
                    "missing_argument",
                    &format!("{at}.arguments"),
                    format!("signature slot `{slot}` of `{method_id}` has no argument"),
                );
                bad = true;
                continue;
            };
            operand_refs.push(arg.reference.clone());
            let Some(value) = self.resolve(&arg.reference, &format!("{at}.arguments.{slot}"))
            else {
                bad = true;
                continue;
            };
            if !value
                .ty
                .claim
                .satisfies(ClaimModel::parse(&decl.claim).unwrap_or(ClaimModel::Nominal))
            {
                self.program_finding(
                    "type_mismatch",
                    &format!("{at}.arguments.{slot}"),
                    format!(
                        "argument claim `{}` does not satisfy slot claim `{}`",
                        value.ty.claim.as_str(),
                        decl.claim
                    ),
                );
                bad = true;
            }
            if value.ty.quantity_kind != decl.quantity_kind {
                self.program_finding(
                    "type_mismatch",
                    &format!("{at}.arguments.{slot}"),
                    format!(
                        "argument kind `{}` ≠ slot kind `{}`",
                        value.ty.quantity_kind, decl.quantity_kind
                    ),
                );
                bad = true;
            }
            let mut missing: Vec<String> = Vec::new();
            for (relation, var) in slot_relations(decl) {
                match value.ty.relations.get(&relation) {
                    None => missing.push(relation.clone()),
                    Some(entity) => {
                        self.check_entity(&relation, entity, &format!("{at}.arguments.{slot}"));
                        match variables.get(&var) {
                            Some(existing) if existing != entity => {
                                self.program_finding(
                                    "type_mismatch",
                                    &format!("{at}.arguments.{slot}"),
                                    format!(
                                        "relation `{relation}` binds `{var}` to `{entity}` but `{var}` already stands for `{existing}`"
                                    ),
                                );
                                bad = true;
                            }
                            _ => {
                                variables.insert(var, entity.clone());
                            }
                        }
                    }
                }
            }
            if !missing.is_empty() {
                self.program_finding(
                    "relation_absent",
                    &format!("{at}.arguments.{slot}"),
                    format!(
                        "slot `{slot}` requires relations {} but the argument does not carry them",
                        missing.join(", ")
                    ),
                );
                bad = true;
            }
            slot_values.insert(slot.clone(), value);
        }
        if let Some(args) = &step.arguments {
            for slot in args.keys() {
                if !method.inputs.contains_key(slot) {
                    self.program_finding(
                        "malformed",
                        &format!("{at}.arguments"),
                        format!("`{slot}` names no slot of `{method_id}`"),
                    );
                    bad = true;
                }
            }
        }

        self.deps
            .insert(bind.to_string(), operand_refs.iter().cloned().collect());
        let mut links: Vec<DependencyLink> = operand_refs
            .iter()
            .map(|r| DependencyLink {
                kind: "operand".into(),
                target: r.clone(),
            })
            .collect();
        links.push(DependencyLink {
            kind: "method".into(),
            target: method_id.to_string(),
        });
        self.depends.insert(bind.to_string(), links);
        let inherited_block = self.propagated_block(operand_refs.iter().cloned());

        if bad {
            self.poison(bind, self.output_type(method, &variables));
            return;
        }

        // Lifecycle premise for this application.
        match self.check_lifecycle(index, method_id) {
            Usability::Refused(reasons) => {
                self.blocked
                    .insert(bind.to_string(), "not_evaluated.lifecycle_refused".into());
                self.program_finding(
                    "lifecycle_refused",
                    &at,
                    format!(
                        "method `{method_id}` refused by lifecycle: {}",
                        reasons.join(", ")
                    ),
                );
                self.poison(bind, self.output_type(method, &variables));
                return;
            }
            Usability::Conflict => {
                self.poison(bind, self.output_type(method, &variables));
                return;
            }
            _ => {}
        }

        // The union of operand relations, minus declared projections, is the
        // output's carried scope. The union is *checked*: operands carrying
        // conflicting entities for the same relation cannot be composed.
        let mut output_relations = RelationMap::new();
        let mut relation_conflict = false;
        for (slot, value) in &slot_values {
            for (key, entity) in &value.ty.relations {
                match output_relations.get(key) {
                    Some(existing) if existing != entity => {
                        self.program_finding(
                            "type_mismatch",
                            &format!("{at}.arguments.{slot}"),
                            format!(
                                "operand `{slot}` carries `{key}: {entity}` but another operand carries `{existing}` — the relation union conflicts"
                            ),
                        );
                        relation_conflict = true;
                    }
                    Some(_) => {}
                    None => {
                        output_relations.insert(key.clone(), entity.clone());
                    }
                }
            }
        }
        for key in &method.projects {
            output_relations.remove(key);
        }
        if relation_conflict {
            self.poison(bind, self.output_type(method, &variables));
            return;
        }
        let declared: BTreeSet<String> =
            output_slot_relations(&method.output).into_iter().collect();
        for key in output_relations.keys() {
            if !declared.contains(key) {
                self.program_finding(
                    "undeclared_projection",
                    &at,
                    format!(
                        "output of `{method_id}` carries `{key}` without declaring it or listing `projects`"
                    ),
                );
            }
        }
        // Every declared output relation must be bound — a signature may not
        // claim a scope no operand supplies.
        for (relation, var) in slot_relations(&method.output) {
            if !variables.contains_key(&var) {
                self.program_finding(
                    "malformed",
                    &at,
                    format!("output relation `{relation}` binds `{var}` but no input supplies it"),
                );
            }
        }

        // Generated obligations — iterated in canonical order: `requires` is
        // a declared set, and source order must never shape findings.
        let obligations_at = self.obligations.len();
        for (position, (_, requirement)) in canonical_order(&method.requires, &projection::REQUIRES)
            .into_iter()
            .enumerate()
        {
            self.obligation(index, position, method, requirement, step, &variables);
        }

        // Declared assumptions instantiate at the applied scope.
        let mut assumptions = Vec::new();
        for (_, name) in canonical_order(&method.assumes, &projection::SCALAR) {
            let params = self
                .library
                .propositions
                .get(name.as_str())
                .copied()
                .unwrap_or(&[]);
            let mut at_map = RelationMap::new();
            for param in params {
                if let Some((var, _)) = method
                    .variables
                    .iter()
                    .find(|(_, relation)| relation.as_str() == param.as_str())
                    && let Some(entity) = variables.get(var)
                {
                    at_map.insert((*param).clone(), entity.clone());
                }
            }
            assumptions.push(ScopedProposition {
                proposition: name.clone(),
                at: at_map,
            });
        }

        // Output value: primitive bodies evaluate now; external produces are
        // typed by the declared postcondition and observed at run time.
        let impl_ = method.implementation.as_ref();
        let (value, unit, state) = match impl_.map(|i| i.kind.as_str()) {
            Some("primitive") => {
                let body = impl_.and_then(|i| i.body.clone()).unwrap_or_default();
                match eval::parse(&body) {
                    Ok(expr) => match eval::eval(&expr, &slot_values, &self.library) {
                        Ok(v) => {
                            // The body's inferred type must satisfy the
                            // declared output slot — the signature never
                            // relabels an incompatible result.
                            let mut ok = true;
                            if v.ty.quantity_kind != method.output.quantity_kind {
                                self.program_finding(
                                    "type_mismatch",
                                    &format!("{at}.implementation"),
                                    format!(
                                        "body produces `{}` but the output declares `{}`",
                                        v.ty.quantity_kind, method.output.quantity_kind
                                    ),
                                );
                                ok = false;
                            }
                            if !v.ty.claim.satisfies(
                                ClaimModel::parse(&method.output.claim)
                                    .unwrap_or(ClaimModel::Nominal),
                            ) {
                                self.program_finding(
                                    "type_mismatch",
                                    &format!("{at}.implementation"),
                                    format!(
                                        "body claim `{}` does not satisfy declared output claim `{}`",
                                        v.ty.claim.as_str(),
                                        method.output.claim
                                    ),
                                );
                                ok = false;
                            }
                            (
                                v.value,
                                v.unit,
                                if ok {
                                    v.state
                                } else {
                                    ValueState::Unestablished
                                },
                            )
                        }
                        Err(failure) => {
                            self.rule_failure(&format!("{at}.implementation"), failure);
                            (None, String::new(), ValueState::Unestablished)
                        }
                    },
                    Err(failure) => {
                        self.parse_failure("program", &format!("{at}.implementation"), &failure);
                        (None, String::new(), ValueState::Unestablished)
                    }
                }
            }
            Some("external") => {
                // The declared postcondition types the value; it is observed
                // at run time. Every declared `ensures` claims a relation on
                // the output — where several are declared their claimed
                // values must agree.
                let mut declared_value = None;
                let mut conflicted = false;
                for ensures in canonical_order(&method.ensures, &projection::ENSURES)
                    .into_iter()
                    .map(|(_, e)| e)
                {
                    if let Ok(expression) = eval::parse_ensures(&ensures.expression)
                        && let Ok(v) = eval::eval(&expression, &slot_values, &self.library)
                    {
                        match (&declared_value, &v.value) {
                            (Some(a), Some(b)) if a != b => conflicted = true,
                            (None, Some(_)) => declared_value = v.value.clone(),
                            _ => {}
                        }
                    }
                }
                if conflicted {
                    self.program_finding(
                        "malformed",
                        &at,
                        format!(
                            "postconditions of `{method_id}` claim different output values — the signature is inconsistent"
                        ),
                    );
                }
                let state = if conflicted
                    || slot_values
                        .values()
                        .any(|v| v.state == ValueState::Unestablished)
                {
                    ValueState::Unestablished
                } else {
                    ValueState::Declared
                };
                let unit = impl_
                    .and_then(|i| i.produces.as_ref())
                    .map(|p| p.unit.clone())
                    .unwrap_or_default();
                // §7 [O1] — under `evaluate` a supplied observation binds
                // here: the observed output materializes the value, and the
                // runtime obligations replay against it. An unbound site
                // stays `declared` and its obligations open.
                if self.evaluating
                    && let Some(observed) =
                        self.bind_observation(index, method, &slot_values, &variables)
                {
                    self.observed_sites.insert(index);
                    self.observed_values.insert(index, observed.value.clone());
                    (observed.value, observed.unit, ValueState::Established)
                } else {
                    (declared_value, unit, state)
                }
            }
            _ => {
                self.program_finding(
                    "malformed",
                    &format!("{at}.implementation"),
                    format!("method `{method_id}` declares no usable implementation"),
                );
                (None, String::new(), ValueState::Unestablished)
            }
        };

        // Ensures obligations: each declared postcondition is an independent
        // obligation, iterated in canonical (set) order — the report is the
        // same whether the source lists them forward or reversed. A
        // primitive postcondition is discharged only when its named check
        // replays over the operand values and equals the body's actual
        // result; a false postcondition is refuted, never checked.
        let is_primitive = impl_.is_some_and(|im| im.kind == "primitive");
        for (position, (_, ensures)) in canonical_order(&method.ensures, &projection::ENSURES)
            .into_iter()
            .enumerate()
        {
            let obligation_at = format!("{at} {method_id}");
            let id = format!("{at}.ensures[{position}]");
            let check_ok = SUPPORTED_CHECKS.contains(&ensures.check.as_str());
            if is_primitive {
                // `output = <rhs>` — the check claims the body's result
                // equals eval(rhs) over the actual operands. A replay that
                // produces a *different* established value refutes the
                // postcondition; one that cannot run (missing values,
                // inapplicable rules, unparseable expression) leaves it
                // open, and the upstream explanation already exists — no
                // second finding.
                let parsed = eval::parse_ensures(&ensures.expression).ok();
                let mut state = ObligationState::Open;
                let mut replay_failure: Option<RuleFailure> = None;
                if check_ok && let Some(rhs) = &parsed {
                    match eval::eval(rhs, &slot_values, &self.library) {
                        Ok(expected) => match (&expected.value, &value) {
                            (Some(want), Some(got)) if want == got => {
                                state = ObligationState::Discharged;
                            }
                            (Some(_), Some(_)) => state = ObligationState::Refuted,
                            _ => {}
                        },
                        Err(failure) => replay_failure = Some(failure),
                    }
                }
                self.step_obligations
                    .entry(index)
                    .or_default()
                    .push(id.clone());
                self.obligations.push(ObligationReport {
                    id,
                    kind: "postcondition".into(),
                    at: obligation_at.clone(),
                    step: index,
                    state: state.as_str().into(),
                    check: Some(ensures.check.clone()),
                    expression: Some(ensures.expression.clone()),
                    operands: slot_values.keys().cloned().collect(),
                    detail: format!("`{}` checked by {}", ensures.expression, ensures.check),
                });
                if state == ObligationState::Refuted {
                    self.program_finding(
                        "obligation_refuted",
                        &obligation_at,
                        format!(
                            "postcondition `{}` replays to a different value than the body produces",
                            ensures.expression
                        ),
                    );
                }
                if let Some(failure) = replay_failure {
                    self.rule_failure(&obligation_at, failure);
                }
            } else {
                // External postconditions are runtime obligations: executed,
                // then checked against the observed values. Under `evaluate`
                // the check replays now — the observed output must equal the
                // value the postcondition computes over the operands.
                let (state, mut replay_failure) = if !self.evaluating {
                    (ObligationState::Runtime, None)
                } else if !self.observed_sites.contains(&index) {
                    (ObligationState::Open, None)
                } else {
                    match eval::parse_ensures(&ensures.expression) {
                        Ok(rhs) => match eval::eval(&rhs, &slot_values, &self.library) {
                            Ok(expected) => match (
                                &expected.value,
                                self.observed_values.get(&index).and_then(|v| v.as_ref()),
                            ) {
                                (Some(want), Some(got)) if want == got => {
                                    (ObligationState::Discharged, None)
                                }
                                (Some(_), Some(_)) => (ObligationState::Refuted, None),
                                _ => (ObligationState::Open, None),
                            },
                            Err(failure) => (ObligationState::Open, Some(failure)),
                        },
                        // A postcondition that cannot even parse is an
                        // authoring defect — already reported; the
                        // obligation stays open.
                        Err(_) => (ObligationState::Open, None),
                    }
                };
                self.step_obligations
                    .entry(index)
                    .or_default()
                    .push(id.clone());
                self.obligations.push(ObligationReport {
                    id,
                    kind: "postcondition".into(),
                    at: obligation_at.clone(),
                    step: index,
                    state: state.as_str().into(),
                    check: Some(ensures.check.clone()),
                    expression: Some(ensures.expression.clone()),
                    operands: slot_values.keys().cloned().collect(),
                    detail: format!("`{}` checked by {}", ensures.expression, ensures.check),
                });
                self.eval_events.push(execution::RuleOutcome {
                    rule: "discharge".into(),
                    subject: obligation_at.clone(),
                    state: state.as_str().into(),
                    detail: format!(
                        "postcondition `{}` {} by {}",
                        ensures.expression,
                        state.as_str(),
                        ensures.check
                    ),
                });
                if state == ObligationState::Refuted {
                    self.program_finding(
                        "obligation_refuted",
                        &obligation_at,
                        format!(
                            "postcondition `{}` replays to a different value than the observation produced",
                            ensures.expression
                        ),
                    );
                }
                if let Some(failure) = replay_failure.take() {
                    self.rule_failure(&obligation_at, failure);
                }
            }
        }
        if !is_primitive && method.ensures.is_empty() {
            // An external application with no declared postcondition still
            // owes the observation itself — the output value enters only
            // through execution, and there is nothing to replay.
            let id = format!("{at}.observation");
            let state = if !self.evaluating {
                ObligationState::Runtime
            } else if self.observed_sites.contains(&index) {
                ObligationState::Discharged
            } else {
                ObligationState::Open
            };
            self.step_obligations
                .entry(index)
                .or_default()
                .push(id.clone());
            self.obligations.push(ObligationReport {
                id,
                kind: "observation".into(),
                at: at.clone(),
                step: index,
                state: state.as_str().into(),
                check: None,
                expression: None,
                operands: slot_values.keys().cloned().collect(),
                detail: format!(
                    "output `{bind}` must be observed at execution; `{method_id}` declares no postcondition — nothing to replay"
                ),
            });
            if self.evaluating {
                self.eval_events.push(execution::RuleOutcome {
                    rule: "observe".into(),
                    subject: at.clone(),
                    state: state.as_str().into(),
                    detail: format!("observation obligation for `{bind}`"),
                });
            }
        }

        // A refuted or open obligation blocks verdicts derived from this
        // step. A refuted precondition means the method does not apply at all
        // — the binding carries no usable value.
        let mut block = inherited_block;
        let mut refuted = false;
        for obligation in &self.obligations[obligations_at..] {
            let state = match obligation.state.as_str() {
                "open" => ObligationState::Open,
                "refuted" => ObligationState::Refuted,
                "runtime" => ObligationState::Runtime,
                _ => ObligationState::Discharged,
            };
            if state == ObligationState::Refuted {
                refuted = true;
            }
            if let Some(rule) = state.blocked_rule() {
                block = Some(rule.to_string());
            }
        }
        if let Some(rule) = block {
            self.blocked.insert(bind.to_string(), rule);
        }

        // Support discharged by witnesses in this application (independence
        // premises) joins the conclusion's support.
        let witness_support: DischargeSupport = std::mem::take(&mut self.pending_support);

        if refuted {
            self.poison(bind, self.output_type(method, &variables));
            return;
        }

        let unit = if unit.is_empty() {
            self.library
                .doc
                .quantity_kinds
                .get(&method.output.quantity_kind)
                .and_then(|k| k.canonical_unit.clone())
                .unwrap_or_default()
        } else {
            unit
        };
        let mut edges: BTreeSet<String> = slot_values
            .values()
            .flat_map(|v| v.edges.iter().cloned())
            .collect();
        let mut assumptions = merge_assumptions(
            assumptions,
            slot_values.values().flat_map(|v| v.assumptions.clone()),
        );
        for assumption in witness_support.assumptions {
            if !assumptions.contains(&assumption) {
                assumptions.push(assumption);
            }
        }
        edges.extend(witness_support.edges);
        // §8.3 — an observed output rides a first-class provenance edge: the
        // receipt's digest is the edge identity, as `attribution` is for
        // declared sources.
        if self.observed_sites.contains(&index)
            && let Some(record) = self.observations.get(&index)
        {
            edges.insert(format!("observation:{}", record.receipt_sha256));
        }
        for premise in witness_support.premises {
            self.depends
                .entry(bind.to_string())
                .or_default()
                .push(DependencyLink {
                    kind: "premise".into(),
                    target: premise,
                });
        }
        self.edges.insert(bind.to_string(), edges.clone());
        self.env.insert(
            bind.to_string(),
            SemanticValue {
                ty: QuantityType {
                    quantity_kind: method.output.quantity_kind.clone(),
                    claim: ClaimModel::parse(&method.output.claim).unwrap_or(ClaimModel::Nominal),
                    relations: output_relations,
                },
                unit,
                value,
                state,
                edges,
                assumptions,
            },
        );
    }

    fn import(&mut self, index: usize, bind: &str, import: &ImportDecl) {
        let at = format!("body[{index}]");
        let ty = self.type_of(&import.ty, &format!("{at}.type"));
        let value = self.admit_value(&import.value, &ty, &at);
        let mut malformed = value.is_none();
        let assumptions: Vec<ScopedProposition> = match &import.assumptions {
            Some(list) => canonical_order(list, &projection::SCOPED_ASSERTION)
                .into_iter()
                .map(|(_, s)| ScopedProposition::from(s))
                .collect(),
            None => {
                malformed = true;
                self.program_finding(
                    "malformed_import",
                    &at,
                    "import declares no `assumptions` field".to_string(),
                );
                Vec::new()
            }
        };
        // An imported certificate must be replayable; an assertion must be
        // attributed. Without a replayable payload nothing is established.
        if let Some(source) = &import.source {
            let kind_ok = SOURCE_KINDS.contains(&source.kind.as_str());
            if !kind_ok {
                self.program_finding(
                    "malformed_import",
                    &format!("{at}.source"),
                    format!("unknown provenance kind `{}`", source.kind),
                );
                malformed = true;
            } else if source.kind == "certificate" {
                let digest_ok = source.digest.as_deref().is_some_and(valid_sha256_digest);
                let check_ok = source
                    .check
                    .as_deref()
                    .is_some_and(|c| SUPPORTED_CHECKS.contains(&c));
                if !digest_ok || !check_ok {
                    self.program_finding(
                        "unsupported",
                        &format!("{at}.source"),
                        "certificate has no replayable payload (supported `check` + `digest`)"
                            .to_string(),
                    );
                    malformed = true;
                }
            } else if source.party.is_none() {
                self.program_finding(
                    "malformed_import",
                    &format!("{at}.source"),
                    format!("`{}` provenance requires `party` attribution", source.kind),
                );
                malformed = true;
            }
        }
        if malformed {
            // An inadmissible import establishes nothing; the declared type
            // still binds so downstream steps can be checked.
            self.poison(bind, ty);
            return;
        }
        self.deps.insert(bind.to_string(), BTreeSet::new());
        self.depends
            .entry(bind.to_string())
            .or_default()
            .push(DependencyLink {
                kind: "import".into(),
                target: at.clone(),
            });
        let edges: BTreeSet<String> = import
            .source
            .as_ref()
            .and_then(|s| s.edge.clone())
            .into_iter()
            .collect();
        if !edges.is_empty() {
            self.edges.insert(bind.to_string(), edges.clone());
        }
        self.env.insert(
            bind.to_string(),
            SemanticValue {
                ty,
                unit: import.value.unit.clone(),
                value,
                state: ValueState::Established,
                edges,
                assumptions,
            },
        );
    }

    // -- obligations ---------------------------------------------------------

    /// Witness support discharged inside an apply — filled by `obligation`
    /// and drained into the result binding.
    /// Interpret one `requires` declaration at an application site.
    /// `position` is the declaration's position in the canonically ordered
    /// `requires` set — it names the obligation id so reordering the set
    /// changes no derived identifier.
    fn obligation(
        &mut self,
        index: usize,
        position: usize,
        method: &MethodDecl,
        requirement: &ObligationDecl,
        step: &super::document::StepDecl,
        variables: &BTreeMap<String, String>,
    ) {
        let at = format!("body[{index}] {}", method.id);
        let obligation_id = format!("body[{index}].requires[{position}]");
        match requirement.kind.as_str() {
            "domain_containment" => {
                let check = self.check_domain_containment(method, requirement, variables);
                let (state, finding) = match &check {
                    DomainCheck::Holds => (ObligationState::Discharged, None),
                    DomainCheck::Refuted => (
                        ObligationState::Refuted,
                        Some((
                            "precondition_refuted",
                            format!(
                                "operating domain `{}` is not contained in material applicability `{}`",
                                requirement.domain.clone().unwrap_or_default(),
                                requirement.within.clone().unwrap_or_default()
                            ),
                        )),
                    ),
                    DomainCheck::Unknown(reason) => (
                        ObligationState::Open,
                        Some(("obligation_unmet", reason.clone())),
                    ),
                };
                let id = obligation_id.clone();
                self.step_obligations
                    .entry(index)
                    .or_default()
                    .push(id.clone());
                self.obligations.push(ObligationReport {
                    id,
                    kind: "domain_containment".into(),
                    at: at.clone(),
                    step: index,
                    state: state.as_str().into(),
                    check: None,
                    expression: Some(format!(
                        "{} ⊆ {}",
                        requirement.domain.clone().unwrap_or_default(),
                        requirement.within.clone().unwrap_or_default()
                    )),
                    operands: Vec::new(),
                    detail: format!(
                        "`{}` ⊆ `{}`",
                        requirement.domain.clone().unwrap_or_default(),
                        requirement.within.clone().unwrap_or_default()
                    ),
                });
                if let Some((kind, detail)) = finding {
                    self.program_finding(kind, &at, detail);
                }
            }
            "scope_check" => {
                // The subject slot's scenario scope must refine the required
                // scope kind (§7.B scope rule).
                let entity = requirement
                    .subject
                    .as_ref()
                    .and_then(|slot| step.arguments.as_ref().and_then(|a| a.get(slot)))
                    .and_then(|r| self.env.get(&r.reference))
                    .and_then(|v| v.ty.relations.get("scenario"))
                    .cloned();
                let scope = entity
                    .as_ref()
                    .and_then(|e| self.program.entities.scenarios.get(e))
                    .map(|s| s.scope.clone());
                let required = requirement.scope.clone().unwrap_or_default();
                let state = match &scope {
                    Some(actual) if scope_refines(actual, &required) => ObligationState::Discharged,
                    Some(_) => ObligationState::Refuted,
                    None => ObligationState::Open,
                };
                let id = obligation_id.clone();
                self.step_obligations
                    .entry(index)
                    .or_default()
                    .push(id.clone());
                self.obligations.push(ObligationReport {
                    id,
                    kind: "scope_check".into(),
                    at: at.clone(),
                    step: index,
                    state: state.as_str().into(),
                    check: None,
                    expression: Some(format!(
                        "scope({}) ≼ {required}",
                        requirement.subject.clone().unwrap_or_default()
                    )),
                    operands: entity.into_iter().collect(),
                    detail: format!(
                        "scope of `{}` must refine `{required}`",
                        requirement.subject.clone().unwrap_or_default()
                    ),
                });
                match state {
                    ObligationState::Refuted => self.program_finding(
                        "precondition_refuted",
                        &at,
                        format!("subject scope `{scope:?}` does not refine `{required}`"),
                    ),
                    ObligationState::Open => self.program_finding(
                        "obligation_unmet",
                        &at,
                        "scope_check subject carries no declared scenario".into(),
                    ),
                    _ => {}
                }
            }
            kind @ ("independence" | "provenance_disjoint") => {
                // `over` names signature slots; the resolved set is the
                // program identifiers bound to those slots.
                let bound: BTreeSet<String> = requirement
                    .over
                    .clone()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|slot| {
                        step.arguments
                            .as_ref()
                            .and_then(|args| args.get(slot))
                            .map(|r| r.reference.clone())
                    })
                    .collect();
                let scope: RelationMap = variables
                    .iter()
                    .filter_map(|(var, entity)| {
                        method
                            .variables
                            .get(var.as_str())
                            .map(|relation| (relation.clone(), entity.clone()))
                    })
                    .collect();
                if kind == "provenance_disjoint" {
                    // Effective edges — recorded provenance plus the witness
                    // cone of every assumption the operand still rests on.
                    let mut verifying = BTreeSet::new();
                    let edge_sets: Vec<BTreeSet<String>> = bound
                        .iter()
                        .map(|id| self.effective_edges(id, &mut verifying).unwrap_or_default())
                        .collect();
                    let all_have_edges = edge_sets.iter().all(|s| !s.is_empty());
                    let mut shared = false;
                    for i in 0..edge_sets.len() {
                        for j in (i + 1)..edge_sets.len() {
                            if edge_sets[i].intersection(&edge_sets[j]).next().is_some() {
                                shared = true;
                            }
                        }
                    }
                    let state = if !all_have_edges {
                        ObligationState::Open
                    } else if shared {
                        ObligationState::Refuted
                    } else {
                        ObligationState::Discharged
                    };
                    let id = obligation_id.clone();
                    self.step_obligations
                        .entry(index)
                        .or_default()
                        .push(id.clone());
                    self.obligations.push(ObligationReport {
                        id,
                        kind: "provenance_disjoint".into(),
                        at: at.clone(),
                        step: index,
                        state: state.as_str().into(),
                        check: None,
                        expression: None,
                        operands: bound.iter().cloned().collect(),
                        detail: format!(
                            "over {}",
                            bound.iter().cloned().collect::<Vec<_>>().join(", ")
                        ),
                    });
                    match state {
                        ObligationState::Refuted => self.program_finding(
                            "obligation_refuted",
                            &at,
                            "recorded source edges are shared across the `over` set".into(),
                        ),
                        ObligationState::Open => self.program_finding(
                            "obligation_unmet",
                            &at,
                            "recorded source edges are incomplete".into(),
                        ),
                        _ => {}
                    }
                } else {
                    // independent(S): discharged only by an admissible,
                    // attributed premise or checked certificate matching
                    // scope and member set; the recorded graph never
                    // discharges it (§7.E4b). Every admissible witness
                    // contributes its support.
                    let witnesses: Vec<usize> =
                        canonical_order(&self.program.premises, &projection::PREMISE)
                            .into_iter()
                            .filter(|(i, p)| {
                                p.proposition == "independent"
                                    && p.at == scope
                                    && p.arguments
                                        .as_ref()
                                        .map(|a| a.over.iter().cloned().collect::<BTreeSet<_>>())
                                        .unwrap_or_default()
                                        == bound
                                    && !self.inadmissible_premises.contains(i)
                                    && p.established_by.is_some()
                            })
                            .map(|(i, _)| i)
                            .collect();
                    let inadmissible_named: Vec<&str> =
                        canonical_order(&self.program.premises, &projection::PREMISE)
                            .into_iter()
                            .filter(|(i, p)| {
                                p.proposition == "independent"
                                    && p.at == scope
                                    && self.inadmissible_premises.contains(i)
                            })
                            .map(|(_, p)| p.id.as_str())
                            .collect();
                    if witnesses.is_empty() {
                        let mut detail = format!(
                            "no premise attests independence over {}",
                            bound.iter().cloned().collect::<Vec<_>>().join(", ")
                        );
                        if !inadmissible_named.is_empty() {
                            detail.push_str(&format!(
                                "; premise {} exists but is inadmissible",
                                inadmissible_named.join(", ")
                            ));
                        }
                        let id = obligation_id.clone();
                        self.step_obligations
                            .entry(index)
                            .or_default()
                            .push(id.clone());
                        self.obligations.push(ObligationReport {
                            id,
                            kind: "independence".into(),
                            at: at.clone(),
                            step: index,
                            state: ObligationState::Open.as_str().into(),
                            check: None,
                            expression: None,
                            operands: bound.iter().cloned().collect(),
                            detail,
                        });
                        self.program_finding(
                            "obligation_unmet",
                            &at,
                            "independence requires an attributed premise; none discharged it"
                                .into(),
                        );
                    } else {
                        let id = obligation_id.clone();
                        self.step_obligations
                            .entry(index)
                            .or_default()
                            .push(id.clone());
                        let names: Vec<String> = witnesses
                            .iter()
                            .map(|&i| self.program.premises[i].id.clone())
                            .collect();
                        self.obligations.push(ObligationReport {
                            id,
                            kind: "independence".into(),
                            at: at.clone(),
                            step: index,
                            state: ObligationState::Discharged.as_str().into(),
                            check: None,
                            expression: None,
                            operands: bound.iter().cloned().collect(),
                            detail: format!("discharged by premise {}", names.join(", ")),
                        });
                        // Witness support joins the conclusion's support —
                        // the discharge is conditional on the witnesses' own
                        // residual assumptions and rides on their edges.
                        for &i in &witnesses {
                            let premise = &self.program.premises[i];
                            self.pending_support.assumptions.extend(
                                canonical_order(
                                    &premise.assumptions,
                                    &projection::SCOPED_ASSERTION,
                                )
                                .into_iter()
                                .map(|(_, s)| ScopedProposition::from(s)),
                            );
                            if let Some(edge) =
                                premise.established_by.as_ref().and_then(|p| p.edge.clone())
                            {
                                self.pending_support.edges.insert(edge);
                            }
                            self.pending_support.premises.push(premise.id.clone());
                        }
                    }
                }
            }
            other => {
                self.program_finding("malformed", &at, format!("unknown requires kind `{other}`"));
            }
        }
    }

    /// Tri-state containment: `domain` ⊆ `within` where both are admitted
    /// intervals over the same quantity kind and unit. Malformed intervals
    /// and unit mismatches are undecidable, not refutations.
    fn check_domain_containment(
        &mut self,
        method: &MethodDecl,
        requirement: &ObligationDecl,
        variables: &BTreeMap<String, String>,
    ) -> DomainCheck {
        let entity_for = |path: &str| -> Option<(String, String)> {
            let (relation, field) = path.split_once('.')?;
            let var = method
                .variables
                .iter()
                .find(|(_, rel)| rel.as_str() == relation)
                .map(|(var, _)| var)?;
            variables.get(var).map(|e| (field.to_string(), e.clone()))
        };
        let (Some((_domain_field, domain_entity)), Some((_within_field, within_entity))) = (
            requirement.domain.as_deref().and_then(&entity_for),
            requirement.within.as_deref().and_then(&entity_for),
        ) else {
            return DomainCheck::Unknown(
                "domain or applicability path does not resolve through the signature".into(),
            );
        };
        let domain = self
            .program
            .entities
            .scenarios
            .get(&domain_entity)
            .and_then(|s| s.operating_domain.as_ref());
        let within = self
            .program
            .entities
            .materials
            .get(&within_entity)
            .map(|m| &m.applicability);
        let (Some(domain), Some(within)) = (domain, within) else {
            return DomainCheck::Unknown(format!(
                "`{domain_entity}` or `{within_entity}` declares no operating domain/applicability"
            ));
        };
        let Some(within_interval) = within.get(&domain.quantity_kind) else {
            return DomainCheck::Unknown(format!(
                "applicability of `{within_entity}` declares no `{}` interval",
                domain.quantity_kind
            ));
        };
        if domain.unit != within_interval.unit {
            self.program_finding(
                "type_mismatch",
                &format!("entities.scenarios[{domain_entity}]"),
                format!(
                    "domain unit `{}` is incompatible with applicability unit `{}`",
                    domain.unit, within_interval.unit
                ),
            );
            return DomainCheck::Unknown("domain and applicability units are incompatible".into());
        }
        let mut parse_pair =
            |lower: &str, upper: &str, at: String| -> Option<(ExactNumber, ExactNumber)> {
                let (Ok(lo), Ok(hi)) = (
                    ExactNumber::from_canonical(lower),
                    ExactNumber::from_canonical(upper),
                ) else {
                    self.program_finding(
                        "malformed",
                        &at,
                        format!("bounds `{lower}`..`{upper}` are not canonical rationals"),
                    );
                    return None;
                };
                if lo.checked_cmp(&hi).is_ok_and(|o| o.is_gt()) {
                    self.program_finding(
                        "malformed",
                        &at,
                        format!("interval is inverted: {lower} > {upper}"),
                    );
                    return None;
                }
                Some((lo, hi))
            };
        let (Some((dl, du)), Some((wl, wu))) = (
            parse_pair(
                &domain.lower,
                &domain.upper,
                format!("entities.scenarios[{domain_entity}].operating_domain"),
            ),
            parse_pair(
                &within_interval.lower,
                &within_interval.upper,
                format!(
                    "entities.materials[{within_entity}].applicability[{}]",
                    domain.quantity_kind
                ),
            ),
        ) else {
            return DomainCheck::Unknown("domain or applicability interval is malformed".into());
        };
        if wl.checked_cmp(&dl).is_ok_and(|o| !o.is_gt())
            && wu.checked_cmp(&du).is_ok_and(|o| !o.is_lt())
        {
            DomainCheck::Holds
        } else {
            DomainCheck::Refuted
        }
    }

    // -- lifecycle -----------------------------------------------------------

    /// §10.2: applicable entries are library-scope and method-scope; states
    /// are drawn from the closed set (validated at grammar admission).
    /// Union-of-refusals — all refusal reasons are named, and each
    /// applicable entry's actual state is retained in the report.
    fn check_lifecycle(&mut self, index: usize, method_id: &str) -> Usability {
        let at = format!("body[{index}]");
        let lib = &self.library.doc.library;
        let keys = [
            format!("library:{}@{}", lib.name, lib.revision),
            format!("method:{}/{}@{}", lib.name, method_id, lib.revision),
        ];
        let mut states: Vec<(String, String)> = Vec::new();
        for key in &keys {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for entry in self.options.lifecycle.iter().filter(|e| e.key == *key) {
                if LIFECYCLE_STATES.contains(&entry.state.as_str()) {
                    seen.insert(entry.state.as_str());
                }
            }
            // Equal-state duplicates collapse; distinct states for the same
            // key are a conflict — iteration order never decides.
            if seen.len() > 1 {
                let ordered: Vec<&str> = seen.iter().copied().collect();
                self.program_finding(
                    "lifecycle_conflict",
                    &at,
                    format!(
                        "conflicting lifecycle states for `{key}`: {}",
                        ordered.join(", ")
                    ),
                );
                return Usability::Conflict;
            }
            if let Some(state) = seen.iter().next() {
                states.push((key.clone(), (*state).to_string()));
            }
        }
        let entries: Vec<LifecycleEntryReport> = states
            .iter()
            .map(|(k, s)| LifecycleEntryReport {
                key: k.clone(),
                state: s.clone(),
            })
            .collect();
        if states.is_empty() {
            self.lifecycle.push(LifecycleReport {
                at,
                method: method_id.to_string(),
                state: "absent".into(),
                entries,
                refusal_reasons: Vec::new(),
                notices: Vec::new(),
            });
            return Usability::Absent;
        }
        let refusals: Vec<String> = states
            .iter()
            .filter(|(_, s)| matches!(s.as_str(), "expired" | "withdrawn"))
            .map(|(k, s)| format!("{k} is {s}"))
            .collect();
        let notices: Vec<String> = states
            .iter()
            .filter(|(_, s)| s.as_str() == "superseded")
            .map(|(k, _)| format!("{k} is superseded"))
            .collect();
        if !refusals.is_empty() {
            self.lifecycle.push(LifecycleReport {
                at,
                method: method_id.to_string(),
                state: "refused".into(),
                entries,
                refusal_reasons: refusals.clone(),
                notices,
            });
            return Usability::Refused(refusals);
        }
        self.lifecycle.push(LifecycleReport {
            at,
            method: method_id.to_string(),
            state: "usable".into(),
            entries,
            refusal_reasons: Vec::new(),
            notices,
        });
        Usability::Usable
    }

    // -- premise discharge (spec A3) -----------------------------------------

    /// Every admissible premise asserting a residual assumption discharges
    /// it; the conclusion's support becomes the union of the witnesses' own
    /// residual support, and their recorded edges join the binding's
    /// provenance. Inadmissible premises never discharge — the assumption
    /// stays residual.
    ///
    /// Discharge is a fixpoint: a witness's own assumptions expand in turn,
    /// so the provenance and support folded into the binding are the full
    /// witness cone (§8.3 — dependency edges persist through derivation),
    /// not just the first hop. Bounded — each distinct proposition expands
    /// once, and the expansion work is capped by the witness budget.
    fn discharge_assumptions(&mut self) {
        let names: Vec<String> = self.env.keys().cloned().collect();
        for name in names {
            let roots = self.env.get(&name).unwrap().assumptions.clone();
            let (residual, edges, links) =
                self.support_cone(&roots, &name.to_string(), &mut BTreeSet::new());
            let value = self.env.get_mut(&name).unwrap();
            value.assumptions = residual;
            value.edges.extend(edges);
            if !links.is_empty() {
                self.depends.entry(name).or_default().extend(links);
            }
        }
        // Sync the provenance store — the folded witness cone joins each
        // binding's recorded edges.
        for (name, value) in &self.env {
            if !value.edges.is_empty() {
                self.edges.insert(name.clone(), value.edges.clone());
            }
        }
    }

    /// Expand a set of assumptions through their admissible witnesses to a
    /// fixpoint: an assumption with a witness is discharged and replaced by
    /// the witness's own support (which expands in turn); an assumption
    /// with none stays residual. Returns the residual propositions, the
    /// provenance edges contributed by every admissible witness in the
    /// cone, and the premise dependency links.
    fn support_cone(
        &mut self,
        roots: &[ScopedProposition],
        at: &str,
        verifying: &mut BTreeSet<usize>,
    ) -> (
        Vec<ScopedProposition>,
        BTreeSet<String>,
        Vec<DependencyLink>,
    ) {
        let mut residual: Vec<ScopedProposition> = Vec::new();
        let mut edges = BTreeSet::new();
        let mut links = Vec::new();
        let mut seen: BTreeSet<ScopedProposition> = roots.iter().cloned().collect();
        let mut steps = 0usize;
        // Roots expand in declaration order so residuals stay stable; each
        // proposition in a cone is visited once via `seen`.
        for root in roots {
            let mut stack = vec![root.clone()];
            while let Some(prop) = stack.pop() {
                steps += 1;
                if steps > MAX_WITNESS_DEPTH * MAX_WITNESS_DEPTH.max(1) {
                    self.program_finding(
                        "budget",
                        at,
                        "witness support expansion exceeds the bound".to_string(),
                    );
                    residual.append(&mut stack);
                    return (residual, edges, links);
                }
                let key = (prop.proposition.clone(), prop.at.clone());
                let witnesses: Vec<usize> = self
                    .admissible_witnesses
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                if witnesses.is_empty() {
                    residual.push(prop);
                    continue;
                }
                let mut fired = false;
                for i in witnesses {
                    let premise = &self.program.premises[i];
                    // A witness asserting disjointness is checked at use
                    // time too — its `over` set may name bindings whose
                    // cones did not exist at admission. `verifying` bounds
                    // the recursion through operand cones.
                    if premise.proposition == "provenance_disjoint" {
                        // Already under verification up-stack — a witness
                        // cycle through operand cones; do not fold.
                        if !verifying.insert(i) {
                            continue;
                        }
                        let bound: Vec<String> = premise
                            .arguments
                            .as_ref()
                            .map(|a| a.over.clone())
                            .unwrap_or_default();
                        // An operand not yet bound cannot be fully
                        // verified — the premise does not discharge yet.
                        if bound.iter().any(|id| !self.env.contains_key(id)) {
                            verifying.remove(&i);
                            continue;
                        }
                        let sets: Vec<BTreeSet<String>> = bound
                            .iter()
                            .map(|id| self.effective_edges(id, verifying).unwrap_or_default())
                            .collect();
                        verifying.remove(&i);
                        let shared = sets.iter().enumerate().any(|(a, s)| {
                            sets[a + 1..]
                                .iter()
                                .any(|t| s.intersection(t).next().is_some())
                        });
                        if sets.iter().all(|s| !s.is_empty()) && shared {
                            self.program_finding(
                                "premise_conflict",
                                &format!("premises[{}]", premise.id),
                                format!(
                                    "premise `{}` asserts provenance_disjoint over operands that share a recorded edge",
                                    premise.id
                                ),
                            );
                            continue;
                        }
                    }
                    fired = true;
                    if let Some(edge) = premise.established_by.as_ref().and_then(|p| p.edge.clone())
                    {
                        edges.insert(edge);
                    }
                    links.push(DependencyLink {
                        kind: "premise".into(),
                        target: premise.id.clone(),
                    });
                    for s in &premise.assumptions {
                        let s = ScopedProposition::from(s);
                        if s != prop && seen.insert(s.clone()) {
                            stack.push(s);
                        }
                    }
                }
                if !fired {
                    residual.push(prop);
                }
            }
        }
        (residual, edges, links)
    }

    /// The provenance a binding provably carries — its recorded edges plus
    /// the witness cone of every assumption it still rests on. Disjointness
    /// must see conditional support: an assumption that discharges through
    /// an admissible premise makes the premise's provenance the binding's
    /// own.
    fn effective_edges(
        &mut self,
        name: &str,
        verifying: &mut BTreeSet<usize>,
    ) -> Option<BTreeSet<String>> {
        let mut edges = self.edges.get(name).cloned().unwrap_or_default();
        if let Some(assumptions) = self.env.get(name).map(|v| v.assumptions.clone()) {
            edges.extend(self.env.get(name).unwrap().edges.iter().cloned());
            let (_, cone, _) = self.support_cone(&assumptions, name, verifying);
            edges.extend(cone);
        }
        if edges.is_empty() { None } else { Some(edges) }
    }

    // -- helpers --------------------------------------------------------------

    fn type_of(&mut self, slot: &SlotDecl, at: &str) -> QuantityType {
        let claim = match ClaimModel::parse(&slot.claim) {
            Some(c) => c,
            None => {
                self.program_finding("malformed", at, format!("unknown claim `{}`", slot.claim));
                ClaimModel::Nominal
            }
        };
        if !self
            .library
            .doc
            .quantity_kinds
            .contains_key(&slot.quantity_kind)
        {
            self.program_finding(
                "malformed",
                at,
                format!(
                    "quantity kind `{}` is not declared by the bound library",
                    slot.quantity_kind
                ),
            );
        }
        let mut relations = RelationMap::new();
        for (key, entity) in [
            ("geometry", &slot.geometry),
            ("scenario", &slot.scenario),
            ("material", &slot.material),
        ] {
            if let Some(e) = entity {
                self.check_entity(key, e, at);
                relations.insert(key.to_string(), e.clone());
            }
        }
        QuantityType {
            quantity_kind: slot.quantity_kind.clone(),
            claim,
            relations,
        }
    }

    fn check_entity(&mut self, relation: &str, entity: &str, at: &str) {
        let known = match relation {
            "geometry" => self.program.entities.geometries.contains_key(entity),
            "scenario" => self.program.entities.scenarios.contains_key(entity),
            "material" => self.program.entities.materials.contains_key(entity),
            _ => true,
        };
        if !known {
            self.program_finding(
                "undeclared_entity",
                at,
                format!("relation `{relation}` names undeclared entity `{entity}`"),
            );
        }
    }

    fn rule_failure(&mut self, at: &str, failure: RuleFailure) {
        let (kind, detail) = match &failure {
            RuleFailure::NominalOperand { operand } => (
                "unsupported",
                format!("no arithmetic rule applies to nominal operand `{operand}`"),
            ),
            RuleFailure::KindMismatch { left, right } => (
                "type_mismatch",
                format!("`{left}` and `{right}` are different quantity kinds"),
            ),
            RuleFailure::NoProductRow { lhs, rhs } => (
                "unsupported",
                format!("no declared kind_products row for `{lhs}` × `{rhs}`"),
            ),
            RuleFailure::RelationConflict { key, left, right } => (
                "type_mismatch",
                format!("operands disagree on relation `{key}`: `{left}` vs `{right}`"),
            ),
            RuleFailure::Budget(detail) => ("budget", detail.clone()),
            RuleFailure::Arithmetic(detail) => ("unsupported", detail.clone()),
        };
        self.program_finding(kind, at, detail);
    }

    fn goal_candidates(&self, goal: &QuantityType) -> Vec<String> {
        let needed: BTreeSet<String> = goal.relations.keys().cloned().collect();
        canonical_order(&self.library.doc.methods, &projection::METHOD)
            .into_iter()
            .map(|(_, m)| m)
            .filter(|m| {
                m.output.quantity_kind == goal.quantity_kind
                    && ClaimModel::parse(&m.output.claim).is_some_and(|c| c.satisfies(goal.claim))
                    && {
                        let declared: BTreeSet<String> =
                            output_slot_relations(&m.output).into_iter().collect();
                        needed.is_subset(&declared)
                    }
            })
            .map(|m| m.id.clone())
            .collect()
    }

    fn output_type(
        &self,
        method: &MethodDecl,
        variables: &BTreeMap<String, String>,
    ) -> QuantityType {
        let mut relations = RelationMap::new();
        for (key, var) in slot_relations(&method.output) {
            if let Some(entity) = variables.get(&var) {
                relations.insert(key, entity.clone());
            }
        }
        QuantityType {
            quantity_kind: method.output.quantity_kind.clone(),
            claim: ClaimModel::parse(&method.output.claim).unwrap_or(ClaimModel::Nominal),
            relations,
        }
    }

    /// Bindings some requirement or goal transitively depends on.
    fn reachable_bindings(&self) -> BTreeSet<String> {
        let mut roots: BTreeSet<String> = self
            .program
            .requirements
            .iter()
            .map(|r| r.subject.reference.clone())
            .collect();
        // Goals are objectives in their own right — always reachable.
        for step in &self.program.body {
            if step.goal.is_some() {
                roots.insert(step.bind.clone());
            }
        }
        let mut reachable = roots.clone();
        let mut stack: Vec<String> = roots.into_iter().collect();
        while let Some(name) = stack.pop() {
            if let Some(deps) = self.deps.get(&name) {
                for dep in deps {
                    if reachable.insert(dep.clone()) {
                        stack.push(dep.clone());
                    }
                }
            }
        }
        reachable
    }

    fn plan_steps(&self, reachable: &BTreeSet<String>) -> Vec<PlanStepReport> {
        self.program
            .body
            .iter()
            .enumerate()
            .filter(|(_i, step)| {
                step.goal.is_some() || (reachable.contains(&step.bind) && step.hole.is_none())
            })
            .map(|(i, step)| {
                let (op, target, arguments) = if let Some(m) = &step.apply {
                    (
                        "apply",
                        Some(m.clone()),
                        step.arguments
                            .as_ref()
                            .map(|args| {
                                args.iter()
                                    .map(|(slot, r)| (slot.clone(), r.reference.clone()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    )
                } else if let Some(infer) = &step.infer {
                    (
                        "infer",
                        Some(infer.rule.clone()),
                        infer
                            .arguments
                            .iter()
                            .enumerate()
                            .map(|(i, r)| (format!("_{i}"), r.reference.clone()))
                            .collect(),
                    )
                } else if step.import.is_some() {
                    ("import", None, BTreeMap::new())
                } else if step.goal.is_some() {
                    ("goal", None, BTreeMap::new())
                } else {
                    ("hole", None, BTreeMap::new())
                };
                PlanStepReport {
                    at: format!("body[{i}]"),
                    bind: step.bind.clone(),
                    operation: op.to_string(),
                    target,
                    arguments,
                    obligations: self.step_obligations.get(&i).cloned().unwrap_or_default(),
                    unit: self.env.get(&step.bind).map(|v| v.unit.clone()),
                }
            })
            .collect()
    }

    /// The `avila.core/execution-plan/v0.1-draft` document this analysis
    /// lowers to (§1 `plan` — spec §7.4). Only reachable `apply` steps whose
    /// implementation is `external` become invocations; every runtime
    /// obligation is attached to its invocation — a step that cannot carry
    /// its obligations refuses the plan rather than omitting them.
    fn emit_execution_plan(
        &self,
        analysis: &LanguageAnalysis,
        reachable: &BTreeSet<String>,
    ) -> execution::ExecutionPlanDocument {
        let context = execution::PlanContext {
            program_sha256: analysis
                .program
                .identity
                .semantic_sha256
                .clone()
                .unwrap_or_default(),
            library_sha256: analysis
                .library
                .identity
                .semantic_sha256
                .clone()
                .unwrap_or_default(),
            analysis_sha256: sha256_of(
                &execution::canonical_json_bytes(&analysis).unwrap_or_default(),
            ),
        };
        let mut invocations = Vec::new();
        for (index, step) in self.program.body.iter().enumerate() {
            let Some(method_id) = &step.apply else {
                continue;
            };
            if !reachable.contains(&step.bind) || step.hole.is_some() {
                continue;
            }
            let Some(method) = self.library.methods.get(method_id.as_str()) else {
                continue;
            };
            let Some(implementation) = &method.implementation else {
                continue;
            };
            if implementation.kind != "external" {
                continue;
            }
            let mut inputs = BTreeMap::new();
            for slot in method.inputs.keys() {
                let binding = step
                    .arguments
                    .as_ref()
                    .and_then(|a| a.get(slot))
                    .map(|r| r.reference.clone())
                    .unwrap_or_default();
                let entry = match self.env.get(&binding).and_then(|value| {
                    execution::staged_value_bytes(value)
                        .ok()
                        .map(|bytes| (value, bytes))
                }) {
                    Some((value, bytes)) => execution::PlannedInput {
                        binding: binding.clone(),
                        state: "staged".into(),
                        value: execution::value_decl(value).ok(),
                        sha256: Some(sha256_of(&bytes)),
                    },
                    // No stageable value reaches this slot — the invocation
                    // cannot run; the plan says so rather than fabricating
                    // an operand the executable never saw.
                    None => execution::PlannedInput {
                        binding: binding.clone(),
                        state: "unstaged".into(),
                        value: None,
                        sha256: None,
                    },
                };
                inputs.insert(slot.clone(), entry);
            }
            invocations.push(execution::PlannedInvocation {
                at: format!("body[{index}]"),
                bind: step.bind.clone(),
                method: method_id.clone(),
                executable: implementation.executable.clone().unwrap_or_default(),
                effects: method.effects.clone(),
                inputs,
                produces: implementation
                    .produces
                    .as_ref()
                    .map(|p| execution::Produces {
                        quantity_kind: p.quantity_kind.clone(),
                        unit: p.unit.clone(),
                    }),
                obligations: self
                    .step_obligations
                    .get(&index)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|id| {
                        self.obligations
                            .iter()
                            .find(|o| &o.id == id)
                            .is_some_and(|o| o.state == "runtime")
                    })
                    .collect(),
            });
        }
        let mut doc = execution::ExecutionPlanDocument {
            schema_version: execution::EXECUTION_PLAN_SCHEMA_VERSION.into(),
            profile: LANGUAGE_PROFILE.into(),
            state: "ready".into(),
            context,
            invocations,
            refused_by: Vec::new(),
            plan_sha256: None,
        };
        doc.plan_sha256 = Some(plan_identity(&doc));
        doc
    }

    /// §7 [O1] — bind a supplied observation to this application site. The
    /// observation must name the site, carry the digest of every staged
    /// input exactly, re-hash its receipt, and agree with the receipt's
    /// own input/output digests — a receipt or output transplanted from
    /// another invocation fails the comparison. Returns the admitted
    /// output value on success; the caller records the site as observed.
    fn bind_observation(
        &mut self,
        index: usize,
        method: &MethodDecl,
        slot_values: &BTreeMap<String, SemanticValue>,
        variables: &BTreeMap<String, String>,
    ) -> Option<SemanticValue> {
        let at = format!("body[{index}]");
        let record = self.observations.get(&index)?.clone();
        let reject = |analyzer: &mut Self, detail: String| {
            analyzer.program_finding("observation_foreign", &at, detail.clone());
            analyzer.eval_events.push(execution::RuleOutcome {
                rule: "observe".into(),
                subject: at.clone(),
                state: "rejected".into(),
                detail,
            });
        };
        if record.at != at {
            reject(
                self,
                format!(
                    "observation declares site `{}` — supplied for `{at}`",
                    record.at
                ),
            );
            return None;
        }
        // Receipt bytes must re-hash to the declared digest — otherwise the
        // observation's own integrity claim fails before any content check.
        let receipt_digest = execution::canonical_json_bytes(&record.receipt)
            .ok()
            .map(|b| sha256_of(&b));
        if receipt_digest.as_deref() != Some(record.receipt_sha256.as_str()) {
            reject(
                self,
                "receipt bytes do not re-hash to the declared `receipt_sha256`".into(),
            );
            return None;
        }
        // The receipt must claim this plan, this site, and this executable —
        // a receipt cut for another invocation is foreign even when its
        // digests are internally consistent.
        let declared_executable = method
            .implementation
            .as_ref()
            .and_then(|i| i.executable.clone())
            .unwrap_or_default();
        if record.receipt.plan_sha256.as_str()
            != self.expected_plan_sha256.as_deref().unwrap_or_default()
            || record.receipt.at != at
            || record.receipt.executable != declared_executable
            || record.executable != declared_executable
        {
            reject(
                self,
                "receipt does not belong to this plan, site, or executable".into(),
            );
            return None;
        }
        // Input digests: every slot the plan staged must appear with the
        // digest of its canonical bytes — same set, same digests.
        let mut expected_inputs = BTreeMap::new();
        for (slot, value) in slot_values {
            if let Ok(bytes) = execution::staged_value_bytes(value) {
                expected_inputs.insert(slot.clone(), sha256_of(&bytes));
            }
        }
        if record.inputs.len() != expected_inputs.len()
            || record
                .inputs
                .iter()
                .any(|(slot, digest)| expected_inputs.get(slot) != Some(digest))
            || record.receipt.inputs.len() != expected_inputs.len()
            || record
                .receipt
                .inputs
                .iter()
                .any(|input| expected_inputs.get(&input.slot) != Some(&input.sha256))
        {
            reject(
                self,
                "observation input digests do not match the invocation's operands".into(),
            );
            return None;
        }
        // The invocation identity re-derives from capability + inputs +
        // invocation; the recorded digest must match — a receipt that
        // claims one thing and attests another fails here.
        let invocation_digest = execution::invocation_identity(&record.receipt);
        if invocation_digest.as_deref() != Some(record.receipt.invocation_sha256.as_str()) {
            reject(
                self,
                "receipt `invocation_sha256` does not re-derive from its parts".into(),
            );
            return None;
        }
        if record.receipt.status != "completed" || record.output.is_none() {
            reject(
                self,
                "the invocation did not produce a completed output".into(),
            );
            return None;
        }
        let Some(output_decl) = &record.output else {
            return None;
        };
        let output_digest = execution::canonical_json_bytes(output_decl)
            .ok()
            .map(|b| sha256_of(&b));
        if output_digest.as_deref() != record.output_sha256.as_deref()
            || record
                .receipt
                .outputs
                .iter()
                .find(|o| o.output_id == "output")
                .is_none_or(|o| o.state != "collected" || o.sha256 != record.output_sha256)
        {
            reject(
                self,
                "output digest does not cover the supplied output document".into(),
            );
            return None;
        }
        // Admit the observed output at the declared output type — the
        // receipt proves the process ran; admission decides the value is
        // usable at this signature.
        let output_ty = self.output_type(method, variables);
        let observed = self.admit_value(output_decl, &output_ty, &format!("{at}.output"))?;
        self.eval_events.push(execution::RuleOutcome {
            rule: "observe".into(),
            subject: at.clone(),
            state: "established".into(),
            detail: format!(
                "invocation digests verified; receipt {} re-hashes",
                record.receipt_sha256
            ),
        });
        Some(SemanticValue {
            ty: output_ty,
            unit: method
                .implementation
                .as_ref()
                .and_then(|i| i.produces.as_ref())
                .map(|p| p.unit.clone())
                .unwrap_or_default(),
            value: Some(observed),
            state: ValueState::Established,
            edges: slot_values
                .values()
                .flat_map(|v| v.edges.iter().cloned())
                .chain([format!("observation:{}", record.receipt_sha256)])
                .collect(),
            assumptions: Vec::new(),
        })
    }

    /// §10 [V-*] — the bounded comparator: an enclosure subject against an
    /// exact limit. `ge` passes when the lower bound meets the limit, fails
    /// when the upper bound falls below it; `le` symmetric. Anything
    /// between is `inconclusive` — never rounded to a nearer verdict.
    fn requirement_verdict(
        &mut self,
        requirement: &RequirementDecl,
        subject: &SemanticValue,
    ) -> RequirementReport {
        let at = format!("requirements[{}]", requirement.id);
        let Some(limit) = requirement
            .limit
            .value
            .as_deref()
            .and_then(|v| ExactNumber::from_canonical(v).ok())
        else {
            // Admission already refuses a malformed limit; unreachable in a
            // ready plan — stay honest anyway.
            return RequirementReport {
                state: "not_evaluated".into(),
                verdict: Some(VerdictReport {
                    status: "not_evaluated".into(),
                    rule: "not_evaluated.malformed".into(),
                    detail: "the requirement limit is not an exact number".into(),
                }),
                declared_value: subject.value_text(),
            };
        };
        let Some(numeric) = subject.value.clone() else {
            return RequirementReport {
                state: "not_evaluated".into(),
                verdict: Some(VerdictReport {
                    status: "not_evaluated".into(),
                    rule: "not_evaluated.missing_evidence".into(),
                    detail: self.unestablished_detail(&requirement.subject.reference),
                }),
                declared_value: None,
            };
        };
        let (lower, upper) = numeric.bounds();
        let cmp = |a: &ExactNumber, b: &ExactNumber| {
            a.checked_cmp(b).unwrap_or(std::cmp::Ordering::Greater)
        };
        let symbol = requirement.comparison.as_str();
        let (status, detail) = match symbol {
            "ge" => {
                if !cmp(&lower, &limit).is_lt() {
                    (
                        "pass",
                        format!(
                            "{} satisfies >= {} {}: {} >= {}",
                            render_subject(subject),
                            limit.canonical_rational(),
                            requirement.limit.unit,
                            lower.canonical_rational(),
                            limit.canonical_rational(),
                        ),
                    )
                } else if cmp(&upper, &limit).is_lt() {
                    (
                        "fail",
                        format!(
                            "upper bound {} < limit {}",
                            upper.canonical_rational(),
                            limit.canonical_rational(),
                        ),
                    )
                } else {
                    (
                        "inconclusive",
                        format!(
                            "{} straddles limit {}",
                            render_subject_no_unit(subject),
                            limit.canonical_rational(),
                        ),
                    )
                }
            }
            _ => {
                if !cmp(&upper, &limit).is_gt() {
                    (
                        "pass",
                        format!(
                            "{} satisfies <= {} {}: {} <= {}",
                            render_subject(subject),
                            limit.canonical_rational(),
                            requirement.limit.unit,
                            upper.canonical_rational(),
                            limit.canonical_rational(),
                        ),
                    )
                } else if cmp(&lower, &limit).is_gt() {
                    (
                        "fail",
                        format!(
                            "lower bound {} > limit {}",
                            lower.canonical_rational(),
                            limit.canonical_rational(),
                        ),
                    )
                } else {
                    (
                        "inconclusive",
                        format!(
                            "{} straddles limit {}",
                            render_subject_no_unit(subject),
                            limit.canonical_rational(),
                        ),
                    )
                }
            }
        };
        // Residual assumptions qualify the verdict — a pass conditional on
        // undischarged premises is reported conditional, not unconditional.
        let conditional = render_conditionals(&subject.assumptions);
        let detail = if subject.assumptions.is_empty() {
            detail
        } else if status == "pass" {
            format!("{detail}, conditional on {conditional}")
        } else {
            format!("{detail}; conditional on {conditional}")
        };
        self.eval_events.push(execution::RuleOutcome {
            rule: format!("bounded.{symbol}"),
            subject: at.clone(),
            state: status.into(),
            detail: detail.clone(),
        });
        RequirementReport {
            state: "evaluated".into(),
            verdict: Some(VerdictReport {
                status: status.into(),
                rule: format!("bounded.{symbol}"),
                detail,
            }),
            declared_value: subject.value_text(),
        }
    }

    /// The `missing_evidence` explanation: name the unavailable inputs and
    /// unestablished bindings in the subject's dependency cone, not just
    /// the subject.
    fn unestablished_detail(&self, subject: &str) -> String {
        let mut cone = BTreeSet::new();
        let mut frontier = vec![subject.to_string()];
        while let Some(name) = frontier.pop() {
            if cone.insert(name.clone())
                && let Some(operands) = self.deps.get(&name)
            {
                frontier.extend(operands.iter().cloned());
            }
        }
        let mut clauses = Vec::new();
        let mut named = BTreeSet::new();
        for input in &self.program.inputs {
            let unavailable = input
                .binding
                .as_ref()
                .is_some_and(|b| b.state == "unavailable");
            if unavailable && cone.contains(&input.id) {
                clauses.push(format!("input {} declared unavailable", input.id));
                named.insert(input.id.clone());
            }
        }
        for name in &cone {
            if name == subject || named.contains(name) {
                continue;
            }
            if self
                .env
                .get(name)
                .is_some_and(|v| v.state == ValueState::Unestablished)
            {
                clauses.push(format!("{name} has no established value"));
            }
        }
        if clauses.is_empty() {
            clauses.push(format!("{subject} has no established value"));
        }
        clauses.join("; ")
    }
}

/// Canonical identity of the plan body — the digest the observation set and
/// every receipt binds to.
fn plan_identity(plan: &execution::ExecutionPlanDocument) -> String {
    let mut body = plan.clone();
    body.plan_sha256 = None;
    execution::canonical_json_bytes(&body)
        .map(|b| sha256_of(&b))
        .unwrap_or_default()
}

/// The `{claim} {value} {unit}` rendering used in verdict details.
fn render_subject(subject: &SemanticValue) -> String {
    let base = render_subject_no_unit(subject);
    if subject.unit.is_empty() {
        base
    } else {
        format!("{base} {}", subject.unit)
    }
}

fn render_subject_no_unit(subject: &SemanticValue) -> String {
    match &subject.value {
        Some(NumericValue::Exact(n)) => {
            format!("{} {}", subject.ty.claim.as_str(), n.canonical_rational())
        }
        Some(NumericValue::Enclosure(e)) => format!(
            "{} [{}, {}]",
            subject.ty.claim.as_str(),
            e.lower.canonical_rational(),
            e.upper.canonical_rational()
        ),
        None => format!("{} <unestablished>", subject.ty.claim.as_str()),
    }
}

/// `proposition@({v1, v2})` — the conditional-qualification render; entity
/// values in key order, matching the expectation vocabulary.
fn render_conditionals(assumptions: &[ScopedProposition]) -> String {
    assumptions
        .iter()
        .map(|a| {
            if a.at.is_empty() {
                a.proposition.clone()
            } else {
                format!(
                    "{}@({})",
                    a.proposition,
                    a.at.values().cloned().collect::<Vec<_>>().join(", ")
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The plan document of a refused analysis: no invocations, the blocking
/// findings' codes as the refusal's explanation.
fn refused_plan(
    program_sha256: Option<String>,
    library_sha256: Option<String>,
    findings: &[LanguageFinding],
) -> execution::ExecutionPlanDocument {
    let mut doc = execution::ExecutionPlanDocument {
        schema_version: execution::EXECUTION_PLAN_SCHEMA_VERSION.into(),
        profile: LANGUAGE_PROFILE.into(),
        state: "refused".into(),
        context: execution::PlanContext {
            program_sha256: program_sha256.unwrap_or_default(),
            library_sha256: library_sha256.unwrap_or_default(),
            analysis_sha256: String::new(),
        },
        invocations: Vec::new(),
        refused_by: findings
            .iter()
            .filter(|f| f.blocking)
            .map(|f| format!("{}:{}", f.code, f.kind))
            .collect(),
        plan_sha256: None,
    };
    doc.plan_sha256 = Some(plan_identity(&doc));
    doc
}

/// The `plan` stage (spec §1): analyze and emit the execution plan binding
/// the analysis identity. Always returns a record — a refused analysis
/// lowers to a refused plan, never to a partial one.
pub fn execution_plan(
    program_bytes: &[u8],
    library_bytes: &[u8],
    options: &AnalysisOptions,
) -> execution::ExecutionPlanDocument {
    analyze_core(program_bytes, library_bytes, options, None, false).1
}

/// The `evaluate` stage (spec §1, §7): replay the analysis with supplied
/// observations bound to their application sites, then derive requirement
/// verdicts. An observation that fails the digest binding is foreign and
/// leaves the obligation open; an absent observation does the same — the
/// requirement ends `not_evaluated`, never silently `pass`.
pub fn evaluate_program(
    program_bytes: &[u8],
    library_bytes: &[u8],
    options: &AnalysisOptions,
    observations_bytes: &[u8],
) -> execution::LanguageEvaluation {
    // Pass one derives the plan identity the observations must answer;
    // pass two replays the derivation with the admitted material bound.
    let (baseline, plan, _) = analyze_core(program_bytes, library_bytes, options, None, false);
    let plan_sha256 = plan.plan_sha256.clone().unwrap_or_default();

    // Admit the observation document — strict schema, canonical bytes.
    let (observations_doc, observations_sha256, observation_findings) =
        match read_authoritative_json(observations_bytes) {
            Ok(canonical) => {
                let bytes = canonical_bytes_of(&canonical).unwrap_or_default();
                let parsed = serde_json::from_slice::<execution::ObservationsDocument>(&bytes);
                match parsed {
                    Ok(doc)
                        if doc.schema_version == execution::OBSERVATIONS_SCHEMA_VERSION
                            && doc.profile == LANGUAGE_PROFILE =>
                    {
                        (Some(doc), sha256_of(&bytes), Vec::new())
                    }
                    Ok(doc) => (
                        None,
                        sha256_of(&bytes),
                        vec![format!(
                            "schema `{}`/profile `{}` is not an observations document",
                            doc.schema_version, doc.profile
                        )],
                    ),
                    Err(error) => (None, sha256_of(&bytes), vec![error.to_string()]),
                }
            }
            Err(error) => (
                None,
                String::new(),
                vec![format!("observations document does not decode: {error}")],
            ),
        };

    // The document-level context gate: an observations set that claims
    // another plan is foreign in toto — every site stays unobserved.
    let context_ok = observations_doc
        .as_ref()
        .is_some_and(|doc| doc.plan_sha256 == plan_sha256)
        && plan.state == "ready";

    let mut site_map: BTreeMap<usize, execution::ObservationRecord> = BTreeMap::new();
    let mut outcomes: Vec<execution::ObservationOutcome> = Vec::new();
    if let Some(doc) = &observations_doc {
        for record in &doc.observations {
            let site = record
                .at
                .strip_prefix("body[")
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<usize>().ok());
            match site {
                Some(index) if context_ok => {
                    site_map.insert(index, record.clone());
                }
                _ => {
                    outcomes.push(execution::ObservationOutcome {
                        at: record.at.clone(),
                        bind: record.bind.clone(),
                        state: "rejected".into(),
                        detail: if context_ok {
                            "observation does not name a `body[{index}]` application site".into()
                        } else {
                            "the supplied observations do not belong to this plan".into()
                        },
                    });
                }
            }
        }
    }

    let material = if context_ok {
        Some(EvalMaterial {
            plan_sha256: plan_sha256.clone(),
            observations: site_map,
        })
    } else {
        None
    };
    let (evaluated, _, events) = analyze_core(
        program_bytes,
        library_bytes,
        options,
        material.as_ref(),
        true,
    );

    // A supplied record that failed the digest binding surfaces as a
    // rejected observation — the `observe` events carry the reason.
    for event in &events {
        if event.rule == "observe" && event.state == "rejected" {
            outcomes.push(execution::ObservationOutcome {
                at: event.subject.clone(),
                bind: String::new(),
                state: "rejected".into(),
                detail: event.detail.clone(),
            });
        }
    }

    // Requirements: the evaluated run's verdicts; fall back to the
    // baseline's pending rows when the evaluation itself was refused.
    let mut requirements = BTreeMap::new();
    for (id, report) in &evaluated.requirements {
        let verdict = report.verdict.clone().unwrap_or(VerdictReport {
            status: "not_evaluated".into(),
            rule: "not_evaluated.missing_evidence".into(),
            detail: "the requirement stayed pending — the observation that would resolve it never arrived".into(),
        });
        requirements.insert(
            id.clone(),
            execution::RequirementVerdict {
                status: verdict.status,
                rule: verdict.rule,
                detail: verdict.detail,
                subject_value: report.declared_value.clone(),
            },
        );
    }

    // Absent-observation outcomes: every planned invocation that no
    // supplied record answered.
    let supplied_sites: BTreeSet<&str> = observations_doc
        .iter()
        .flat_map(|d| d.observations.iter())
        .map(|o| o.at.as_str())
        .collect();
    for invocation in &plan.invocations {
        if !supplied_sites.contains(invocation.at.as_str()) {
            outcomes.push(execution::ObservationOutcome {
                at: invocation.at.clone(),
                bind: invocation.bind.clone(),
                state: "absent".into(),
                detail: "no observation supplied for this invocation — its obligations stay open"
                    .into(),
            });
        }
    }

    let context = execution::EvaluationContext {
        semantic_profile: LANGUAGE_PROFILE.into(),
        program_sha256: baseline
            .program
            .identity
            .semantic_sha256
            .clone()
            .unwrap_or_default(),
        library_sha256: baseline
            .library
            .identity
            .semantic_sha256
            .clone()
            .unwrap_or_default(),
        analysis_sha256: sha256_of(&execution::canonical_json_bytes(&baseline).unwrap_or_default()),
        plan_sha256,
        observations_sha256,
        lifecycle: options
            .lifecycle
            .iter()
            .map(|e| format!("{}={}", e.key, e.state))
            .collect(),
    };
    let context_sha256 = sha256_of(&execution::canonical_json_bytes(&context).unwrap_or_default());
    let mut evaluation = execution::LanguageEvaluation {
        schema_version: execution::EVALUATION_SCHEMA_VERSION.into(),
        profile: LANGUAGE_PROFILE.into(),
        context,
        context_sha256,
        obligations: evaluated
            .obligations
            .iter()
            .map(|o| execution::RuleOutcome {
                rule: o.kind.clone(),
                subject: o.id.clone(),
                state: o.state.clone(),
                detail: o.detail.clone(),
            })
            .collect(),
        observations: outcomes,
        requirements,
        document_findings: observation_findings,
        findings: evaluated.findings.clone(),
        applications: Vec::new(),
        evaluation_sha256: None,
    };
    evaluation.applications = events;
    evaluation.evaluation_sha256 = Some(evaluation_identity(&evaluation));
    evaluation
}

fn evaluation_identity(evaluation: &execution::LanguageEvaluation) -> String {
    let mut body = evaluation.clone();
    body.evaluation_sha256 = None;
    execution::canonical_json_bytes(&body)
        .map(|b| sha256_of(&b))
        .unwrap_or_default()
}

impl Analyzer<'_> {
    /// Requirement admission already ran in the static phase; here each
    /// requirement's subject is resolved to a report — `pending` under
    /// `analyze`, a verdict under `evaluate`.
    fn requirement_reports(&mut self) -> BTreeMap<String, RequirementReport> {
        let mut reports = BTreeMap::new();
        // `program` is a document reference — requirement decls borrow from
        // it, not from `self`, so verdict derivation can take `&mut self`.
        let program = self.program;
        for (_, requirement) in canonical_order(&program.requirements, &projection::REQUIREMENT) {
            let at = format!("requirements[{}]", requirement.id);
            if self.malformed_requirements.contains(&requirement.id) {
                reports.insert(
                    requirement.id.clone(),
                    RequirementReport {
                        state: "not_evaluated".into(),
                        verdict: Some(VerdictReport {
                            status: "not_evaluated".into(),
                            rule: "not_evaluated.malformed".into(),
                            detail: "the requirement itself failed admission".into(),
                        }),
                        declared_value: None,
                    },
                );
                continue;
            }
            let Some(subject) = self.env.get(&requirement.subject.reference).cloned() else {
                // The subject binding was never produced — an upstream
                // finding already explains it; the requirement is not
                // evaluable.
                reports.insert(
                    requirement.id.clone(),
                    RequirementReport {
                        state: "not_evaluated".into(),
                        verdict: Some(VerdictReport {
                            status: "not_evaluated".into(),
                            rule: "not_evaluated.missing_evidence".into(),
                            detail: "subject has no established value".into(),
                        }),
                        declared_value: None,
                    },
                );
                continue;
            };
            let not_evaluated = |rule: &str, detail: String| RequirementReport {
                state: "not_evaluated".into(),
                verdict: Some(VerdictReport {
                    status: "not_evaluated".into(),
                    rule: rule.to_string(),
                    detail,
                }),
                declared_value: subject.value_text(),
            };
            // Causal precedence: a poisoned binding is unestablished *because*
            // a poison rule fired — report the poison rule first. An
            // unestablished subject blocked only by an open obligation
            // reports missing evidence — the value never arrived to check.
            let block_rule = self.blocked.get(&requirement.subject.reference).cloned();
            if subject.state == ValueState::Unestablished {
                let rule = block_rule
                    .filter(|r| {
                        matches!(
                            r.as_str(),
                            "not_evaluated.obligation_refuted"
                                | "not_evaluated.precondition_refuted"
                                | "not_evaluated.lifecycle_refused"
                                | "not_evaluated.contradiction"
                        )
                    })
                    .unwrap_or_else(|| {
                        if subject.ty.claim == ClaimModel::Nominal {
                            "not_evaluated.unestablished".to_string()
                        } else {
                            "not_evaluated.missing_evidence".to_string()
                        }
                    });
                let detail = if rule == "not_evaluated.missing_evidence" {
                    self.unestablished_detail(&requirement.subject.reference)
                } else {
                    "subject has no established value".into()
                };
                reports.insert(requirement.id.clone(), not_evaluated(&rule, detail));
                continue;
            }
            // A generated obligation's refusal blocks verdicts on otherwise
            // usable subjects.
            if let Some(rule) = block_rule {
                reports.insert(
                    requirement.id.clone(),
                    not_evaluated(&rule, "a generated obligation blocks this subject".into()),
                );
                continue;
            }
            if subject.ty.quantity_kind != requirement.quantity_kind {
                self.program_finding(
                    "type_mismatch",
                    &at,
                    format!(
                        "requirement kind `{}` ≠ subject kind `{}`",
                        requirement.quantity_kind, subject.ty.quantity_kind
                    ),
                );
                reports.insert(
                    requirement.id.clone(),
                    not_evaluated(
                        "not_evaluated.type_mismatch",
                        "subject kind does not match the requirement".into(),
                    ),
                );
                continue;
            }
            // Coverage is two independent checks: scenario identity and scope
            // refinement (§7.B) — a missing scenario relation stays open.
            let subject_scenario = subject.ty.relations.get("scenario").cloned();
            if let Some(scenario) = &requirement.scenario
                && subject_scenario.as_deref() != Some(scenario.as_str())
            {
                self.program_finding(
                    "coverage",
                    &at,
                    format!("subject does not carry required scenario `{scenario}`"),
                );
                reports.insert(
                    requirement.id.clone(),
                    not_evaluated(
                        "not_evaluated.coverage",
                        "required scenario identity is not carried by the subject".into(),
                    ),
                );
                continue;
            }
            if let Some(required) = &requirement.scope {
                let covered = subject_scenario
                    .as_ref()
                    .and_then(|s| self.program.entities.scenarios.get(s))
                    .is_some_and(|s| scope_refines(&s.scope, required));
                if !covered {
                    self.program_finding(
                        "coverage",
                        &at,
                        format!("subject's scenario does not carry scope `{required}`"),
                    );
                    reports.insert(
                        requirement.id.clone(),
                        not_evaluated(
                            "not_evaluated.coverage",
                            "requirement scope is not covered by the subject's scenario".into(),
                        ),
                    );
                    continue;
                }
            }
            match subject.state {
                ValueState::Unestablished => unreachable!("handled above"),
                ValueState::Declared => {
                    // A declared value the observation will never satisfy the
                    // required claim — nominal outputs cannot be bounded.
                    if subject.ty.claim == ClaimModel::Nominal {
                        reports.insert(
                            requirement.id.clone(),
                            not_evaluated(
                                "not_evaluated.unestablished",
                                "subject is a nominal assertion; the requirement needs an established claim".into(),
                            ),
                        );
                    } else if self.evaluating {
                        // Declared under `evaluate` means the observation
                        // never bound — the runtime evidence is absent.
                        reports.insert(
                            requirement.id.clone(),
                            not_evaluated(
                                "not_evaluated.missing_evidence",
                                "the runtime observation that would establish the subject did not bind"
                                    .into(),
                            ),
                        );
                    } else {
                        reports.insert(
                            requirement.id.clone(),
                            RequirementReport {
                                state: "pending".into(),
                                verdict: None,
                                declared_value: subject.value_text(),
                            },
                        );
                    }
                }
                ValueState::Established => {
                    // The subject is usable and the requirement well-formed —
                    // pass/fail/inconclusive verdicts are `evaluate`'s output,
                    // not `analyze`'s: under plain analysis the requirement
                    // stays pending until an execution context discharges
                    // its observations.
                    if subject.ty.claim == ClaimModel::Nominal {
                        reports.insert(
                            requirement.id.clone(),
                            not_evaluated(
                                "not_evaluated.unestablished",
                                "subject claim cannot satisfy a bounded requirement".into(),
                            ),
                        );
                    } else if self.evaluating {
                        reports.insert(
                            requirement.id.clone(),
                            self.requirement_verdict(requirement, &subject),
                        );
                    } else {
                        reports.insert(
                            requirement.id.clone(),
                            RequirementReport {
                                state: "pending".into(),
                                verdict: None,
                                declared_value: subject.value_text(),
                            },
                        );
                    }
                }
            }
        }
        reports
    }
}

enum DomainCheck {
    Holds,
    Refuted,
    Unknown(String),
}

/// §2.1 scope lattice: `steady-state ≼ any`, `transient ≼ any`, `x ≼ x`,
/// `steady-state ⋠ transient`.
fn scope_refines(actual: &str, required: &str) -> bool {
    required == "any" || actual == required
}

/// Names occurring in an expression (operand positions).
fn collect_names(expression: &Expression, names: &mut BTreeSet<String>) {
    match expression {
        Expression::Name(name) => {
            names.insert(name.clone());
        }
        Expression::Call(_, args) => {
            for a in args {
                collect_names(a, names);
            }
        }
    }
}

/// The references a body step makes — `apply` slot arguments and `infer`
/// positional arguments, with their path positions.
fn step_references(step: &super::document::StepDecl) -> Vec<(String, &str)> {
    let mut out = Vec::new();
    if let Some(args) = &step.arguments {
        for (slot, r) in args {
            out.push((format!("arguments.{slot}"), r.reference.as_str()));
        }
    }
    if let Some(infer) = &step.infer {
        for (i, r) in infer.arguments.iter().enumerate() {
            out.push((format!("arguments[{i}]"), r.reference.as_str()));
        }
    }
    out
}

fn slot_relations(slot: &SlotDecl) -> Vec<(String, String)> {
    [
        ("geometry", &slot.geometry),
        ("scenario", &slot.scenario),
        ("material", &slot.material),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.as_ref().map(|e| (k.to_string(), e.clone())))
    .collect()
}

fn output_slot_relations(slot: &SlotDecl) -> Vec<String> {
    slot_relations(slot).into_iter().map(|(k, _)| k).collect()
}

fn merge_assumptions(
    declared: Vec<ScopedProposition>,
    inherited: impl Iterator<Item = ScopedProposition>,
) -> Vec<ScopedProposition> {
    let mut out = declared;
    for a in inherited {
        if !out.contains(&a) {
            out.push(a);
        }
    }
    out
}
