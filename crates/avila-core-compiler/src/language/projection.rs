//! Schema-directed semantic projection (spec §10.1).
//!
//! A document has two identities. `document_sha256` is the canonical digest of
//! the full bytes. `semantic_sha256` is the canonical digest of the document's
//! *projection*: annotation fields removed only at the positions the schema
//! declares as annotations, declared sets sorted, declared sequences — program
//! `body`, `infer`/`apply` positional `arguments`, tuple-like arrays — kept in
//! order, and user-defined map keys always preserved as semantic identifiers.
//!
//! The projection is defined only for admitted documents; callers hash after
//! admission, never to launder a malformed document into an identity.

use avila_core_kernel::CanonicalJsonValue;

/// The role of one position in the document tree.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Role {
    /// Semantic scalar or map whose keys are user-declared identifiers.
    Node,
    /// Annotation field — removed at declared positions.
    Annotation,
    /// Fixed-key record: only the declared fields exist; each has its role.
    Struct(&'static [(&'static str, Role)]),
    /// User-keyed map: every key is a semantic identifier; values share a role.
    Map(&'static Role),
    /// Declared set: canonical order does not matter.
    Set(&'static Role),
    /// Declared sequence: position is semantic.
    Seq(&'static Role),
}

pub(crate) const SCALAR: Role = Role::Node;

pub(crate) const RELATION_KEYS: Role = Role::Map(&SCALAR);

const PROVENANCE: Role = Role::Struct(&[
    ("kind", SCALAR),
    ("party", SCALAR),
    ("edge", SCALAR),
    ("digest", SCALAR),
    ("check", SCALAR),
    ("statement", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

pub(crate) const VALUE: Role = Role::Struct(&[
    ("kind", SCALAR),
    ("value", SCALAR),
    ("lower", SCALAR),
    ("upper", SCALAR),
    ("unit", SCALAR),
]);

pub(crate) const SLOT: Role = Role::Struct(&[
    ("quantity_kind", SCALAR),
    ("claim", SCALAR),
    ("geometry", SCALAR),
    ("scenario", SCALAR),
    ("material", SCALAR),
]);

pub(crate) const SCOPED_ASSERTION: Role =
    Role::Struct(&[("proposition", SCALAR), ("at", RELATION_KEYS)]);

pub(crate) const KIND_PRODUCT: Role = Role::Struct(&[
    ("lhs_kind", SCALAR),
    ("rhs_kind", SCALAR),
    ("result_kind", SCALAR),
    ("result_unit", SCALAR),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

const PROPOSITION_DECL: Role =
    Role::Struct(&[("params", Role::Set(&SCALAR)), ("gloss", Role::Annotation)]);

const QUANTITY_KIND_DECL: Role =
    Role::Struct(&[("canonical_unit", SCALAR), ("gloss", Role::Annotation)]);

pub(crate) const REQUIRES: Role = Role::Struct(&[
    ("kind", SCALAR),
    ("domain", SCALAR),
    ("within", SCALAR),
    ("subject", SCALAR),
    ("scope", SCALAR),
    ("over", Role::Set(&SCALAR)),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

pub(crate) const ENSURES: Role = Role::Struct(&[
    ("kind", SCALAR),
    ("expression", SCALAR),
    ("check", SCALAR),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

const PRODUCES: Role = Role::Struct(&[("quantity_kind", SCALAR), ("unit", SCALAR)]);

const IMPLEMENTATION: Role = Role::Struct(&[
    ("kind", SCALAR),
    ("executable", SCALAR),
    ("produces", PRODUCES),
    ("body", SCALAR),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

pub(crate) const METHOD: Role = Role::Struct(&[
    ("id", SCALAR),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
    ("variables", Role::Map(&SCALAR)),
    ("inputs", Role::Map(&SLOT)),
    ("output", SLOT),
    ("projects", Role::Set(&SCALAR)),
    ("requires", Role::Set(&REQUIRES)),
    ("ensures", Role::Set(&ENSURES)),
    ("assumes", Role::Set(&SCALAR)),
    ("effects", Role::Set(&SCALAR)),
    ("implementation", IMPLEMENTATION),
]);

const LIBRARY_HEADER: Role = Role::Struct(&[("name", SCALAR), ("revision", SCALAR)]);

const LIBRARY: Role = Role::Struct(&[
    ("schema_version", SCALAR),
    ("profile", SCALAR),
    ("library", LIBRARY_HEADER),
    ("title", Role::Annotation),
    ("description", Role::Annotation),
    ("quantity_kinds", Role::Map(&QUANTITY_KIND_DECL)),
    ("kind_products", Role::Set(&KIND_PRODUCT)),
    ("propositions", Role::Map(&PROPOSITION_DECL)),
    ("methods", Role::Set(&METHOD)),
]);

const GEOMETRY_ENTITY: Role = Role::Struct(&[
    ("source", PROVENANCE),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

const DOMAIN: Role = Role::Struct(&[
    ("quantity_kind", SCALAR),
    ("unit", SCALAR),
    ("lower", SCALAR),
    ("upper", SCALAR),
]);

const SCENARIO_ENTITY: Role = Role::Struct(&[
    ("scope", SCALAR),
    ("operating_domain", DOMAIN),
    ("source", PROVENANCE),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

const INTERVAL: Role = Role::Struct(&[("unit", SCALAR), ("lower", SCALAR), ("upper", SCALAR)]);

const MATERIAL_ENTITY: Role = Role::Struct(&[
    ("applicability", Role::Map(&INTERVAL)),
    ("source", PROVENANCE),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

const ENTITIES: Role = Role::Struct(&[
    ("geometries", Role::Map(&GEOMETRY_ENTITY)),
    ("scenarios", Role::Map(&SCENARIO_ENTITY)),
    ("materials", Role::Map(&MATERIAL_ENTITY)),
]);

const BINDING: Role = Role::Struct(&[
    ("state", SCALAR),
    ("value", VALUE),
    ("source", PROVENANCE),
    ("party", SCALAR),
    ("reason", Role::Annotation),
]);

pub(crate) const INPUT: Role = Role::Struct(&[
    ("id", SCALAR),
    ("type", SLOT),
    ("binding", BINDING),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

pub(crate) const ASSUMPTION: Role = Role::Struct(&[
    ("id", SCALAR),
    ("asserts", SCALAR),
    ("denies", SCALAR),
    ("at", RELATION_KEYS),
    ("source", PROVENANCE),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

const PREMISE_ARGUMENTS: Role = Role::Struct(&[("over", Role::Set(&SCALAR))]);

pub(crate) const PREMISE: Role = Role::Struct(&[
    ("id", SCALAR),
    ("proposition", SCALAR),
    ("arguments", PREMISE_ARGUMENTS),
    ("at", RELATION_KEYS),
    ("established_by", PROVENANCE),
    ("assumptions", Role::Set(&SCOPED_ASSERTION)),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

const ARG_REF: Role = Role::Struct(&[("ref", SCALAR)]);

const INFER: Role = Role::Struct(&[
    ("rule", SCALAR),
    // Positional operands: order is semantic.
    ("arguments", Role::Seq(&ARG_REF)),
]);

const IMPORT: Role = Role::Struct(&[
    ("type", SLOT),
    ("value", VALUE),
    ("assumptions", Role::Set(&SCOPED_ASSERTION)),
    ("source", PROVENANCE),
]);

pub(crate) const STEP: Role = Role::Struct(&[
    ("bind", SCALAR),
    ("apply", SCALAR),
    // Named arguments: a map whose keys are slot identifiers.
    ("arguments", Role::Map(&ARG_REF)),
    ("infer", INFER),
    ("import", IMPORT),
    ("hole", SLOT),
    ("goal", SLOT),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

pub(crate) const REQUIREMENT: Role = Role::Struct(&[
    ("id", SCALAR),
    ("subject", ARG_REF),
    ("comparison", SCALAR),
    ("quantity_kind", SCALAR),
    ("limit", VALUE),
    ("scope", SCALAR),
    ("scenario", SCALAR),
    ("label", Role::Annotation),
    ("note", Role::Annotation),
    ("reason", Role::Annotation),
]);

const PROGRAM_LIBRARY_PIN: Role = Role::Struct(&[
    ("name", SCALAR),
    ("revision", SCALAR),
    ("semantic_sha256", SCALAR),
]);

const PROGRAM: Role = Role::Struct(&[
    ("schema_version", SCALAR),
    ("profile", SCALAR),
    ("id", SCALAR),
    ("title", Role::Annotation),
    ("description", Role::Annotation),
    ("library", PROGRAM_LIBRARY_PIN),
    ("entities", ENTITIES),
    ("inputs", Role::Set(&INPUT)),
    ("assumptions", Role::Set(&ASSUMPTION)),
    ("premises", Role::Set(&PREMISE)),
    // Body order controls sequential reference resolution — a sequence.
    ("body", Role::Seq(&STEP)),
    ("requirements", Role::Set(&REQUIREMENT)),
]);

/// Which document shape a value projects under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageDocument {
    Library,
    Program,
}

impl LanguageDocument {
    fn role(self) -> &'static Role {
        match self {
            Self::Library => &LIBRARY,
            Self::Program => &PROGRAM,
        }
    }
}

/// A projection failure: the tree does not match the declared schema — which
/// means admission was skipped. Projection never repairs; it reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionError {
    pub detail: String,
}

fn project(
    value: &CanonicalJsonValue,
    role: &Role,
    path: &str,
) -> Result<CanonicalJsonValue, ProjectionError> {
    match (role, value) {
        (Role::Annotation, _) => unreachable!("annotation fields are dropped by the parent struct"),
        (Role::Node, _) => Ok(value.clone()),
        (Role::Struct(fields), CanonicalJsonValue::Object(entries)) => {
            let mut projected = Vec::with_capacity(fields.len());
            for (name, field_role) in *fields {
                let Some((_, field_value)) = entries.iter().find(|(key, _)| key == name) else {
                    continue;
                };
                if matches!(field_role, Role::Annotation) {
                    continue;
                }
                projected.push((
                    (*name).to_string(),
                    project(field_value, field_role, &format!("{path}/{name}"))?,
                ));
            }
            for (key, _) in entries {
                if !fields.iter().any(|(name, _)| name == key) {
                    return Err(ProjectionError {
                        detail: format!("undeclared field `{key}` at {path}"),
                    });
                }
            }
            Ok(CanonicalJsonValue::Object(projected))
        }
        (Role::Map(value_role), CanonicalJsonValue::Object(entries)) => {
            let mut projected = Vec::with_capacity(entries.len());
            for (key, entry_value) in entries {
                projected.push((
                    key.clone(),
                    project(entry_value, value_role, &format!("{path}/{key}"))?,
                ));
            }
            Ok(CanonicalJsonValue::Object(projected))
        }
        (Role::Set(element_role), CanonicalJsonValue::Array(values))
        | (Role::Seq(element_role), CanonicalJsonValue::Array(values)) => {
            let mut projected = Vec::with_capacity(values.len());
            for (index, element) in values.iter().enumerate() {
                projected.push(project(element, element_role, &format!("{path}/{index}"))?);
            }
            if matches!(role, Role::Set(_)) {
                // Canonical order for a declared set: by the canonical bytes
                // of each projected element — the same bytes that would be
                // hashed — so ordering does not depend on internal field
                // order.
                let sort_key = |element: &CanonicalJsonValue| {
                    serde_json::to_vec(element)
                        .ok()
                        .and_then(|bytes| avila_core_kernel::canonicalize_json(&bytes).ok())
                        .unwrap_or_default()
                };
                projected.sort_by_key(sort_key);
            }
            Ok(CanonicalJsonValue::Array(projected))
        }
        (role, value) => Err(ProjectionError {
            detail: format!(
                "shape mismatch at {path}: role {:?} cannot project {}",
                role_kind(role),
                value_kind(value)
            ),
        }),
    }
}

fn role_kind(role: &Role) -> &'static str {
    match role {
        Role::Node => "node",
        Role::Annotation => "annotation",
        Role::Struct(_) => "struct",
        Role::Map(_) => "map",
        Role::Set(_) => "set",
        Role::Seq(_) => "sequence",
    }
}

fn value_kind(value: &CanonicalJsonValue) -> &'static str {
    match value {
        CanonicalJsonValue::Bool(_) => "bool",
        CanonicalJsonValue::Integer(_) => "integer",
        CanonicalJsonValue::String(_) => "string",
        CanonicalJsonValue::Array(_) => "array",
        CanonicalJsonValue::Object(_) => "object",
    }
}

/// Project one element under its declared role — the analyzer sorts
/// declared sets by the bytes this produces.
pub(crate) fn project_element(
    value: &CanonicalJsonValue,
    role: &Role,
) -> Result<CanonicalJsonValue, ProjectionError> {
    project(value, role, "")
}

/// Projects a canonical document to its semantic body.
pub fn project_document(
    value: &CanonicalJsonValue,
    document: LanguageDocument,
) -> Result<CanonicalJsonValue, ProjectionError> {
    project(value, document.role(), "")
}
