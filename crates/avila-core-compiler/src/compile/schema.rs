//! Source-layer shape validation against the embedded `v0.2-draft` schemas.
//!
//! The two JSON Schemas under `schemas/` are the published description of the
//! documents the compiler accepts. This validator makes them executable so a
//! document's shape problems are all reported in one pass, at the exact
//! pointer, before typed decoding runs.
//!
//! The boundary is exact: this stage enforces what the typed decoder would
//! refuse, which is shape, value family, unknown and missing properties, tagged
//! variants, and canonical number form. Every rule the decoder would accept but
//! a semantic pass checks, such as cardinality, uniqueness, minimums, empty
//! identifiers, and identity formats, stays with that pass so the finding
//! carries its semantic code and accountable owner. The keyword test below
//! refuses any keyword in the schemas that is neither enforced here nor named
//! as semantic-layer owned.

use std::sync::OnceLock;

use avila_core_kernel::{CanonicalJsonValue, ExactNumber};
use serde_json::Value;

use super::findings::{escape_pointer_token, owner_for};
use super::values::compiler_repair;
use crate::diagnostic::{
    CORE_R3501, CORE_S1101, CORE_S1102, CoreDiagnostic, DiagnosticRepair, FindingClass,
    RepairApplicability, RepairEdit, SourceLocation,
};

const CONTRACT_SCHEMA: &str =
    include_str!("../../../../schemas/evidence-contract.v0.2-draft.schema.json");
const REGISTRY_SCHEMA: &str =
    include_str!("../../../../schemas/registry-snapshot.v0.2-draft.schema.json");
const CLAIMS_SCHEMA: &str =
    include_str!("../../../../schemas/evidence-claims.v0.2-draft.schema.json");

/// Regular expressions in the schemas are bound to an exact native check or
/// to a semantic pass, so the validator carries no regex engine. The exact
/// number pattern is the kernel's own canonical-form rule.
const EXACT_NUMBER_PATTERN: &str =
    r"^(?:(?:0|-?[1-9][0-9]*)(?:\.[0-9]*[1-9])?|-?[1-9][0-9]*/[1-9][0-9]*)$";
/// Checked by the review pass as `CORE-R3401`, not here.
#[cfg(test)]
const SHA256_PATTERN: &str = "^sha256:[a-f0-9]{64}$";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemaDocument {
    Contract,
    Registry,
    Claims,
}

pub(super) fn schema(document: SchemaDocument) -> &'static Value {
    static CONTRACT: OnceLock<Value> = OnceLock::new();
    static REGISTRY: OnceLock<Value> = OnceLock::new();
    static CLAIMS: OnceLock<Value> = OnceLock::new();
    match document {
        SchemaDocument::Claims => CLAIMS
            .get_or_init(|| serde_json::from_str(CLAIMS_SCHEMA).expect("embedded claims schema")),
        SchemaDocument::Contract => CONTRACT.get_or_init(|| {
            serde_json::from_str(CONTRACT_SCHEMA).expect("embedded contract schema")
        }),
        SchemaDocument::Registry => REGISTRY.get_or_init(|| {
            serde_json::from_str(REGISTRY_SCHEMA).expect("embedded registry schema")
        }),
    }
}

/// Reports every shape violation of `instance` against the named schema.
pub(super) fn validate_shape(
    document: &str,
    which: SchemaDocument,
    instance: &CanonicalJsonValue,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let root = schema(which);
    let mut validator = Validator {
        root,
        document,
        owner: owner_for(document),
        findings,
    };
    validator.check(root, instance, String::new());
}

/// Reports every violation of `instance` against an externally supplied
/// schema value: a registry role's declared `input_schema`, validated at the
/// package boundary rather than against one of the three embedded documents.
/// The schema must already have passed [`validate_role_schema_definition`];
/// this reuses the same `Validator` the embedded documents use, so its
/// findings carry the same codes (`CORE-S1101` for an unknown key,
/// `CORE-S1102` for every other shape or canonical-value violation) and the
/// caller is free to attribute them under a different code and owner.
pub fn validate_against_schema(
    schema: &Value,
    document: &str,
    owner: &'static str,
    instance: &CanonicalJsonValue,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let mut validator = Validator {
        root: schema,
        document,
        owner,
        findings,
    };
    validator.check(schema, instance, String::new());
}

/// Keywords a registry role's `input_schema` may use: exactly the restricted
/// subset [`Validator`] understands, plus `description` for documentation.
/// `$ref`/`$defs` (no cross-references are needed for a self-contained role
/// schema) and every keyword the embedded schemas route to a semantic pass
/// (`minItems`, `minimum`, ...) are outside it.
const ROLE_SCHEMA_KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "const",
    "oneOf",
    "pattern",
    "description",
];

/// Refuses a registry role's declared `input_schema` for any keyword outside
/// [`ROLE_SCHEMA_KEYWORDS`], an `additionalProperties` that is neither a
/// boolean nor a schema object, or a `pattern` other than the canonical
/// decimal rule. Findings are reported as `CORE-R3501`, owned by the
/// registry owner, at the exact pointer within the registry document.
/// Returns whether the definition is entirely within the supported subset.
pub(crate) fn validate_role_schema_definition(
    schema: &Value,
    pointer: &str,
    findings: &mut Vec<CoreDiagnostic>,
) -> bool {
    let mut ok = true;
    walk_role_schema(schema, pointer, &mut ok, findings);
    ok
}

fn walk_role_schema(
    node: &Value,
    pointer: &str,
    ok: &mut bool,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let Some(object) = node.as_object() else {
        role_schema_invalid(pointer, "a schema node must be a JSON object", ok, findings);
        return;
    };
    for (key, value) in object {
        let child = format!("{pointer}/{}", escape_pointer_token(key));
        if !ROLE_SCHEMA_KEYWORDS.contains(&key.as_str()) {
            role_schema_invalid(
                &child,
                format!(
                    "role schema keyword `{key}` is outside the supported subset ({})",
                    ROLE_SCHEMA_KEYWORDS.join(", ")
                ),
                ok,
                findings,
            );
            continue;
        }
        match key.as_str() {
            "pattern" if value.as_str() != Some(EXACT_NUMBER_PATTERN) => {
                role_schema_invalid(
                    &child,
                    "role schema `pattern` is supported only for the canonical decimal rule",
                    ok,
                    findings,
                );
            }
            "properties" => {
                for (name, child_schema) in value.as_object().into_iter().flatten() {
                    walk_role_schema(
                        child_schema,
                        &format!("{child}/{}", escape_pointer_token(name)),
                        ok,
                        findings,
                    );
                }
            }
            "items" => walk_role_schema(value, &child, ok, findings),
            "additionalProperties" => {
                if value.is_object() {
                    walk_role_schema(value, &child, ok, findings);
                } else if !value.is_boolean() {
                    role_schema_invalid(
                        &child,
                        "role schema `additionalProperties` must be a boolean or a schema object",
                        ok,
                        findings,
                    );
                }
            }
            "oneOf" => {
                if let Some(branches) = value.as_array() {
                    for (index, branch) in branches.iter().enumerate() {
                        walk_role_schema(branch, &format!("{child}/{index}"), ok, findings);
                    }
                } else {
                    role_schema_invalid(
                        &child,
                        "role schema `oneOf` must be an array",
                        ok,
                        findings,
                    );
                }
            }
            _ => {}
        }
    }
}

fn role_schema_invalid(
    pointer: &str,
    message: impl Into<String>,
    ok: &mut bool,
    findings: &mut Vec<CoreDiagnostic>,
) {
    *ok = false;
    findings.push(CoreDiagnostic::new(
        CORE_R3501,
        FindingClass::Invalid,
        "registry_owner",
        SourceLocation::new("registry", pointer),
        message,
    ));
}

struct Validator<'a> {
    root: &'a Value,
    document: &'a str,
    owner: &'static str,
    findings: &'a mut Vec<CoreDiagnostic>,
}

impl<'a> Validator<'a> {
    fn resolve(&self, mut node: &'a Value) -> &'a Value {
        while let Some(reference) = node.get("$ref").and_then(Value::as_str) {
            let name = reference
                .strip_prefix("#/$defs/")
                .expect("embedded schemas reference only local definitions");
            node = self
                .root
                .get("$defs")
                .and_then(|definitions| definitions.get(name))
                .expect("embedded schemas define every referenced name");
        }
        node
    }

    fn check(&mut self, node: &'a Value, instance: &CanonicalJsonValue, pointer: String) {
        let node = self.resolve(node);
        if let Some(branches) = node.get("oneOf").and_then(Value::as_array) {
            self.check_one_of(branches, instance, pointer);
            return;
        }
        if let Some(expected) = node.get("const")
            && !matches_scalar(instance, expected)
        {
            let repair = choice(&pointer, vec![render_plain(expected)]);
            self.invalid(
                pointer,
                format!("value must be {}", render(expected)),
                Some(repair),
            );
            return;
        }
        if let Some(options) = node.get("enum").and_then(Value::as_array)
            && !options
                .iter()
                .any(|option| matches_scalar(instance, option))
        {
            let candidates: Vec<_> = options.iter().map(render_plain).collect();
            let repair = choice(&pointer, candidates.clone());
            self.invalid(
                pointer,
                format!("value must be one of {}", candidates.join(", ")),
                Some(repair),
            );
            return;
        }
        if let Some(kind) = node.get("type").and_then(Value::as_str)
            && !type_matches(kind, instance)
        {
            self.invalid(
                pointer,
                format!("expected {kind}, found {}", describe(instance)),
                None,
            );
            return;
        }
        match instance {
            CanonicalJsonValue::String(text) => {
                if let Some(pattern) = node.get("pattern").and_then(Value::as_str) {
                    self.check_pattern(pattern, text, pointer);
                }
            }
            CanonicalJsonValue::Object(entries) => self.check_object(node, entries, pointer),
            CanonicalJsonValue::Array(values) => {
                if let Some(items) = node.get("items") {
                    for (index, value) in values.iter().enumerate() {
                        self.check(items, value, format!("{pointer}/{index}"));
                    }
                }
            }
            CanonicalJsonValue::Integer(_) | CanonicalJsonValue::Bool(_) => {}
        }
    }

    fn check_object(
        &mut self,
        node: &'a Value,
        entries: &[(String, CanonicalJsonValue)],
        pointer: String,
    ) {
        let properties = node.get("properties").and_then(Value::as_object);
        let additional = node.get("additionalProperties");
        if let Some(required) = node.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !entries.iter().any(|(key, _)| key == name) {
                    self.missing(
                        format!("{pointer}/{}", escape_pointer_token(name)),
                        format!("required property `{name}` is missing"),
                    );
                }
            }
        }
        for (key, value) in entries {
            let child = format!("{pointer}/{}", escape_pointer_token(key));
            if let Some(schema) = properties.and_then(|properties| properties.get(key)) {
                self.check(schema, value, child);
            } else if additional == Some(&Value::Bool(false)) {
                let mut repair = DiagnosticRepair::removal(
                    RepairApplicability::ConstrainedChoice,
                    format!("remove `{key}`"),
                    &child,
                );
                let present: Vec<&str> = entries.iter().map(|(name, _)| name.as_str()).collect();
                for known in properties
                    .into_iter()
                    .flat_map(|properties| properties.keys())
                    .filter(|known| !present.contains(&known.as_str()))
                    .filter(|known| levenshtein(known, key) <= 2)
                {
                    repair = repair.alternative(
                        format!("rename to `{known}`"),
                        vec![
                            RepairEdit::Remove {
                                path: child.clone(),
                            },
                            RepairEdit::Add {
                                path: format!("{pointer}/{}", escape_pointer_token(known)),
                                value: to_json(value),
                            },
                        ],
                    );
                }
                self.findings.push(
                    CoreDiagnostic::new(
                        CORE_S1101,
                        FindingClass::Invalid,
                        self.owner,
                        SourceLocation::new(self.document, child),
                        format!("unknown property `{key}`; the schema does not define it"),
                    )
                    .with_repair(repair),
                );
            } else if let Some(schema) = additional.filter(|value| value.is_object()) {
                self.check(schema, value, child);
            }
        }
    }

    /// Every `oneOf` in the embedded schemas is a tagged union: each branch
    /// fixes one shared property to a `const` or `enum`. The tag selects the
    /// branch, so a wrong tag names the admitted tags and a right tag reports
    /// that branch's own shape problems at their exact pointers.
    fn check_one_of(
        &mut self,
        branches: &'a [Value],
        instance: &CanonicalJsonValue,
        pointer: String,
    ) {
        let branches: Vec<&'a Value> = branches.iter().map(|branch| self.resolve(branch)).collect();
        let tag = discriminator(&branches).expect("embedded schemas use tagged unions only");
        let CanonicalJsonValue::Object(entries) = instance else {
            self.invalid(
                pointer,
                format!("expected object, found {}", describe(instance)),
                None,
            );
            return;
        };
        let tag_pointer = format!("{pointer}/{}", escape_pointer_token(tag));
        let Some((_, value)) = entries.iter().find(|(key, _)| key == tag) else {
            self.missing(
                tag_pointer,
                format!("required property `{tag}` is missing; it selects the variant"),
            );
            return;
        };
        let admitted: Vec<String> = branches
            .iter()
            .flat_map(|branch| tag_values(branch, tag))
            .map(render_plain)
            .collect();
        match branches.iter().find(|branch| {
            tag_values(branch, tag)
                .iter()
                .any(|option| render_plain(option) == render_plain_canonical(value))
        }) {
            Some(branch) => self.check(branch, instance, pointer),
            None => {
                let repair = choice(&tag_pointer, admitted.clone());
                self.invalid(
                    tag_pointer,
                    format!("`{tag}` must be one of {}", admitted.join(", ")),
                    Some(repair),
                );
            }
        }
    }

    /// Only the exact-number pattern is checked here, natively, because it is
    /// the kernel's own canonical-form rule. The `sha256` identity pattern is
    /// owned by the review pass, which reports it as `CORE-R3401` with the
    /// policy owner accountable.
    fn check_pattern(&mut self, pattern: &str, value: &str, pointer: String) {
        if pattern != EXACT_NUMBER_PATTERN {
            return;
        }
        if let Err(error) = ExactNumber::from_canonical(value) {
            let repair = error
                .repair()
                .map(|repair| compiler_repair(repair, &pointer));
            self.invalid(
                pointer,
                format!(
                    "value must be a canonical decimal or reduced rational: {}",
                    error.detail()
                ),
                repair,
            );
        }
    }

    fn invalid(
        &mut self,
        pointer: String,
        message: impl Into<String>,
        repair: Option<DiagnosticRepair>,
    ) {
        let mut diagnostic = CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            self.owner,
            SourceLocation::new(self.document, pointer),
            message,
        );
        if let Some(repair) = repair {
            diagnostic = diagnostic.with_repair(repair);
        }
        self.findings.push(diagnostic);
    }

    fn missing(&mut self, pointer: String, message: impl Into<String>) {
        self.findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Missing,
            self.owner,
            SourceLocation::new(self.document, pointer),
            message,
        ));
    }
}

/// The property that every branch of a tagged union fixes with `const` or `enum`.
fn discriminator<'a>(branches: &[&'a Value]) -> Option<&'a str> {
    let first = branches.first()?.get("properties")?.as_object()?;
    first.keys().map(String::as_str).find(|name| {
        branches
            .iter()
            .all(|branch| !tag_values(branch, name).is_empty())
    })
}

fn tag_values<'a>(branch: &'a Value, tag: &str) -> Vec<&'a Value> {
    let Some(property) = branch
        .get("properties")
        .and_then(|properties| properties.get(tag))
    else {
        return Vec::new();
    };
    if let Some(constant) = property.get("const") {
        return vec![constant];
    }
    property
        .get("enum")
        .and_then(Value::as_array)
        .map(|options| options.iter().collect())
        .unwrap_or_default()
}

fn choice(path: &str, candidates: Vec<String>) -> DiagnosticRepair {
    DiagnosticRepair::replacements(RepairApplicability::ConstrainedChoice, path, candidates)
}

/// Edit distance used to offer a rename for a misspelled property name.
fn levenshtein(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (row, left_char) in left.chars().enumerate() {
        let mut current = vec![row + 1];
        for (column, right_char) in right.iter().enumerate() {
            let substitution = previous[column] + usize::from(left_char != *right_char);
            current.push(
                substitution
                    .min(previous[column + 1] + 1)
                    .min(current[column] + 1),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

fn to_json(value: &CanonicalJsonValue) -> Value {
    match value {
        CanonicalJsonValue::Bool(value) => Value::Bool(*value),
        CanonicalJsonValue::Integer(value) => Value::from(*value),
        CanonicalJsonValue::String(value) => Value::String(value.clone()),
        CanonicalJsonValue::Array(values) => Value::Array(values.iter().map(to_json).collect()),
        CanonicalJsonValue::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), to_json(value)))
                .collect(),
        ),
    }
}

fn type_matches(kind: &str, instance: &CanonicalJsonValue) -> bool {
    matches!(
        (kind, instance),
        ("object", CanonicalJsonValue::Object(_))
            | ("array", CanonicalJsonValue::Array(_))
            | ("string", CanonicalJsonValue::String(_))
            | ("integer", CanonicalJsonValue::Integer(_))
            | ("boolean", CanonicalJsonValue::Bool(_))
    )
}

fn matches_scalar(instance: &CanonicalJsonValue, expected: &Value) -> bool {
    match (instance, expected) {
        (CanonicalJsonValue::String(actual), Value::String(expected)) => actual == expected,
        (CanonicalJsonValue::Bool(actual), Value::Bool(expected)) => actual == expected,
        (CanonicalJsonValue::Integer(actual), Value::Number(expected)) => {
            expected.as_i64() == Some(*actual)
        }
        _ => false,
    }
}

fn describe(instance: &CanonicalJsonValue) -> &'static str {
    match instance {
        CanonicalJsonValue::Object(_) => "object",
        CanonicalJsonValue::Array(_) => "array",
        CanonicalJsonValue::String(_) => "string",
        CanonicalJsonValue::Integer(_) => "integer",
        CanonicalJsonValue::Bool(_) => "boolean",
    }
}

fn render(value: &Value) -> String {
    match value {
        Value::String(text) => format!("`{text}`"),
        other => other.to_string(),
    }
}

fn render_plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn render_plain_canonical(value: &CanonicalJsonValue) -> String {
    match value {
        CanonicalJsonValue::String(text) => text.clone(),
        CanonicalJsonValue::Bool(value) => value.to_string(),
        CanonicalJsonValue::Integer(value) => value.to_string(),
        CanonicalJsonValue::Array(_) => "[array]".into(),
        CanonicalJsonValue::Object(_) => "{object}".into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use avila_core_kernel::read_authoritative_json;

    use super::*;

    /// Keywords the validator enforces: exactly what typed decoding would refuse.
    const ENFORCED_KEYWORDS: &[&str] = &[
        "$ref",
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "enum",
        "const",
        "oneOf",
        "pattern",
    ];
    /// Keywords that are documentation only.
    const ANNOTATION_KEYWORDS: &[&str] =
        &["$schema", "$id", "$defs", "title", "description", "default"];
    /// Keywords whose rule is owned by a semantic pass so the finding carries
    /// its semantic code rather than a generic shape code.
    const SEMANTIC_LAYER_KEYWORDS: &[&str] = &[
        "minItems",
        "maxItems",
        "uniqueItems",
        "minLength",
        "minimum",
        "not",
    ];

    fn keywords(node: &Value, out: &mut BTreeSet<String>) {
        let Some(object) = node.as_object() else {
            return;
        };
        for (key, value) in object {
            out.insert(key.clone());
            match key.as_str() {
                "properties" | "$defs" => {
                    for child in value.as_object().into_iter().flat_map(|map| map.values()) {
                        keywords(child, out);
                    }
                }
                "items" | "additionalProperties" | "not" => keywords(value, out),
                "oneOf" => {
                    for branch in value.as_array().into_iter().flatten() {
                        keywords(branch, out);
                    }
                }
                _ => {}
            }
        }
    }

    #[test]
    fn every_schema_keyword_is_enforced_annotation_or_semantic_layer() {
        let mut used = BTreeSet::new();
        keywords(schema(SchemaDocument::Contract), &mut used);
        keywords(schema(SchemaDocument::Registry), &mut used);
        keywords(schema(SchemaDocument::Claims), &mut used);
        let known: BTreeSet<_> = ENFORCED_KEYWORDS
            .iter()
            .chain(ANNOTATION_KEYWORDS)
            .chain(SEMANTIC_LAYER_KEYWORDS)
            .map(|keyword| (*keyword).to_owned())
            .collect();
        let unknown: Vec<_> = used.difference(&known).collect();
        assert!(unknown.is_empty(), "unhandled schema keywords: {unknown:?}");
    }

    #[test]
    fn every_schema_pattern_is_native_or_semantic_layer_owned() {
        fn patterns(node: &Value, out: &mut BTreeSet<String>) {
            match node {
                Value::Object(map) => {
                    if let Some(Value::String(pattern)) = map.get("pattern") {
                        out.insert(pattern.clone());
                    }
                    map.values().for_each(|value| patterns(value, out));
                }
                Value::Array(values) => values.iter().for_each(|value| patterns(value, out)),
                _ => {}
            }
        }
        let mut used = BTreeSet::new();
        patterns(schema(SchemaDocument::Contract), &mut used);
        patterns(schema(SchemaDocument::Registry), &mut used);
        patterns(schema(SchemaDocument::Claims), &mut used);
        let known: BTreeSet<String> = [EXACT_NUMBER_PATTERN, SHA256_PATTERN]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            used, known,
            "every schema pattern is native or semantic-layer owned"
        );
    }

    fn shape_findings(document: &str) -> Vec<(String, FindingClass, String, Vec<Vec<String>>)> {
        let value = read_authoritative_json(document.as_bytes()).unwrap();
        let mut findings = Vec::new();
        validate_shape("contract", SchemaDocument::Contract, &value, &mut findings);
        findings
            .into_iter()
            .map(|finding| {
                (
                    finding.code,
                    finding.class,
                    finding.primary.pointer,
                    finding
                        .repairs
                        .into_iter()
                        .map(|repair| repair.candidates)
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn shape_violations_are_all_reported_at_their_pointers() {
        let contract = r#"{
            "schema_version": "avila.core/evidence-contract/v0.2-draft",
            "semantic_profile": "avila.core/semantic/0.2-draft",
            "contract_id": "",
            "revision": 0,
            "status": "drafted",
            "question": "Does the shape check report everything at once?",
            "workflow": [{"step_id": "a", "capability_type": {"id": "t", "major": 1}, "bogus": 1}],
            "requirements": [{
                "requirement_id": "R",
                "purpose": {"id": "p", "major": 1},
                "metric": {"source": "nowhere"},
                "comparison": "less_than_or_equal",
                "limit": {"kind": "k", "value": "100.0", "unit": "u"},
                "basis": {"kind": "bounded"}
            }]
        }"#;
        let findings = shape_findings(contract);
        let summary: Vec<_> = findings
            .iter()
            .map(|(code, class, pointer, _)| (code.as_str(), *class, pointer.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "CORE-S1102",
                    FindingClass::Missing,
                    "/requirements/0/statement"
                ),
                (
                    "CORE-S1102",
                    FindingClass::Invalid,
                    "/requirements/0/limit/value"
                ),
                (
                    "CORE-S1102",
                    FindingClass::Invalid,
                    "/requirements/0/metric/source"
                ),
                ("CORE-S1102", FindingClass::Invalid, "/status"),
                ("CORE-S1101", FindingClass::Invalid, "/workflow/0/bogus"),
            ],
            "empty ids and zero revisions are semantic findings; shape findings follow canonical key order"
        );
        let repairs = |pointer: &str| {
            findings
                .iter()
                .find(|(_, _, candidate, _)| candidate == pointer)
                .map(|(_, _, _, repairs)| repairs.clone())
                .unwrap()
        };
        assert_eq!(
            repairs("/status"),
            vec![vec![
                "draft".to_owned(),
                "in_review".into(),
                "approved".into(),
                "retired".into()
            ]]
        );
        assert_eq!(
            repairs("/requirements/0/metric/source"),
            vec![vec!["contract_input".to_owned(), "step_output".into()]]
        );
        assert_eq!(
            repairs("/requirements/0/limit/value"),
            vec![vec!["100".to_owned()]]
        );
    }

    #[test]
    fn a_wrong_family_stops_descent_and_a_missing_tag_is_missing() {
        let findings = shape_findings(
            r#"{"schema_version": 1, "workflow": "steps", "requirements": [{"metric": {}}]}"#,
        );
        let summary: Vec<_> = findings
            .iter()
            .map(|(code, class, pointer, _)| (code.as_str(), *class, pointer.as_str()))
            .collect();
        assert!(summary.contains(&("CORE-S1102", FindingClass::Invalid, "/schema_version")));
        assert!(summary.contains(&("CORE-S1102", FindingClass::Invalid, "/workflow")));
        assert!(summary.contains(&(
            "CORE-S1102",
            FindingClass::Missing,
            "/requirements/0/metric/source"
        )));
        assert!(summary.contains(&("CORE-S1102", FindingClass::Missing, "/contract_id")));
    }
}
