//! The case-specific adapter for the coupled slab-shield search's
//! activation step, which drives
//! `examples/capabilities/shield-coupled/activate.py` under a digest-pinned
//! Python interpreter. The script runs ACTINV once per layer over that
//! layer's own neutron spectrum (interface I3,
//! `avila.shielding/layer-spectra/v1`), the shared shielding material
//! table, and one declared irradiation/cooling schedule, and writes the I4
//! activation result (`avila.shielding/activation-result/v1`). The adapter
//! maps slots to arguments and extracts claims from the produced document;
//! it interprets nothing.

use std::collections::BTreeMap;
use std::time::Duration;

use avila_core_kernel::{lower_authored_decimal, read_authoritative_decimal};
use serde_json::{Value, json};

use super::{AdapterOutput, ExtractedClaim, StepContext};

pub const ADAPTER_ID: &str = "avila-labs.shielding/activation@1";
pub const CAPABILITY_TYPE_ID: &str = "shielding.slab-activation";
pub const CAPABILITY_TYPE_MAJOR: u64 = 1;
pub const INPUT_SLOTS: &[&str] = &[
    "script",
    "spectra",
    "materials",
    "schedule",
    "actinv",
    "activation-library",
    "activation-index",
    "decay-primary",
    "decay-fallback",
];
pub const OUTPUTS: &[AdapterOutput] = &[AdapterOutput {
    output_id: "activation-result",
    workspace_path: "outputs/activation-result.json",
    media_type: "application/vnd.avila.shield-activation+json",
}];
pub const OUTPUT_SLOTS: &[&str] = &["specific-activity", "decay-heat", "activation-result"];
pub const TIMEOUT: Duration = Duration::from_secs(900);
pub const SCHEMA: &str = "avila.shielding/activation-result/v1";

/// Static environment for the ACTINV cache, mirroring
/// `actinv_build::environment()` exactly: a workspace-relative directory so
/// nothing outside the step is read from or written to, and the receipt
/// records the same value on every machine.
pub const CACHE_DIR: &str = ".cache/actinv";

pub fn environment() -> BTreeMap<String, String> {
    BTreeMap::from([("ACTINV_CACHE_DIR".to_string(), CACHE_DIR.to_string())])
}

fn staged(staged: &BTreeMap<String, String>, slot: &str) -> Result<String, String> {
    staged
        .get(slot)
        .cloned()
        .ok_or_else(|| format!("input slot `{slot}` is not staged"))
}

fn check_slots(staged: &BTreeMap<String, String>) -> Result<(), String> {
    for slot in staged.keys() {
        if !INPUT_SLOTS.contains(&slot.as_str()) {
            return Err(format!(
                "input slot `{slot}` is not accepted by adapter {ADAPTER_ID}"
            ));
        }
    }
    Ok(())
}

pub fn arguments(
    staged_paths: &BTreeMap<String, String>,
    _context: &StepContext,
) -> Result<Vec<String>, String> {
    check_slots(staged_paths)?;
    Ok(vec![
        staged(staged_paths, "script")?,
        "--spectra".into(),
        staged(staged_paths, "spectra")?,
        "--materials".into(),
        staged(staged_paths, "materials")?,
        "--schedule".into(),
        staged(staged_paths, "schedule")?,
        "--actinv".into(),
        staged(staged_paths, "actinv")?,
        "--activation-library".into(),
        staged(staged_paths, "activation-library")?,
        "--activation-index".into(),
        staged(staged_paths, "activation-index")?,
        "--decay-primary".into(),
        staged(staged_paths, "decay-primary")?,
        "--decay-fallback".into(),
        staged(staged_paths, "decay-fallback")?,
        "--output".into(),
        OUTPUTS[0].workspace_path.into(),
    ])
}

fn document(outputs: &BTreeMap<String, Vec<u8>>) -> Result<Value, String> {
    let bytes = outputs
        .get("activation-result")
        .ok_or_else(|| "output `activation-result` was not collected".to_string())?;
    let document: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("output `activation-result` is not valid JSON: {error}"))?;
    if document.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        return Err(format!(
            "output `activation-result` does not declare schema `{SCHEMA}`"
        ));
    }
    Ok(document)
}

/// A `{ "value": "...", "unit": "..." }` object with a canonical decimal, as
/// an `unquantified` claim's nominal value. Mirrors `shielding::quantity`
/// and how `shielding::screen_claims` encodes the screen's nominal numbers
/// exactly: same helpers, same claim shape, applied to the activation
/// totals instead of the screen result.
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

/// The two numeric totals are `unquantified` claims under a nominal-basis
/// requirement (I4: ACTINV reports no per-value bound for this use), the
/// same basis and claim model CASE-001's screen uses. ACTINV's contact-dose
/// proxy is not extracted: it needs a photon-response table that is not
/// among this adapter's inputs, and the case runner requires every adapter
/// output slot to be bound and produced, so an output that no bound data
/// can produce is not a slot.
pub fn extract_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    _context: &StepContext,
) -> Result<Vec<ExtractedClaim>, String> {
    let result = document(outputs)?;
    let output_id = "activation-result".to_string();
    let mut claims = vec![
        ExtractedClaim {
            output_slot: "specific-activity".into(),
            output_id: output_id.clone(),
            claim: json!({
                "model": "unquantified",
                "nominal": quantity(&result, "/totals/max_specific_activity")?,
            }),
        },
        ExtractedClaim {
            output_slot: "decay-heat".into(),
            output_id: output_id.clone(),
            claim: json!({
                "model": "unquantified",
                "nominal": quantity(&result, "/totals/total_decay_heat")?,
            }),
        },
    ];
    claims.push(ExtractedClaim {
        output_slot: "activation-result".into(),
        output_id,
        claim: json!({ "model": "unquantified" }),
    });
    Ok(claims)
}

/// True when every digit in `text` is `0` (so `"0"`, `"0.0"`, `"-0.00"` all
/// count as zero-thickness); a value with no digits at all is treated as
/// not zero, matching the fail-open shape of a facts function (a genuinely
/// malformed document fails elsewhere, at the adapter's own extraction or
/// the script's own validation, not silently here).
fn is_zero_thickness(text: &str) -> bool {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    !digits.is_empty() && digits.chars().all(|digit| digit == '0')
}

/// Facts the activation qualification can be written over: the slab's
/// layer count (the spectra file already omits zero-thickness layers, as
/// I3 requires of its producer, so this also defensively skips any that
/// slipped through) and each layer's material as an attribute of the
/// spectra input, padded with `none` beyond the last layer exactly as
/// `shielding::transport_facts` pads the candidate's layers; and the
/// declared schedule's irradiation and cooling durations as quantity facts
/// in seconds, read from the staged schedule document
/// (`avila.shielding/irradiation-schedule/v1`).
pub fn activation_facts(
    staged: &[(String, String, String, Vec<u8>)],
    invocation_sha256: &str,
    facts: &mut serde_json::Map<String, Value>,
    inputs: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let receipt = format!("plan:{invocation_sha256}");
    let source_of = |identity: &str| {
        json!({ "class": "validated_input", "identity": identity,
                "validator": ADAPTER_ID, "receipt": receipt })
    };
    for (slot, _, sha256, bytes) in staged {
        match slot.as_str() {
            "spectra" => {
                let document: Value = serde_json::from_slice(bytes)
                    .map_err(|error| format!("spectra document: {error}"))?;
                let layers = document
                    .get("layers")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut attributes = serde_json::Map::new();
                let mut layer_count = 0usize;
                for (index, layer) in layers.iter().enumerate() {
                    attributes.insert(
                        format!("layer.{}.material", index + 1),
                        json!(layer.get("material").and_then(Value::as_str).unwrap_or("")),
                    );
                    let zero = layer
                        .get("thickness_cm")
                        .and_then(Value::as_str)
                        .is_some_and(is_zero_thickness);
                    if !zero {
                        layer_count += 1;
                    }
                }
                for index in layers.len()..3 {
                    attributes.insert(format!("layer.{}.material", index + 1), json!("none"));
                }
                facts.insert(
                    "slab.layer_count".into(),
                    json!({ "value": layer_count, "source": source_of(sha256) }),
                );
                let entry = inputs
                    .entry("spectra".to_string())
                    .or_insert_with(|| json!({ "attributes": {} }));
                if let Some(existing) = entry.get_mut("attributes").and_then(Value::as_object_mut) {
                    existing.extend(attributes);
                }
            }
            "schedule" => {
                let document: Value = serde_json::from_slice(bytes)
                    .map_err(|error| format!("schedule document: {error}"))?;
                let irradiation = document
                    .get("irradiation_s")
                    .and_then(Value::as_str)
                    .ok_or("schedule document has no irradiation_s")?;
                let cooling = document
                    .get("cooling_s")
                    .and_then(Value::as_str)
                    .ok_or("schedule document has no cooling_s")?;
                facts.insert(
                    "schedule.irradiation".into(),
                    json!({ "value": { "value": irradiation, "unit": "s" }, "source": source_of(sha256) }),
                );
                facts.insert(
                    "schedule.cooling".into(),
                    json!({ "value": { "value": cooling, "unit": "s" }, "source": source_of(sha256) }),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged_paths() -> BTreeMap<String, String> {
        INPUT_SLOTS
            .iter()
            .map(|slot| ((*slot).to_string(), format!("in/{slot}")))
            .collect()
    }

    #[test]
    fn arguments_map_every_slot_to_its_flag() {
        let arguments = arguments(&staged_paths(), &StepContext::default()).unwrap();
        assert_eq!(arguments[0], "in/script");
        for (flag, slot) in [
            ("--spectra", "spectra"),
            ("--materials", "materials"),
            ("--schedule", "schedule"),
            ("--actinv", "actinv"),
            ("--activation-library", "activation-library"),
            ("--activation-index", "activation-index"),
            ("--decay-primary", "decay-primary"),
            ("--decay-fallback", "decay-fallback"),
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
                .any(|pair| pair == ["--output", OUTPUTS[0].workspace_path])
        );
    }

    #[test]
    fn an_unaccepted_slot_is_refused() {
        let mut staged = staged_paths();
        staged.insert("candidate".into(), "in/candidate".into());
        assert!(
            arguments(&staged, &StepContext::default())
                .unwrap_err()
                .contains("candidate")
        );
    }

    fn result_document() -> Value {
        json!({
            "schema": SCHEMA,
            "totals": {
                "max_specific_activity": { "value": "13.646396", "unit": "Bq/g" },
                "total_decay_heat": { "value": "0.0000002201145", "unit": "W" },
            }
        })
    }

    #[test]
    fn claims_are_extracted_as_canonical_unquantified_quantities() {
        let outputs = BTreeMap::from([(
            "activation-result".to_string(),
            result_document().to_string().into_bytes(),
        )]);
        let claims = extract_claims(&outputs, &StepContext::default()).unwrap();
        let slots: Vec<&str> = claims
            .iter()
            .map(|claim| claim.output_slot.as_str())
            .collect();
        assert_eq!(
            slots,
            ["specific-activity", "decay-heat", "activation-result"]
        );
        assert_eq!(claims[0].claim["model"], json!("unquantified"));
        assert_eq!(claims[0].claim["nominal"]["value"], json!("13.646396"));
        assert_eq!(claims[0].claim["nominal"]["unit"], json!("Bq/g"));
        assert_eq!(
            claims[1].claim["nominal"]["value"],
            json!("0.0000002201145")
        );
        assert_eq!(claims[2].claim, json!({ "model": "unquantified" }));
    }

    #[test]
    fn wrong_schema_is_refused() {
        let document = json!({ "schema": "wrong", "totals": {} });
        let outputs = BTreeMap::from([(
            "activation-result".to_string(),
            document.to_string().into_bytes(),
        )]);
        assert!(
            extract_claims(&outputs, &StepContext::default())
                .unwrap_err()
                .contains("schema")
        );
    }

    fn spectra_bytes(layers: &[(&str, &str)]) -> Vec<u8> {
        let layers: Vec<Value> = layers
            .iter()
            .enumerate()
            .map(|(index, (material, thickness))| {
                json!({
                    "index": index,
                    "material": material,
                    "thickness_cm": thickness,
                })
            })
            .collect();
        json!({
            "schema": "avila.shielding/layer-spectra/v1",
            "layers": layers,
        })
        .to_string()
        .into_bytes()
    }

    fn schedule_bytes(irradiation_s: &str, cooling_s: &str) -> Vec<u8> {
        json!({
            "schema": "avila.shielding/irradiation-schedule/v1",
            "irradiation_s": irradiation_s,
            "cooling_s": cooling_s,
            "flux_scale": "1",
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn activation_facts_report_layer_count_padded_materials_and_schedule() {
        let staged = vec![
            (
                "spectra".to_string(),
                "application/json".to_string(),
                "spectra-sha".to_string(),
                spectra_bytes(&[("polyethylene", "40"), ("iron", "10")]),
            ),
            (
                "schedule".to_string(),
                "application/json".to_string(),
                "schedule-sha".to_string(),
                schedule_bytes("2592000", "86400"),
            ),
        ];
        let mut facts = serde_json::Map::new();
        let mut inputs = serde_json::Map::new();
        activation_facts(&staged, "invocation-sha", &mut facts, &mut inputs).unwrap();

        assert_eq!(facts["slab.layer_count"]["value"], json!(2));
        assert_eq!(
            facts["schedule.irradiation"]["value"],
            json!({ "value": "2592000", "unit": "s" })
        );
        assert_eq!(
            facts["schedule.cooling"]["value"],
            json!({ "value": "86400", "unit": "s" })
        );
        let attributes = &inputs["spectra"]["attributes"];
        assert_eq!(attributes["layer.1.material"], json!("polyethylene"));
        assert_eq!(attributes["layer.2.material"], json!("iron"));
        assert_eq!(attributes["layer.3.material"], json!("none"));
    }

    #[test]
    fn a_zero_thickness_layer_is_not_counted() {
        let staged = vec![(
            "spectra".to_string(),
            "application/json".to_string(),
            "spectra-sha".to_string(),
            spectra_bytes(&[("polyethylene", "40"), ("iron", "0")]),
        )];
        let mut facts = serde_json::Map::new();
        let mut inputs = serde_json::Map::new();
        activation_facts(&staged, "invocation-sha", &mut facts, &mut inputs).unwrap();
        assert_eq!(facts["slab.layer_count"]["value"], json!(1));
        // Padding still reflects every array entry positionally, including
        // the zero-thickness one, mirroring `transport_facts`.
        assert_eq!(
            inputs["spectra"]["attributes"]["layer.2.material"],
            json!("iron")
        );
    }
}
