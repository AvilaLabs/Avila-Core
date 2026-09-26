//! Typed decoding of the engineering-language document schemas.
//!
//! The two authoritative documents are `avila.core/method-library/v0.1-draft`
//! and `avila.core/language-program/v0.1-draft`. Every struct uses
//! `deny_unknown_fields`: an annotation-named field at a position the schema
//! does not declare fails admission instead of being silently dropped — the
//! strict decode *is* the §10.1 annotation-position rule.
//!
//! Annotation fields are admitted for the record and stripped by the
//! projection; the analyzer never reads them.

#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const LIBRARY_SCHEMA_VERSION: &str = "avila.core/method-library/v0.1-draft";
pub const PROGRAM_SCHEMA_VERSION: &str = "avila.core/language-program/v0.1-draft";
pub const ANALYSIS_SCHEMA_VERSION: &str = "avila.core/language-analysis/v0.1-draft";
pub const LANGUAGE_PROFILE: &str = "avila.core/language/0.1-draft";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryDocument {
    pub schema_version: String,
    pub profile: String,
    pub library: LibraryHeader,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub quantity_kinds: BTreeMap<String, QuantityKindDecl>,
    #[serde(default)]
    pub kind_products: Vec<KindProduct>,
    #[serde(default)]
    pub propositions: BTreeMap<String, PropositionDecl>,
    #[serde(default)]
    pub methods: Vec<MethodDecl>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryHeader {
    pub name: String,
    pub revision: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityKindDecl {
    #[serde(default)]
    pub canonical_unit: Option<String>,
    #[serde(default)]
    pub gloss: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KindProduct {
    pub lhs_kind: String,
    pub rhs_kind: String,
    pub result_kind: String,
    pub result_unit: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PropositionDecl {
    #[serde(default)]
    pub params: Vec<String>,
    #[serde(default)]
    pub gloss: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MethodDecl {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub inputs: BTreeMap<String, SlotDecl>,
    pub output: SlotDecl,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub requires: Vec<ObligationDecl>,
    #[serde(default)]
    pub ensures: Vec<EnsuresDecl>,
    #[serde(default)]
    pub assumes: Vec<String>,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub implementation: Option<ImplementationDecl>,
}

/// A slot or declared quantity type: fixed relation names, entity values.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SlotDecl {
    pub quantity_kind: String,
    pub claim: String,
    #[serde(default)]
    pub geometry: Option<String>,
    #[serde(default)]
    pub scenario: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObligationDecl {
    pub kind: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub within: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub over: Option<Vec<String>>,
    /// `scope_check` operand: the scope kind the subject's scenario must
    /// refine (§7.B).
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnsuresDecl {
    pub kind: String,
    pub expression: String,
    pub check: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImplementationDecl {
    pub kind: String,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub produces: Option<ProducesDecl>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProducesDecl {
    pub quantity_kind: String,
    pub unit: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramDocument {
    pub schema_version: String,
    pub profile: String,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub library: LibraryPin,
    #[serde(default)]
    pub entities: EntitiesDecl,
    #[serde(default)]
    pub inputs: Vec<InputDecl>,
    #[serde(default)]
    pub assumptions: Vec<AssumptionDecl>,
    #[serde(default)]
    pub premises: Vec<PremiseDecl>,
    #[serde(default)]
    pub body: Vec<StepDecl>,
    #[serde(default)]
    pub requirements: Vec<RequirementDecl>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryPin {
    pub name: String,
    pub revision: i64,
    pub semantic_sha256: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EntitiesDecl {
    #[serde(default)]
    pub geometries: BTreeMap<String, EntityDecl>,
    #[serde(default)]
    pub scenarios: BTreeMap<String, ScenarioDecl>,
    #[serde(default)]
    pub materials: BTreeMap<String, MaterialDecl>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EntityDecl {
    #[serde(default)]
    pub source: Option<ProvenanceDecl>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDecl {
    pub scope: String,
    #[serde(default)]
    pub operating_domain: Option<DomainDecl>,
    #[serde(default)]
    pub source: Option<ProvenanceDecl>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DomainDecl {
    pub quantity_kind: String,
    pub unit: String,
    pub lower: String,
    pub upper: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialDecl {
    #[serde(default)]
    pub applicability: BTreeMap<String, IntervalDecl>,
    #[serde(default)]
    pub source: Option<ProvenanceDecl>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntervalDecl {
    pub unit: String,
    pub lower: String,
    pub upper: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceDecl {
    pub kind: String,
    #[serde(default)]
    pub party: Option<String>,
    #[serde(default)]
    pub edge: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub statement: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputDecl {
    pub id: String,
    #[serde(rename = "type")]
    pub ty: SlotDecl,
    #[serde(default)]
    pub binding: Option<BindingDecl>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingDecl {
    pub state: String,
    #[serde(default)]
    pub value: Option<ValueDecl>,
    #[serde(default)]
    pub source: Option<ProvenanceDecl>,
    #[serde(default)]
    pub party: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValueDecl {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<String>,
    pub unit: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssumptionDecl {
    pub id: String,
    #[serde(default)]
    pub asserts: Option<String>,
    #[serde(default)]
    pub denies: Option<String>,
    #[serde(default)]
    pub at: BTreeMap<String, String>,
    #[serde(default)]
    pub source: Option<ProvenanceDecl>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PremiseDecl {
    pub id: String,
    pub proposition: String,
    #[serde(default)]
    pub arguments: Option<PremiseArguments>,
    #[serde(default)]
    pub at: BTreeMap<String, String>,
    #[serde(default)]
    pub established_by: Option<ProvenanceDecl>,
    #[serde(default)]
    pub assumptions: Vec<ScopedAssertionDecl>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PremiseArguments {
    #[serde(default)]
    pub over: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedAssertionDecl {
    pub proposition: String,
    #[serde(default)]
    pub at: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StepDecl {
    pub bind: String,
    #[serde(default)]
    pub apply: Option<String>,
    #[serde(default)]
    pub arguments: Option<BTreeMap<String, RefDecl>>,
    #[serde(default)]
    pub infer: Option<InferDecl>,
    #[serde(default)]
    pub import: Option<ImportDecl>,
    #[serde(default)]
    pub hole: Option<SlotDecl>,
    #[serde(default)]
    pub goal: Option<SlotDecl>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefDecl {
    #[serde(rename = "ref")]
    pub reference: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InferDecl {
    pub rule: String,
    #[serde(default)]
    pub arguments: Vec<RefDecl>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportDecl {
    #[serde(rename = "type")]
    pub ty: SlotDecl,
    pub value: ValueDecl,
    /// Present-vs-absent is semantic: `[]` asserts unconditional, absence is
    /// not a declaration at all.
    pub assumptions: Option<Vec<ScopedAssertionDecl>>,
    #[serde(default)]
    pub source: Option<ProvenanceDecl>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementDecl {
    pub id: String,
    pub subject: RefDecl,
    pub comparison: String,
    pub quantity_kind: String,
    pub limit: ValueDecl,
    /// Scope kind the subject's scenario must refine (`any` accepts every
    /// scope kind). Distinct from `scenario`, which names identity.
    #[serde(default)]
    pub scope: Option<String>,
    /// Scenario entity the subject must carry — identity, not scope kind.
    #[serde(default)]
    pub scenario: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}
