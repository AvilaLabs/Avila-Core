use std::collections::BTreeMap;

use avila_core_kernel::{
    Aggregation, ExactNumber, KindDefinition, KindRegistry, SEMANTIC_PROFILE, UnitDefinition,
    VerdictCase, VerdictEvaluator, VerdictStatus, aggregate_verdicts,
};
use serde::Deserialize;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde_json::Value;

const VECTORS: &str =
    include_str!("../../../fixtures/semantic-core/vectors/verdict-calculus.v1.json");

#[derive(Debug, Deserialize)]
struct VectorSet {
    semantic_profile: String,
    kinds: BTreeMap<String, FixtureKind>,
    vectors: Vec<Vector>,
    aggregation_vectors: Vec<AggregationVector>,
}

#[derive(Debug, Deserialize)]
struct FixtureKind {
    canonical_unit: String,
    #[serde(deserialize_with = "ordered_string_map")]
    unit_class: Vec<(String, String)>,
}

#[derive(Debug, Deserialize)]
struct Vector {
    id: String,
    input: VerdictCase,
    expected: Value,
}

#[derive(Debug, Deserialize)]
struct AggregationVector {
    id: String,
    input: AggregationInput,
    expected: Value,
}

#[derive(Debug, Deserialize)]
struct AggregationInput {
    aggregation: Aggregation,
    verdicts: Vec<VerdictStatus>,
}

#[test]
fn verdict_vectors_are_executable() {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("verdict vector file is JSON");
    assert_eq!(set.semantic_profile, SEMANTIC_PROFILE);
    assert_eq!(
        set.vectors.len(),
        41,
        "update the corpus count intentionally"
    );
    assert_eq!(
        set.aggregation_vectors.len(),
        8,
        "update the corpus count intentionally"
    );

    let registry = registry_from_fixture(&set.kinds);
    let evaluator = VerdictEvaluator::new(&registry);
    for vector in set.vectors {
        let actual = evaluator
            .evaluate(&vector.input)
            .unwrap_or_else(|error| panic!("vector {} failed: {error}", vector.id));
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            vector.expected,
            "vector {}",
            vector.id
        );
    }

    for vector in set.aggregation_vectors {
        let status = aggregate_verdicts(vector.input.aggregation, &vector.input.verdicts)
            .unwrap_or_else(|error| panic!("vector {} failed: {error}", vector.id));
        assert_eq!(
            serde_json::to_value(serde_json::json!({ "status": status })).unwrap(),
            vector.expected,
            "vector {}",
            vector.id
        );
    }
}

fn registry_from_fixture(kinds: &BTreeMap<String, FixtureKind>) -> KindRegistry {
    let mut registry = KindRegistry::default();
    for (id, fixture) in kinds {
        let units = fixture
            .unit_class
            .iter()
            .map(|(symbol, factor)| {
                UnitDefinition::new(
                    symbol,
                    ExactNumber::from_canonical(factor).expect("unit factor is canonical"),
                )
                .expect("unit definition is valid")
            })
            .collect();
        registry
            .insert_kind(
                KindDefinition::new(id, &fixture.canonical_unit, units)
                    .expect("kind definition is valid"),
            )
            .expect("fixture kind ids are unique");
    }
    registry
}

fn ordered_string_map<'de, D>(deserializer: D) -> Result<Vec<(String, String)>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OrderedMapVisitor;

    impl<'de> Visitor<'de> for OrderedMapVisitor {
        type Value = Vec<(String, String)>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an ordered string-to-string map")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
            while let Some(entry) = map.next_entry()? {
                entries.push(entry);
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(OrderedMapVisitor)
}
