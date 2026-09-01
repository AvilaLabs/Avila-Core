use avila_core_kernel::{
    ApplicabilityContext, ApplicabilityEvaluator, ExactNumber, KindDefinition, KindRegistry,
    Predicate, SEMANTIC_PROFILE, UnitDefinition,
};
use serde::Deserialize;
use serde_json::{Value, json};

const VECTORS: &str =
    include_str!("../../../fixtures/semantic-core/vectors/scope-predicates.v1.json");

#[derive(Debug, Deserialize)]
struct VectorSet {
    semantic_profile: String,
    vectors: Vec<Vector>,
}

#[derive(Debug, Deserialize)]
struct Vector {
    id: String,
    input: VectorInput,
    expected: Value,
}

#[derive(Debug, Deserialize)]
struct VectorInput {
    predicate: Predicate,
    context: ApplicabilityContext,
}

#[test]
fn scope_predicate_vectors_are_executable() {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("predicate vector file is JSON");
    assert_eq!(set.semantic_profile, SEMANTIC_PROFILE);
    assert_eq!(
        set.vectors.len(),
        19,
        "update the corpus count intentionally"
    );

    let registry = time_registry();
    let mut evaluator = ApplicabilityEvaluator::new(&registry);
    evaluator
        .register_parameter_kind("cooling_time", "core.time.duration@1")
        .unwrap();
    evaluator
        .register_fact_kind("time_step", "core.time.duration@1")
        .unwrap();

    for vector in set.vectors {
        let result = evaluator
            .evaluate(&vector.input.predicate, &vector.input.context)
            .unwrap_or_else(|error| panic!("vector {} failed: {error}", vector.id));
        assert_eq!(
            serde_json::to_value(json!({ "result": result })).unwrap(),
            vector.expected,
            "vector {}",
            vector.id
        );
    }
}

fn time_registry() -> KindRegistry {
    let mut registry = KindRegistry::default();
    let units = [("s", "1"), ("min", "60"), ("h", "3600"), ("d", "86400")]
        .into_iter()
        .map(|(symbol, factor)| {
            UnitDefinition::new(symbol, ExactNumber::from_canonical(factor).unwrap()).unwrap()
        })
        .collect();
    registry
        .insert_kind(
            KindDefinition::new("core.time.duration@1", "s", units).expect("time kind is valid"),
        )
        .unwrap();
    registry
}
