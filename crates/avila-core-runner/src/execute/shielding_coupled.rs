//! Case-specific adapter for the second-generation, coupled shielding
//! transport capability: neutron AND secondary-photon dose in the detector
//! cell, per-layer neutron flux spectra in the FISPACT-709 group structure
//! for a downstream activation step, and deterministic variance reduction
//! (survival biasing plus an analytic neutron weight-window mesh). This is a
//! new adapter id, `avila-labs.shielding/slab-transport@2`; the v1 adapter in
//! `shielding.rs` (`avila-labs.shielding/slab-transport@1`) and its behaviour
//! are unchanged.

use std::collections::BTreeMap;
use std::time::Duration;

use avila_core_kernel::{lower_authored_decimal, read_authoritative_decimal};
use serde_json::{Value, json};

use super::shielding::transport_facts_for;
use super::{AdapterOutput, ExtractedClaim, StepContext};

pub const TRANSPORT_ADAPTER_ID: &str = "avila-labs.shielding/slab-transport@2";
pub const TRANSPORT_TYPE_ID: &str = "shielding.slab-transport-coupled";
pub const TRANSPORT_INPUT_SLOTS: &[&str] = &[
    "script",
    "candidate",
    "materials",
    "source",
    "cross-section-index",
    "groups",
];
pub const TRANSPORT_OUTPUTS: &[AdapterOutput] = &[
    AdapterOutput {
        output_id: "transport-result",
        workspace_path: "outputs/transport-result.json",
        media_type: "application/vnd.avila.shield-transport+json",
    },
    AdapterOutput {
        output_id: "layer-spectra",
        workspace_path: "outputs/layer-spectra.json",
        media_type: "application/vnd.avila.shield-layer-spectra+json",
    },
];
pub const TRANSPORT_OUTPUT_SLOTS: &[&str] = &[
    "neutron-dose-rate",
    "photon-dose-rate",
    "layer-spectra",
    "transport-result",
];
pub const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(3_600);
pub const TRANSPORT_SCHEMA: &str = "avila.shielding/transport-result/v2";
pub const LAYER_SPECTRA_SCHEMA: &str = "avila.shielding/layer-spectra/v1";
/// The environment variable OpenMC reads to find its data library. Identical
/// requirement to v1: the package declares it as a required key, the
/// operator supplies the value, and the script checks the staged index's
/// digest against it.
pub const TRANSPORT_ENVIRONMENT_KEYS: &[&str] = &["OPENMC_CROSS_SECTIONS"];

/// The static environment of a transport run, identical to v1's: `HOME=.`
/// because the OpenMC build in use initializes Open MPI and aborts without a
/// home directory, and a fixed `OMP_NUM_THREADS` so the receipt records a
/// stable thread count.
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
        "--groups".into(),
        staged(staged_paths, "groups")?,
        "--particles".into(),
        integer_parameter(context, "particles")?.to_string(),
        "--batches".into(),
        integer_parameter(context, "batches")?.to_string(),
        "--seed".into(),
        seed,
        "--output".into(),
        TRANSPORT_OUTPUTS[0].workspace_path.into(),
        "--layer-spectra-output".into(),
        TRANSPORT_OUTPUTS[1].workspace_path.into(),
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

/// One dose-rate slot's `coverage_interval` claim, extracted the same way as
/// v1's single `dose-rate` claim: lower, upper, nominal, and the coverage
/// carried alongside as a canonical decimal.
fn dose_rate_claim(
    result: &Value,
    field: &str,
    output_slot: &str,
) -> Result<ExtractedClaim, String> {
    let base = format!("/{field}");
    let coverage = result
        .pointer(&format!("{base}/coverage"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("transport result has no coverage at `{base}`"))?;
    let lowered = lower_authored_decimal(coverage).map_err(|error| error.detail().to_string())?;
    Ok(ExtractedClaim {
        output_slot: output_slot.into(),
        output_id: "transport-result".into(),
        claim: json!({
            "model": "coverage_interval",
            "lower": quantity(result, &format!("{base}/lower"))?,
            "upper": quantity(result, &format!("{base}/upper"))?,
            "nominal": quantity(result, &format!("{base}/nominal"))?,
            "coverage": lowered,
        }),
    })
}

pub fn transport_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    _context: &StepContext,
) -> Result<Vec<ExtractedClaim>, String> {
    let result = document(outputs, "transport-result", TRANSPORT_SCHEMA)?;
    // Parsed for schema validation only: a layer-spectra output that fails to
    // parse or declares the wrong schema must fail claim extraction exactly
    // as a malformed transport-result would, even though its only claim is
    // unquantified.
    document(outputs, "layer-spectra", LAYER_SPECTRA_SCHEMA)?;

    Ok(vec![
        dose_rate_claim(&result, "neutron_dose_rate", "neutron-dose-rate")?,
        dose_rate_claim(&result, "photon_dose_rate", "photon-dose-rate")?,
        ExtractedClaim {
            output_slot: "transport-result".into(),
            output_id: "transport-result".into(),
            claim: json!({ "model": "unquantified" }),
        },
        ExtractedClaim {
            output_slot: "layer-spectra".into(),
            output_id: "layer-spectra".into(),
            claim: json!({ "model": "unquantified" }),
        },
    ])
}

/// Facts for the coupled transport step's qualification envelope: identical
/// to v1's, by direct reuse of the same extraction (source energy and
/// geometry, the slab's total thickness and layer count, each layer's
/// material). A method owner may bind a qualification record over this
/// capability id with the same scope shape v1 uses.
pub fn transport_facts_coupled(
    staged: &[(String, String, String, Vec<u8>)],
    invocation_sha256: &str,
    facts: &mut serde_json::Map<String, Value>,
    inputs: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    transport_facts_for(
        staged,
        invocation_sha256,
        facts,
        inputs,
        TRANSPORT_ADAPTER_ID,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(seed: Option<&str>) -> StepContext {
        StepContext {
            parameters: BTreeMap::from([
                (
                    "particles".to_string(),
                    json!({ "type": "integer", "value": 200000 }),
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
                .any(|pair| pair == ["--particles", "200000"])
        );
        assert!(arguments.windows(2).any(|pair| pair == ["--batches", "10"]));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--groups", "in/groups"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--cross-sections-index", "in/cross-section-index"])
        );
        assert!(transport_arguments(&staged, &context(None)).is_err());
    }

    #[test]
    fn transport_arguments_reject_unaccepted_slot() {
        let mut staged: BTreeMap<String, String> = TRANSPORT_INPUT_SLOTS
            .iter()
            .map(|slot| ((*slot).to_string(), format!("in/{slot}")))
            .collect();
        staged.insert("extra".to_string(), "in/extra".to_string());
        assert!(transport_arguments(&staged, &context(Some("1"))).is_err());
    }

    #[test]
    fn claims_are_extracted_as_canonical_quantities() {
        let transport = json!({
            "schema": TRANSPORT_SCHEMA,
            "neutron_dose_rate": {
                "nominal": { "value": "27.1", "unit": "uSv/h" },
                "lower": { "value": "25.4", "unit": "uSv/h" },
                "upper": { "value": "28.8", "unit": "uSv/h" },
                "coverage": "0.95"
            },
            "photon_dose_rate": {
                "nominal": { "value": "9.30", "unit": "uSv/h" },
                "lower": { "value": "8.10", "unit": "uSv/h" },
                "upper": { "value": "10.60", "unit": "uSv/h" },
                "coverage": "0.95"
            }
        });
        let layer_spectra = json!({ "schema": LAYER_SPECTRA_SCHEMA, "layers": [] });
        let outputs = BTreeMap::from([
            (
                "transport-result".to_string(),
                transport.to_string().into_bytes(),
            ),
            (
                "layer-spectra".to_string(),
                layer_spectra.to_string().into_bytes(),
            ),
        ]);
        let claims = transport_claims(&outputs, &context(None)).unwrap();
        assert_eq!(claims.len(), 4);

        let neutron = claims
            .iter()
            .find(|claim| claim.output_slot == "neutron-dose-rate")
            .unwrap();
        assert_eq!(neutron.claim["model"], json!("coverage_interval"));
        assert_eq!(neutron.claim["coverage"], json!("0.95"));
        assert_eq!(neutron.claim["nominal"]["value"], json!("27.1"));
        assert_eq!(neutron.output_id, "transport-result");

        let photon = claims
            .iter()
            .find(|claim| claim.output_slot == "photon-dose-rate")
            .unwrap();
        assert_eq!(photon.claim["model"], json!("coverage_interval"));
        assert_eq!(photon.claim["nominal"]["value"], json!("9.3"));

        let transport_result_claim = claims
            .iter()
            .find(|claim| claim.output_slot == "transport-result")
            .unwrap();
        assert_eq!(transport_result_claim.claim["model"], json!("unquantified"));

        let layer_spectra_claim = claims
            .iter()
            .find(|claim| claim.output_slot == "layer-spectra")
            .unwrap();
        assert_eq!(layer_spectra_claim.claim["model"], json!("unquantified"));
        assert_eq!(layer_spectra_claim.output_id, "layer-spectra");
    }

    #[test]
    fn claims_fail_when_layer_spectra_output_is_missing() {
        let transport = json!({
            "schema": TRANSPORT_SCHEMA,
            "neutron_dose_rate": {
                "nominal": { "value": "1", "unit": "uSv/h" },
                "lower": { "value": "1", "unit": "uSv/h" },
                "upper": { "value": "1", "unit": "uSv/h" },
                "coverage": "0.95"
            },
            "photon_dose_rate": {
                "nominal": { "value": "1", "unit": "uSv/h" },
                "lower": { "value": "1", "unit": "uSv/h" },
                "upper": { "value": "1", "unit": "uSv/h" },
                "coverage": "0.95"
            }
        });
        let outputs = BTreeMap::from([(
            "transport-result".to_string(),
            transport.to_string().into_bytes(),
        )]);
        assert!(transport_claims(&outputs, &context(None)).is_err());
    }
}
