//! ADR-0022: instantiation binding checks (`CORE-A4801`..`CORE-A4805`) and
//! the `CORE-A4806` `template_superseded` notice.
//!
//! A contract naming `instantiated_from` is checked against the record it
//! names and the template that record pins: identity, parameter bindings,
//! policy-floor tightening, recorded eligibility, and validation cases. The
//! engine never instantiates — it verifies the recorded instantiation.

use std::collections::{BTreeMap, BTreeSet};

use avila_core_kernel::{ExactNumber, KindRegistry};
use serde_json::Value;

use super::prefixed_sha256;
use crate::diagnostic::{
    CORE_A4801, CORE_A4802, CORE_A4803, CORE_A4804, CORE_A4805, CORE_A4806, CoreDiagnostic,
    FindingClass,
};
use crate::document::{
    ContractInstantiation, ContractSource, ContractTemplate, EligibilityOutcome,
    EligibilityPredicate, InputField, RecordedEligibility,
};

pub use super::CompilationMaterial;

/// The document name findings on an instantiation record or template carry —
/// these are package documents, not compile inputs, so they are attributed
/// by role rather than folded into `contract` findings.
const RECORD_DOCUMENT: &str = "contract_instantiation";
const TEMPLATE_DOCUMENT: &str = "contract_template";

const INSTANTIATION_SCHEMA: &str =
    include_str!("../../../../schemas/contract-instantiation.v0.1-draft.schema.json");
const TEMPLATE_SCHEMA: &str =
    include_str!("../../../../schemas/contract-template.v0.1-draft.schema.json");

fn instantiation_schema() -> &'static Value {
    static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(INSTANTIATION_SCHEMA).expect("embedded schema"))
}

fn template_schema() -> &'static Value {
    static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(TEMPLATE_SCHEMA).expect("embedded schema"))
}

/// Canonical digest of a document's bytes — the same canonicalization every
/// document identity uses. Bytes that are not authoritative JSON have no
/// canonical digest and can never match a pin.
pub(super) fn canonical_sha256(bytes: &[u8]) -> Option<String> {
    let value = avila_core_kernel::read_authoritative_json(bytes).ok()?;
    serde_json::to_vec(&value)
        .ok()
        .map(|canonical| prefixed_sha256(&canonical))
}

pub(super) fn finding(
    code: &str,
    document: &str,
    pointer: String,
    message: String,
    findings: &mut Vec<CoreDiagnostic>,
) {
    findings.push(CoreDiagnostic::new(
        code,
        FindingClass::Invalid,
        "requester",
        crate::diagnostic::SourceLocation::new(document, pointer),
        message,
    ));
}

pub(super) fn check_instantiation(
    contract: &ContractSource,
    contract_sha256: &str,
    registry_bytes: &[u8],
    kinds: &KindRegistry,
    material: &CompilationMaterial<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let Some(from) = &contract.instantiated_from else {
        return;
    };

    // --- CORE-A4801: the record the contract names must be bound, parse, and
    // pin the contract being compiled. ---
    let record = match resolve_record(from, material, findings) {
        Some(record) => record,
        None => return,
    };
    if record.contract.contract_id != contract.contract_id
        || record.contract.revision != contract.revision
    {
        finding(
            CORE_A4801,
            RECORD_DOCUMENT,
            "/contract".into(),
            format!(
                "instantiation record pins contract `{}@{}`, but the compiled contract is `{}@{}`",
                record.contract.contract_id,
                record.contract.revision,
                contract.contract_id,
                contract.revision
            ),
            findings,
        );
    }
    if record.contract.sha256 != contract_sha256 {
        finding(
            CORE_A4801,
            RECORD_DOCUMENT,
            "/contract/sha256".into(),
            "instantiation record's contract digest does not match the contract being compiled"
                .into(),
            findings,
        );
    }

    // --- the template the record pins must be bound and self-consistent. ---
    let Some(template) = resolve_template(&record, material, findings) else {
        // The record is structurally fine but names a template the package
        // does not bind — no further checks are derivable.
        return;
    };

    // A newer revision of the same template among the bound documents is a
    // notice, never a gate: instances pin the revision they named.
    report_superseded(&record, &template, material, findings);

    check_parameters(&record, &template, kinds, findings);
    check_inputs(&record, &template, contract, findings);
    check_policy_floor(contract, &template, findings);
    check_eligibility(&record, &template, contract, findings);
    check_validation_cases(&template, registry_bytes, findings);
}

/// Find the bound record `instantiated_from` names and parse it. Absent or
/// ambiguous records are CORE-A4801 — the contract claims an origin that
/// cannot be shown.
fn resolve_record(
    from: &crate::document::InstantiationRef,
    material: &CompilationMaterial<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<ContractInstantiation> {
    let matched: Vec<&[u8]> = material
        .records
        .iter()
        .filter(|bytes| {
            serde_json::from_slice::<ContractInstantiation>(bytes)
                .is_ok_and(|record| record.instantiation_id == from.instantiation_id)
        })
        .copied()
        .collect();
    match matched.as_slice() {
        [bytes] => {
            // Structural shape first — the record is a package document and
            // carries the same S1101/S1102 shape discipline every document does.
            let canonical = avila_core_kernel::read_authoritative_json(bytes)
                .expect("a matched digest implies authoritative JSON");
            let mut shape = Vec::new();
            super::schema::validate_against_schema(
                instantiation_schema(),
                RECORD_DOCUMENT,
                "requester",
                &canonical,
                &mut shape,
            );
            if !shape.is_empty() {
                findings.extend(shape);
                return None;
            }
            match serde_json::from_slice::<ContractInstantiation>(bytes) {
                Ok(record) => Some(record),
                Err(error) => {
                    finding(
                        CORE_A4801,
                        RECORD_DOCUMENT,
                        String::new(),
                        format!("the pinned instantiation record does not parse: {error}"),
                        findings,
                    );
                    None
                }
            }
        }
        [] => {
            finding(
                CORE_A4801,
                "contract",
                "/instantiated_from".into(),
                format!(
                    "contract names instantiation `{}`, which no bound package document carries",
                    from.instantiation_id
                ),
                findings,
            );
            None
        }
        _ => {
            finding(
                CORE_A4801,
                "contract",
                "/instantiated_from".into(),
                format!(
                    "contract's `instantiated_from` resolves to {} bound records — one instantiation, one record",
                    matched.len()
                ),
                findings,
            );
            None
        }
    }
}

/// Resolve the record's template pin to a bound, parseable template whose
/// declared identity matches the pin.
fn resolve_template<'a>(
    record: &ContractInstantiation,
    material: &CompilationMaterial<'a>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<ContractTemplate> {
    let pin = &record.template;
    let mut candidates = material
        .templates
        .iter()
        .filter(|bytes| canonical_sha256(bytes).as_deref() == Some(pin.sha256.as_str()));
    let Some(bytes) = candidates.next() else {
        finding(
            CORE_A4801,
            RECORD_DOCUMENT,
            "/template/sha256".into(),
            format!(
                "instantiation record pins template `{}` revision {} at digest `{}`, which no bound package document carries",
                pin.template_id, pin.template_revision, pin.sha256
            ),
            findings,
        );
        return None;
    };
    let canonical = avila_core_kernel::read_authoritative_json(bytes)
        .expect("a matched digest implies authoritative JSON");
    let mut shape = Vec::new();
    super::schema::validate_against_schema(
        template_schema(),
        TEMPLATE_DOCUMENT,
        "requester",
        &canonical,
        &mut shape,
    );
    if !shape.is_empty() {
        findings.extend(shape);
        return None;
    }
    let template = match serde_json::from_slice::<ContractTemplate>(bytes) {
        Ok(template) => template,
        Err(error) => {
            finding(
                CORE_A4801,
                TEMPLATE_DOCUMENT,
                String::new(),
                format!("the pinned contract template does not parse: {error}"),
                findings,
            );
            return None;
        }
    };
    if template.template_id != pin.template_id
        || template.template_revision != pin.template_revision
    {
        finding(
            CORE_A4801,
            TEMPLATE_DOCUMENT,
            String::new(),
            format!(
                "instantiation record pins template `{}` revision {}, but the pinned document declares `{}` revision {}",
                pin.template_id,
                pin.template_revision,
                template.template_id,
                template.template_revision
            ),
            findings,
        );
    }
    Some(template)
}

/// CORE-A4806: a bound template document carrying the same `template_id` at
/// a higher revision is drift worth seeing — the pin is immutable, the
/// notice is information.
fn report_superseded(
    record: &ContractInstantiation,
    template: &ContractTemplate,
    material: &CompilationMaterial<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let newer = material
        .templates
        .iter()
        .filter_map(|bytes| serde_json::from_slice::<ContractTemplate>(bytes).ok())
        .filter(|candidate| {
            candidate.template_id == record.template.template_id
                && candidate.template_revision > template.template_revision
        })
        .map(|candidate| candidate.template_revision)
        .max();
    if let Some(revision) = newer {
        findings.push(CoreDiagnostic::new(
            CORE_A4806,
            FindingClass::Notice,
            "requester",
            crate::diagnostic::SourceLocation::new(RECORD_DOCUMENT, "/template"),
            format!(
                "template `{}` has a bound revision {} newer than the revision {} this instance pins — the pin is immutable and this is drift information, not invalidation",
                template.template_id, revision, template.template_revision
            ),
        ));
    }
}

/// CORE-A4802: the record's parameter bindings must cover every required
/// declaration, name only declared parameters, and satisfy each declared
/// domain; every `{"ref": ...}` in the template's workflow/requirements must
/// resolve to a declared parameter the record bound.
fn check_parameters(
    record: &ContractInstantiation,
    template: &ContractTemplate,
    kinds: &KindRegistry,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let declared: BTreeMap<&str, &crate::document::ParameterDefinition> = template
        .parameters
        .iter()
        .map(|p| (p.parameter_id.as_str(), p))
        .collect();
    for declaration in &template.parameters {
        if declaration.required && !record.parameters.contains_key(&declaration.parameter_id) {
            finding(
                CORE_A4802,
                RECORD_DOCUMENT,
                format!("/parameters/{}", declaration.parameter_id),
                format!(
                    "template parameter `{}` is required but the instance binds no value",
                    declaration.parameter_id
                ),
                findings,
            );
        }
    }
    for (name, value) in &record.parameters {
        let Some(declaration) = declared.get(name.as_str()) else {
            finding(
                CORE_A4802,
                RECORD_DOCUMENT,
                format!("/parameters/{name}"),
                format!("instance binds `{name}`, which the template does not declare"),
                findings,
            );
            continue;
        };
        if let Some(reason) =
            super::values::attribute_domain_error(&declaration.value_type, value, kinds)
        {
            finding(
                CORE_A4802,
                RECORD_DOCUMENT,
                format!("/parameters/{name}"),
                format!("bound parameter `{name}` {reason}"),
                findings,
            );
        }
    }
    // Every `{"ref": p}` in the template must resolve to a declared,
    // bound parameter.
    for (where_is, ref_name) in template_parameter_refs(template) {
        match declared.get(ref_name.as_str()) {
            None => finding(
                CORE_A4802,
                TEMPLATE_DOCUMENT,
                where_is,
                format!("template references `{ref_name}`, which it does not declare"),
                findings,
            ),
            Some(_) if !record.parameters.contains_key(&ref_name) => finding(
                CORE_A4802,
                RECORD_DOCUMENT,
                where_is,
                format!(
                    "template references `{ref_name}`, which the instance record does not bind"
                ),
                findings,
            ),
            _ => {}
        }
    }
}

/// Walk the template's workflow/requirements JSON and collect every
/// `{"ref": "<name>"}` object with the pointer it sits at.
fn template_parameter_refs(template: &ContractTemplate) -> Vec<(String, String)> {
    fn walk(value: &Value, pointer: &str, out: &mut Vec<(String, String)>) {
        match value {
            Value::Object(map) => {
                if map.len() == 1
                    && let Some(Value::String(name)) = map.get("ref")
                {
                    out.push((pointer.to_owned(), name.clone()));
                    return;
                }
                for (key, inner) in map {
                    walk(inner, &format!("{pointer}/{}", key), out);
                }
            }
            Value::Array(items) => {
                for (index, inner) in items.iter().enumerate() {
                    walk(inner, &format!("{pointer}/{index}"), out);
                }
            }
            _ => {}
        }
    }
    let mut refs = Vec::new();
    for (index, step) in template.workflow.iter().enumerate() {
        walk(step, &format!("/workflow/{index}"), &mut refs);
    }
    for (index, requirement) in template.requirements.iter().enumerate() {
        walk(requirement, &format!("/requirements/{index}"), &mut refs);
    }
    refs
}

/// CORE-A4803: the instance's `execution_policy` must tighten — never
/// loosen — the template's `policy_floor`, field by field. The order is
/// mechanical and total: every field's instance value is compared to the
/// floor's, and anything not provably at-least-as-strict fails.
fn check_policy_floor(
    contract: &ContractSource,
    template: &ContractTemplate,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let floor = &template.policy_floor;
    let instance = &contract.execution_policy;
    let mut loosened = |field: &str, detail: String| {
        finding(
            CORE_A4803,
            "contract",
            format!("/execution_policy/{field}"),
            detail,
            findings,
        );
    };

    // Restrictive flags — turning on tightens. A floor rule the instance
    // drops is a loosening.
    for (field, floor_on, instance_on) in [
        (
            "require_qualification",
            floor.require_qualification,
            instance.require_qualification,
        ),
        (
            "require_signatures",
            floor.require_signatures,
            instance.require_signatures,
        ),
        (
            "require_provider_independence",
            floor.require_provider_independence,
            instance.require_provider_independence,
        ),
        (
            "require_diverse_implementations",
            floor.require_diverse_implementations,
            instance.require_diverse_implementations,
        ),
        (
            "forbid_self_preference",
            floor.forbid_self_preference,
            instance.forbid_self_preference,
        ),
    ] {
        if floor_on && !instance_on {
            loosened(
                field,
                format!(
                    "the template's policy floor requires `{field}`; the instance declares it off — weakening a floor rule is a loosening"
                ),
            );
        }
    }
    // `permit_nominal_basis` runs the other way: permitting the nominal
    // basis is the looser side. A floor that does not permit it forbids an
    // instance that does.
    if !floor.permit_nominal_basis && instance.permit_nominal_basis {
        loosened(
            "permit_nominal_basis",
            "the template's policy floor does not permit the nominal basis; the instance permits it — a permissive rule added is a loosening".into(),
        );
    }

    // `deny_providers` is a deny list — the instance's set must contain the
    // floor's (denying more tightens).
    let floor_deny: BTreeSet<&str> = floor.deny_providers.iter().map(String::as_str).collect();
    let instance_deny: BTreeSet<&str> =
        instance.deny_providers.iter().map(String::as_str).collect();
    if !floor_deny.is_subset(&instance_deny) {
        loosened(
            "deny_providers",
            format!(
                "the template's policy floor denies providers {floor_deny:?}; the instance denies {instance_deny:?} — a missing denial is a loosening"
            ),
        );
    }

    // `allow_providers` is a closed allow set — an absent floor means no
    // constraint; a declared floor admits only subsets of itself.
    if !floor.allow_providers.is_empty() {
        let floor_allow: BTreeSet<&str> =
            floor.allow_providers.iter().map(String::as_str).collect();
        let instance_allow: BTreeSet<&str> = instance
            .allow_providers
            .iter()
            .map(String::as_str)
            .collect();
        if !instance_allow.is_subset(&floor_allow) {
            loosened(
                "allow_providers",
                format!(
                    "the template's policy floor allows providers {floor_allow:?}; the instance allows {instance_allow:?} — a provider outside the floor's set is a loosening"
                ),
            );
        }
    }

    // `permitted_nondeterministic_roles` is a permission list — absent in the
    // floor means *none* permitted (the strictest posture), so any instance
    // addition loosens.
    let floor_permitted: BTreeSet<_> = floor.permitted_nondeterministic_roles.iter().collect();
    let instance_permitted: BTreeSet<_> =
        instance.permitted_nondeterministic_roles.iter().collect();
    if !instance_permitted.is_subset(&floor_permitted) {
        loosened(
            "permitted_nondeterministic_roles",
            "the template's policy floor permits no nondeterministic roles the instance adds"
                .into(),
        );
    }

    // `recognized_qualification_owners`: an absent floor is no recognition
    // constraint; a declared floor admits only subsets of its own
    // owner→key pairs — a new owner or a rotated key is a loosening.
    if !floor.recognized_qualification_owners.is_empty() {
        for (owner, key) in &instance.recognized_qualification_owners {
            match floor.recognized_qualification_owners.get(owner) {
                Some(floor_key) if floor_key == key => {}
                _ => {
                    loosened(
                        "recognized_qualification_owners",
                        format!(
                            "the instance recognizes qualification owner `{owner}` under a key the template's policy floor does not recognize"
                        ),
                    );
                    break;
                }
            }
        }
    }

    // `maturity_floor`: higher tightens; an absent floor is no constraint.
    if let Some(floor_maturity) = floor.maturity_floor {
        match instance.maturity_floor {
            Some(instance_maturity) if instance_maturity >= floor_maturity => {}
            _ => loosened(
                "maturity_floor",
                format!(
                    "the template's policy floor requires maturity at least `{floor_maturity:?}`; the instance declares a lower or absent floor — a loosening"
                ),
            ),
        }
    }

    // `cost_cap`: lower-or-equal in the same currency tightens; an absent
    // floor is no constraint.
    if let Some(floor_cap) = &floor.cost_cap {
        let tightens = instance.cost_cap.as_ref().is_some_and(|instance_cap| {
            instance_cap.currency == floor_cap.currency
                && ExactNumber::from_canonical(&instance_cap.value)
                    .and_then(|i| ExactNumber::from_canonical(&floor_cap.value).map(|f| (i, f)))
                    .is_ok_and(|(i, f)| {
                        i.checked_cmp(&f)
                            .is_ok_and(|o| o != std::cmp::Ordering::Greater)
                    })
        });
        if !tightens {
            loosened(
                "cost_cap",
                "the template's policy floor caps cost; the instance declares a higher, incomparable, or absent cap — a loosening".into(),
            );
        }
    }
}

/// CORE-A4804: re-derive every eligibility rule's outcome over the
/// contract's declared inputs. `ineligible` and `unknown` fail identically —
/// eligible-or-refused. A recorded outcome the declared fields do not imply
/// is itself an eligibility failure: the record cannot say what the fields
/// do not show.
fn check_eligibility(
    record: &ContractInstantiation,
    template: &ContractTemplate,
    contract: &ContractSource,
    findings: &mut Vec<CoreDiagnostic>,
) {
    if template.eligibility.is_empty() && record.eligibility.is_empty() {
        return;
    }
    let recorded: BTreeMap<&str, EligibilityOutcome> = record
        .eligibility
        .iter()
        .map(|e| (e.rule_id.as_str(), e.outcome))
        .collect();
    let mut derived = Vec::with_capacity(template.eligibility.len());
    for rule in &template.eligibility {
        let input = contract
            .inputs
            .iter()
            .find(|input| input.input_id == rule.input_id);
        let outcome = derive_rule(rule, input);
        derived.push(outcome);
        if outcome != EligibilityOutcome::Eligible {
            finding(
                CORE_A4804,
                "contract",
                "/inputs".into(),
                format!(
                    "eligibility rule `{}` derives `{outcome:?}` — the instance admits an input the template does not declare it eligible for",
                    rule.rule_id
                ),
                findings,
            );
            continue;
        }
        match recorded.get(rule.rule_id.as_str()) {
            Some(&recorded_outcome) if recorded_outcome == outcome => {}
            Some(&recorded_outcome) => finding(
                CORE_A4804,
                RECORD_DOCUMENT,
                "/eligibility".into(),
                format!(
                    "eligibility rule `{}` records `{recorded_outcome:?}` but the contract's declared fields derive `eligible` — the record does not match the instance",
                    rule.rule_id
                ),
                findings,
            ),
            None => finding(
                CORE_A4804,
                RECORD_DOCUMENT,
                "/eligibility".into(),
                format!(
                    "eligibility rule `{}` has no recorded outcome in the instantiation record",
                    rule.rule_id
                ),
                findings,
            ),
        }
    }
    // The recorded aggregate must equal the aggregate the rules imply: every
    // rule eligible → eligible; any ineligible → ineligible; else unknown.
    let implied = if derived.iter().all(|o| *o == EligibilityOutcome::Eligible) {
        EligibilityOutcome::Eligible
    } else if derived.contains(&EligibilityOutcome::Ineligible) {
        EligibilityOutcome::Ineligible
    } else {
        EligibilityOutcome::Unknown
    };
    if record.aggregate != implied {
        finding(
            CORE_A4804,
            RECORD_DOCUMENT,
            "/aggregate".into(),
            format!(
                "instantiation record aggregates `{:?}` but the rules imply `{:?}`",
                record.aggregate, implied
            ),
            findings,
        );
    }
    // A recorded rule the template does not declare is a record that does
    // not describe this template.
    let declared_rules: BTreeSet<&str> = template
        .eligibility
        .iter()
        .map(|r| r.rule_id.as_str())
        .collect();
    for RecordedEligibility { rule_id, .. } in &record.eligibility {
        if !declared_rules.contains(rule_id.as_str()) {
            finding(
                CORE_A4804,
                RECORD_DOCUMENT,
                "/eligibility".into(),
                format!(
                    "instantiation record records rule `{rule_id}`, which the template does not declare"
                ),
                findings,
            );
        }
    }
}

/// Derive one rule's outcome over the contract's declared input — the
/// contract input is the claim; `None` (the input isn't declared) derives
/// `unknown`.
fn derive_rule(
    rule: &crate::document::EligibilityRule,
    input: Option<&crate::document::ContractInput>,
) -> EligibilityOutcome {
    let Some(input) = input else {
        return EligibilityOutcome::Unknown;
    };
    derive_predicate(&rule.predicate, input)
}

fn derive_predicate(
    predicate: &EligibilityPredicate,
    input: &crate::document::ContractInput,
) -> EligibilityOutcome {
    match predicate {
        EligibilityPredicate::All(terms) => {
            let outcomes: Vec<EligibilityOutcome> = terms
                .iter()
                .map(|term| derive_predicate(term, input))
                .collect();
            if outcomes.contains(&EligibilityOutcome::Ineligible) {
                EligibilityOutcome::Ineligible
            } else if outcomes.contains(&EligibilityOutcome::Unknown) {
                EligibilityOutcome::Unknown
            } else {
                EligibilityOutcome::Eligible
            }
        }
        EligibilityPredicate::Any(terms) => {
            let outcomes: Vec<EligibilityOutcome> = terms
                .iter()
                .map(|term| derive_predicate(term, input))
                .collect();
            if outcomes.contains(&EligibilityOutcome::Eligible) {
                EligibilityOutcome::Eligible
            } else if outcomes.contains(&EligibilityOutcome::Unknown) {
                EligibilityOutcome::Unknown
            } else {
                EligibilityOutcome::Ineligible
            }
        }
        EligibilityPredicate::Not(term) => match derive_predicate(term, input) {
            EligibilityOutcome::Eligible => EligibilityOutcome::Ineligible,
            EligibilityOutcome::Ineligible => EligibilityOutcome::Eligible,
            EligibilityOutcome::Unknown => EligibilityOutcome::Unknown,
        },
        EligibilityPredicate::InputFieldIn(field_predicate) => {
            let claimed = match field_predicate.field {
                InputField::MediaType => Value::String(input.media_type.clone()),
                InputField::ClaimModel => {
                    serde_json::to_value(&input.claim_model).unwrap_or(Value::Null)
                }
                InputField::Role => serde_json::to_value(&input.role).unwrap_or(Value::Null),
            };
            if field_predicate.values.contains(&claimed) {
                EligibilityOutcome::Eligible
            } else {
                EligibilityOutcome::Ineligible
            }
        }
        EligibilityPredicate::AttributeIn(set) => {
            let Some(value) = input.attributes.get(&set.attribute) else {
                return EligibilityOutcome::Unknown;
            };
            if set.values.iter().any(|v| v == value) {
                EligibilityOutcome::Eligible
            } else {
                EligibilityOutcome::Ineligible
            }
        }
        EligibilityPredicate::AttributeInRange(range) => {
            let Some(raw) = input.attributes.get(&range.attribute) else {
                return EligibilityOutcome::Unknown;
            };
            let Some(actual) = attribute_exact_number(raw) else {
                return EligibilityOutcome::Unknown;
            };
            if let Some(minimum) = &range.min {
                let Ok(minimum) = ExactNumber::from_canonical(minimum) else {
                    return EligibilityOutcome::Unknown;
                };
                let Ok(ordering) = actual.checked_cmp(&minimum) else {
                    return EligibilityOutcome::Unknown;
                };
                if ordering == std::cmp::Ordering::Less
                    || (!range.min_inclusive && ordering == std::cmp::Ordering::Equal)
                {
                    return EligibilityOutcome::Ineligible;
                }
            }
            if let Some(maximum) = &range.max {
                let Ok(maximum) = ExactNumber::from_canonical(maximum) else {
                    return EligibilityOutcome::Unknown;
                };
                let Ok(ordering) = actual.checked_cmp(&maximum) else {
                    return EligibilityOutcome::Unknown;
                };
                if ordering == std::cmp::Ordering::Greater
                    || (!range.max_inclusive && ordering == std::cmp::Ordering::Equal)
                {
                    return EligibilityOutcome::Ineligible;
                }
            }
            EligibilityOutcome::Eligible
        }
    }
}

/// An attribute value an exact bound can compare: a canonical string or a
/// JSON integer, exactly as `input_attribute_in_range` reads them.
fn attribute_exact_number(value: &Value) -> Option<ExactNumber> {
    match value {
        Value::String(text) => ExactNumber::from_canonical(text).ok(),
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            ExactNumber::from_canonical(&number.to_string()).ok()
        }
        _ => None,
    }
}

/// CORE-A4805: every declared validation case must materialize — each
/// `{"ref": ...}` resolving to a bound value — and the materialized contract
/// must compile under the instance's registry. A template whose own declared
/// shape cannot compile is broken; an instance compiled against it does not
/// inherit a pass the template never earned.
fn check_validation_cases(
    template: &ContractTemplate,
    registry_bytes: &[u8],
    findings: &mut Vec<CoreDiagnostic>,
) {
    for case in &template.validation_cases {
        match materialize(template, &case.parameters, &case.inputs, &case.case_id) {
            Err(reason) => finding(
                CORE_A4805,
                TEMPLATE_DOCUMENT,
                "/validation_cases".into(),
                format!(
                    "validation case `{}` does not materialize: {reason}",
                    case.case_id
                ),
                findings,
            ),
            Ok(contract) => {
                let contract_bytes = match serde_json::to_vec(&contract) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        finding(
                            CORE_A4805,
                            TEMPLATE_DOCUMENT,
                            "/validation_cases".into(),
                            format!(
                                "validation case `{}` materializes to a contract that cannot serialize: {error}",
                                case.case_id
                            ),
                            findings,
                        );
                        continue;
                    }
                };
                let report = super::compile_documents(&contract_bytes, registry_bytes);
                let compiles = matches!(report, Ok(super::Compilation::Compiled(_)));
                if !compiles {
                    finding(
                        CORE_A4805,
                        TEMPLATE_DOCUMENT,
                        "/validation_cases".into(),
                        format!(
                            "validation case `{}` materializes to a contract that does not compile under the instance's registry",
                            case.case_id
                        ),
                        findings,
                    );
                }
            }
        }
    }
}

/// Substitute every `{"ref": "<parameter>"}` in the template's workflow and
/// requirements with the bound value, then decode the materialized contract.
/// `inputs` must cover every input the template declares, by id.
pub(super) fn materialize(
    template: &ContractTemplate,
    parameters: &BTreeMap<String, Value>,
    inputs: &[String],
    case_id: &str,
) -> Result<ContractSource, String> {
    let declared_inputs: BTreeSet<&str> = template
        .inputs
        .iter()
        .map(|input| input.input_id.as_str())
        .collect();
    let bound_inputs: BTreeSet<&str> = inputs.iter().map(String::as_str).collect();
    if declared_inputs != bound_inputs {
        return Err(format!(
            "the case binds inputs {bound_inputs:?}, but the template declares {declared_inputs:?} — every declared input must be filled, no more and no less"
        ));
    }
    let substitute = |value: &Value| -> Result<Value, String> {
        fn subst(v: &Value, params: &BTreeMap<String, Value>) -> Result<Value, String> {
            match v {
                Value::Object(map) => {
                    if map.len() == 1
                        && let Some(Value::String(name)) = map.get("ref")
                    {
                        return params
                            .get(name)
                            .cloned()
                            .ok_or_else(|| format!("parameter `{name}` is not bound"));
                    }
                    map.iter()
                        .map(|(k, inner)| Ok((k.clone(), subst(inner, params)?)))
                        .collect::<Result<serde_json::Map<String, Value>, String>>()
                        .map(Value::Object)
                }
                Value::Array(items) => items
                    .iter()
                    .map(|inner| subst(inner, params))
                    .collect::<Result<Vec<Value>, String>>()
                    .map(Value::Array),
                _ => Ok(v.clone()),
            }
        }
        subst(value, parameters)
    };
    let workflow = template
        .workflow
        .iter()
        .map(substitute)
        .collect::<Result<Vec<Value>, String>>()?;
    let requirements = template
        .requirements
        .iter()
        .map(substitute)
        .collect::<Result<Vec<Value>, String>>()?;
    let inputs: Vec<crate::document::ContractInput> = template
        .inputs
        .iter()
        .map(|input| crate::document::ContractInput {
            input_id: input.input_id.clone(),
            role: input.role.clone(),
            media_type: input.media_type.clone(),
            claim_model: input.claim_model.clone(),
            attributes: BTreeMap::new(),
            input_metadata: BTreeMap::new(),
        })
        .collect();
    let source = ContractSource {
        schema_version: crate::document::CONTRACT_SCHEMA_VERSION.into(),
        semantic_profile: avila_core_kernel::SEMANTIC_PROFILE.into(),
        contract_id: format!("validation-case:{}:{case_id}", template.template_id),
        revision: 1,
        status: crate::document::ContractStatus::Draft,
        question: format!("validation case {case_id}"),
        assumptions: Vec::new(),
        execution_policy: template.policy_floor.clone(),
        inputs,
        workflow: serde_json::from_value(Value::Array(workflow))
            .map_err(|error| format!("workflow does not materialize: {error}"))?,
        requirements: serde_json::from_value(Value::Array(requirements))
            .map_err(|error| format!("requirements do not materialize: {error}"))?,
        categorical_requirements: Vec::new(),
        instantiated_from: None,
        completion: None,
    };
    Ok(source)
}

/// The instance's `inputs` the record fills must be exactly the template's
/// declared inputs — checked under CORE-A4802 alongside the parameter
/// bindings.
fn check_inputs(
    record: &ContractInstantiation,
    template: &ContractTemplate,
    contract: &ContractSource,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let declared: BTreeSet<&str> = template
        .inputs
        .iter()
        .map(|input| input.input_id.as_str())
        .collect();
    let bound: BTreeSet<&str> = record.inputs.iter().map(String::as_str).collect();
    if declared != bound {
        finding(
            CORE_A4802,
            RECORD_DOCUMENT,
            "/inputs".into(),
            format!(
                "the record fills inputs {bound:?}, but the template declares {declared:?} — every declared input must be filled, no more and no less"
            ),
            findings,
        );
        return;
    }
    for input_id in &record.inputs {
        let Some(input) = contract
            .inputs
            .iter()
            .find(|input| &input.input_id == input_id)
        else {
            finding(
                CORE_A4802,
                RECORD_DOCUMENT,
                "/inputs".into(),
                format!(
                    "the record fills input `{input_id}`, which the compiled contract does not declare"
                ),
                findings,
            );
            continue;
        };
        let template_input = template
            .inputs
            .iter()
            .find(|declared| declared.input_id == *input_id)
            .expect("declared == bound was checked");
        // The filled input must satisfy the declared shape — equal media
        // type and claim model, and a role satisfying the declared one.
        if input.media_type != template_input.media_type
            || input.claim_model != template_input.claim_model
            || !input.role.satisfies(&template_input.role)
        {
            finding(
                CORE_A4802,
                RECORD_DOCUMENT,
                "/inputs".into(),
                format!(
                    "filled input `{input_id}` does not satisfy the template's declared shape (role `{}`, media type, claim model)",
                    template_input.role.label()
                ),
                findings,
            );
        }
    }
}
