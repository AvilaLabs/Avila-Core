//! The case-specific adapter for Aftermatter's `evaluate` command.
//!
//! It knows exactly one thing about Aftermatter: how the compiled
//! `aftermatter.activated-metal-disposition@1` step's input slots map onto the
//! command line, which file the program writes, and where in that document
//! the Class A mixture fractions and their recorded numeric error bounds
//! live. It does not interpret the result scientifically; the interval it
//! extracts is Aftermatter's own recorded decimal-floor and rounding bound.

use std::collections::BTreeMap;
use std::time::Duration;

use avila_core_kernel::{ExactNumber, lower_authored_decimal, read_authoritative_decimal};
use serde_json::{Value, json};

use super::claims::canonical_decimal;
use super::{AdapterOutput, ExtractedClaim};

pub const ADAPTER_ID: &str = "avila-labs.aftermatter/evaluate@1";
pub const CAPABILITY_TYPE_ID: &str = "aftermatter.activated-metal-disposition";
pub const CAPABILITY_TYPE_MAJOR: u64 = 1;
pub const INPUT_SLOTS: &[&str] = &[
    "case",
    "inventory",
    "decay-metadata",
    "federal-rulepack",
    "clive-rulepack",
    "wcs-rulepack",
    "sources-manifest",
];
pub const OUTPUT_ID: &str = "route-result";
pub const OUTPUT_PATH: &str = "outputs/route-result.json";
pub const OUTPUT_MEDIA_TYPE: &str = "application/vnd.aftermatter.route-result+json";
pub const OUTPUTS: &[AdapterOutput] = &[AdapterOutput {
    output_id: OUTPUT_ID,
    workspace_path: OUTPUT_PATH,
    media_type: OUTPUT_MEDIA_TYPE,
}];
pub const OUTPUT_SLOTS: &[&str] = &[
    "table-1-class-a-fraction",
    "table-2-class-a-fraction",
    "route-result",
];
pub const TIMEOUT: Duration = Duration::from_secs(600);
pub const CHECKPOINT_PARAMETER: &str = "checkpoint_id";
pub const ROUTE_RESULT_SCHEMA: &str = "aftermatter-route-result-2";
const FRACTION_UNIT: &str = "1";

/// The portable argument list. Rulepacks are passed in the order Aftermatter's
/// frozen R0 request uses, because the program records its inputs in argument
/// order and that order is part of the output document's identity.
pub fn arguments(staged: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
    for slot in staged.keys() {
        if !INPUT_SLOTS.contains(&slot.as_str()) {
            return Err(format!(
                "input slot `{slot}` is not accepted by adapter {ADAPTER_ID}"
            ));
        }
    }
    let path = |slot: &str| {
        staged
            .get(slot)
            .cloned()
            .ok_or_else(|| format!("input slot `{slot}` is not staged"))
    };
    Ok(vec![
        "evaluate".into(),
        "--root".into(),
        ".".into(),
        "--case".into(),
        path("case")?,
        "--inventory".into(),
        path("inventory")?,
        "--decay".into(),
        path("decay-metadata")?,
        "--rulepack".into(),
        path("federal-rulepack")?,
        "--rulepack".into(),
        path("clive-rulepack")?,
        "--rulepack".into(),
        path("wcs-rulepack")?,
        "--sources-manifest".into(),
        path("sources-manifest")?,
        "--output".into(),
        OUTPUT_PATH.into(),
    ])
}

/// Extract the three output claims from the produced route result: two
/// bounded Class A mixture fractions at the compiled checkpoint, each an
/// interval `[fraction - error_bound, fraction + error_bound]` with the
/// fraction as nominal, and the whole document as an unquantified artifact.
pub fn extract_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    parameters: &BTreeMap<String, Value>,
) -> Result<Vec<ExtractedClaim>, String> {
    let checkpoint_id = parameters
        .get(CHECKPOINT_PARAMETER)
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!("compiled step carries no text parameter `{CHECKPOINT_PARAMETER}`")
        })?;
    let bytes = outputs
        .get(OUTPUT_ID)
        .ok_or_else(|| format!("output `{OUTPUT_ID}` was not collected"))?;
    let document: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("route result is not valid JSON: {error}"))?;
    if document.get("schema").and_then(Value::as_str) != Some(ROUTE_RESULT_SCHEMA) {
        return Err(format!(
            "route result does not declare schema `{ROUTE_RESULT_SCHEMA}`"
        ));
    }
    let checkpoint = document
        .get("checkpoints")
        .and_then(Value::as_array)
        .and_then(|checkpoints| {
            checkpoints.iter().find(|checkpoint| {
                checkpoint.get("checkpoint_id").and_then(Value::as_str) == Some(checkpoint_id)
            })
        })
        .ok_or_else(|| format!("route result has no checkpoint `{checkpoint_id}`"))?;

    let mut claims = Vec::with_capacity(OUTPUT_SLOTS.len());
    for (slot, table) in [
        ("table-1-class-a-fraction", "table_1"),
        ("table-2-class-a-fraction", "table_2"),
    ] {
        let boundaries = checkpoint
            .pointer(&format!("/classification/{table}/boundaries"))
            .and_then(Value::as_array)
            .ok_or_else(|| format!("checkpoint `{checkpoint_id}` has no `{table}` boundaries"))?;
        let class_a: Vec<&Value> = boundaries
            .iter()
            .filter(|boundary| {
                boundary.get("class_if_qualifies").and_then(Value::as_str) == Some("A")
            })
            .collect();
        let [boundary] = class_a.as_slice() else {
            return Err(format!(
                "checkpoint `{checkpoint_id}` `{table}` has {} Class A boundaries; exactly one is required",
                class_a.len()
            ));
        };
        let fraction = decimal_field(boundary, "fraction", table)?;
        let error_bound = decimal_field(boundary, "error_bound", table)?;
        if !(error_bound.is_positive() || error_bound.is_zero()) {
            return Err(format!("`{table}` error bound is negative"));
        }
        let lower = fraction
            .checked_sub(&error_bound)
            .map_err(|error| format!("`{table}` lower bound: {error}"))?;
        let upper = fraction
            .checked_add(&error_bound)
            .map_err(|error| format!("`{table}` upper bound: {error}"))?;
        claims.push(ExtractedClaim {
            output_slot: slot.into(),
            output_id: OUTPUT_ID.into(),
            claim: json!({
                "model": "interval",
                "lower": quantity(&lower)?,
                "upper": quantity(&upper)?,
                "nominal": quantity(&fraction)?,
            }),
        });
    }
    claims.push(ExtractedClaim {
        output_slot: "route-result".into(),
        output_id: OUTPUT_ID.into(),
        claim: json!({ "model": "unquantified" }),
    });
    Ok(claims)
}

fn decimal_field(boundary: &Value, field: &str, table: &str) -> Result<ExactNumber, String> {
    let text = boundary
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{table}` Class A boundary has no text `{field}`"))?;
    let lowered = lower_authored_decimal(text)
        .map_err(|error| format!("`{table}` `{field}` `{text}`: {}", error.detail()))?;
    read_authoritative_decimal(&lowered)
        .map_err(|error| format!("`{table}` `{field}` `{text}`: {}", error.detail()))
}

fn quantity(value: &ExactNumber) -> Result<Value, String> {
    Ok(json!({ "value": canonical_decimal(value)?, "unit": FRACTION_UNIT }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameters() -> BTreeMap<String, Value> {
        BTreeMap::from([(
            CHECKPOINT_PARAMETER.to_string(),
            json!({ "type": "text", "value": "cool-50y" }),
        )])
    }

    fn document(fraction_1: &str, bound_1: &str) -> Vec<u8> {
        json!({
            "schema": ROUTE_RESULT_SCHEMA,
            "checkpoints": [
                { "checkpoint_id": "cool-25y", "classification": {} },
                {
                    "checkpoint_id": "cool-50y",
                    "classification": {
                        "table_1": { "boundaries": [
                            { "class_if_qualifies": "A", "fraction": fraction_1, "error_bound": bound_1 },
                            { "class_if_qualifies": "C", "fraction": "0.08", "error_bound": "0.000001" }
                        ] },
                        "table_2": { "boundaries": [
                            { "class_if_qualifies": "A", "fraction": "0.25", "error_bound": "0.00001" }
                        ] }
                    }
                }
            ]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn arguments_follow_the_frozen_request_order() {
        let staged: BTreeMap<String, String> = INPUT_SLOTS
            .iter()
            .map(|slot| ((*slot).to_string(), format!("in/{slot}.json")))
            .collect();
        let arguments = arguments(&staged).unwrap();
        assert_eq!(arguments[0], "evaluate");
        assert_eq!(&arguments[1..3], ["--root", "."]);
        let rulepacks: Vec<&String> = arguments
            .windows(2)
            .filter(|pair| pair[0] == "--rulepack")
            .map(|pair| &pair[1])
            .collect();
        assert_eq!(
            rulepacks,
            [
                "in/federal-rulepack.json",
                "in/clive-rulepack.json",
                "in/wcs-rulepack.json"
            ]
        );
        assert_eq!(arguments.last().unwrap(), OUTPUT_PATH);
        assert!(arguments.iter().all(|arg| !arg.starts_with('/')));
    }

    #[test]
    fn extraction_is_exact_and_ordered() {
        let outputs = BTreeMap::from([(
            OUTPUT_ID.to_string(),
            document(
                "0.817559455198327183456183455",
                "0.0000000000000000000000062816",
            ),
        )]);
        let claims = extract_claims(&outputs, &parameters()).unwrap();
        assert_eq!(claims.len(), 3);
        assert_eq!(claims[0].output_slot, "table-1-class-a-fraction");
        assert_eq!(
            claims[0].claim["lower"]["value"],
            json!("0.8175594551983271834561771734")
        );
        assert_eq!(
            claims[0].claim["upper"]["value"],
            json!("0.8175594551983271834561897366")
        );
        assert_eq!(
            claims[0].claim["nominal"]["value"],
            json!("0.817559455198327183456183455")
        );
        assert_eq!(claims[1].claim["lower"]["value"], json!("0.24999"));
        assert_eq!(claims[1].claim["upper"]["value"], json!("0.25001"));
        assert_eq!(claims[2].claim, json!({ "model": "unquantified" }));
    }

    #[test]
    fn extraction_refuses_ambiguity_and_absence() {
        let missing = BTreeMap::from([(OUTPUT_ID.to_string(), document("0.5", "0"))]);
        let mut parameters = parameters();
        parameters.insert(
            CHECKPOINT_PARAMETER.into(),
            json!({ "type": "text", "value": "cool-100y" }),
        );
        let error = extract_claims(&missing, &parameters).unwrap_err();
        assert!(error.contains("no checkpoint `cool-100y`"), "{error}");

        let negative = BTreeMap::from([(OUTPUT_ID.to_string(), document("0.5", "-0.1"))]);
        let error = extract_claims(&negative, &self::parameters()).unwrap_err();
        assert!(error.contains("negative"), "{error}");

        let error = extract_claims(&BTreeMap::new(), &self::parameters()).unwrap_err();
        assert!(error.contains("was not collected"), "{error}");
    }
}
