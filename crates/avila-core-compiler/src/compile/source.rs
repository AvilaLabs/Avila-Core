//! Source layer: authoritative reading, shape validation, typed decoding, and
//! header checks.
//!
//! The layer reports in one pass wherever a typed document does not yet exist
//! to continue with: every value refusal from the authoritative reader, then
//! every shape violation against the embedded schema, and only then typed
//! decoding, which after a clean shape pass fails only for internal drift.

use avila_core_kernel::{
    CanonicalJsonValue, EXACT_NUMBER_DECODE_PREFIX, ExactNumber, SEMANTIC_PROFILE,
    diagnose_authoritative_json, read_authoritative_json,
};

use super::findings::{escape_pointer_token, owner_for};
use super::ir::DocumentIdentity;
use super::prefixed_sha256;
use super::schema::{SchemaDocument, validate_shape};
use super::values::compiler_repair;
use crate::diagnostic::{
    CORE_S1101, CORE_S1102, CoreDiagnostic, DiagnosticRepair, FindingClass, SourceLocation,
};
use crate::document::{
    CONTRACT_SCHEMA_VERSION, ContractSource, REGISTRY_SCHEMA_VERSION, RegistrySnapshot,
};

pub(super) const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn read_document<T: serde::de::DeserializeOwned>(
    document: &str,
    which: SchemaDocument,
    bytes: &[u8],
    identities: &mut Vec<DocumentIdentity>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<T> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            owner_for(document),
            SourceLocation::new(document, ""),
            format!(
                "document contains {} bytes; the compiler input limit is {MAX_DOCUMENT_BYTES}",
                bytes.len()
            ),
        ));
        return None;
    }
    let canonical_value = match read_authoritative_json(bytes) {
        Ok(value) => value,
        Err(_) => {
            report_every_refusal(document, which, bytes, findings);
            return None;
        }
    };
    let canonical = match serde_json::to_vec(&canonical_value) {
        Ok(canonical) => canonical,
        Err(error) => {
            findings.push(CoreDiagnostic::new(
                CORE_S1102,
                FindingClass::Invalid,
                owner_for(document),
                SourceLocation::new(document, ""),
                error.to_string(),
            ));
            return None;
        }
    };
    identities.push(DocumentIdentity {
        document: document.into(),
        sha256: prefixed_sha256(&canonical),
    });

    let mut shape = Vec::new();
    validate_shape(document, which, &canonical_value, &mut shape);
    if !shape.is_empty() {
        findings.extend(shape);
        return None;
    }

    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    match serde_path_to_error::deserialize::<_, T>(&mut deserializer) {
        Ok(value) => Some(value),
        Err(error) => {
            let detail = error.inner().to_string();
            let mut pointer = pointer_from_decode_path(error.path());
            let code = if let Some(field) = unknown_field_name(&detail) {
                let token = escape_pointer_token(field);
                if pointer.rsplit('/').next() != Some(token.as_str()) {
                    pointer.push('/');
                    pointer.push_str(&token);
                }
                CORE_S1101
            } else {
                CORE_S1102
            };
            let repair = canonical_number_repair(&canonical_value, &pointer, &detail);
            let mut diagnostic = CoreDiagnostic::new(
                code,
                FindingClass::Invalid,
                owner_for(document),
                SourceLocation::new(document, pointer),
                detail,
            );
            if let Some(repair) = repair {
                diagnostic = diagnostic.with_repair(repair);
            }
            findings.push(diagnostic);
            None
        }
    }
}

/// Reports every reader refusal in the document, then every shape violation
/// of the diagnostic tree that is not already explained by a refusal at or
/// above its pointer. Nothing reported here is authoritative.
fn report_every_refusal(
    document: &str,
    which: SchemaDocument,
    bytes: &[u8],
    findings: &mut Vec<CoreDiagnostic>,
) {
    let (tree, refusals) = diagnose_authoritative_json(bytes);
    let refused: Vec<String> = refusals
        .iter()
        .map(|refusal| refusal.pointer().unwrap_or_default().to_owned())
        .collect();
    for refusal in &refusals {
        findings.push(CoreDiagnostic::new(
            refusal.code(),
            FindingClass::Invalid,
            owner_for(document),
            SourceLocation::new(document, refusal.pointer().unwrap_or_default()),
            refusal.detail(),
        ));
    }
    let Some(tree) = tree else {
        return;
    };
    let mut shape = Vec::new();
    validate_shape(document, which, &tree, &mut shape);
    findings.extend(shape.into_iter().filter(|finding| {
        !refused.iter().any(|pointer| {
            finding.primary.pointer == *pointer
                || finding.primary.pointer.starts_with(&format!("{pointer}/"))
        })
    }));
}

/// Lowers the path at which typed decoding stopped to a JSON Pointer.
///
/// Enum-variant segments are not JSON keys and are omitted. Decoding inside an
/// internally tagged enum is buffered by serde, so a failure there points at
/// the enum value rather than the field inside it.
pub(super) fn pointer_from_decode_path(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;

    let mut pointer = String::new();
    for segment in path.iter() {
        match segment {
            Segment::Seq { index } => {
                pointer.push('/');
                pointer.push_str(&index.to_string());
            }
            Segment::Map { key } => {
                pointer.push('/');
                pointer.push_str(&escape_pointer_token(key));
            }
            Segment::Enum { .. } | Segment::Unknown => {}
        }
    }
    pointer
}

/// Extracts the field name from serde's stable `unknown field` message so the
/// finding can point at the offending key even when the decode path stopped at
/// the parent object.
pub(super) fn unknown_field_name(detail: &str) -> Option<&str> {
    let rest = detail.strip_prefix("unknown field `")?;
    let end = rest.find('`')?;
    Some(&rest[..end])
}

/// Recovers the mechanically safe canonical form for a number that failed
/// typed decoding, by re-reading the raw authored string at the pointer.
pub(super) fn canonical_number_repair(
    document: &CanonicalJsonValue,
    pointer: &str,
    detail: &str,
) -> Option<DiagnosticRepair> {
    if !detail.starts_with(EXACT_NUMBER_DECODE_PREFIX) {
        return None;
    }
    let CanonicalJsonValue::String(raw) = document.pointer(pointer)? else {
        return None;
    };
    ExactNumber::from_canonical(raw)
        .err()?
        .repair()
        .map(compiler_repair)
}

pub(super) fn validate_document_headers(
    contract: &ContractSource,
    registry: &RegistrySnapshot,
    findings: &mut Vec<CoreDiagnostic>,
) {
    check_header(
        "contract",
        &contract.schema_version,
        CONTRACT_SCHEMA_VERSION,
        &contract.semantic_profile,
        findings,
    );
    check_header(
        "registry",
        &registry.schema_version,
        REGISTRY_SCHEMA_VERSION,
        &registry.semantic_profile,
        findings,
    );
}

pub(super) fn check_header(
    document: &str,
    schema_version: &str,
    expected_schema: &str,
    semantic_profile: &str,
    findings: &mut Vec<CoreDiagnostic>,
) {
    if schema_version != expected_schema {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            owner_for(document),
            SourceLocation::new(document, "/schema_version"),
            format!("unsupported schema `{schema_version}`; expected `{expected_schema}`"),
        ));
    }
    if semantic_profile != SEMANTIC_PROFILE {
        findings.push(CoreDiagnostic::new(
            CORE_S1102,
            FindingClass::Invalid,
            owner_for(document),
            SourceLocation::new(document, "/semantic_profile"),
            format!(
                "unsupported semantic profile `{semantic_profile}`; expected `{SEMANTIC_PROFILE}`"
            ),
        ));
    }
}
