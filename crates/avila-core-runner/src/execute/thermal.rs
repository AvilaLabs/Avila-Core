//! Case-specific adapters for the thermal-spreader configuration search: a
//! fast one-dimensional series-resistance screen and a finite-element slab
//! solve. Both drive a hash-bound Python script under a digest-pinned
//! interpreter — the screen under the system interpreter, the finite-element
//! solve under the thermal virtual environment's interpreter (scikit-fem).
//! The adapters map slots to arguments and extract claims from the produced
//! documents; they interpret nothing.

use std::collections::BTreeMap;
use std::time::Duration;

use avila_core_kernel::{ExactNumber, lower_authored_decimal, read_authoritative_decimal};
use serde_json::{Value, json};

use super::claims::canonical_decimal;
use super::{AdapterOutput, ExtractedClaim, StepContext};

pub const SCREEN_ADAPTER_ID: &str = "avila-labs.thermal/screen@1";
pub const SCREEN_TYPE_ID: &str = "thermal.spreader-screen";
pub const SCREEN_INPUT_SLOTS: &[&str] = &["script", "candidate", "materials", "source"];
pub const SCREEN_OUTPUTS: &[AdapterOutput] = &[AdapterOutput {
    output_id: "screen-result",
    workspace_path: "outputs/screen-result.json",
    media_type: "application/vnd.avila.thermal-screen+json",
}];
pub const SCREEN_OUTPUT_SLOTS: &[&str] = &[
    "hotspot-temperature",
    "areal-mass",
    "thickness",
    "screen-result",
];
pub const SCREEN_TIMEOUT: Duration = Duration::from_secs(120);
pub const SCREEN_SCHEMA: &str = "avila.thermal/screen-result/v1";

pub const FE_ADAPTER_ID: &str = "avila-labs.thermal/spreader-fe@1";
pub const FE_TYPE_ID: &str = "thermal.spreader-fe";
pub const FE_INPUT_SLOTS: &[&str] = &["script", "candidate", "materials", "source"];
pub const FE_OUTPUTS: &[AdapterOutput] = &[AdapterOutput {
    output_id: "fe-result",
    workspace_path: "outputs/fe-result.json",
    media_type: "application/vnd.avila.thermal-fe+json",
}];
pub const FE_OUTPUT_SLOTS: &[&str] = &["hotspot-temperature", "fe-result"];
pub const FE_TIMEOUT: Duration = Duration::from_secs(600);
pub const FE_SCHEMA: &str = "avila.thermal/fe-result/v1";

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

pub fn fe_arguments(
    staged_paths: &BTreeMap<String, String>,
    _context: &StepContext,
) -> Result<Vec<String>, String> {
    check_slots(staged_paths, FE_INPUT_SLOTS, FE_ADAPTER_ID)?;
    Ok(vec![
        staged(staged_paths, "script")?,
        "--candidate".into(),
        staged(staged_paths, "candidate")?,
        "--materials".into(),
        staged(staged_paths, "materials")?,
        "--source".into(),
        staged(staged_paths, "source")?,
        "--output".into(),
        FE_OUTPUTS[0].workspace_path.into(),
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
/// Mirrors `shielding::quantity` and `activation::quantity`: same helpers,
/// same claim shape, applied to the thermal screen's nominal numbers.
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

/// A `{ "value": "...", "unit": "..." }` object built from a *bare* decimal
/// string at `value_pointer` and a unit shared by both bounds at
/// `unit_pointer`, rather than a nested `{value, unit}` at `value_pointer`
/// itself. The finite-element result's `hotspot_temperature` is
/// `{lower, upper, unit, interpretation}` — one unit for both bounds, not one
/// per bound the way the shielding transport result's `dose_rate` is — so
/// this cannot reuse `quantity` above; it is the same canonicalisation and
/// the same error shape applied to that different document layout.
fn bare_quantity(
    document: &Value,
    value_pointer: &str,
    unit_pointer: &str,
) -> Result<Value, String> {
    let text = document
        .pointer(value_pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("result has no text value at `{value_pointer}`"))?;
    let unit = document
        .pointer(unit_pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("result has no unit at `{unit_pointer}`"))?;
    let lowered = lower_authored_decimal(text)
        .map_err(|error| format!("`{value_pointer}` `{text}`: {}", error.detail()))?;
    read_authoritative_decimal(&lowered)
        .map_err(|error| format!("`{value_pointer}` `{text}`: {}", error.detail()))?;
    Ok(json!({ "value": lowered, "unit": unit }))
}

/// The screen's hotspot temperature is `unquantified` (a nominal 1-D guide,
/// like the shielding screen's `dose-rate`); areal mass and thickness are
/// `exact` arithmetic over the candidate and material table, exactly as the
/// shielding screen's `mass` and `thickness` are.
pub fn screen_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    _context: &StepContext,
) -> Result<Vec<ExtractedClaim>, String> {
    let result = document(outputs, "screen-result", SCREEN_SCHEMA)?;
    let output_id = "screen-result".to_string();
    Ok(vec![
        ExtractedClaim {
            output_slot: "hotspot-temperature".into(),
            output_id: output_id.clone(),
            claim: json!({ "model": "unquantified", "nominal": quantity(&result, "/hotspot_temperature")? }),
        },
        ExtractedClaim {
            output_slot: "areal-mass".into(),
            output_id: output_id.clone(),
            claim: json!({ "model": "exact", "nominal": quantity(&result, "/areal_mass")? }),
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

/// The finite-element hotspot temperature is a discretisation bracket
/// (`interval`: `lower`/`upper`, no coverage — this is mesh-refinement
/// spread, not a statistical or metrological coverage claim).
pub fn fe_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    _context: &StepContext,
) -> Result<Vec<ExtractedClaim>, String> {
    let result = document(outputs, "fe-result", FE_SCHEMA)?;
    let output_id = "fe-result".to_string();
    Ok(vec![
        ExtractedClaim {
            output_slot: "hotspot-temperature".into(),
            output_id: output_id.clone(),
            claim: json!({
                "model": "interval",
                "lower": bare_quantity(&result, "/hotspot_temperature/lower", "/hotspot_temperature/unit")?,
                "upper": bare_quantity(&result, "/hotspot_temperature/upper", "/hotspot_temperature/unit")?,
            }),
        },
        ExtractedClaim {
            output_slot: "fe-result".into(),
            output_id,
            claim: json!({ "model": "unquantified" }),
        },
    ])
}

/// Facts a qualification envelope over this plate geometry can be written
/// over: the candidate's total thickness and layer count, and each layer's
/// material as an attribute of the candidate input, padded with `none`
/// beyond the third layer exactly as `shielding::transport_facts_for` pads
/// the slab candidate's layers, and the source document's heat flux,
/// convection coefficient, and strip width as quantity facts. Shared by the
/// screen and the finite-element adapters, each calling it with its own
/// adapter id as `validator`: same shape, same provenance convention, as
/// `shielding::transport_facts_for` applied to the thermal source and
/// candidate documents instead of the shielding ones.
pub fn thermal_facts_for(
    staged: &[(String, String, String, Vec<u8>)],
    invocation_sha256: &str,
    facts: &mut serde_json::Map<String, Value>,
    inputs: &mut serde_json::Map<String, Value>,
    validator: &str,
) -> Result<(), String> {
    let receipt = format!("plan:{invocation_sha256}");
    let source_of = |identity: &str| {
        json!({ "class": "validated_input", "identity": identity,
                "validator": validator, "receipt": receipt })
    };
    for (slot, _, sha256, bytes) in staged {
        match slot.as_str() {
            "candidate" => {
                let document: Value = serde_json::from_slice(bytes)
                    .map_err(|error| format!("candidate document: {error}"))?;
                let layers = document
                    .get("layers")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut total =
                    ExactNumber::from_canonical("0").map_err(|error| error.detail().to_string())?;
                let mut attributes = serde_json::Map::new();
                for (index, layer) in layers.iter().enumerate() {
                    let thickness = layer
                        .get("thickness_mm")
                        .and_then(Value::as_str)
                        .ok_or_else(|| format!("layer {} has no thickness_mm", index + 1))?;
                    let value = ExactNumber::from_canonical(thickness).map_err(|error| {
                        format!(
                            "layer {} thickness_mm `{thickness}`: {}",
                            index + 1,
                            error.detail()
                        )
                    })?;
                    total = total
                        .checked_add(&value)
                        .map_err(|error| error.detail().to_string())?;
                    attributes.insert(
                        format!("layer.{}.material", index + 1),
                        json!(layer.get("material").and_then(Value::as_str).unwrap_or("")),
                    );
                }
                for index in layers.len()..3 {
                    attributes.insert(format!("layer.{}.material", index + 1), json!("none"));
                }
                facts.insert(
                    "plate.total_thickness".into(),
                    json!({ "value": { "value": canonical_decimal(&total)?, "unit": "mm" },
                            "source": source_of(sha256) }),
                );
                facts.insert(
                    "plate.layer_count".into(),
                    json!({ "value": layers.len(), "source": source_of(sha256) }),
                );
                let entry = inputs
                    .entry("candidate".to_string())
                    .or_insert_with(|| json!({ "attributes": {} }));
                if let Some(existing) = entry.get_mut("attributes").and_then(Value::as_object_mut) {
                    existing.extend(attributes);
                }
            }
            "source" => {
                let document: Value = serde_json::from_slice(bytes)
                    .map_err(|error| format!("source document: {error}"))?;
                if let Some(flux) = document.get("heat_flux_W_m2").and_then(Value::as_str) {
                    facts.insert(
                        "source.heat_flux".into(),
                        json!({ "value": { "value": flux, "unit": "W/m2" }, "source": source_of(sha256) }),
                    );
                }
                if let Some(convection) = document.get("convection_W_m2K").and_then(Value::as_str) {
                    facts.insert(
                        "source.convection".into(),
                        json!({ "value": { "value": convection, "unit": "W/m2/K" }, "source": source_of(sha256) }),
                    );
                }
                if let Some(strip) = document.get("strip_width_mm").and_then(Value::as_str) {
                    facts.insert(
                        "source.strip_width".into(),
                        json!({ "value": { "value": strip, "unit": "mm" }, "source": source_of(sha256) }),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged_paths(slots: &[&str]) -> BTreeMap<String, String> {
        slots
            .iter()
            .map(|slot| ((*slot).to_string(), format!("in/{slot}")))
            .collect()
    }

    #[test]
    fn screen_arguments_map_every_slot_to_its_flag() {
        let arguments =
            screen_arguments(&staged_paths(SCREEN_INPUT_SLOTS), &StepContext::default()).unwrap();
        assert_eq!(arguments[0], "in/script");
        for (flag, slot) in [
            ("--candidate", "candidate"),
            ("--materials", "materials"),
            ("--source", "source"),
        ] {
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| pair == [flag, &format!("in/{slot}")]),
                "missing {flag} for slot {slot}"
            );
        }
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--output", SCREEN_OUTPUTS[0].workspace_path])
        );
    }

    #[test]
    fn fe_arguments_map_every_slot_to_its_flag() {
        let arguments =
            fe_arguments(&staged_paths(FE_INPUT_SLOTS), &StepContext::default()).unwrap();
        assert_eq!(arguments[0], "in/script");
        for (flag, slot) in [
            ("--candidate", "candidate"),
            ("--materials", "materials"),
            ("--source", "source"),
        ] {
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| pair == [flag, &format!("in/{slot}")]),
                "missing {flag} for slot {slot}"
            );
        }
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--output", FE_OUTPUTS[0].workspace_path])
        );
    }

    #[test]
    fn an_unaccepted_slot_is_refused_by_both_adapters() {
        let mut screen_staged = staged_paths(SCREEN_INPUT_SLOTS);
        screen_staged.insert("spectra".into(), "in/spectra".into());
        assert!(
            screen_arguments(&screen_staged, &StepContext::default())
                .unwrap_err()
                .contains("spectra")
        );

        let mut fe_staged = staged_paths(FE_INPUT_SLOTS);
        fe_staged.insert("spectra".into(), "in/spectra".into());
        assert!(
            fe_arguments(&fe_staged, &StepContext::default())
                .unwrap_err()
                .contains("spectra")
        );
    }

    #[test]
    fn screen_claims_are_extracted_as_canonical_quantities() {
        let screen = json!({
            "schema": SCREEN_SCHEMA,
            "hotspot_temperature": { "value": "340.10", "unit": "K" },
            "areal_mass": { "value": "53.880", "unit": "kg" },
            "thickness": { "value": "13", "unit": "mm" }
        });
        let outputs =
            BTreeMap::from([("screen-result".to_string(), screen.to_string().into_bytes())]);
        let claims = screen_claims(&outputs, &StepContext::default()).unwrap();
        let slots: Vec<&str> = claims.iter().map(|c| c.output_slot.as_str()).collect();
        assert_eq!(
            slots,
            [
                "hotspot-temperature",
                "areal-mass",
                "thickness",
                "screen-result"
            ]
        );
        assert_eq!(claims[0].claim["model"], json!("unquantified"));
        assert_eq!(claims[0].claim["nominal"]["value"], json!("340.1"));
        assert_eq!(claims[1].claim["model"], json!("exact"));
        assert_eq!(claims[1].claim["nominal"]["value"], json!("53.88"));
        assert_eq!(claims[2].claim["model"], json!("exact"));
        assert_eq!(claims[2].claim["nominal"]["value"], json!("13"));
        assert_eq!(claims[3].claim, json!({ "model": "unquantified" }));
    }

    #[test]
    fn fe_claims_are_extracted_as_a_shared_unit_interval() {
        let fe = json!({
            "schema": FE_SCHEMA,
            "hotspot_temperature": {
                "lower": "322.81",
                "upper": "322.83",
                "unit": "K",
                "interpretation": "discretisation bracket from two mesh levels; not a coverage claim",
            },
        });
        let outputs = BTreeMap::from([("fe-result".to_string(), fe.to_string().into_bytes())]);
        let claims = fe_claims(&outputs, &StepContext::default()).unwrap();
        assert_eq!(claims[0].output_slot, "hotspot-temperature");
        assert_eq!(claims[0].claim["model"], json!("interval"));
        assert_eq!(
            claims[0].claim["lower"],
            json!({ "value": "322.81", "unit": "K" })
        );
        assert_eq!(
            claims[0].claim["upper"],
            json!({ "value": "322.83", "unit": "K" })
        );
        assert_eq!(claims[1].claim, json!({ "model": "unquantified" }));
    }

    #[test]
    fn wrong_schema_is_refused() {
        let document = json!({ "schema": "wrong" });
        let outputs = BTreeMap::from([(
            "screen-result".to_string(),
            document.to_string().into_bytes(),
        )]);
        assert!(
            screen_claims(&outputs, &StepContext::default())
                .unwrap_err()
                .contains("schema")
        );
    }

    fn candidate_bytes(layers: &[(&str, &str)]) -> Vec<u8> {
        let layers: Vec<Value> = layers
            .iter()
            .map(|(material, thickness_mm)| json!({ "material": material, "thickness_mm": thickness_mm }))
            .collect();
        json!({
            "schema": "avila.thermal/candidate/v1",
            "candidate_id": "test-candidate",
            "layers": layers,
        })
        .to_string()
        .into_bytes()
    }

    fn source_bytes() -> Vec<u8> {
        json!({
            "schema": "avila.thermal/source/v1",
            "width_mm": "100",
            "strip_width_mm": "20",
            "heat_flux_W_m2": "50000",
            "convection_W_m2K": "500",
            "ambient_K": "300",
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn thermal_facts_report_layer_count_padded_materials_and_source_quantities() {
        let staged = vec![
            (
                "candidate".to_string(),
                "application/json".to_string(),
                "candidate-sha".to_string(),
                candidate_bytes(&[("copper", "3"), ("aluminium", "10")]),
            ),
            (
                "source".to_string(),
                "application/json".to_string(),
                "source-sha".to_string(),
                source_bytes(),
            ),
        ];
        let mut facts = serde_json::Map::new();
        let mut inputs = serde_json::Map::new();
        thermal_facts_for(
            &staged,
            "invocation-sha",
            &mut facts,
            &mut inputs,
            FE_ADAPTER_ID,
        )
        .unwrap();

        assert_eq!(facts["plate.layer_count"]["value"], json!(2));
        assert_eq!(
            facts["plate.total_thickness"]["value"],
            json!({ "value": "13", "unit": "mm" })
        );
        assert_eq!(
            facts["plate.total_thickness"]["source"]["validator"],
            json!(FE_ADAPTER_ID)
        );
        assert_eq!(
            facts["source.heat_flux"]["value"],
            json!({ "value": "50000", "unit": "W/m2" })
        );
        assert_eq!(
            facts["source.convection"]["value"],
            json!({ "value": "500", "unit": "W/m2/K" })
        );
        assert_eq!(
            facts["source.strip_width"]["value"],
            json!({ "value": "20", "unit": "mm" })
        );
        let attributes = &inputs["candidate"]["attributes"];
        assert_eq!(attributes["layer.1.material"], json!("copper"));
        assert_eq!(attributes["layer.2.material"], json!("aluminium"));
        assert_eq!(attributes["layer.3.material"], json!("none"));
    }

    #[test]
    fn a_single_layer_candidate_pads_two_layers_with_none() {
        let staged = vec![(
            "candidate".to_string(),
            "application/json".to_string(),
            "candidate-sha".to_string(),
            candidate_bytes(&[("aluminium", "30")]),
        )];
        let mut facts = serde_json::Map::new();
        let mut inputs = serde_json::Map::new();
        thermal_facts_for(
            &staged,
            "invocation-sha",
            &mut facts,
            &mut inputs,
            SCREEN_ADAPTER_ID,
        )
        .unwrap();
        assert_eq!(facts["plate.layer_count"]["value"], json!(1));
        assert_eq!(
            facts["plate.total_thickness"]["value"],
            json!({ "value": "30", "unit": "mm" })
        );
        assert_eq!(
            facts["plate.total_thickness"]["source"]["validator"],
            json!(SCREEN_ADAPTER_ID)
        );
        let attributes = &inputs["candidate"]["attributes"];
        assert_eq!(attributes["layer.1.material"], json!("aluminium"));
        assert_eq!(attributes["layer.2.material"], json!("none"));
        assert_eq!(attributes["layer.3.material"], json!("none"));
    }
}
