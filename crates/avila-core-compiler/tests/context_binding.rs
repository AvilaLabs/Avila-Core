//! RA-06 boundary tests for the context-bound evaluation path (ADR-0026):
//! forged observations, mixed contexts, stale qualification, tampered
//! derivations, and the derivation bound — each exercised against the
//! public API a client crate sees, with the honest positive counterparts.

use std::fs;
use std::path::{Path, PathBuf};

use avila_core_compiler::{
    ArtifactCheckState, ArtifactObservations, Compilation, ContextError, EvaluationContext,
    EvaluationError, MAX_RULE_APPLICATIONS, VerdictDerivation, compile_documents,
    evaluate_campaign_in_context, explain_derivation_changes, verify_derivation,
};
use serde_json::{Value, json};

fn case_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases/case-000-actinv-aftermatter")
}

fn case_docs() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let root = case_root();
    (
        fs::read(root.join("contract.json")).unwrap(),
        fs::read(root.join("registry.json")).unwrap(),
        fs::read(root.join("claims.json")).unwrap(),
    )
}

fn compiled(contract: &[u8], registry: &[u8]) -> avila_core_compiler::CompiledContract {
    match compile_documents(contract, registry).unwrap() {
        Compilation::Compiled(checked) => checked.contract().clone(),
        Compilation::Rejected(report) => {
            panic!("the case-000 fixture must compile: {:?}", report.findings)
        }
    }
}

/// A claims document derived from case-000 with one mutation applied.
fn mutated_claims(mutate: impl FnOnce(&mut Value)) -> Vec<u8> {
    let (_, _, claims) = case_docs();
    let mut document: Value = serde_json::from_slice(&claims).unwrap();
    mutate(&mut document);
    serde_json::to_vec(&document).unwrap()
}

#[test]
fn binding_a_different_registry_is_a_mixed_context_refusal() {
    let (contract, registry, claims) = case_docs();
    let compiled = compiled(&contract, &registry);

    // A semantically different registry — the snapshot was built against
    // the committed bytes, so these do not bind.
    let mut other: Value = serde_json::from_slice(&registry).unwrap();
    other["revision"] = json!(other["revision"].as_u64().unwrap() + 1);
    let other = serde_json::to_vec(&other).unwrap();

    match EvaluationContext::bind(&compiled, &other, &claims, ArtifactObservations::none()) {
        Err(ContextError::RegistryMismatch { expected, found }) => {
            assert_ne!(expected, found, "a mixed context must name both identities");
        }
        other => panic!("expected ContextError::RegistryMismatch, got {other:?}"),
    }

    // Whitespace alone changes nothing: canonicalization makes the same
    // document the same identity, so the honest edit path stays open.
    let spaced = format!("  {}", String::from_utf8(registry.clone()).unwrap());
    EvaluationContext::bind(
        &compiled,
        spaced.as_bytes(),
        &claims,
        ArtifactObservations::none(),
    )
    .expect("canonical-equal registry bytes bind the same context");
}

#[test]
fn admissions_minted_under_another_context_are_refused() {
    let (contract, registry, claims) = case_docs();
    let compiled = compiled(&contract, &registry);

    let here = EvaluationContext::bind(&compiled, &registry, &claims, ArtifactObservations::none())
        .unwrap();
    // Observations are part of the context identity: the same documents
    // under a different observation set are a different context.
    let mut observations = ArtifactObservations::none();
    observations.check_bytes(b"an artifact the claims never attest");
    let there = EvaluationContext::bind(&compiled, &registry, &claims, observations).unwrap();
    assert_ne!(
        here.context_sha256(),
        there.context_sha256(),
        "different bound material must give different context identities"
    );

    let (admissions, _) = here.admit();
    match there.derive_verdicts(&admissions) {
        Err(EvaluationError::ContextMismatch { expected, found }) => {
            assert_eq!(expected, there.context_sha256());
            assert_eq!(found, here.context_sha256());
        }
        Ok(_) => panic!("verdicts must not be derived from foreign admissions"),
    }

    // The positive counterpart: same-context admissions derive verdicts.
    let verdicts = here
        .derive_verdicts(&here.admit().0)
        .expect("same-context admissions derive");
    assert_eq!(verdicts.records().len(), 3);
}

#[test]
fn binding_refuses_claims_produced_for_a_different_compilation() {
    // The reviewer's reproduction: claims naming a foreign
    // compiled_snapshot_sha256 reached `bind → admit → derive_verdicts`
    // when the check lived only in the convenience wrapper. The bind
    // boundary itself must refuse — no public path can mint verdicts
    // from a mixed context.
    let (contract, registry, _) = case_docs();
    let compiled = compiled(&contract, &registry);
    let zeroed = mutated_claims(|document| {
        document["compiled_snapshot_sha256"] =
            json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    });

    match EvaluationContext::bind(&compiled, &registry, &zeroed, ArtifactObservations::none()) {
        Err(ContextError::SnapshotMismatch {
            expected,
            found,
            claims_sha256,
        }) => {
            assert_eq!(expected, compiled.snapshot_sha256());
            assert_eq!(
                found,
                "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            );
            assert!(
                claims_sha256.starts_with("sha256:"),
                "the refusal still names what was supplied"
            );
        }
        Ok(_) => panic!("wrong-snapshot claims must never bind a context"),
        Err(other) => panic!("expected SnapshotMismatch, got {other:?}"),
    }

    // The convenience path reports the same refusal, as CORE-E7001.
    let evaluation =
        evaluate_campaign_in_context(&contract, &registry, &zeroed, ArtifactObservations::none())
            .unwrap();
    assert!(
        evaluation.derivation().is_none(),
        "a bind refusal has no bound context to derive under"
    );
    let report = evaluation.report();
    assert_eq!(
        serde_json::to_value(report.status).unwrap(),
        json!("rejected")
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "CORE-E7001"),
        "the refusal is the documented CORE-E7001 finding"
    );

    // Positive counterpart: honest claims still bind and derive.
    let (_, _, claims) = case_docs();
    EvaluationContext::bind(&compiled, &registry, &claims, ArtifactObservations::none())
        .expect("honest claims bind");
}

#[test]
fn verify_derivation_rejects_a_forged_context_block() {
    let (contract, registry, claims) = case_docs();
    let evaluation =
        evaluate_campaign_in_context(&contract, &registry, &claims, ArtifactObservations::none())
            .unwrap();
    let bytes = serde_json::to_vec(evaluation.derivation().unwrap()).unwrap();

    // Forge: change the identities inside the embedded context body and
    // honestly recompute the outer digest. The old verifier compared
    // context_sha256 alone — the embedded body never hashed to it.
    let mut forged: VerdictDerivation = serde_json::from_slice(&bytes).unwrap();
    forged.context.registry_id = "forged-registry".into();
    forged.context.compiled_snapshot_sha256 =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
    forged.derivation_sha256 = forged.recompute_identity().map(Some).unwrap();
    let forged_bytes = serde_json::to_vec(&forged).unwrap();

    let verification = verify_derivation(
        &contract,
        &registry,
        &claims,
        ArtifactObservations::none(),
        &forged_bytes,
    )
    .unwrap();
    assert!(
        !verification.is_verified(),
        "a forged context body must not verify"
    );
    assert!(
        verification.checks.iter().any(|check| {
            check.subject == "context_record"
                && check.state == avila_core_compiler::DerivationCheckState::Mismatch
        }),
        "the embedded context no longer hashes to context_sha256"
    );
    assert!(
        verification.checks.iter().any(|check| {
            check.subject == "bound_material"
                && check.state == avila_core_compiler::DerivationCheckState::Mismatch
        }),
        "the embedded record differs from what the supplied material binds"
    );
}

#[test]
fn verify_derivation_rejects_a_forged_evaluator_and_an_omitted_campaign() {
    let (contract, registry, claims) = case_docs();
    let evaluation =
        evaluate_campaign_in_context(&contract, &registry, &claims, ArtifactObservations::none())
            .unwrap();
    let bytes = serde_json::to_vec(evaluation.derivation().unwrap()).unwrap();

    // An evaluator this verifier does not implement is unsupported
    // material — `not_checked`, never `verified`.
    let mut forged: VerdictDerivation = serde_json::from_slice(&bytes).unwrap();
    forged.evaluator = "avila.core/unknown-evaluator@9.9.9".into();
    forged.derivation_sha256 = forged.recompute_identity().map(Some).unwrap();
    let verification = verify_derivation(
        &contract,
        &registry,
        &claims,
        ArtifactObservations::none(),
        &serde_json::to_vec(&forged).unwrap(),
    )
    .unwrap();
    assert!(!verification.is_verified());
    assert!(
        verification.checks.iter().any(|check| {
            check.subject == "evaluator"
                && check.state == avila_core_compiler::DerivationCheckState::NotChecked
        }),
        "an unknown evaluator is unsupported material"
    );

    // Omitting the campaign identity does not erase the report the replay
    // produces — the omission itself is reported.
    let mut forged: VerdictDerivation = serde_json::from_slice(&bytes).unwrap();
    forged.campaign_sha256 = None;
    forged.derivation_sha256 = forged.recompute_identity().map(Some).unwrap();
    let verification = verify_derivation(
        &contract,
        &registry,
        &claims,
        ArtifactObservations::none(),
        &serde_json::to_vec(&forged).unwrap(),
    )
    .unwrap();
    assert!(!verification.is_verified());
    assert!(
        verification.checks.iter().any(|check| {
            check.check == "campaign"
                && check.state == avila_core_compiler::DerivationCheckState::NotChecked
        }),
        "a dropped campaign identity must not read as verified"
    );
}

#[test]
fn a_supplied_artifact_digest_is_witnessed_not_asserted() {
    let (contract, registry, claims) = case_docs();

    // Forge: supply bytes that hash to nothing any claim attests. The
    // observation set can only ever contain digests it computed — there is
    // no insertion route — so every attested artifact reports not_checked.
    let mut forged = ArtifactObservations::none();
    let forged_digest = forged.check_bytes(b"bytes that are nobody's artifact");
    assert!(!forged.contains("sha256:not-a-real-digest"));

    let evaluation = evaluate_campaign_in_context(&contract, &registry, &claims, forged).unwrap();
    let report = evaluation.report();
    for record in &report.admissions {
        let artifact = record
            .artifact
            .as_ref()
            .expect("observations were supplied");
        assert_eq!(
            artifact.check,
            ArtifactCheckState::NotChecked,
            "{} must not read as verified",
            record.evidence_id
        );
    }
    let context = evaluation.context().expect("a bound context");
    assert!(
        context
            .record()
            .artifact_observations
            .iter()
            .all(|digest| digest == &forged_digest),
        "only the actually-computed digest may appear in the record"
    );

    // Positive counterpart: bytes hashing to an attested identity verify.
    // No fixture artifact bytes exist in the repo, so re-point one
    // attestation at bytes we do have — the observation then verifies the
    // exact digest the claims name.
    let claims_value: Value = serde_json::from_slice(&claims).unwrap();
    let attested_input = claims_value["inputs"][0]["input_id"]
        .as_str()
        .unwrap()
        .to_string();
    let attested = claims_value["inputs"][0]["artifact"]["sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let mut real = ArtifactObservations::none();
    let digest = real.check_bytes(b"the attested artifact bytes");
    let claims = mutated_claims(|document| {
        document["inputs"][0]["artifact"]["sha256"] = json!(digest);
    });
    let evaluation = evaluate_campaign_in_context(&contract, &registry, &claims, real).unwrap();
    assert_ne!(
        attested, digest,
        "the test must actually re-point the attestation"
    );
    let record = evaluation
        .report()
        .admissions
        .iter()
        .find(|record| record.evidence_id == format!("input:{attested_input}"))
        .expect("the re-pointed input has an admission record");
    assert_eq!(
        record.artifact.as_ref().unwrap().check,
        ArtifactCheckState::Verified,
        "bytes hashing to the attested identity verify"
    );
}

#[test]
fn a_stale_qualification_keeps_the_requirement_not_evaluated() {
    let (contract, registry, _) = case_docs();
    let claims = mutated_claims(|document| {
        for claim in document["claims"].as_array_mut().unwrap() {
            if claim["output_slot"] == "table-1-class-a-fraction" {
                claim["qualification"] = json!({
                    "qualification_id": "aftermatter-procedure",
                    "revision": 1,
                    "sha256": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                    "state": "expired",
                    "not_after": "2025-01-01T00:00:00Z",
                    "context": { "facts": {}, "inputs": {} }
                });
            }
        }
    });
    let evaluation =
        evaluate_campaign_in_context(&contract, &registry, &claims, ArtifactObservations::none())
            .unwrap();
    let report = evaluation.report();
    let r1 = report
        .verdicts
        .iter()
        .find(|verdict| verdict.requirement_id == "CASE-000-R1")
        .unwrap();
    assert_eq!(
        serde_json::to_value(r1.verdict.status).unwrap(),
        json!("not_evaluated"),
        "an expired qualification must not evaluate the bounded requirement"
    );
    assert_eq!(r1.verdict.rule, "not_evaluated.qualification_expired");

    // The derivation carries the envelope gate as a replayable application.
    let derivation = evaluation.derivation().unwrap();
    let gate = derivation
        .applications
        .iter()
        .find(|application| application.rule == "qualification.envelope")
        .expect("the expired envelope is a recorded rule application");
    assert_eq!(gate.conclusion, "expired");
}

#[test]
fn an_honest_derivation_replays_and_a_forged_one_does_not() {
    let (contract, registry, claims) = case_docs();
    let evaluation =
        evaluate_campaign_in_context(&contract, &registry, &claims, ArtifactObservations::none())
            .unwrap();
    let derivation = evaluation.derivation().expect("a derivation was recorded");
    let bytes = serde_json::to_vec(derivation).unwrap();

    // The untampered record replays cleanly.
    let verification = verify_derivation(
        &contract,
        &registry,
        &claims,
        ArtifactObservations::none(),
        &bytes,
    )
    .unwrap();
    assert!(
        verification.is_verified(),
        "an honest derivation must verify: {:?}",
        verification
            .checks
            .iter()
            .filter(|c| c.state != avila_core_compiler::DerivationCheckState::Verified)
            .collect::<Vec<_>>()
    );

    // Forge: flip a verdict application's conclusion, then honestly
    // recompute the outer digest — the file is self-consistent, the
    // inference is not. Replay must reject the bad inference, not merely
    // the old digest.
    let mut forged: VerdictDerivation = serde_json::from_slice(&bytes).unwrap();
    let verdict_application = forged
        .applications
        .iter_mut()
        .find(|application| application.subject == "CASE-000-R1")
        .expect("a verdict application for R1");
    verdict_application.conclusion = "fail".into();
    forged.derivation_sha256 = forged.recompute_identity().map(Some).unwrap();
    let forged_bytes = serde_json::to_vec(&forged).unwrap();

    let verification = verify_derivation(
        &contract,
        &registry,
        &claims,
        ArtifactObservations::none(),
        &forged_bytes,
    )
    .unwrap();
    assert!(!verification.is_verified());
    assert!(
        verification.checks.iter().any(|check| {
            check.check == "application"
                && check.subject.contains("CASE-000-R1")
                && check.state == avila_core_compiler::DerivationCheckState::Mismatch
        }),
        "the forged verdict application must mismatch on replay"
    );
    // The forged file's own digest was honest, so identity still verifies —
    // it is the application replay that catches the lie.
    assert!(
        verification
            .checks
            .iter()
            .any(|check| check.check == "identity"
                && check.state == avila_core_compiler::DerivationCheckState::Verified)
    );
}

#[test]
fn the_derivation_bound_is_a_refusal_not_a_truncation() {
    let (contract, registry, _) = case_docs();
    // Every duplicate claim still costs a rule application; enough of them
    // exceed the derivation bound and the evaluation refuses rather than
    // emitting a truncated record.
    let claims = mutated_claims(|document| {
        let claims = document["claims"].as_array().unwrap();
        let template = claims[0].clone();
        let document_claims = document["claims"].as_array_mut().unwrap();
        while document_claims.len() <= MAX_RULE_APPLICATIONS {
            document_claims.push(template.clone());
        }
    });
    let evaluation =
        evaluate_campaign_in_context(&contract, &registry, &claims, ArtifactObservations::none())
            .unwrap();
    let report = evaluation.report();
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "CORE-E7501"),
        "an evaluation over the derivation bound must refuse with CORE-E7501"
    );
    assert_eq!(
        serde_json::to_value(report.status).unwrap(),
        json!("rejected"),
        "the bound refusal is a rejected evaluation, never a partial record"
    );
}

#[test]
fn changed_uses_carry_explanations() {
    let (contract, registry, claims) = case_docs();
    let before_evaluation =
        evaluate_campaign_in_context(&contract, &registry, &claims, ArtifactObservations::none())
            .unwrap();
    let before = before_evaluation.derivation().unwrap().clone();

    // Equal-number premise shift: move an attested input identity — the
    // verdicts may hold but the derivation's premises differ.
    let shifted = mutated_claims(|document| {
        document["inputs"][0]["artifact"]["sha256"] =
            json!("sha256:1111111111111111111111111111111111111111111111111111111111111111");
    });
    let after_evaluation =
        evaluate_campaign_in_context(&contract, &registry, &shifted, ArtifactObservations::none())
            .unwrap();
    let after = after_evaluation.derivation().unwrap().clone();

    let diff = explain_derivation_changes(&before, &after);
    assert!(
        !diff.same_context,
        "different claims mint different contexts"
    );
    assert!(
        !diff.uses.is_empty(),
        "a premise change must be a reported use change"
    );
    assert!(
        diff.explanations
            .iter()
            .all(|explanation| !explanation.is_empty()),
        "every changed use names its reason"
    );
    assert_ne!(
        before.context_sha256, after.context_sha256,
        "same verdicts under different premises stay distinct identities"
    );
}
