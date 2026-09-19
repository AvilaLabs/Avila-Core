//! ADR-0023: the `organization_policy` floor and the mechanical `tightens`
//! merge order.
//!
//! An `execution_policy.organization_policy` pin names the bound floor
//! document the contract derives from; `relation` declares how the
//! contract's own rule fields merge against it:
//!
//! - `tightens` — every *declared* contract field must be at least as
//!   strict as the floor, field by field; undeclared fields inherit the
//!   floor value (`CORE-A4901` on any loosening).
//! - `exact` — the floor verbatim; every other rule field undeclared
//!   (`CORE-A4902`).
//! - `replaces` — the floor superseded; requires `replacement_attestation`
//!   naming a bound `attestation` in which a `policy_owner` approved this
//!   contract identity replacing this policy (`CORE-A4903`). The
//!   signature itself is the runner's boundary — the compiler checks the
//!   binding fields only.
//! - `none` — the status quo; legal only with the pin absent.
//!
//! The merged `ExecutionPolicy` is what lands in the compiled snapshot —
//! the runner enforces it unchanged (SC-8.2: "the engine does not merge
//! policies by judgment — it checks the recorded merge against a
//! mechanical order").
//!
//! `CORE-A4904` names a malformed floor document — parse or schema
//! failure, a floor carrying naming fields of its own, or (runner-side)
//! a signature that cannot verify under a `policy_owner` key.

use std::collections::BTreeSet;

use avila_core_kernel::ExactNumber;

use super::CompilationMaterial;
use super::instantiation::{canonical_sha256, finding};
use crate::diagnostic::{
    CORE_A4901, CORE_A4902, CORE_A4903, CORE_A4904, CORE_A4905, CoreDiagnostic, FindingClass,
    SourceLocation,
};
use crate::document::{
    ContractSource, ExecutionPolicy, OrganizationPolicy, OrganizationPolicyRef, PolicyRelation,
};

const POLICY_DOCUMENT: &str = "organization_policy";

const ORGANIZATION_POLICY_SCHEMA: &str =
    include_str!("../../../../schemas/organization-policy.v0.1-draft.schema.json");
const ATTESTATION_SCHEMA: &str =
    include_str!("../../../../schemas/attestation.v0.1-draft.schema.json");

fn organization_policy_schema() -> &'static serde_json::Value {
    static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    SCHEMA
        .get_or_init(|| serde_json::from_str(ORGANIZATION_POLICY_SCHEMA).expect("embedded schema"))
}

fn attestation_schema() -> &'static serde_json::Value {
    static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(ATTESTATION_SCHEMA).expect("embedded schema"))
}

/// A structural view of a bound attestation — the compiler reads the
/// binding fields only; signature verification is the runner's.
#[derive(Debug, serde::Deserialize)]
struct AttestationView {
    subject: AttestationSubjectView,
    statement: String,
    role: String,
    #[serde(default)]
    target: Option<AttestationTargetView>,
}

#[derive(Debug, serde::Deserialize)]
struct AttestationSubjectView {
    kind: String,
    identity: String,
    sha256: String,
}

#[derive(Debug, serde::Deserialize)]
struct AttestationTargetView {
    kind: String,
    identity: String,
}

/// The contract's `id@revision` rendering — the identity every subject
/// and target field uses.
fn contract_identity(contract: &ContractSource) -> String {
    format!("{}@{}", contract.contract_id, contract.revision)
}

fn policy_identity(pin: &OrganizationPolicyRef) -> String {
    format!("{}@{}", pin.policy_id, pin.policy_revision)
}

/// Merge the contract's `execution_policy` against its pinned
/// organization floor. Returns `Some(merged)` when a relation was proven
/// — `tightens` merges contract-declared fields over the floor, `exact`
/// adopts the floor, `replaces` takes the contract's own fields. `None`
/// means the authored policy stands: either no floor was named or a
/// blocking finding already covers the failure.
pub(super) fn check_org_policy(
    contract: &ContractSource,
    material: &CompilationMaterial<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<ExecutionPolicy> {
    let policy = &contract.execution_policy;
    let Some(pin) = &policy.organization_policy else {
        // No floor named — any declared relation beyond `none` or an
        // attestation field is a merge declaration with nothing to merge.
        match policy.relation {
            None | Some(PolicyRelation::None) => {}
            Some(relation) => {
                merge_finding(
                    CORE_A4902,
                    "/execution_policy/relation",
                    format!(
                        "relation `{relation:?}` is declared but `organization_policy` pins no floor — a relation must name the floor it merges against"
                    ),
                    findings,
                );
            }
        }
        if policy.replacement_attestation.is_some() {
            merge_finding(
                CORE_A4902,
                "/execution_policy/replacement_attestation",
                "`replacement_attestation` is declared but no `replaces` relation pins a floor"
                    .into(),
                findings,
            );
        }
        return None;
    };

    // A pin with no declared relation — or one declaring `none` — is a
    // merge that cannot name its semantics.
    let Some(relation) = policy.relation else {
        merge_finding(
            CORE_A4902,
            "/execution_policy/relation",
            "`organization_policy` pins a floor but `relation` is undeclared — the merge must name `tightens`, `exact`, or `replaces`".into(),
            findings,
        );
        return None;
    };
    if relation == PolicyRelation::None {
        merge_finding(
            CORE_A4902,
            "/execution_policy/relation",
            "`organization_policy` pins a floor but `relation` declares `none` — the pin and the relation contradict".into(),
            findings,
        );
        return None;
    }

    // --- resolve the pinned floor among the bound documents. ---
    let policy_doc = resolve_policy(pin, material, findings)?;
    report_superseded(pin, &policy_doc, material, findings);

    // An unsigned floor is meaningless — a requester could mint any
    // policy. The signature itself is verified under the trust root at
    // run time; absent is refused here.
    if policy_doc.signature.is_none() {
        merge_finding(
            CORE_A4904,
            "/signature",
            format!(
                "organization policy `{}` carries no signature — an unsigned floor cannot stand",
                policy_identity(pin)
            ),
            findings,
        );
        return None;
    }

    // A floor may not carry naming fields of its own — `rules` is the
    // rule vocabulary only.
    let rules = &policy_doc.rules;
    if rules.organization_policy.is_some()
        || rules.relation.is_some()
        || rules.replacement_attestation.is_some()
    {
        merge_finding(
            CORE_A4904,
            "/rules",
            "the organization policy's `rules` carries merge-naming fields (`organization_policy`, `relation`, `replacement_attestation`) — a floor declares rules only".into(),
            findings,
        );
        return None;
    }

    match relation {
        PolicyRelation::Tightens => {
            if check_tightens(policy, rules, findings) {
                Some(merge_tightens(policy, rules))
            } else {
                None
            }
        }
        PolicyRelation::Exact => {
            if check_exact(policy, findings) {
                let mut merged = rules.clone();
                merged.organization_policy = None;
                merged.relation = None;
                merged.replacement_attestation = None;
                Some(merged)
            } else {
                None
            }
        }
        PolicyRelation::Replaces => {
            check_replaces(contract, pin, material, findings)?;
            let mut merged = policy.clone();
            merged.organization_policy = None;
            merged.relation = None;
            merged.replacement_attestation = None;
            Some(merged)
        }
        PolicyRelation::None => unreachable!("relation `none` returns above"),
    }
}

/// Find the bound `organization_policy` the pin names. Resolution is by
/// canonical digest first, then identity fields — a doc whose digest
/// matches but whose `policy_id`/`policy_revision` differ is A4902.
fn resolve_policy(
    pin: &OrganizationPolicyRef,
    material: &CompilationMaterial<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<OrganizationPolicy> {
    let matched: Vec<&[u8]> = material
        .organization_policies
        .iter()
        .filter(|bytes| canonical_sha256(bytes).as_deref() == Some(pin.sha256.as_str()))
        .copied()
        .collect();
    let [bytes] = matched.as_slice() else {
        merge_finding(
            CORE_A4902,
            "/execution_policy/organization_policy",
            if matched.is_empty() {
                format!(
                    "the contract pins organization policy `{}` at digest {}, but no bound document carries that digest",
                    policy_identity(pin),
                    pin.sha256
                )
            } else {
                format!(
                    "{} bound documents carry the pinned policy digest — the pin must resolve to exactly one",
                    matched.len()
                )
            },
            findings,
        );
        return None;
    };
    let canonical = avila_core_kernel::read_authoritative_json(bytes)
        .expect("a matched digest implies authoritative JSON");
    let mut shape = Vec::new();
    super::schema::validate_against_schema(
        organization_policy_schema(),
        POLICY_DOCUMENT,
        "requester",
        &canonical,
        &mut shape,
    );
    if !shape.is_empty() {
        findings.extend(shape);
        return None;
    }
    let Ok(document) = serde_json::from_slice::<OrganizationPolicy>(bytes) else {
        merge_finding(
            CORE_A4904,
            "",
            "organization policy validates its schema but does not decode".into(),
            findings,
        );
        return None;
    };
    if document.policy_id != pin.policy_id || document.policy_revision != pin.policy_revision {
        merge_finding(
            CORE_A4902,
            "/execution_policy/organization_policy",
            format!(
                "the pinned document is `{}@{}`, but the contract names `{}` — the pin's identity fields must match the document it digests",
                document.policy_id,
                document.policy_revision,
                policy_identity(pin)
            ),
            findings,
        );
        return None;
    }
    Some(document)
}

/// A bound newer revision of the pinned policy is drift information —
/// the `policy_superseded` notice (CORE-A4905), never invalidation.
fn report_superseded(
    _pin: &OrganizationPolicyRef,
    document: &OrganizationPolicy,
    material: &CompilationMaterial<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) {
    let newer = material
        .organization_policies
        .iter()
        .filter_map(|bytes| serde_json::from_slice::<OrganizationPolicy>(bytes).ok())
        .filter(|candidate| {
            candidate.policy_id == document.policy_id
                && candidate.policy_revision > document.policy_revision
        })
        .map(|candidate| candidate.policy_revision)
        .max();
    if let Some(revision) = newer {
        findings.push(CoreDiagnostic::new(
            CORE_A4905,
            FindingClass::Notice,
            "policy_owner",
            SourceLocation::new(POLICY_DOCUMENT, "/policy_revision"),
            format!(
                "organization policy `{}` has a bound revision {} newer than the revision {} this contract pins — the pin is immutable and this is drift information, not invalidation",
                document.policy_id, revision, document.policy_revision
            ),
        ));
    }
}

/// CORE-A4901: every *declared* contract field must be at least as
/// strict as the floor. Undeclared fields inherit (serde defaults are
/// indistinguishable from absent), so the only failures are declared
/// values that loosen.
fn check_tightens(
    contract: &ExecutionPolicy,
    floor: &ExecutionPolicy,
    findings: &mut Vec<CoreDiagnostic>,
) -> bool {
    let mut ok = true;
    let mut loosened = |field: &str, message: String| {
        ok = false;
        merge_finding(
            CORE_A4901,
            &format!("/execution_policy/{field}"),
            message,
            findings,
        );
    };

    // `deny_providers`: the contract's deny set must contain the floor's.
    let floor_deny: BTreeSet<&str> = floor.deny_providers.iter().map(String::as_str).collect();
    let contract_deny: BTreeSet<&str> =
        contract.deny_providers.iter().map(String::as_str).collect();
    if !contract_deny.is_empty() && !floor_deny.is_subset(&contract_deny) {
        loosened(
            "deny_providers",
            format!(
                "the contract denies {contract_deny:?}, but the floor denies {floor_deny:?} — a missing denial is a loosening"
            ),
        );
    }

    // `allow_providers`: a declared contract allow-list must be a subset
    // of the floor's; an undeclared floor allows everything, so any
    // contract allow-list tightens it.
    if !contract.allow_providers.is_empty() && !floor.allow_providers.is_empty() {
        let floor_allow: BTreeSet<&str> =
            floor.allow_providers.iter().map(String::as_str).collect();
        let contract_allow: BTreeSet<&str> = contract
            .allow_providers
            .iter()
            .map(String::as_str)
            .collect();
        if !contract_allow.is_subset(&floor_allow) {
            loosened(
                "allow_providers",
                format!(
                    "the contract allows {contract_allow:?}, but the floor admits only {floor_allow:?} — a wider allow-list is a loosening"
                ),
            );
        }
    }

    // Grant lists — `permitted_nondeterministic_roles` and
    // `recognized_qualification_owners`: the floor names what the
    // organization permits; a contract grant outside it loosens.
    let floor_roles: BTreeSet<String> = floor
        .permitted_nondeterministic_roles
        .iter()
        .map(|role| role.label())
        .collect();
    for role in &contract.permitted_nondeterministic_roles {
        if !floor_roles.contains(&role.label()) {
            loosened(
                "permitted_nondeterministic_roles",
                format!(
                    "the contract permits nondeterministic role `{}`, which the floor does not permit — a grant outside the floor is a loosening",
                    role.label()
                ),
            );
        }
    }
    for owner in contract.recognized_qualification_owners.keys() {
        if !floor.recognized_qualification_owners.contains_key(owner) {
            loosened(
                "recognized_qualification_owners",
                format!(
                    "the contract recognizes qualification issuer `{owner}`, which the floor does not recognize — a recognition outside the floor is a loosening"
                ),
            );
        }
    }

    // `permit_nominal_basis` is the permissive flag: the contract may
    // not permit what the floor forbids. Restrictive flags cannot be
    // loosened — an undeclared boolean inherits the floor value.
    if contract.permit_nominal_basis && !floor.permit_nominal_basis {
        loosened(
            "permit_nominal_basis",
            "the floor does not permit the nominal basis; the contract permits it — a permissive rule added is a loosening".into(),
        );
    }

    // `maturity_floor`: a declared contract maturity must rank at least
    // the floor's.
    if let (Some(contract_maturity), Some(floor_maturity)) =
        (contract.maturity_floor, floor.maturity_floor)
        && contract_maturity < floor_maturity
    {
        loosened(
            "maturity_floor",
            format!(
                "the contract's maturity floor {contract_maturity:?} ranks below the organization's {floor_maturity:?} — a lower floor is a loosening"
            ),
        );
    }

    // `cost_cap`: a declared contract cap must not exceed the floor's in
    // the same currency.
    if let (Some(contract_cap), Some(floor_cap)) = (&contract.cost_cap, &floor.cost_cap) {
        let loosening = contract_cap.currency != floor_cap.currency
            || !ExactNumber::from_canonical(&contract_cap.value)
                .and_then(|c| ExactNumber::from_canonical(&floor_cap.value).map(|f| (c, f)))
                .is_ok_and(|(c, f)| {
                    c.checked_cmp(&f)
                        .is_ok_and(|o| o != std::cmp::Ordering::Greater)
                });
        if loosening {
            loosened(
                "cost_cap",
                format!(
                    "the contract's cost cap {} {} exceeds or changes the floor's {} {} — a higher cap is a loosening",
                    contract_cap.value, contract_cap.currency, floor_cap.value, floor_cap.currency
                ),
            );
        }
    }
    ok
}

/// The merge itself: every field resolves to the contract's declared
/// value, else the floor's. Post-`check_tightens` a declared contract
/// field is provably at least as strict, so the merge is mechanical.
fn merge_tightens(contract: &ExecutionPolicy, floor: &ExecutionPolicy) -> ExecutionPolicy {
    ExecutionPolicy {
        permitted_nondeterministic_roles: if contract.permitted_nondeterministic_roles.is_empty() {
            floor.permitted_nondeterministic_roles.clone()
        } else {
            contract.permitted_nondeterministic_roles.clone()
        },
        // Booleans merge to `org || contract`: a contract can only
        // declare `true`, and `true` is the strict side of every
        // restrictive flag — for the permissive `permit_nominal_basis`
        // the loosening case was already refused.
        permit_nominal_basis: floor.permit_nominal_basis || contract.permit_nominal_basis,
        require_qualification: floor.require_qualification || contract.require_qualification,
        recognized_qualification_owners: if contract.recognized_qualification_owners.is_empty() {
            floor.recognized_qualification_owners.clone()
        } else {
            contract.recognized_qualification_owners.clone()
        },
        require_signatures: floor.require_signatures || contract.require_signatures,
        deny_providers: if contract.deny_providers.is_empty() {
            floor.deny_providers.clone()
        } else {
            contract.deny_providers.clone()
        },
        allow_providers: if contract.allow_providers.is_empty() {
            floor.allow_providers.clone()
        } else {
            contract.allow_providers.clone()
        },
        require_provider_independence: floor.require_provider_independence
            || contract.require_provider_independence,
        require_diverse_implementations: floor.require_diverse_implementations
            || contract.require_diverse_implementations,
        maturity_floor: contract.maturity_floor.or(floor.maturity_floor),
        forbid_self_preference: floor.forbid_self_preference || contract.forbid_self_preference,
        cost_cap: contract.cost_cap.clone().or_else(|| floor.cost_cap.clone()),
        organization_policy: None,
        relation: None,
        replacement_attestation: None,
    }
}

/// CORE-A4902: `exact` adopts the floor verbatim — every other rule
/// field must be undeclared (serde defaults are the undeclared value).
fn check_exact(contract: &ExecutionPolicy, findings: &mut Vec<CoreDiagnostic>) -> bool {
    let declared = contract.permit_nominal_basis
        || contract.require_qualification
        || contract.require_signatures
        || contract.require_provider_independence
        || contract.require_diverse_implementations
        || contract.forbid_self_preference
        || !contract.deny_providers.is_empty()
        || !contract.allow_providers.is_empty()
        || !contract.permitted_nondeterministic_roles.is_empty()
        || !contract.recognized_qualification_owners.is_empty()
        || contract.maturity_floor.is_some()
        || contract.cost_cap.is_some();
    if declared {
        merge_finding(
            CORE_A4902,
            "/execution_policy",
            "`relation: exact` adopts the floor verbatim, but the contract declares other rule fields — there is nothing to merge".into(),
            findings,
        );
    }
    !declared
}

/// CORE-A4903: `replaces` requires `replacement_attestation` naming a
/// bound attestation in which a `policy_owner` approved this contract
/// identity replacing this policy. The compiler checks the binding
/// fields; the signature itself is the runner's boundary (CORE-A4904).
fn check_replaces(
    contract: &ContractSource,
    pin: &OrganizationPolicyRef,
    material: &CompilationMaterial<'_>,
    findings: &mut Vec<CoreDiagnostic>,
) -> Option<()> {
    let Some(attestation_sha256) = &contract.execution_policy.replacement_attestation else {
        merge_finding(
            CORE_A4903,
            "/execution_policy/replacement_attestation",
            "`relation: replaces` supersedes the organization floor but names no `replacement_attestation` — the escape is refused without a policy_owner authorization".into(),
            findings,
        );
        return None;
    };
    let matched: Vec<&[u8]> = material
        .attestations
        .iter()
        .filter(|bytes| canonical_sha256(bytes).as_deref() == Some(attestation_sha256.as_str()))
        .copied()
        .collect();
    let [bytes] = matched.as_slice() else {
        merge_finding(
            CORE_A4903,
            "/execution_policy/replacement_attestation",
            if matched.is_empty() {
                format!(
                    "`replacement_attestation` pins digest {attestation_sha256}, but no bound attestation document carries it"
                )
            } else {
                format!(
                    "{} bound attestations carry the pinned digest — the pin must resolve to exactly one",
                    matched.len()
                )
            },
            findings,
        );
        return None;
    };
    let canonical = avila_core_kernel::read_authoritative_json(bytes)
        .expect("a matched digest implies authoritative JSON");
    let mut shape = Vec::new();
    super::schema::validate_against_schema(
        attestation_schema(),
        "attestation",
        "requester",
        &canonical,
        &mut shape,
    );
    if !shape.is_empty() {
        findings.extend(shape);
        return None;
    }
    let Ok(attestation) = serde_json::from_slice::<AttestationView>(bytes) else {
        merge_finding(
            CORE_A4903,
            "/execution_policy/replacement_attestation",
            "the named attestation validates its schema but does not decode".into(),
            findings,
        );
        return None;
    };

    // The binding fields: the statement must cover the pinned policy
    // (subject) and this contract identity (target), in the
    // `policy_owner` role, with the `approves` statement.
    let expected_policy = policy_identity(pin);
    let expected_contract = contract_identity(contract);
    let mut ok = true;
    if attestation.subject.kind != "organization_policy"
        || attestation.subject.identity != expected_policy
        || attestation.subject.sha256 != pin.sha256
    {
        ok = false;
        merge_finding(
            CORE_A4903,
            "/execution_policy/replacement_attestation",
            format!(
                "the replacement attestation's subject is `{}:{}` at `{}`, not `organization_policy:{expected_policy}` at `{policy_sha}` — it must cover the pinned policy exactly",
                attestation.subject.kind,
                attestation.subject.identity,
                attestation.subject.sha256,
                policy_sha = pin.sha256
            ),
            findings,
        );
    }
    match &attestation.target {
        Some(target) if target.kind == "contract" && target.identity == expected_contract => {}
        Some(target) => {
            ok = false;
            merge_finding(
                CORE_A4903,
                "/execution_policy/replacement_attestation",
                format!(
                    "the replacement attestation's target is `{}:{}`, not `contract:{expected_contract}` — the authorization must name this exact contract",
                    target.kind, target.identity
                ),
                findings,
            );
        }
        None => {
            ok = false;
            merge_finding(
                CORE_A4903,
                "/execution_policy/replacement_attestation",
                "the replacement attestation carries no `target` — it must name the contract identity it authorizes".into(),
                findings,
            );
        }
    }
    if attestation.statement != "approves" {
        ok = false;
        merge_finding(
            CORE_A4903,
            "/execution_policy/replacement_attestation",
            format!(
                "the replacement attestation's statement is `{}`, not `approves` — only an approval authorizes a replacement",
                attestation.statement
            ),
            findings,
        );
    }
    if attestation.role != "policy_owner" {
        ok = false;
        merge_finding(
            CORE_A4903,
            "/execution_policy/replacement_attestation",
            format!(
                "the replacement attestation's role is `{}`, not `policy_owner` — a requester cannot authorize replacing an org floor",
                attestation.role
            ),
            findings,
        );
    }
    ok.then_some(())
}

/// Contract-side findings (`/execution_policy/...` pointers) attribute to
/// the contract document; floor/attestation-side findings attribute to
/// the bound document they concern.
fn merge_finding(code: &str, pointer: &str, message: String, findings: &mut Vec<CoreDiagnostic>) {
    let document = if pointer.starts_with("/execution_policy") {
        "contract"
    } else {
        POLICY_DOCUMENT
    };
    finding(code, document, pointer.into(), message, findings);
}
