//! Loading the package's qualification records and checking each one against
//! the capability it names, before any envelope is evaluated against a run's
//! facts (ADR-0008).

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

use avila_core_compiler::QualificationRecord;
use avila_core_compiler::SourceLocation;
use avila_core_compiler::parse_qualification;
use avila_core_compiler::parse_revocation;
use avila_core_compiler::{CompiledContract, RegistrySnapshot};
use avila_core_evidence::VerifiedCasePackage;
use avila_core_evidence::signature;
use avila_core_kernel::{KindRegistry, Predicate};

use crate::case_run::FindingClass;
use crate::diagnostic::{CORE_T2701, CORE_X3404, CORE_X3405, RunStage};

use super::RunFinding;

/// A qualification record the package binds, already checked against the
/// capability it names.
pub struct BoundQualification {
    pub sha256: String,
    pub record: QualificationRecord,
}

/// What envelope evaluation needs: the bound records, the lifecycle facts
/// the bound set asserts over them, and the registry's quantity kinds.
pub struct Envelopes {
    pub records: Vec<BoundQualification>,
    /// Bound record digest → digest of the bound record superseding it.
    /// A record is dead when another bound record names its digest in
    /// `supersedes`; its assessment still attaches, marked, but no claim
    /// citing it can satisfy a bounded or enclosure requirement.
    pub superseded_by: BTreeMap<String, String>,
    /// Bound record digest → digest of the bound `qualification_revocation`
    /// document withdrawing it.
    pub revoked_by: BTreeMap<String, String>,
    pub kinds: KindRegistry,
}

/// The package's qualification evidence: the bound records, the lifecycle
/// facts the bound set asserts over them (supersession edges carried on the
/// superseding records, withdrawals carried on `qualification_revocation`
/// documents), and any findings the load produced.
pub struct LoadedQualifications {
    pub records: Vec<BoundQualification>,
    pub superseded_by: BTreeMap<String, String>,
    pub revoked_by: BTreeMap<String, String>,
    pub findings: Vec<RunFinding>,
}

/// Load the package's `qualification` documents and refuse any whose bound
/// executable is not the one the package binds under that capability id.
///
/// When the contract's execution policy names recognized issuers
/// (`recognized_qualification_owners`), a listed owner's record must also
/// carry a signature document over its bound bytes that verifies under the
/// declared key — an unsigned or unverifiable record is refused here, not
/// attached, with a `CORE-X3404` finding. A record whose owner is not listed
/// still loads: its assessment attaches to the step's claims as data, and the
/// campaign's recognition check refuses it as `CORE-A4601` evidence.
///
/// Lifecycle: a bound record is dead when another bound record names its
/// digest in `supersedes`, or a bound `qualification_revocation` names it.
/// A revocation against a recognized owner's record must itself verify
/// under the declared issuer key — an unverifiable one is ignored with a
/// `CORE-X3405` finding and the record stands. Without a listed owner a
/// revocation is package-asserted and applies unsigned: it can only deny
/// evidence, never manufacture acceptance.
pub(crate) fn load_qualifications(
    package: &VerifiedCasePackage,
    compiled: &CompiledContract,
    registry: &[u8],
    recognized_owners: &BTreeMap<String, String>,
) -> Result<LoadedQualifications, Box<dyn Error>> {
    let registry_snapshot: RegistrySnapshot = serde_json::from_slice(registry)?;
    let mut bound = Vec::new();
    let mut findings = Vec::new();
    for document in package
        .manifest()
        .documents
        .iter()
        .filter(|document| document.role == "qualification")
    {
        let bytes = package
            .document_by_id(&document.document_id)
            .ok_or_else(|| {
                format!(
                    "qualification document `{}` has no bytes",
                    document.document_id
                )
            })?;
        let record = parse_qualification(bytes)
            .map_err(|error| format!("document `{}`: {error}", document.document_id))?;
        let capability = package
            .manifest()
            .capabilities
            .iter()
            .find(|capability| capability.capability_id == record.capability.capability_id)
            .ok_or_else(|| {
                format!(
                    "qualification `{}` names capability `{}`, which the package does not bind",
                    record.qualification_id, record.capability.capability_id
                )
            })?;
        if capability.executable_sha256 != record.capability.executable_sha256 {
            return Err(format!(
                "qualification `{}` covers executable {} but the package binds {} as `{}`",
                record.qualification_id,
                record.capability.executable_sha256,
                capability.executable_sha256,
                capability.capability_id
            )
            .into());
        }
        // The pair the record covers must be exercised by the workflow —
        // unless the record is bound only as a supersession witness: a
        // record carrying `supersedes` may replace a record for a
        // capability this package no longer executes. It still must name a
        // bound capability implementation exactly, checked just above, and
        // it never applies to a step whose pair is not exercised.
        if record.supersedes.is_empty()
            && !package.manifest().executions.iter().any(|execution| {
                execution.adapter == record.adapter
                    && execution.capability_id == record.capability.capability_id
            })
        {
            return Err(format!(
                "qualification `{}` covers adapter `{}` under `{}`, but no execution uses that pair",
                record.qualification_id, record.adapter, record.capability.capability_id
            )
            .into());
        }
        // ADR-0025 CORE-T2701: an `input_attribute_in*` predicate may only
        // address an attribute the bound slot's role declares — the
        // refusal exists only where a declared vocabulary does, so a role
        // carrying no `attributes` map cannot refuse a name.
        let exercised_steps: BTreeSet<&str> = package
            .manifest()
            .executions
            .iter()
            .filter(|execution| {
                execution.adapter == record.adapter
                    && execution.capability_id == record.capability.capability_id
            })
            .map(|execution| execution.step_id.as_str())
            .collect();
        if let Some(reason) =
            check_scope_attributes(&record, compiled, &registry_snapshot, &exercised_steps)
        {
            findings.push(RunFinding::runtime(
                CORE_T2701,
                FindingClass::Inadmissible,
                RunStage::PackageIntegrity,
                "requester",
                SourceLocation::new(
                    "manifest",
                    format!("/documents/*/document_id={}", document.document_id),
                ),
                format!(
                    "qualification `{}` {reason} — the record is not applied",
                    record.qualification_id
                ),
            ));
            continue;
        }
        // Recognition: a listed owner's record stands only behind a
        // signature document over its bound bytes that verifies under the
        // issuer's declared key. An unlisted owner's record still loads —
        // its assessment attaches as data and the campaign refuses it as
        // `CORE-A4601` evidence — but a listed owner's unsigned or
        // unverifiable record is refused here.
        if let Some(declared_key) = recognized_owners.get(&record.owner)
            && let Some(reason) =
                check_issuer_signature(package, "qualification", document, declared_key)
        {
            findings.push(RunFinding::runtime(
                CORE_X3404,
                FindingClass::Inadmissible,
                RunStage::PackageIntegrity,
                "policy_owner",
                SourceLocation::new(
                    "manifest",
                    format!("/documents/*/document_id={}", document.document_id),
                ),
                format!(
                    "qualification `{}` names recognized issuer `{}` but {reason} — the record is not applied",
                    record.qualification_id, record.owner
                ),
            ));
            continue;
        }
        bound.push(BoundQualification {
            sha256: document.sha256.clone(),
            record,
        });
    }

    // Supersession: a bound record is dead where another bound record names
    // its digest in `supersedes`. The edge rides on the superseding record,
    // so it carries exactly that record's own authenticity — under issuer
    // recognition a listed owner's record was already signature-checked
    // above; without it the edge is package-asserted.
    let mut superseded_by = BTreeMap::new();
    for dead in &bound {
        if let Some(superseder) = bound
            .iter()
            .find(|candidate| candidate.record.supersedes.contains(&dead.sha256))
        {
            superseded_by.insert(dead.sha256.clone(), superseder.sha256.clone());
        }
    }

    // Revocation: a bound `qualification_revocation` naming a bound record's
    // digest withdraws it. Against a recognized owner's record the document
    // must verify under the declared issuer key — an unverifiable one is
    // ignored with a finding and the record stands. Without a listed owner
    // it is package-asserted and applies unsigned.
    let mut revoked_by = BTreeMap::new();
    for document in package
        .manifest()
        .documents
        .iter()
        .filter(|document| document.role == "qualification_revocation")
    {
        let bytes = package
            .document_by_id(&document.document_id)
            .ok_or_else(|| {
                format!(
                    "qualification revocation `{}` has no bytes",
                    document.document_id
                )
            })?;
        let revocation = parse_revocation(bytes)
            .map_err(|error| format!("document `{}`: {error}", document.document_id))?;
        let Some(target) = bound
            .iter()
            .find(|bound| bound.sha256 == revocation.record_sha256)
        else {
            continue;
        };
        if let Some(declared_key) = recognized_owners.get(&target.record.owner)
            && let Some(reason) =
                check_issuer_signature(package, "qualification_revocation", document, declared_key)
        {
            findings.push(RunFinding::runtime(
                CORE_X3405,
                FindingClass::Inadmissible,
                RunStage::PackageIntegrity,
                "policy_owner",
                SourceLocation::new(
                    "manifest",
                    format!("/documents/*/document_id={}", document.document_id),
                ),
                format!(
                    "revocation of qualification `{}` {reason} — the withdrawal is ignored and the record stands",
                    revocation.qualification_id
                ),
            ));
            continue;
        }
        revoked_by.insert(target.sha256.clone(), document.sha256.clone());
    }

    Ok(LoadedQualifications {
        records: bound,
        superseded_by,
        revoked_by,
        findings,
    })
}

/// Verify a bound document's signature under the issuer key the contract's
/// recognition policy declares for its owner. `Some(reason)` describes each
/// failure; `None` only when a signature document covers the document's
/// bound bytes and verifies under exactly that key.
fn check_issuer_signature(
    package: &VerifiedCasePackage,
    role: &str,
    document: &avila_core_evidence::PackageDocument,
    declared_key: &str,
) -> Option<String> {
    let Ok(declared_key_id) = signature::key_id_from_public_hex(declared_key) else {
        return Some("the declared issuer key is malformed".into());
    };
    let Some((_, signature_document)) =
        super::signing::find_signature_for(package, role, &document.document_id)
    else {
        return Some("carries no signature document over its bound bytes".into());
    };
    if signature_document.signed_document.sha256 != document.sha256 {
        return Some("its signature covers different bytes than the package binds".into());
    }
    if signature_document.key_id != declared_key_id {
        return Some(format!(
            "its signature is by `{}`, not the declared issuer key",
            signature_document.key_id
        ));
    }
    let Ok(digest) = signature::signed_target_digest(&signature_document) else {
        return Some("its signature's target digest is malformed".into());
    };
    match signature::verify_digest(declared_key, &digest, &signature_document.signature_hex) {
        Ok(true) => None,
        Ok(false) => Some("its signature does not verify under the declared issuer key".into()),
        Err(error) => Some(format!("its signature could not be verified — {error}")),
    }
}

/// ADR-0025 T2701's record-load check: collect every `input_attribute_in`/
/// `input_attribute_in_range` reference in the record's scope, resolve
/// each named slot through the compiled bindings to the bound contract
/// input's role, and refuse the record when the role's declared
/// vocabulary does not carry the named attribute.
///
/// Boundaries: a slot bound to a step output — not a contract input —
/// carries no declared-attribute channel, so it is not checkable here.
/// A role with an empty `attributes` map declares no vocabulary and
/// therefore cannot refuse a name (the ADR's positive-vocabulary rule).
/// An unparsable scope is not a vocabulary question — the evaluator
/// reports it structurally.
fn check_scope_attributes(
    record: &QualificationRecord,
    compiled: &CompiledContract,
    registry: &RegistrySnapshot,
    exercised_steps: &BTreeSet<&str>,
) -> Option<String> {
    let scope: Predicate = serde_json::from_value(record.scope.clone()).ok()?;
    let mut references = Vec::new();
    collect_attribute_references(&scope, &mut references);
    let inputs: BTreeMap<&str, &avila_core_compiler::VersionedRef> = compiled
        .inputs()
        .iter()
        .map(|input| (input.input_id.as_str(), &input.role))
        .collect();
    for (slot, attribute) in references {
        // Only the steps exercising this record's capability pair bind
        // the slot for this assessment — the same slot name elsewhere in
        // the workflow is a different channel.
        let mut bound_input = None;
        for step in compiled.workflow() {
            if !exercised_steps.contains(step.step_id.as_str()) {
                continue;
            }
            for binding in &step.bindings {
                if binding.input_slot == slot
                    && let avila_core_compiler::SourceRef::ContractInput { input_id } =
                        &binding.source
                {
                    bound_input = inputs.get(input_id.as_str());
                }
            }
        }
        let Some(role_ref) = bound_input.copied() else {
            continue;
        };
        let vocabulary = registry
            .roles
            .iter()
            .find(|role| &role.role == role_ref)
            .map(|role| &role.attributes);
        let Some(vocabulary) = vocabulary else {
            continue;
        };
        if !vocabulary.is_empty() && !vocabulary.contains_key(attribute.as_str()) {
            return Some(format!(
                "predicate addresses attribute `{attribute}` on slot `{slot}`, which the bound role `{}` does not declare",
                role_ref.label()
            ));
        }
    }
    None
}

fn collect_attribute_references(predicate: &Predicate, out: &mut Vec<(String, String)>) {
    match predicate {
        Predicate::InputAttributeIn(set) => {
            out.push((set.slot.clone(), set.attribute.clone()));
        }
        Predicate::InputAttributeInRange(range) => {
            out.push((range.slot.clone(), range.attribute.clone()));
        }
        Predicate::All(inner) | Predicate::Any(inner) => {
            for predicate in inner {
                collect_attribute_references(predicate, out);
            }
        }
        Predicate::Not(inner) => collect_attribute_references(inner, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// A one-step contract whose single input carries the declared
    /// attribute, and a registry whose role declares the vocabulary —
    /// compiled through `compile_documents` so the check sees the same
    /// `CompiledContract` the runner does.
    fn compiled_with_vocabulary() -> (CompiledContract, RegistrySnapshot) {
        let contract = json!({
            "schema_version": "avila.core/evidence-contract/v0.2-draft",
            "semantic_profile": "avila.core/semantic/0.2-draft",
            "contract_id": "test.scope.vocabulary",
            "revision": 1,
            "question": "q",
            "status": "draft",
            "inputs": [{
                "input_id": "candidate",
                "role": {"id": "test.candidate", "major": 1},
                "media_type": "application/json",
                "claim_model": {"model": "unquantified"},
                "attributes": {"chain_format": "canonical"}
            }],
            "workflow": [{
                "step_id": "check",
                "capability_type": {"id": "test.check", "major": 1},
                "bindings": [{
                    "input_slot": "candidate",
                    "source": {"source": "contract_input", "input_id": "candidate"}
                }]
            }],
            "requirements": [{
                "requirement_id": "R1",
                "statement": "x",
                "purpose": {"id": "test.purpose", "major": 1},
                "metric": {"source": "step_output", "step_id": "check", "output_slot": "remaining"},
                "comparison": "less_than_or_equal",
                "limit": {"kind": "test.count", "value": "0", "unit": "1"},
                "basis": {"kind": "bounded"}
            }]
        });
        let registry = json!({
            "schema_version": "avila.core/registry-snapshot/v0.2-draft",
            "semantic_profile": "avila.core/semantic/0.2-draft",
            "registry_id": "test.scope.registry",
            "revision": 1,
            "kinds": [{
                "kind_id": "test.count", "canonical_unit": "1",
                "unit_class": "test.count.units@1", "owner": "test",
                "units": [{"symbol": "1", "factor": "1"}]
            }],
            "purposes": [{
                "purpose": {"id": "test.purpose", "major": 1},
                "owner": "test", "description": "test only"
            }],
            "roles": [
                {
                    "role": {"id": "test.candidate", "major": 1}, "owner": "test",
                    "validator": "test/check@1", "accepted_media_types": ["application/json"],
                    "permitted_claim_models": [{"model": "unquantified"}],
                    "attributes": {
                        "chain_format": {
                            "required": true,
                            "value_type": {"type": "text", "allowed_values": ["canonical", "extended"]}
                        }
                    }
                },
                {
                    "role": {"id": "test.remaining", "major": 1}, "owner": "test",
                    "validator": "test/check@1", "quantity_kind": "test.count",
                    "unit_class": "test.count.units@1", "accepted_media_types": ["application/json"],
                    "permitted_claim_models": [{"model": "exact"}]
                }
            ],
            "capability_types": [{
                "capability_type": {"id": "test.check", "major": 1}, "owner": "test",
                "reproducibility": {"determinism": "deterministic"},
                "inputs": [{
                    "slot_id": "candidate", "role": {"id": "test.candidate", "major": 1},
                    "accepted_media_types": ["application/json"]
                }],
                "outputs": [{
                    "slot_id": "remaining", "role": {"id": "test.remaining", "major": 1},
                    "media_type": "application/json",
                    "permitted_claim_models": [{"model": "exact"}]
                }]
            }]
        });
        let registry_bytes = serde_json::to_vec(&registry).unwrap();
        let compiled = avila_core_compiler::compile_documents(
            &serde_json::to_vec(&contract).unwrap(),
            &registry_bytes,
        )
        .expect("the fixture compiles")
        .into_report()
        .compiled
        .expect("the fixture compiles");
        (compiled, serde_json::from_slice(&registry_bytes).unwrap())
    }

    fn record(scope: Value) -> QualificationRecord {
        serde_json::from_value(json!({
            "schema_version": "avila.core/qualification/v0.1-draft",
            "qualification_id": "q1",
            "revision": 1,
            "owner": "test.owner",
            "adapter": "test/check@1",
            "capability": {
                "capability_id": "test.check.impl",
                "executable_sha256": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "statement": "s",
            "scope": scope
        }))
        .unwrap()
    }

    #[test]
    fn a_predicate_addressing_an_undeclared_attribute_is_refused() {
        let (compiled, registry) = compiled_with_vocabulary();
        let steps: BTreeSet<&str> = ["check"].into_iter().collect();
        let reason = check_scope_attributes(
            &record(json!({
                "input_attribute_in": {
                    "slot": "candidate",
                    "attribute": "undeclared_name",
                    "values": ["x"]
                }
            })),
            &compiled,
            &registry,
            &steps,
        );
        assert!(
            reason
                .as_deref()
                .is_some_and(|message| message.contains("undeclared_name")),
            "expected a refusal naming the attribute, got {reason:?}"
        );
    }

    #[test]
    fn declared_attributes_pass_and_range_predicates_are_checked() {
        let (compiled, registry) = compiled_with_vocabulary();
        let steps: BTreeSet<&str> = ["check"].into_iter().collect();
        // Declared name — admitted.
        assert_eq!(
            check_scope_attributes(
                &record(json!({
                    "input_attribute_in": {
                        "slot": "candidate",
                        "attribute": "chain_format",
                        "values": ["canonical"]
                    }
                })),
                &compiled,
                &registry,
                &steps,
            ),
            None
        );
        // The range form is checked through `not` composition too.
        let reason = check_scope_attributes(
            &record(json!({
                "not": {
                    "input_attribute_in_range": {
                        "slot": "candidate",
                        "attribute": "undeclared_range",
                        "min": "0"
                    }
                }
            })),
            &compiled,
            &registry,
            &steps,
        );
        assert!(reason.is_some(), "an undeclared range attribute refuses");
    }

    #[test]
    fn a_role_without_a_vocabulary_cannot_refuse() {
        // The same scope over a role that declares no `attributes` map is
        // uncheckable — the positive-vocabulary rule means absence of a
        // vocabulary is not a refusal.
        let (compiled, mut registry) = compiled_with_vocabulary();
        registry.roles[0].attributes.clear();
        let steps: BTreeSet<&str> = ["check"].into_iter().collect();
        assert_eq!(
            check_scope_attributes(
                &record(json!({
                    "input_attribute_in": {
                        "slot": "candidate",
                        "attribute": "anything",
                        "values": ["x"]
                    }
                })),
                &compiled,
                &registry,
                &steps,
            ),
            None
        );
    }
}
