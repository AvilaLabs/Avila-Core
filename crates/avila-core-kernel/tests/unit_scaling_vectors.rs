use std::collections::BTreeMap;

use avila_core_kernel::{
    ExactNumber, KindDefinition, KindRegistry, SEMANTIC_PROFILE, UnitDefinition,
};
use serde::Deserialize;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde_json::{Value, json};

const VECTORS: &str = include_str!("../../../fixtures/semantic-core/vectors/unit-scaling.v1.json");

#[derive(Debug, Deserialize)]
struct VectorSet {
    semantic_profile: String,
    kinds: BTreeMap<String, FixtureKind>,
    vectors: Vec<Vector>,
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
    input: VectorInput,
    expected: Value,
}

#[derive(Debug, Deserialize)]
struct VectorInput {
    quantity: FixtureQuantity,
    kind: String,
}

#[derive(Debug, Deserialize)]
struct FixtureQuantity {
    value: String,
    unit: String,
}

#[test]
fn unit_scaling_vectors_are_executable() {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("unit vector file is JSON");
    assert_eq!(set.semantic_profile, SEMANTIC_PROFILE);
    assert_eq!(
        set.vectors.len(),
        10,
        "update the corpus count intentionally"
    );
    let registry = registry_from_fixture(&set.kinds);

    for vector in set.vectors {
        let value = ExactNumber::from_canonical(&vector.input.quantity.value)
            .expect("fixture quantity is canonical");
        let actual = match registry.scale_quantity(
            &vector.input.kind,
            &value,
            &vector.input.quantity.unit,
        ) {
            Ok(quantity) => serde_json::to_value(quantity).unwrap(),
            Err(error) => {
                let mut result = json!({ "error": error.code() });
                if let Some(repair) = error.repair() {
                    result["repair"] = serde_json::to_value(repair).unwrap();
                }
                result
            }
        };
        assert_eq!(actual, vector.expected, "vector {}", vector.id);
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
        .register_conversion(
            "nuclear.absorbed_dose@1",
            "nuclear.dose_equivalent_rate@1",
            "core.convert.absorbed_dose_to_dose_equivalent@1",
        )
        .unwrap();
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
