use avila_core_kernel::{
    CORE_S1102, SEMANTIC_PROFILE, canonicalize_json, lower_authored_decimal,
    read_authoritative_decimal, read_authoritative_json, read_authoritative_rational,
};
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../fixtures/semantic-core/vectors/canon.v1.json");

#[derive(Debug, Deserialize)]
struct VectorSet {
    semantic_profile: String,
    vectors: Vec<Vector>,
}

#[derive(Debug, Deserialize)]
struct Vector {
    id: String,
    operation: String,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    input_bytes: Option<String>,
    #[serde(default)]
    expected: Option<String>,
    #[serde(default)]
    expected_bytes: Option<String>,
    #[serde(default)]
    expected_error: Option<String>,
}

#[test]
fn canonicalization_vectors_are_executable() {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("canonical vector file is JSON");
    assert_eq!(set.semantic_profile, SEMANTIC_PROFILE);
    assert_eq!(
        set.vectors.len(),
        12,
        "update the declared corpus count intentionally"
    );

    for vector in set.vectors {
        let result = execute(&vector);
        match (
            &vector.expected,
            &vector.expected_bytes,
            &vector.expected_error,
        ) {
            (Some(expected), None, None) => assert_eq!(
                result.expect("vector should succeed"),
                *expected,
                "vector {}",
                vector.id
            ),
            (None, Some(expected), None) => assert_eq!(
                result.expect("vector should succeed"),
                *expected,
                "vector {}",
                vector.id
            ),
            (None, None, Some(code)) => assert_eq!(
                result.expect_err("vector should be refused"),
                *code,
                "vector {}",
                vector.id
            ),
            _ => panic!("vector {} has an invalid expectation shape", vector.id),
        }
    }
}

fn execute(vector: &Vector) -> Result<String, &'static str> {
    let input = vector
        .input
        .as_deref()
        .or(vector.input_bytes.as_deref())
        .unwrap();
    match vector.operation.as_str() {
        "canonicalize_json" => canonicalize_json(input.as_bytes())
            .map(|bytes| String::from_utf8(bytes).expect("canonical JSON is UTF-8"))
            .map_err(|error| error.code()),
        "read_authoritative_json" => read_authoritative_json(input.as_bytes())
            .map(|_| input.to_owned())
            .map_err(|error| error.code()),
        "read_authoritative_decimal" => read_authoritative_decimal(input)
            .map(|_| input.to_owned())
            .map_err(|error| error.code()),
        "lower_authored_decimal" => lower_authored_decimal(input).map_err(|error| error.code()),
        "read_authoritative_rational" => read_authoritative_rational(input)
            .map(|value| value.canonical_rational())
            .map_err(|error| error.code()),
        other => panic!("unsupported canonical-vector operation {other}"),
    }
}

#[test]
fn numeric_resource_limits_fail_closed() {
    let exponent = lower_authored_decimal("1e999999").expect_err("huge exponent must fail");
    assert_eq!(exponent.code(), CORE_S1102);
}

#[test]
fn canonical_json_uses_utf16_property_order() {
    // U+1F600 sorts before U+FFFD by UTF-16 code units, despite the scalar values.
    let canonical = canonicalize_json("{\"�\":1,\"😀\":2}".as_bytes()).unwrap();
    assert_eq!(String::from_utf8(canonical).unwrap(), "{\"😀\":2,\"�\":1}");
}

#[test]
fn duplicate_keys_are_refused_at_nested_depth() {
    let error = read_authoritative_json(br#"{"outer":{"same":1,"same":2}}"#).unwrap_err();
    assert_eq!(error.code(), avila_core_kernel::CORE_S1103);
}
