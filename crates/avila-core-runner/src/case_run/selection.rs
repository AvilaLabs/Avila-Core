//! Provider selection records (ADR-0020, SC-8): the org's recorded decision
//! of which capability implementation serves each step, verified against the
//! package it claims to describe.
//!
//! The engine does not select — it checks the recorded selection. A bound
//! `capability_selection` document is checked for internal consistency
//! (every candidate decided and reasoned, criteria in the legitimate
//! vocabulary, at most one winner) and against the package (the selected
//! triple is exactly the capability type, capability, adapter, and
//! executable the contract and manifest bind for the step). When the
//! contract's `execution_policy` declares provider rules, the record is
//! required and the rules gate admissibility: a refusal stops the run
//! before any execution is spent, and never enters a verdict.

use std::collections::BTreeMap;
use std::error::Error;

use avila_core_compiler::{
    BANNED_CRITERIA, CandidateDecision, CompiledContract, LEGITIMATE_CRITERIA, RegistrySnapshot,
    SourceLocation, parse_selection,
};
use avila_core_evidence::signature::{KeyRole, TrustRoot, verify_signature_document};
use avila_core_evidence::{PackageDocument, PackageExecution, VerifiedCasePackage};
use avila_core_kernel::read_authoritative_decimal;

use crate::diagnostic::{
    CORE_P5101, CORE_P5102, CORE_P5103, CORE_P5201, CORE_P5301, CORE_P5302, CORE_P5303, CORE_P5304,
    CORE_P5401, CORE_P5501, CORE_P5601, CORE_P5602, RunStage,
};

use super::signing::find_signature_for;
use crate::RunFinding;

/// The capability implementation a step is actually bound to: the contract
/// step's declared type plus the manifest execution's capability id and
/// adapter plus that capability's bound executable digest.
struct BoundTriple {
    capability_type: avila_core_compiler::VersionedRef,
    capability_id: String,
    adapter: String,
    executable_sha256: String,
}

/// Load and check the package's `capability_selection` document. Returns
/// the findings to attach; the caller stops the run when any finding is
/// `Inadmissible`. `registry_doc_sha256` is the manifest-pinned digest of
/// the bound registry document.
pub(crate) fn load_selection(
    package: &VerifiedCasePackage,
    compiled: &CompiledContract,
    registry: &RegistrySnapshot,
    registry_doc_sha256: &str,
    trust_root: Option<&TrustRoot>,
) -> Result<Vec<RunFinding>, Box<dyn Error>> {
    let policy = &compiled.execution_policy;
    let policy_active = !policy.deny_providers.is_empty()
        || !policy.allow_providers.is_empty()
        || policy.require_provider_independence
        || policy.require_diverse_implementations
        || policy.maturity_floor.is_some()
        || policy.forbid_self_preference
        || policy.cost_cap.is_some();

    let documents: Vec<_> = package
        .manifest
        .documents
        .iter()
        .filter(|document| document.role == "capability_selection")
        .collect();
    if documents.len() > 1 {
        return Err(format!(
            "the package binds {} `capability_selection` documents; one document records the whole package's selections",
            documents.len()
        )
        .into());
    }
    let mut findings = Vec::new();
    let Some(document) = documents.first() else {
        if policy_active {
            findings.push(finding(
                CORE_P5103,
                "policy_owner",
                None,
                "the contract's execution policy declares provider-selection rules, but no `capability_selection` document is bound — a policy without its record cannot be checked".into(),
            ));
        }
        return Ok(findings);
    };

    if let Some(reason) = selection_signature_reason(package, document, compiled, trust_root) {
        findings.push(finding(CORE_P5103, "policy_owner", None, reason));
    }

    let bytes = package
        .document_by_id(&document.document_id)
        .ok_or_else(|| {
            format!(
                "capability_selection document `{}` has no bytes",
                document.document_id
            )
        })?;
    let selection = parse_selection(bytes)
        .map_err(|error| format!("document `{}`: {error}", document.document_id))?;

    // The record's discovery boundary must be the exact registry snapshot
    // this package compiled against — a record naming any other snapshot
    // describes a different candidate set.
    let snapshot = &selection.registry_snapshot;
    if snapshot.registry_id != registry.registry_id
        || snapshot.revision != registry.revision
        || snapshot.sha256 != registry_doc_sha256
    {
        findings.push(finding(
            CORE_P5102,
            "requester",
            None,
            format!(
                "the selection record's registry snapshot `{}@{}` ({}) is not the package's bound registry `{}@{}` ({}) — the record's candidate set is bounded by different material",
                snapshot.registry_id,
                snapshot.revision,
                snapshot.sha256,
                registry.registry_id,
                registry.revision,
                registry_doc_sha256
            ),
        ));
    }

    // A selection entry for a step the package does not execute is a
    // dangling record — it claims a decision about a binding that does not
    // exist.
    for entry in &selection.selections {
        if !package
            .manifest
            .executions
            .iter()
            .any(|execution| execution.step_id == entry.step_id)
        {
            return Err(format!(
                "capability_selection records a selection for step `{}`, which the package does not execute",
                entry.step_id
            )
            .into());
        }
    }

    let mut selected_owners: Vec<(String, String)> = Vec::new();
    let mut selected_executables: Vec<(String, String)> = Vec::new();
    for execution in &package.manifest.executions {
        let step_id = execution.step_id.as_str();
        let Some(entry) = selection
            .selections
            .iter()
            .find(|entry| entry.step_id == step_id)
        else {
            if policy_active {
                findings.push(finding(
                    CORE_P5103,
                    "policy_owner",
                    Some(step_id),
                    format!("step `{step_id}` has no selection entry — the policy's rules cannot be checked for it"),
                ));
            }
            continue;
        };

        // Structural honesty: every considered candidate carries a decision
        // and its reasons; every criterion is legitimate vocabulary.
        for candidate in &entry.candidates {
            if candidate.decision.is_none() || candidate.reasons.is_empty() {
                findings.push(finding(
                    CORE_P5602,
                    "policy_owner",
                    Some(step_id),
                    format!("step `{step_id}` candidate `{}`/`{}` carries no recorded decision or reasons", candidate.capability_id, candidate.adapter),
                ));
            }
            if candidate.decision == Some(CandidateDecision::Excluded)
                && candidate
                    .reasons
                    .iter()
                    .any(|reason| reason.rule_id == "contract_constraint")
            {
                findings.push(finding_class(
                    CORE_P5201,
                    avila_core_compiler::FindingClass::Notice,
                    "policy_owner",
                    Some(step_id),
                    format!(
                        "step `{step_id}` candidate `{}`/`{}` is excluded by a contract constraint",
                        candidate.capability_id, candidate.adapter
                    ),
                ));
            }
        }
        for criterion in &entry.criteria {
            if !LEGITIMATE_CRITERIA.contains(&criterion.as_str()) {
                let banned = BANNED_CRITERIA.contains(&criterion.as_str());
                findings.push(finding(
                    CORE_P5601,
                    "policy_owner",
                    Some(step_id),
                    if banned {
                        format!("step `{step_id}` selection criterion `{criterion}` is banned — provider payment and Avila margin may never rank a selection, visibly or otherwise")
                    } else {
                        format!("step `{step_id}` selection criterion `{criterion}` is not in the legitimate vocabulary ({})", LEGITIMATE_CRITERIA.join(", "))
                    },
                ));
            }
        }

        let Some(selected) = entry.selected() else {
            let reasons = entry
                .candidates
                .iter()
                .flat_map(|candidate| {
                    candidate
                        .reasons
                        .iter()
                        .map(|reason| format!("`{}` ({})", reason.rule_id, reason.detail))
                })
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(finding(
                CORE_P5101,
                "policy_owner",
                Some(step_id),
                format!("step `{step_id}` records no eligible candidate — every candidate was excluded or inadmissible: {reasons}"),
            ));
            continue;
        };

        // The recorded winner must be exactly what the contract and
        // manifest bind for the step.
        let bound = bound_triple(compiled, package, execution)?;
        if selected.capability_type != bound.capability_type
            || selected.capability_id != bound.capability_id
            || selected.adapter != bound.adapter
            || selected.executable_sha256 != bound.executable_sha256
        {
            findings.push(finding(
                CORE_P5102,
                "requester",
                Some(step_id),
                format!("step `{step_id}` binds `{}`/`{}` ({}) under type `{}@{}`, but the recorded selection names `{}`/`{}` ({}) under `{}@{}`",
                    bound.capability_id, bound.adapter, bound.executable_sha256,
                    bound.capability_type.id, bound.capability_type.major,
                    selected.capability_id, selected.adapter, selected.executable_sha256,
                    selected.capability_type.id, selected.capability_type.major),
            ));
        }

        // Provider rules gate on the selected type's registry owner and
        // declared maturity.
        let owner = registry
            .capability_types
            .iter()
            .find(|capability| capability.capability_type == selected.capability_type)
            .map(|capability| capability.owner.as_str());
        let Some(owner) = owner else {
            return Err(format!(
                "capability_selection's selected candidate for step `{step_id}` names capability type `{}@{}`, which the bound registry does not declare",
                selected.capability_type.id, selected.capability_type.major
            )
            .into());
        };
        if policy.deny_providers.iter().any(|denied| denied == owner) {
            findings.push(finding(
                CORE_P5301,
                "policy_owner",
                Some(step_id),
                format!("step `{step_id}` selected provider `{owner}`, which `execution_policy.deny_providers` forbids"),
            ));
        }
        if !policy.allow_providers.is_empty()
            && !policy
                .allow_providers
                .iter()
                .any(|allowed| allowed == owner)
        {
            findings.push(finding(
                CORE_P5301,
                "policy_owner",
                Some(step_id),
                format!("step `{step_id}` selected provider `{owner}`, which `execution_policy.allow_providers` does not list"),
            ));
        }
        if let Some(floor) = policy.maturity_floor {
            let declared = registry
                .capability_types
                .iter()
                .find(|capability| capability.capability_type == selected.capability_type)
                .and_then(|capability| capability.maturity);
            if declared.is_none_or(|maturity| maturity < floor) {
                findings.push(finding(
                    CORE_P5303,
                    "policy_owner",
                    Some(step_id),
                    match declared {
                        Some(maturity) => format!("step `{step_id}` selected a capability type whose declared maturity `{maturity:?}` is below `execution_policy.maturity_floor`"),
                        None => format!("step `{step_id}` selected a capability type declaring no maturity — `execution_policy.maturity_floor` cannot be met"),
                    },
                ));
            }
        }
        if policy.forbid_self_preference
            && entry.avila_provided
            && entry.self_preference_check.is_none()
        {
            findings.push(finding(
                CORE_P5501,
                "policy_owner",
                Some(step_id),
                format!("step `{step_id}` selected an Avila-provided implementation with no recorded self-preference check — `execution_policy.forbid_self_preference` requires the check be recorded"),
            ));
        }
        if let Some(cap) = &policy.cost_cap {
            let refused = match &entry.cost_estimate {
                None => Some(format!(
                    "step `{step_id}` records no cost estimate — `execution_policy.cost_cap` cannot be checked"
                )),
                Some(estimate) if estimate.currency != cap.currency => Some(format!(
                    "step `{step_id}` cost estimate is in `{}` but `execution_policy.cost_cap` is in `{}` — the cap cannot be checked across currencies",
                    estimate.currency, cap.currency
                )),
                Some(estimate) => {
                    // Fails closed: a value either side can't read as an
                    // exact decimal counts as over the cap.
                    let over = read_authoritative_decimal(&estimate.value)
                        .ok()
                        .zip(read_authoritative_decimal(&cap.value).ok())
                        .and_then(|(estimate, cap)| estimate.checked_cmp(&cap).ok())
                        .map(|ordering| ordering.is_gt())
                        .unwrap_or(true);
                    over.then(|| {
                        format!("step `{step_id}` cost estimate {} {} exceeds `execution_policy.cost_cap` {} {} with no recorded confirmation", estimate.value, estimate.currency, cap.value, cap.currency)
                    })
                }
            };
            if let Some(reason) = refused
                && entry.cost_confirmed_by.is_none()
            {
                findings.push(finding(CORE_P5401, "policy_owner", Some(step_id), reason));
            }
        }

        selected_owners.push((step_id.to_string(), owner.to_string()));
        selected_executables.push((step_id.to_string(), selected.executable_sha256.clone()));
    }

    // Cross-step rules: a shared provider defeats independence; identical
    // bytes defeat diversity.
    if policy.require_provider_independence {
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        for (step_id, owner) in &selected_owners {
            if let Some(other) = seen.insert(owner.as_str(), step_id.as_str()) {
                findings.push(finding(
                    CORE_P5302,
                    "policy_owner",
                    Some(step_id),
                    format!("steps `{other}` and `{step_id}` both selected provider `{owner}` — `execution_policy.require_provider_independence` forbids a shared provider"),
                ));
            }
        }
    }
    if policy.require_diverse_implementations {
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        for (step_id, executable) in &selected_executables {
            if let Some(other) = seen.insert(executable.as_str(), step_id.as_str()) {
                findings.push(finding(
                    CORE_P5304,
                    "policy_owner",
                    Some(step_id),
                    format!("steps `{other}` and `{step_id}` selected the same executable bytes — `execution_policy.require_diverse_implementations` forbids identical implementations"),
                ));
            }
        }
    }

    Ok(findings)
}

/// The capability triple a manifest execution actually binds: the contract
/// step's declared type, the execution's capability id and adapter, and the
/// bound executable's digest.
fn bound_triple(
    compiled: &CompiledContract,
    package: &VerifiedCasePackage,
    execution: &PackageExecution,
) -> Result<BoundTriple, Box<dyn Error>> {
    let step = compiled
        .workflow
        .iter()
        .find(|step| step.step_id == execution.step_id)
        .ok_or_else(|| {
            format!(
                "manifest execution names step `{}`, which the contract does not declare",
                execution.step_id
            )
        })?;
    let capability = package
        .manifest
        .capabilities
        .iter()
        .find(|capability| capability.capability_id == execution.capability_id)
        .ok_or_else(|| {
            format!(
                "manifest execution for step `{}` names capability `{}`, which the package does not bind",
                execution.step_id, execution.capability_id
            )
        })?;
    Ok(BoundTriple {
        capability_type: step.capability_type.clone(),
        capability_id: execution.capability_id.clone(),
        adapter: execution.adapter.clone(),
        executable_sha256: capability.executable_sha256.clone(),
    })
}

/// Under `require_signatures` the record must verify under a requester key —
/// the org's selection is a signed act, like the manifest. `Some(reason)`
/// describes each failure.
fn selection_signature_reason(
    package: &VerifiedCasePackage,
    document: &PackageDocument,
    compiled: &CompiledContract,
    trust_root: Option<&TrustRoot>,
) -> Option<String> {
    if !compiled.execution_policy.require_signatures {
        return None;
    }
    let Some((_, signature_document)) =
        find_signature_for(package, "capability_selection", &document.document_id)
    else {
        return Some("the `capability_selection` document carries no signature — `require_signatures` demands the requester's".into());
    };
    if signature_document.signed_document.sha256 != document.sha256 {
        return Some(
            "the `capability_selection` signature covers different bytes than the package binds"
                .into(),
        );
    }
    let Some(trust_root) = trust_root else {
        return Some(
            "no trust root was supplied to verify the `capability_selection` signature against"
                .into(),
        );
    };
    match verify_signature_document(&signature_document, trust_root, KeyRole::Requester) {
        Ok(_) => None,
        Err(error) => Some(format!(
            "the `capability_selection` signature does not verify under a requester key — {error}"
        )),
    }
}

fn finding(code: &'static str, owner: &str, step_id: Option<&str>, message: String) -> RunFinding {
    finding_class(
        code,
        avila_core_compiler::FindingClass::Inadmissible,
        owner,
        step_id,
        message,
    )
}

fn finding_class(
    code: &'static str,
    class: avila_core_compiler::FindingClass,
    owner: &str,
    step_id: Option<&str>,
    message: String,
) -> RunFinding {
    let mut finding = RunFinding::runtime(
        code,
        class,
        RunStage::ExecutionPlanning,
        owner,
        SourceLocation::new("capability_selection", "/selections"),
        message,
    );
    finding.step_id = step_id.map(str::to_string);
    finding
}
