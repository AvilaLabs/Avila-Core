//! Qualification envelopes: a method owner's scoped statement of where a
//! capability's evidence may be trusted, evaluated over facts about the case.
//!
//! A qualification record binds one adapter and one exact executable and
//! carries a closed applicability predicate (the kernel's `Predicate`) over
//! facts the adapter extracts from verified inputs. The runner evaluates the
//! envelope before a step runs and attaches the assessment to every claim the
//! step produces; the campaign then refuses to let a claim from outside its
//! envelope, or of unknown position, establish a bounded requirement. Nothing
//! here makes a method correct: the envelope is what the owner claims to have
//! validated, and it is as good as the validation evidence it names.

use std::collections::BTreeMap;

use avila_core_kernel::{
    ApplicabilityContext, ApplicabilityEvaluator, KindRegistry, Predicate, TruthValue,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::compile::registry::RegistryIndex;
use crate::document::RegistrySnapshot;

pub const QUALIFICATION_SCHEMA_VERSION: &str = "avila.core/qualification/v0.1-draft";

/// A method owner's qualification of one capability implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationRecord {
    pub schema_version: String,
    pub qualification_id: String,
    pub revision: u64,
    pub owner: String,
    /// The adapter identifier this qualification covers.
    pub adapter: String,
    pub capability: QualifiedCapability,
    pub statement: String,
    /// The kernel applicability predicate, kept as authored for reporting.
    pub scope: Value,
    /// Quantity kinds for the quantity-valued facts the scope names.
    #[serde(default)]
    pub fact_kinds: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_evidence: Vec<ValidationEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifiedCapability {
    pub capability_id: String,
    pub executable_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationEvidence {
    pub description: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeState {
    Inside,
    Outside,
    Unknown,
}

/// One top-level term of the scope and how it evaluated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeTerm {
    pub predicate: String,
    pub result: TruthValue,
}

/// The envelope evaluated over one step's facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeAssessment {
    pub qualification_id: String,
    pub revision: u64,
    /// Identity of the qualification record's bytes.
    pub sha256: String,
    pub state: EnvelopeState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<EnvelopeTerm>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

/// What an output claim carries about its producer's envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimQualification {
    pub qualification_id: String,
    pub revision: u64,
    pub sha256: String,
    pub state: EnvelopeState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<EnvelopeTerm>,
}

impl ClaimQualification {
    pub fn failed_terms(&self) -> Vec<String> {
        self.terms
            .iter()
            .filter(|term| term.result != TruthValue::True)
            .map(|term| format!("{} -> {:?}", term.predicate, term.result))
            .collect()
    }
}

impl From<&EnvelopeAssessment> for ClaimQualification {
    fn from(assessment: &EnvelopeAssessment) -> Self {
        Self {
            qualification_id: assessment.qualification_id.clone(),
            revision: assessment.revision,
            sha256: assessment.sha256.clone(),
            state: assessment.state,
            terms: assessment.terms.clone(),
        }
    }
}

/// Parse and validate a qualification record from its bytes.
pub fn parse_qualification(bytes: &[u8]) -> Result<QualificationRecord, String> {
    let record: QualificationRecord =
        serde_json::from_slice(bytes).map_err(|error| format!("qualification: {error}"))?;
    if record.schema_version != QUALIFICATION_SCHEMA_VERSION {
        return Err(format!(
            "qualification schema `{}` is not `{QUALIFICATION_SCHEMA_VERSION}`",
            record.schema_version
        ));
    }
    for (field, value) in [
        ("qualification_id", &record.qualification_id),
        ("owner", &record.owner),
        ("adapter", &record.adapter),
        ("statement", &record.statement),
        ("capability.capability_id", &record.capability.capability_id),
        (
            "capability.executable_sha256",
            &record.capability.executable_sha256,
        ),
    ] {
        if value.trim().is_empty() {
            return Err(format!("qualification `{field}` must not be empty"));
        }
    }
    if record.revision == 0 {
        return Err("qualification revision must be at least 1".into());
    }
    serde_json::from_value::<Predicate>(record.scope.clone())
        .map_err(|error| format!("qualification scope is not a valid predicate: {error}"))?;
    Ok(record)
}

/// The quantity kinds of a registry snapshot, for scaling quantity facts.
pub fn registry_kinds(registry_bytes: &[u8]) -> Result<KindRegistry, String> {
    let registry: RegistrySnapshot =
        serde_json::from_slice(registry_bytes).map_err(|error| format!("registry: {error}"))?;
    let mut findings = Vec::new();
    let index = RegistryIndex::build(&registry, &mut findings);
    Ok(index.kinds)
}

/// Evaluate the record's scope over a context (the kernel's
/// `ApplicabilityContext` as JSON). Each top-level `all` term is also
/// evaluated on its own so the report names what failed.
pub fn evaluate_envelope(
    record: &QualificationRecord,
    record_sha256: &str,
    kinds: &KindRegistry,
    context: &Value,
) -> EnvelopeAssessment {
    let mut assessment = EnvelopeAssessment {
        qualification_id: record.qualification_id.clone(),
        revision: record.revision,
        sha256: record_sha256.into(),
        state: EnvelopeState::Unknown,
        terms: Vec::new(),
        issues: Vec::new(),
    };
    let context: ApplicabilityContext = match serde_json::from_value(context.clone()) {
        Ok(context) => context,
        Err(error) => {
            assessment
                .issues
                .push(format!("applicability context is malformed: {error}"));
            return assessment;
        }
    };
    let mut evaluator = ApplicabilityEvaluator::new(kinds);
    for (fact, kind) in &record.fact_kinds {
        if let Err(error) = evaluator.register_fact_kind(fact.clone(), kind.clone()) {
            assessment
                .issues
                .push(format!("fact kind `{fact}`: {}", error.detail()));
            return assessment;
        }
    }
    let terms: Vec<Value> = match record.scope.get("all").and_then(Value::as_array) {
        Some(items) if !items.is_empty() => items.clone(),
        _ => vec![record.scope.clone()],
    };
    for term in terms {
        let predicate = match serde_json::from_value::<Predicate>(term.clone()) {
            Ok(predicate) => predicate,
            Err(error) => {
                assessment
                    .issues
                    .push(format!("scope term is not a valid predicate: {error}"));
                return assessment;
            }
        };
        let result = match evaluator.evaluate(&predicate, &context) {
            Ok(result) => result,
            Err(error) => {
                assessment.issues.push(format!(
                    "scope term could not be evaluated: {}",
                    error.detail()
                ));
                TruthValue::Unknown
            }
        };
        assessment.terms.push(EnvelopeTerm {
            predicate: compact(&term),
            result,
        });
    }
    assessment.state = if assessment
        .terms
        .iter()
        .any(|term| term.result == TruthValue::False)
    {
        EnvelopeState::Outside
    } else if assessment
        .terms
        .iter()
        .all(|term| term.result == TruthValue::True)
    {
        EnvelopeState::Inside
    } else {
        EnvelopeState::Unknown
    };
    assessment
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(scope: Value) -> QualificationRecord {
        parse_qualification(
            &serde_json::to_vec(&json!({
                "schema_version": QUALIFICATION_SCHEMA_VERSION,
                "qualification_id": "test/q", "revision": 1, "owner": "test",
                "adapter": "test/adapter@1",
                "capability": { "capability_id": "stub", "executable_sha256": "sha256:aa" },
                "statement": "test envelope",
                "scope": scope,
                "fact_kinds": { "slab.total_thickness": "core.length" }
            }))
            .unwrap(),
        )
        .unwrap()
    }

    fn kinds() -> KindRegistry {
        let registry = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/cases/case-001-shield-search/registry.json"),
        )
        .unwrap();
        registry_kinds(&registry).unwrap()
    }

    fn scope() -> Value {
        json!({ "all": [
            { "fact": { "name": "slab.total_thickness", "op": "le", "value": { "value": "120", "unit": "cm" },
                        "source_requirement": { "class": "validated_input", "validator": "test/adapter@1" } } },
            { "input_attribute_in": { "slot": "candidate", "attribute": "layer.1.material", "values": ["polyethylene", "none"] } }
        ] })
    }

    fn context(thickness: &str, material: &str) -> Value {
        json!({
            "facts": { "slab.total_thickness": { "value": { "value": thickness, "unit": "cm" },
                        "source": { "class": "validated_input", "identity": "sha256:in", "validator": "test/adapter@1", "receipt": "plan:x" } } },
            "inputs": { "candidate": { "attributes": { "layer.1.material": material } } }
        })
    }

    #[test]
    fn inside_outside_and_unknown_are_reported_per_term() {
        let record = record(scope());
        let kinds = kinds();
        let inside = evaluate_envelope(&record, "sha256:q", &kinds, &context("90", "polyethylene"));
        assert_eq!(inside.state, EnvelopeState::Inside, "{inside:?}");
        assert_eq!(inside.terms.len(), 2);

        let outside =
            evaluate_envelope(&record, "sha256:q", &kinds, &context("1.5", "polyethylene"));
        assert_eq!(outside.state, EnvelopeState::Inside);
        let outside =
            evaluate_envelope(&record, "sha256:q", &kinds, &context("150", "polyethylene"));
        assert_eq!(outside.state, EnvelopeState::Outside);
        assert_eq!(outside.terms[0].result, TruthValue::False);
        let claim = ClaimQualification::from(&outside);
        assert_eq!(claim.failed_terms().len(), 1);

        let unknown = evaluate_envelope(&record, "sha256:q", &kinds, &json!({ "facts": {} }));
        assert_eq!(unknown.state, EnvelopeState::Unknown);
        assert!(
            unknown
                .terms
                .iter()
                .all(|term| term.result == TruthValue::Unknown)
        );

        // A fact from a weaker source than the term requires is unknown, not
        // accepted.
        let mut weak = context("90", "polyethylene");
        weak["facts"]["slab.total_thickness"]["source"]["class"] = json!("claimed");
        let weak = evaluate_envelope(&record, "sha256:q", &kinds, &weak);
        assert_eq!(weak.state, EnvelopeState::Unknown);
    }

    #[test]
    fn a_malformed_record_is_refused() {
        assert!(parse_qualification(b"{}").is_err());
        let bad_scope = json!({
            "schema_version": QUALIFICATION_SCHEMA_VERSION,
            "qualification_id": "q", "revision": 1, "owner": "o", "adapter": "a",
            "capability": { "capability_id": "c", "executable_sha256": "sha256:aa" },
            "statement": "s", "scope": { "nonsense": true }
        });
        assert!(
            parse_qualification(&serde_json::to_vec(&bad_scope).unwrap())
                .unwrap_err()
                .contains("valid predicate")
        );
    }
}
