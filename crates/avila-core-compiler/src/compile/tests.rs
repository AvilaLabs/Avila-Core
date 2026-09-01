use super::values::NOT_DEFINED_PLACEHOLDER;
use super::{CompilationStatus, CompileReport, ReviewFulfillment, compile_documents};
use crate::diagnostic::{
    CORE_R3101, CORE_R3102, CORE_R3201, CORE_R3202, CORE_R3203, CORE_R3301, CORE_R3401, CORE_R3501,
    CORE_R3601, CORE_R3602, CORE_S1101, CORE_S1102, CORE_T2001, CORE_T2101, CORE_T2102, CORE_T2103,
    CORE_T2104, CORE_T2201, CORE_T2203, CORE_T2301, CORE_T2601, DiagnosticRepair, FindingClass,
    RepairApplicability,
};
use crate::document::{
    AuthoredBinding, BasisKind, BoundSide, ClaimModelDeclaration, Comparison, ContractSource,
    ContractStatus, DeterminismClass, ParameterType, RegistrySnapshot, RequirementBasis, SourceRef,
    TypedQuantity, VersionedRef,
};
use avila_core_kernel::ExactNumber;
use std::collections::BTreeSet;

const CONTRACT: &[u8] =
    include_bytes!("../../../../fixtures/semantic-core/types/types.R1.resolved.pass.contract.json");
const REGISTRY: &[u8] =
    include_bytes!("../../../../fixtures/semantic-core/types/compiler.registry.v1.json");
const PARAMETER_CONTRACT: &[u8] = include_bytes!(
    "../../../../fixtures/semantic-core/types/types.R7.parameters.pass.contract.json"
);
const PARAMETER_REGISTRY: &[u8] =
    include_bytes!("../../../../fixtures/semantic-core/types/compiler.parameters.registry.v1.json");
const REVIEW_CONTRACT: &[u8] = include_bytes!(
    "../../../../fixtures/semantic-core/types/types.R9.review-bound.pass.contract.json"
);
const REVIEW_REGISTRY: &[u8] =
    include_bytes!("../../../../fixtures/semantic-core/types/compiler.review.registry.v1.json");
const PURPOSE_CONTRACT: &[u8] =
    include_bytes!("../../../../fixtures/semantic-core/types/types.R10.allowed.pass.contract.json");
const PURPOSE_REGISTRY: &[u8] =
    include_bytes!("../../../../fixtures/semantic-core/types/compiler.purpose.registry.v1.json");

fn contract() -> ContractSource {
    serde_json::from_slice(CONTRACT).unwrap()
}

fn registry() -> RegistrySnapshot {
    serde_json::from_slice(REGISTRY).unwrap()
}

fn parameter_contract() -> ContractSource {
    serde_json::from_slice(PARAMETER_CONTRACT).unwrap()
}

fn parameter_registry() -> RegistrySnapshot {
    serde_json::from_slice(PARAMETER_REGISTRY).unwrap()
}

fn review_contract() -> ContractSource {
    serde_json::from_slice(REVIEW_CONTRACT).unwrap()
}

fn review_registry() -> RegistrySnapshot {
    serde_json::from_slice(REVIEW_REGISTRY).unwrap()
}

fn purpose_contract() -> ContractSource {
    serde_json::from_slice(PURPOSE_CONTRACT).unwrap()
}

fn purpose_registry() -> RegistrySnapshot {
    serde_json::from_slice(PURPOSE_REGISTRY).unwrap()
}

fn compile_contract(contract: &ContractSource) -> CompileReport {
    compile_documents(&serde_json::to_vec(contract).unwrap(), REGISTRY).unwrap()
}

fn compile_with_registry(contract: &ContractSource, registry: &RegistrySnapshot) -> CompileReport {
    compile_documents(
        &serde_json::to_vec(contract).unwrap(),
        &serde_json::to_vec(registry).unwrap(),
    )
    .unwrap()
}

fn codes(report: &CompileReport) -> BTreeSet<&str> {
    report
        .findings
        .iter()
        .map(|finding| finding.code.as_str())
        .collect()
}

#[test]
fn resolved_fixture_compiles_deterministically() {
    let first = compile_documents(CONTRACT, REGISTRY).unwrap();
    let second = compile_documents(CONTRACT, REGISTRY).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.status, CompilationStatus::Compiled);
    assert!(first.findings.is_empty());
    let compiled = first.compiled.unwrap();
    assert_eq!(compiled.workflow.len(), 2);
    assert_eq!(compiled.workflow[1].bindings.len(), 1);
    assert_eq!(compiled.requirements[0].limit.value, "1/36000000");
    assert_eq!(compiled.requirements[0].limit.unit, "Sv/s");
    assert_eq!(
        compiled.snapshot_sha256,
        "sha256:7a6968cc0e3cf02ea2b7835a500418813175b17a383767481e2758376199b775"
    );
}

#[test]
fn authoritative_failures_become_findings() {
    let report = compile_documents(br#"{"value":1.0}"#, REGISTRY).unwrap();
    assert_eq!(report.status, CompilationStatus::Rejected);
    assert_eq!(report.findings[0].code, CORE_S1102);
    assert_eq!(report.findings[0].primary.pointer, "/value");
    assert!(report.compiled.is_none());
}

#[test]
fn source_layer_findings_name_the_offending_value() {
    let mut float_parameter: serde_json::Value = serde_json::from_slice(CONTRACT).unwrap();
    float_parameter["workflow"][0]["parameters"]["x"] = serde_json::json!(1.5);
    let report =
        compile_documents(&serde_json::to_vec(&float_parameter).unwrap(), REGISTRY).unwrap();
    assert_eq!(report.findings[0].code, CORE_S1102);
    assert_eq!(
        report.findings[0].primary.pointer,
        "/workflow/0/parameters/x"
    );

    let mut unknown_field: serde_json::Value = serde_json::from_slice(CONTRACT).unwrap();
    unknown_field["workflow"][1]["bogus_field"] = serde_json::json!(1);
    let report = compile_documents(&serde_json::to_vec(&unknown_field).unwrap(), REGISTRY).unwrap();
    assert_eq!(report.findings[0].code, CORE_S1101);
    assert_eq!(
        report.findings[0].primary.pointer,
        "/workflow/1/bogus_field"
    );

    let mut wrong_variant: serde_json::Value = serde_json::from_slice(CONTRACT).unwrap();
    wrong_variant["requirements"][0]["comparison"] = serde_json::json!("lessthan");
    let report = compile_documents(&serde_json::to_vec(&wrong_variant).unwrap(), REGISTRY).unwrap();
    assert_eq!(report.findings[0].code, CORE_S1102);
    assert_eq!(
        report.findings[0].primary.pointer,
        "/requirements/0/comparison"
    );
    assert!(report.findings[0].repairs.is_empty());

    let mut noncanonical: serde_json::Value = serde_json::from_slice(CONTRACT).unwrap();
    noncanonical["requirements"][0]["limit"]["value"] = serde_json::json!("100.0");
    let report = compile_documents(&serde_json::to_vec(&noncanonical).unwrap(), REGISTRY).unwrap();
    assert_eq!(report.findings[0].code, CORE_S1102);
    assert_eq!(
        report.findings[0].primary.pointer,
        "/requirements/0/limit/value"
    );
    assert_eq!(
        report.findings[0].repairs,
        vec![DiagnosticRepair {
            applicability: RepairApplicability::MechanicallySafe,
            candidates: vec!["100".into()],
        }]
    );
}

#[test]
fn invalid_parameter_declarations_fail_as_registry_findings() {
    let source = parameter_contract();

    let mut empty_integer_domain = parameter_registry();
    let ParameterType::Integer { min: Some(min), .. } =
        &mut empty_integer_domain.capability_types[0].parameters[1].value_type
    else {
        panic!("fixture parameter must be an integer");
    };
    min.value = 101;
    assert!(codes(&compile_with_registry(&source, &empty_integer_domain)).contains(CORE_R3501));

    let mut reserved_choice = parameter_registry();
    let ParameterType::Text {
        allowed_values: Some(values),
    } = &mut reserved_choice.capability_types[0].parameters[3].value_type
    else {
        panic!("fixture parameter must be text");
    };
    values.push(NOT_DEFINED_PLACEHOLDER.into());
    assert!(codes(&compile_with_registry(&source, &reserved_choice)).contains(CORE_R3501));

    let mut unknown_kind = parameter_registry();
    let ParameterType::Quantity { kind, .. } =
        &mut unknown_kind.capability_types[0].parameters[0].value_type
    else {
        panic!("fixture parameter must be a quantity");
    };
    *kind = "fixture.unknown_kind".into();
    assert!(codes(&compile_with_registry(&source, &unknown_kind)).contains(CORE_R3501));
}

#[test]
fn required_slots_fail_closed_when_unresolved_or_ambiguous() {
    let mut unresolved = contract();
    unresolved.inputs.clear();
    let report = compile_contract(&unresolved);
    assert!(codes(&report).contains(CORE_R3101));
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.code == CORE_R3101)
        .unwrap();
    assert_eq!(
        finding.repairs,
        vec![DiagnosticRepair {
            applicability: RepairApplicability::ConstrainedChoice,
            candidates: vec!["declare_input:fixture.source_document@1".into()],
        }]
    );

    let mut no_producer = contract();
    no_producer.workflow.remove(0);
    let report = compile_contract(&no_producer);
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.code == CORE_R3101)
        .unwrap();
    assert_eq!(finding.primary.pointer, "/workflow/0/inputs/dose_rate");
    assert_eq!(
        finding.repairs[0].candidates,
        vec![
            "declare_input:nuclear.shutdown_dose_rate@1".to_owned(),
            "add_step:fixture.bound_dose@1/bounded_dose_rate".to_owned(),
            "add_step:fixture.calculate_dose@1/dose_rate".to_owned(),
        ]
    );

    let mut ambiguous = contract();
    let mut second = ambiguous.inputs[0].clone();
    second.input_id = "second-case".into();
    ambiguous.inputs.push(second);
    let report = compile_contract(&ambiguous);
    assert!(codes(&report).contains(CORE_R3102));
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.code == CORE_R3102)
        .unwrap();
    assert_eq!(
        finding.repairs[0].applicability,
        RepairApplicability::ConstrainedChoice
    );
    assert_eq!(finding.repairs[0].candidates.len(), 2);
}

#[test]
fn review_obligation_never_becomes_a_technical_verdict() {
    let report = compile_documents(REVIEW_CONTRACT, REVIEW_REGISTRY).unwrap();
    assert_eq!(report.status, CompilationStatus::Compiled);
    let compiled = report.compiled.unwrap();
    let review = compiled
        .workflow
        .iter()
        .find(|step| step.step_id == "review")
        .and_then(|step| step.review_obligation.as_ref())
        .unwrap();
    assert_eq!(review.fulfillment, ReviewFulfillment::PendingExternalReview);
    assert_eq!(review.presented_evidence[0].input_slot, "trace");
    assert_eq!(review.presented_evidence[1].input_slot, "result");
    assert!(compiled.requirements.iter().all(|requirement| {
        !matches!(
            &requirement.metric,
            SourceRef::StepOutput { step_id, .. } if step_id == "review"
        )
    }));
}

#[test]
fn incomplete_review_type_declarations_fail_closed() {
    let source = review_contract();

    let mut deterministic = review_registry();
    deterministic.capability_types[1]
        .reproducibility
        .determinism = DeterminismClass::Deterministic;
    assert!(codes(&compile_with_registry(&source, &deterministic)).contains(CORE_R3401));

    let mut hidden_input = review_registry();
    hidden_input.capability_types[1]
        .review
        .as_mut()
        .unwrap()
        .presented_input_slots
        .pop();
    assert!(codes(&compile_with_registry(&source, &hidden_input)).contains(CORE_R3401));

    let mut optional_input = review_registry();
    optional_input.capability_types[1].inputs[0].required = false;
    assert!(codes(&compile_with_registry(&source, &optional_input)).contains(CORE_R3401));

    let mut technical_decision = review_registry();
    technical_decision.capability_types[1].outputs[0].permitted_claim_models =
        vec![ClaimModelDeclaration::Interval { nominal: false }];
    assert!(codes(&compile_with_registry(&source, &technical_decision)).contains(CORE_R3401));

    let mut quantitative_decision = review_registry();
    quantitative_decision.capability_types[1].outputs[0].role =
        quantitative_decision.roles[0].role.clone();
    quantitative_decision.capability_types[1].outputs[0].media_type =
        "application/vnd.fixture.quantity+json".into();
    assert!(codes(&compile_with_registry(&source, &quantitative_decision)).contains(CORE_R3401));

    let mut duplicate_disposition = review_registry();
    let declaration = duplicate_disposition.capability_types[1]
        .review
        .as_mut()
        .unwrap();
    declaration
        .allowed_dispositions
        .push(declaration.allowed_dispositions[0]);
    assert!(codes(&compile_with_registry(&source, &duplicate_disposition)).contains(CORE_R3401));
}

#[test]
fn purpose_exclusions_are_nominal_and_fail_closed() {
    let allowed = compile_documents(PURPOSE_CONTRACT, PURPOSE_REGISTRY).unwrap();
    assert_eq!(allowed.status, CompilationStatus::Compiled);
    assert_eq!(
        allowed.compiled.unwrap().requirements[0].purpose,
        VersionedRef {
            id: "fixture.design_compliance".into(),
            major: 1,
        }
    );

    let mut excluded = purpose_contract();
    excluded.requirements[0].purpose = VersionedRef {
        id: "fixture.screening".into(),
        major: 1,
    };
    assert!(codes(&compile_with_registry(&excluded, &purpose_registry())).contains(CORE_T2601));

    let mut similar_name = purpose_contract();
    similar_name.requirements[0].purpose = VersionedRef {
        id: "fixture.screening_research".into(),
        major: 1,
    };
    assert_eq!(
        compile_with_registry(&similar_name, &purpose_registry()).status,
        CompilationStatus::Compiled
    );

    let mut unknown = purpose_contract();
    unknown.requirements[0].purpose = VersionedRef {
        id: "fixture.unknown".into(),
        major: 1,
    };
    assert!(codes(&compile_with_registry(&unknown, &purpose_registry())).contains(CORE_T2601));
}

#[test]
fn invalid_purpose_vocabularies_and_exclusions_are_registry_findings() {
    let source = purpose_contract();

    let mut duplicate_purpose = purpose_registry();
    duplicate_purpose
        .purposes
        .push(duplicate_purpose.purposes[0].clone());
    assert!(codes(&compile_with_registry(&source, &duplicate_purpose)).contains(CORE_R3501));

    let mut duplicate_exclusion = purpose_registry();
    let repeated = duplicate_exclusion.capability_types[0].outputs[0].excluded_purposes[0].clone();
    duplicate_exclusion.capability_types[0].outputs[0]
        .excluded_purposes
        .push(repeated);
    assert!(codes(&compile_with_registry(&source, &duplicate_exclusion)).contains(CORE_R3501));

    let mut unknown_exclusion = purpose_registry();
    unknown_exclusion.capability_types[0].outputs[0].excluded_purposes[0] = VersionedRef {
        id: "fixture.unknown".into(),
        major: 1,
    };
    assert!(codes(&compile_with_registry(&source, &unknown_exclusion)).contains(CORE_R3501));
}

#[test]
fn explicit_binding_resolves_ambiguity_without_mutating_source() {
    let mut source = contract();
    let mut second = source.inputs[0].clone();
    second.input_id = "second-case".into();
    source.inputs.push(second);
    source.workflow[0].bindings.push(AuthoredBinding {
        input_slot: "source".into(),
        source: SourceRef::ContractInput {
            input_id: "case".into(),
        },
    });

    let report = compile_contract(&source);
    assert_eq!(report.status, CompilationStatus::Compiled);
    let compiled = report.compiled.unwrap();
    assert_eq!(
        compiled.workflow[0].bindings[0].source,
        SourceRef::ContractInput {
            input_id: "case".into()
        }
    );
    assert_eq!(source.inputs.len(), 2);
}

#[test]
fn explicit_binding_checks_nominal_role_and_media_independently() {
    let mut role_mismatch = contract();
    role_mismatch.inputs[0].role = VersionedRef {
        id: "fixture.alternate_source_document".into(),
        major: 1,
    };
    role_mismatch.workflow[0].bindings.push(AuthoredBinding {
        input_slot: "source".into(),
        source: SourceRef::ContractInput {
            input_id: "case".into(),
        },
    });
    let report = compile_contract(&role_mismatch);
    assert!(codes(&report).contains(CORE_T2101));
    assert!(!codes(&report).contains(CORE_T2301));

    let mut media_mismatch = contract();
    media_mismatch.inputs[0].media_type = "application/octet-stream".into();
    media_mismatch.workflow[0].bindings.push(AuthoredBinding {
        input_slot: "source".into(),
        source: SourceRef::ContractInput {
            input_id: "case".into(),
        },
    });
    let report = compile_contract(&media_mismatch);
    assert!(codes(&report).contains(CORE_T2301));
    assert!(!codes(&report).contains(CORE_T2101));
    let media_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.code == CORE_T2301)
        .collect();
    assert_eq!(media_findings.len(), 1);
    assert_eq!(media_findings[0].primary.pointer, "/inputs/0/media_type");
}

#[test]
fn graph_shape_rejects_self_unknown_and_cyclic_dependencies() {
    let mut self_dependency = contract();
    self_dependency.workflow[1].bindings.push(AuthoredBinding {
        input_slot: "dose_rate".into(),
        source: SourceRef::StepOutput {
            step_id: "bound".into(),
            output_slot: "bounded_dose_rate".into(),
        },
    });
    assert!(codes(&compile_contract(&self_dependency)).contains(CORE_R3201));

    let mut unknown = contract();
    unknown.workflow[1].bindings.push(AuthoredBinding {
        input_slot: "dose_rate".into(),
        source: SourceRef::StepOutput {
            step_id: "missing".into(),
            output_slot: "bounded_dose_rate".into(),
        },
    });
    assert!(codes(&compile_contract(&unknown)).contains(CORE_R3203));

    let mut cycle = contract();
    cycle.inputs.clear();
    cycle.workflow = vec![cycle.workflow[1].clone(), cycle.workflow[1].clone()];
    cycle.workflow[0].step_id = "left".into();
    cycle.workflow[1].step_id = "right".into();
    cycle.workflow[0].bindings = vec![AuthoredBinding {
        input_slot: "dose_rate".into(),
        source: SourceRef::StepOutput {
            step_id: "right".into(),
            output_slot: "bounded_dose_rate".into(),
        },
    }];
    cycle.workflow[1].bindings = vec![AuthoredBinding {
        input_slot: "dose_rate".into(),
        source: SourceRef::StepOutput {
            step_id: "left".into(),
            output_slot: "bounded_dose_rate".into(),
        },
    }];
    cycle.requirements[0].metric = Some(SourceRef::StepOutput {
        step_id: "right".into(),
        output_slot: "bounded_dose_rate".into(),
    });
    assert!(codes(&compile_contract(&cycle)).contains(CORE_R3202));
}

#[test]
fn unknown_capability_types_are_reported_once_without_downstream_cascade() {
    let mut source = contract();
    source.workflow[0].capability_type.id = "fixture.does_not_exist".into();
    source.workflow[1].bindings.push(AuthoredBinding {
        input_slot: "dose_rate".into(),
        source: SourceRef::StepOutput {
            step_id: "calculate".into(),
            output_slot: "dose_rate".into(),
        },
    });
    let report = compile_contract(&source);
    assert_eq!(report.status, CompilationStatus::Rejected);
    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    assert_eq!(report.findings[0].code, CORE_R3101);
    assert_eq!(
        report.findings[0].primary.pointer,
        "/workflow/0/capability_type"
    );

    let mut metric_on_unknown = contract();
    metric_on_unknown.workflow[1].capability_type.id = "fixture.does_not_exist".into();
    let report = compile_contract(&metric_on_unknown);
    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    assert_eq!(report.findings[0].code, CORE_R3101);

    let mut wrong_slot = contract();
    wrong_slot.workflow[1].bindings.push(AuthoredBinding {
        input_slot: "dose_rate".into(),
        source: SourceRef::StepOutput {
            step_id: "calculate".into(),
            output_slot: "nonexistent".into(),
        },
    });
    let report = compile_contract(&wrong_slot);
    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    assert_eq!(report.findings[0].code, CORE_R3203);
    assert!(
        report.findings[0]
            .message
            .contains("declares no output slot")
    );
}

#[test]
fn unconsumed_declarations_are_notices_that_never_block() {
    let mut unused_input = contract();
    let mut spare = unused_input.inputs[0].clone();
    spare.input_id = "spare".into();
    spare.role = VersionedRef {
        id: "fixture.alternate_source_document".into(),
        major: 1,
    };
    unused_input.inputs.push(spare);
    let report = compile_contract(&unused_input);
    assert_eq!(report.status, CompilationStatus::Compiled);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].code, CORE_R3601);
    assert_eq!(report.findings[0].class, FindingClass::Notice);
    assert_eq!(report.findings[0].primary.pointer, "/inputs/1");
    assert!(!report.findings[0].blocks_compilation());

    let mut unconsumed_step = contract();
    let mut spare = unconsumed_step.workflow[0].clone();
    spare.step_id = "spare".into();
    unconsumed_step.workflow.push(spare);
    unconsumed_step.workflow[1].bindings.push(AuthoredBinding {
        input_slot: "dose_rate".into(),
        source: SourceRef::StepOutput {
            step_id: "calculate".into(),
            output_slot: "dose_rate".into(),
        },
    });
    let report = compile_contract(&unconsumed_step);
    assert_eq!(report.status, CompilationStatus::Compiled);
    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    assert_eq!(report.findings[0].code, CORE_R3602);
    assert_eq!(report.findings[0].primary.pointer, "/workflow/2");

    let mut blocked = unused_input;
    blocked.requirements[0].metric = None;
    let report = compile_contract(&blocked);
    assert_eq!(codes(&report), BTreeSet::from([CORE_R3301]));
}

#[test]
fn requirements_need_a_metric_and_exactly_compatible_limit() {
    let mut unbound = contract();
    unbound.requirements[0].metric = None;
    assert!(codes(&compile_contract(&unbound)).contains(CORE_R3301));

    let mut wrong_kind = contract();
    wrong_kind.requirements[0].limit.kind = "nuclear.absorbed_dose_rate".into();
    assert!(codes(&compile_contract(&wrong_kind)).contains(CORE_T2102));

    let mut wrong_unit_class = contract();
    wrong_unit_class.requirements[0].limit.unit = "Gy/s".into();
    assert!(codes(&compile_contract(&wrong_unit_class)).contains(CORE_T2103));

    let mut unknown_unit = contract();
    unknown_unit.requirements[0].limit.unit = "Mpa".into();
    assert!(codes(&compile_contract(&unknown_unit)).contains(CORE_T2001));
}

#[test]
fn equality_requires_a_nonnegative_tolerance_of_the_metric_kind() {
    let tolerance = |value: &str, kind: &str, unit: &str| TypedQuantity {
        kind: kind.into(),
        value: ExactNumber::from_canonical(value).unwrap(),
        unit: unit.into(),
    };
    let metric_kind = "nuclear.dose_equivalent_rate";

    let mut missing = contract();
    missing.requirements[0].comparison = Comparison::Equal;
    let report = compile_contract(&missing);
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.code == CORE_T2104)
        .unwrap();
    assert_eq!(finding.class, FindingClass::Missing);
    assert_eq!(finding.primary.pointer, "/requirements/0/tolerance");

    let mut misplaced = contract();
    misplaced.requirements[0].tolerance = Some(tolerance("1", metric_kind, "uSv/h"));
    let report = compile_contract(&misplaced);
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.code == CORE_T2104)
        .unwrap();
    assert_eq!(finding.class, FindingClass::Invalid);

    let mut negative = contract();
    negative.requirements[0].comparison = Comparison::Equal;
    negative.requirements[0].tolerance = Some(tolerance("-1", metric_kind, "uSv/h"));
    let report = compile_contract(&negative);
    assert!(report.findings.iter().any(|finding| {
        finding.code == CORE_S1102 && finding.primary.pointer == "/requirements/0/tolerance/value"
    }));

    let mut wrong_kind = contract();
    wrong_kind.requirements[0].comparison = Comparison::Equal;
    wrong_kind.requirements[0].tolerance =
        Some(tolerance("1", "nuclear.absorbed_dose_rate", "Gy/s"));
    let report = compile_contract(&wrong_kind);
    assert!(report.findings.iter().any(|finding| {
        finding.code == CORE_T2102 && finding.primary.pointer == "/requirements/0/tolerance/kind"
    }));

    let mut wrong_unit = contract();
    wrong_unit.requirements[0].comparison = Comparison::Equal;
    wrong_unit.requirements[0].tolerance = Some(tolerance("1", metric_kind, "rem/h"));
    let report = compile_contract(&wrong_unit);
    assert!(report.findings.iter().any(|finding| {
        finding.code == CORE_T2001 && finding.primary.pointer == "/requirements/0/tolerance/unit"
    }));

    let mut valid = contract();
    valid.requirements[0].comparison = Comparison::Equal;
    valid.requirements[0].tolerance = Some(tolerance("1", metric_kind, "uSv/h"));
    let report = compile_contract(&valid);
    assert_eq!(report.status, CompilationStatus::Compiled);
    let compiled = report.compiled.unwrap();
    let tolerance = compiled.requirements[0].tolerance.as_ref().unwrap();
    assert_eq!(tolerance.value, "1/3600000000");
    assert_eq!(tolerance.unit, "Sv/s");
}

#[test]
fn coverage_must_be_a_canonical_decimal_in_the_unit_interval_on_a_bounded_basis() {
    let with_coverage = |kind: BasisKind, coverage: &str| {
        let mut source = contract();
        source.requirements[0].basis = RequirementBasis {
            kind,
            coverage: Some(coverage.into()),
        };
        compile_contract(&source)
    };
    let coverage_finding = |report: &CompileReport| {
        report
            .findings
            .iter()
            .find(|finding| finding.primary.pointer == "/requirements/0/basis/coverage")
            .cloned()
    };

    for out_of_range in ["1.2", "0", "-0.5", "abc"] {
        let report = with_coverage(BasisKind::Bounded, out_of_range);
        let finding = coverage_finding(&report).unwrap_or_else(|| panic!("{out_of_range}"));
        assert_eq!(finding.code, CORE_S1102, "{out_of_range}");
        assert!(finding.repairs.is_empty(), "{out_of_range}");
    }

    let noncanonical = with_coverage(BasisKind::Bounded, "0.950");
    let finding = coverage_finding(&noncanonical).unwrap();
    assert_eq!(finding.code, CORE_S1102);
    assert_eq!(
        finding.repairs,
        vec![DiagnosticRepair {
            applicability: RepairApplicability::MechanicallySafe,
            candidates: vec!["0.95".into()],
        }]
    );

    let misplaced = with_coverage(BasisKind::Enclosure, "0.95");
    assert_eq!(coverage_finding(&misplaced).unwrap().code, CORE_S1102);

    for valid in ["0.95", "1"] {
        let report = with_coverage(BasisKind::Bounded, valid);
        assert_eq!(report.status, CompilationStatus::Compiled, "{valid}");
        assert_eq!(
            report.compiled.unwrap().requirements[0]
                .basis
                .coverage
                .as_deref(),
            Some(valid)
        );
    }
}

#[test]
fn type_level_claim_models_must_satisfy_the_comparison_basis() {
    let source = contract();

    let mut irreducible = registry();
    irreducible.capability_types[1].outputs[0].permitted_claim_models =
        vec![ClaimModelDeclaration::StandardUncertainty];
    let report = compile_with_registry(&source, &irreducible);
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.code == CORE_T2203)
        .unwrap();
    assert_eq!(finding.class, FindingClass::Unsatisfied);
    assert_eq!(
        finding.repairs[0].applicability,
        RepairApplicability::MethodOwnerJudgment
    );

    let mut wrong_side = registry();
    wrong_side.capability_types[1].outputs[0].permitted_claim_models =
        vec![ClaimModelDeclaration::WorstCase {
            side: BoundSide::Lower,
            nominal: false,
        }];
    assert!(codes(&compile_with_registry(&source, &wrong_side)).contains(CORE_T2201));

    let mut sufficient_side = registry();
    sufficient_side.capability_types[1].outputs[0].permitted_claim_models =
        vec![ClaimModelDeclaration::WorstCase {
            side: BoundSide::Upper,
            nominal: false,
        }];
    assert_eq!(
        compile_with_registry(&source, &sufficient_side).status,
        CompilationStatus::Compiled
    );

    let mut coverage_for_enclosure = registry();
    coverage_for_enclosure.capability_types[1].outputs[0].permitted_claim_models =
        vec![ClaimModelDeclaration::CoverageInterval];
    let mut enclosure_contract = source;
    enclosure_contract.requirements[0].basis.kind = BasisKind::Enclosure;
    assert!(
        codes(&compile_with_registry(
            &enclosure_contract,
            &coverage_for_enclosure
        ))
        .contains(CORE_T2201)
    );
}

#[test]
fn independent_root_findings_are_reported_in_one_pass() {
    let mut source = contract();
    source.status = ContractStatus::Approved;
    source.inputs.clear();
    source.requirements[0].metric = None;
    source.workflow[1].bindings.push(AuthoredBinding {
        input_slot: "dose_rate".into(),
        source: SourceRef::StepOutput {
            step_id: "missing".into(),
            output_slot: "bounded_dose_rate".into(),
        },
    });
    let report = compile_contract(&source);
    let actual = codes(&report);
    assert!(actual.contains(CORE_R3101));
    assert!(actual.contains(CORE_R3203));
    assert!(actual.contains(CORE_R3301));
}
