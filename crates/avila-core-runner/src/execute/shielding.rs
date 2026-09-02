//! Case-specific adapters for the shielding configuration search: a fast
//! one-dimensional attenuation screen and a Monte Carlo slab transport run.
//! Both drive a hash-bound Python script under a digest-pinned interpreter.
//! The adapters map slots to arguments and extract claims from the produced
//! documents; they interpret nothing.

use std::collections::BTreeMap;
use std::time::Duration;

use avila_core_kernel::{lower_authored_decimal, read_authoritative_decimal};
use serde_json::{Value, json};

use super::{AdapterOutput, ExtractedClaim, StepContext};

pub const SCREEN_ADAPTER_ID: &str = "avila-labs.shielding/screen@1";
pub const SCREEN_TYPE_ID: &str = "shielding.attenuation-screen";
pub const SCREEN_INPUT_SLOTS: &[&str] = &["script", "candidate", "materials", "source"];
pub const SCREEN_OUTPUTS: &[AdapterOutput] = &[AdapterOutput {
    output_id: "screen-result",
    workspace_path: "outputs/screen-result.json",
    media_type: "application/vnd.avila.shield-screen+json",
}];
pub const SCREEN_OUTPUT_SLOTS: &[&str] = &["dose-rate", "mass", "thickness", "screen-result"];
pub const SCREEN_TIMEOUT: Duration = Duration::from_secs(120);
pub const SCREEN_SCHEMA: &str = "avila.shielding/screen-result/v1";

pub const TRANSPORT_ADAPTER_ID: &str = "avila-labs.shielding/slab-transport@1";
pub const TRANSPORT_TYPE_ID: &str = "shielding.slab-transport";
pub const TRANSPORT_INPUT_SLOTS: &[&str] = &[
    "script",
    "candidate",
    "materials",
    "source",
    "cross-section-index",
];
pub const TRANSPORT_OUTPUTS: &[AdapterOutput] = &[AdapterOutput {
    output_id: "transport-result",
    workspace_path: "outputs/transport-result.json",
    media_type: "application/vnd.avila.shield-transport+json",
}];
pub const TRANSPORT_OUTPUT_SLOTS: &[&str] = &["dose-rate", "transport-result"];
pub const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(3_600);
pub const TRANSPORT_SCHEMA: &str = "avila.shielding/transport-result/v1";
/// The environment variable OpenMC reads to find its data library. The
/// package declares it as a required key and the operator supplies the
/// value; the staged index's digest is checked against it by the script.
pub const TRANSPORT_ENVIRONMENT_KEYS: &[&str] = &["OPENMC_CROSS_SECTIONS"];

/// The static environment of a transport run. `OMP_NUM_THREADS` fixes the
/// thread count the receipt records. `HOME` is set because the OpenMC build
/// in use initializes Open MPI, which aborts without a home directory; the
/// step directory is used so nothing outside the workspace is read or
/// written and the recorded value is the same on every machine.
pub fn transport_environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("HOME".to_string(), ".".to_string()),
        ("OMP_NUM_THREADS".to_string(), "8".to_string()),
    ])
}

fn staged(staged: &BTreeMap<String, String>, slot: &str) -> Result<String, String> {
    staged
        .get(slot)
        .cloned()
        .ok_or_else(|| format!("input slot `{slot}` is not staged"))
}

fn check_slots(
    staged: &BTreeMap<String, String>,
    accepted: &[&str],
    adapter: &str,
) -> Result<(), String> {
    for slot in staged.keys() {
        if !accepted.contains(&slot.as_str()) {
            return Err(format!(
                "input slot `{slot}` is not accepted by adapter {adapter}"
            ));
        }
    }
    Ok(())
}

pub fn screen_arguments(
    staged_paths: &BTreeMap<String, String>,
    _context: &StepContext,
) -> Result<Vec<String>, String> {
    check_slots(staged_paths, SCREEN_INPUT_SLOTS, SCREEN_ADAPTER_ID)?;
    Ok(vec![
        staged(staged_paths, "script")?,
        "--candidate".into(),
        staged(staged_paths, "candidate")?,
        "--materials".into(),
        staged(staged_paths, "materials")?,
        "--source".into(),
        staged(staged_paths, "source")?,
        "--output".into(),
        SCREEN_OUTPUTS[0].workspace_path.into(),
    ])
}

fn integer_parameter(context: &StepContext, name: &str) -> Result<i64, String> {
    context
        .parameters
        .get(name)
        .and_then(|value| value.get("value"))
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("compiled step carries no integer parameter `{name}`"))
}

pub fn transport_arguments(
    staged_paths: &BTreeMap<String, String>,
    context: &StepContext,
) -> Result<Vec<String>, String> {
    check_slots(staged_paths, TRANSPORT_INPUT_SLOTS, TRANSPORT_ADAPTER_ID)?;
    let seed = context
        .seed
        .clone()
        .ok_or("the transport step must bind a seed; the capability is seeded-stochastic")?;
    Ok(vec![
        staged(staged_paths, "script")?,
        "--candidate".into(),
        staged(staged_paths, "candidate")?,
        "--materials".into(),
        staged(staged_paths, "materials")?,
        "--source".into(),
        staged(staged_paths, "source")?,
        "--cross-sections-index".into(),
        staged(staged_paths, "cross-section-index")?,
        "--particles".into(),
        integer_parameter(context, "particles")?.to_string(),
        "--batches".into(),
        integer_parameter(context, "batches")?.to_string(),
        "--seed".into(),
        seed,
        "--output".into(),
        TRANSPORT_OUTPUTS[0].workspace_path.into(),
    ])
}

fn document(
    outputs: &BTreeMap<String, Vec<u8>>,
    output_id: &str,
    schema: &str,
) -> Result<Value, String> {
    let bytes = outputs
        .get(output_id)
        .ok_or_else(|| format!("output `{output_id}` was not collected"))?;
    let document: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("output `{output_id}` is not valid JSON: {error}"))?;
    if document.get("schema").and_then(Value::as_str) != Some(schema) {
        return Err(format!(
            "output `{output_id}` does not declare schema `{schema}`"
        ));
    }
    Ok(document)
}

/// A `{ "value": "...", "unit": "..." }` object with a canonical decimal.
fn quantity(document: &Value, pointer: &str) -> Result<Value, String> {
    let object = document
        .pointer(pointer)
        .ok_or_else(|| format!("result has no quantity at `{pointer}`"))?;
    let text = object
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{pointer}` has no text value"))?;
    let unit = object
        .get("unit")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{pointer}` has no unit"))?;
    let lowered = lower_authored_decimal(text)
        .map_err(|error| format!("`{pointer}` `{text}`: {}", error.detail()))?;
    read_authoritative_decimal(&lowered)
        .map_err(|error| format!("`{pointer}` `{text}`: {}", error.detail()))?;
    Ok(json!({ "value": lowered, "unit": unit }))
}

pub fn screen_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    _context: &StepContext,
) -> Result<Vec<ExtractedClaim>, String> {
    let result = document(outputs, "screen-result", SCREEN_SCHEMA)?;
    let output_id = "screen-result".to_string();
    Ok(vec![
        ExtractedClaim {
            output_slot: "dose-rate".into(),
            output_id: output_id.clone(),
            claim: json!({ "model": "unquantified", "nominal": quantity(&result, "/dose_rate")? }),
        },
        ExtractedClaim {
            output_slot: "mass".into(),
            output_id: output_id.clone(),
            claim: json!({ "model": "exact", "nominal": quantity(&result, "/mass")? }),
        },
        ExtractedClaim {
            output_slot: "thickness".into(),
            output_id: output_id.clone(),
            claim: json!({ "model": "exact", "nominal": quantity(&result, "/thickness")? }),
        },
        ExtractedClaim {
            output_slot: "screen-result".into(),
            output_id,
            claim: json!({ "model": "unquantified" }),
        },
    ])
}

pub fn transport_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    _context: &StepContext,
) -> Result<Vec<ExtractedClaim>, String> {
    let result = document(outputs, "transport-result", TRANSPORT_SCHEMA)?;
    let coverage = result
        .pointer("/dose_rate/coverage")
        .and_then(Value::as_str)
        .ok_or("transport result has no coverage")?;
    let lowered = lower_authored_decimal(coverage).map_err(|error| error.detail().to_string())?;
    let output_id = "transport-result".to_string();
    Ok(vec![
        ExtractedClaim {
            output_slot: "dose-rate".into(),
            output_id: output_id.clone(),
            claim: json!({
                "model": "coverage_interval",
                "lower": quantity(&result, "/dose_rate/lower")?,
                "upper": quantity(&result, "/dose_rate/upper")?,
                "nominal": quantity(&result, "/dose_rate/nominal")?,
                "coverage": lowered,
            }),
        },
        ExtractedClaim {
            output_slot: "transport-result".into(),
            output_id,
            claim: json!({ "model": "unquantified" }),
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(seed: Option<&str>) -> StepContext {
        StepContext {
            parameters: BTreeMap::from([
                (
                    "particles".to_string(),
                    json!({ "type": "integer", "value": 1000 }),
                ),
                (
                    "batches".to_string(),
                    json!({ "type": "integer", "value": 10 }),
                ),
            ]),
            seed: seed.map(str::to_owned),
        }
    }

    #[test]
    fn transport_arguments_carry_seed_and_parameters() {
        let staged: BTreeMap<String, String> = TRANSPORT_INPUT_SLOTS
            .iter()
            .map(|slot| ((*slot).to_string(), format!("in/{slot}")))
            .collect();
        let arguments = transport_arguments(&staged, &context(Some("7"))).unwrap();
        assert_eq!(arguments[0], "in/script");
        assert!(arguments.windows(2).any(|pair| pair == ["--seed", "7"]));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--particles", "1000"])
        );
        assert!(transport_arguments(&staged, &context(None)).is_err());
    }

    #[test]
    fn claims_are_extracted_as_canonical_quantities() {
        let screen = json!({
            "schema": SCREEN_SCHEMA,
            "dose_rate": { "value": "9.40", "unit": "uSv/h" },
            "mass": { "value": "846", "unit": "kg" },
            "thickness": { "value": "90", "unit": "cm" }
        });
        let outputs =
            BTreeMap::from([("screen-result".to_string(), screen.to_string().into_bytes())]);
        let claims = screen_claims(&outputs, &context(None)).unwrap();
        assert_eq!(claims[0].claim["nominal"]["value"], json!("9.4"));
        assert_eq!(claims[1].claim["model"], json!("exact"));

        let transport = json!({
            "schema": TRANSPORT_SCHEMA,
            "dose_rate": { "nominal": { "value": "27.1", "unit": "uSv/h" }, "lower": { "value": "25.4", "unit": "uSv/h" }, "upper": { "value": "28.8", "unit": "uSv/h" }, "coverage": "0.95" }
        });
        let outputs = BTreeMap::from([(
            "transport-result".to_string(),
            transport.to_string().into_bytes(),
        )]);
        let claims = transport_claims(&outputs, &context(None)).unwrap();
        assert_eq!(claims[0].claim["model"], json!("coverage_interval"));
        assert_eq!(claims[0].claim["coverage"], json!("0.95"));
    }
}
