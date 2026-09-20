//! Built-in adapter for the one-dimensional breeding-blanket transport
//! capability (`avila-labs.blanket/slab-breeding@1`): tritium breeding ratio,
//! fast fluence and nuclear heating in the coil pack, helium production and
//! displacement damage in the vacuum vessel, radial build and Li-6 fraction,
//! plus per-layer neutron spectra for a downstream activation step. The
//! script reports Monte Carlo means with standard deviations; this adapter
//! turns each into a `coverage_interval` claim at 1.96 standard deviations
//! (coverage 0.95) so the compiled contract judges statistical error, and it
//! refuses a tally with zero hits rather than report 0 +/- 0. Applicability
//! facts (breeder thickness, multiplier thickness, Li-6 fraction, total
//! build, source geometry) are extracted so a qualification envelope bound to
//! validation evidence can fire on the breeding row.

use std::collections::BTreeMap;
use std::time::Duration;

use avila_core_kernel::{lower_authored_decimal, read_authoritative_decimal};
use serde_json::{Value, json};

use super::{AdapterOutput, ExtractedClaim, StepContext};

pub const ADAPTER_ID: &str = "avila-labs.blanket/slab-breeding@1";
pub const TYPE_ID: &str = "blanket.slab-breeding";
pub const INPUT_SLOTS: &[&str] = &[
    "script",
    "candidate",
    "materials",
    "source",
    "cross-section-index",
    "groups",
];
pub const OUTPUTS: &[AdapterOutput] = &[
    AdapterOutput {
        output_id: "transport-result",
        workspace_path: "outputs/transport-result.json",
        media_type: "application/vnd.avila.blanket-transport+json",
    },
    AdapterOutput {
        output_id: "layer-spectra",
        workspace_path: "outputs/layer-spectra.json",
        media_type: "application/vnd.avila.shield-layer-spectra+json",
    },
];
pub const OUTPUT_SLOTS: &[&str] = &[
    "tritium-breeding-ratio",
    "coil-fast-fluence-per-fpy",
    "coil-heating",
    "vessel-helium",
    "vessel-dpa",
    "radial-build",
    "li6-enrichment",
    "layer-spectra",
    "transport-result",
];
pub const TIMEOUT: Duration = Duration::from_secs(7_200);
pub const SCHEMA: &str = "avila.blanket/transport-result/v2";
pub const LAYER_SPECTRA_SCHEMA: &str = "avila.shielding/layer-spectra/v1";
pub const ENVIRONMENT_KEYS: &[&str] = &["OPENMC_CROSS_SECTIONS"];
const Z: f64 = 1.96;
const COVERAGE: &str = "0.95";

pub fn environment() -> BTreeMap<String, String> {
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

fn integer_parameter(context: &StepContext, name: &str) -> Result<i64, String> {
    context
        .parameters
        .get(name)
        .and_then(|value| value.get("value"))
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("compiled step carries no integer parameter `{name}`"))
}

pub fn arguments(
    staged_paths: &BTreeMap<String, String>,
    context: &StepContext,
) -> Result<Vec<String>, String> {
    for slot in staged_paths.keys() {
        if !INPUT_SLOTS.contains(&slot.as_str()) {
            return Err(format!(
                "input slot `{slot}` is not accepted by adapter {ADAPTER_ID}"
            ));
        }
    }
    let seed = context.seed.clone().ok_or(
        "the blanket transport step must bind a seed; the capability is seeded-stochastic",
    )?;
    Ok(vec![
        staged(staged_paths, "script")?,
        "--candidate".into(),
        staged(staged_paths, "candidate")?,
        "--materials".into(),
        staged(staged_paths, "materials")?,
        "--source".into(),
        staged(staged_paths, "source")?,
        "--groups".into(),
        staged(staged_paths, "groups")?,
        "--particles".into(),
        integer_parameter(context, "particles")?.to_string(),
        "--batches".into(),
        integer_parameter(context, "batches")?.to_string(),
        "--seed".into(),
        seed,
        "--threads".into(),
        "8".into(),
        "--output".into(),
        OUTPUTS[0].workspace_path.into(),
        "--layer-spectra-output".into(),
        OUTPUTS[1].workspace_path.into(),
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

/// A number from the script (JSON float, integer or decimal text) as an
/// authoritative canonical decimal. Rust's `Display` for `f64` never uses an
/// exponent, so the text is a plain decimal before it is lowered.
fn number(document: &Value, pointer: &str) -> Result<f64, String> {
    let value = document
        .pointer(pointer)
        .ok_or_else(|| format!("transport result has no value at `{pointer}`"))?;
    match value {
        Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| format!("`{pointer}` is not a finite number")),
        Value::String(s) => s
            .parse::<f64>()
            .map_err(|_| format!("`{pointer}` `{s}` is not a number")),
        _ => Err(format!("`{pointer}` is not a number")),
    }
}

fn quantity(value: f64, unit: &str) -> Result<Value, String> {
    if !value.is_finite() {
        return Err("non-finite quantity".into());
    }
    let text = format!("{value}");
    let lowered = lower_authored_decimal(&text).map_err(|error| error.detail().to_string())?;
    read_authoritative_decimal(&lowered).map_err(|error| error.detail().to_string())?;
    Ok(json!({ "value": lowered, "unit": unit }))
}

fn coverage_claim(
    slot: &str,
    mean: f64,
    sd: f64,
    scale: f64,
    unit: &str,
) -> Result<ExtractedClaim, String> {
    if mean == 0.0 && sd == 0.0 {
        return Err(format!(
            "`{slot}`: the tally has zero hits at this history count; undetermined, not zero"
        ));
    }
    let mean = mean * scale;
    let sd = sd * scale;
    let lower = (mean - Z * sd).max(0.0);
    Ok(ExtractedClaim {
        output_slot: slot.into(),
        output_id: "transport-result".into(),
        claim: json!({
            "model": "coverage_interval",
            "lower": quantity(lower, unit)?,
            "upper": quantity(mean + Z * sd, unit)?,
            "nominal": quantity(mean, unit)?,
            "coverage": COVERAGE,
        }),
    })
}

fn exact_claim(slot: &str, value: f64, unit: &str) -> Result<ExtractedClaim, String> {
    Ok(ExtractedClaim {
        output_slot: slot.into(),
        output_id: "transport-result".into(),
        claim: json!({ "model": "exact", "nominal": quantity(value, unit)? }),
    })
}

pub fn extract_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    _context: &StepContext,
) -> Result<Vec<ExtractedClaim>, String> {
    let result = document(outputs, "transport-result", SCHEMA)?;
    document(outputs, "layer-spectra", LAYER_SPECTRA_SCHEMA)?;
    let fluence = number(&result, "/coil/fast_fluence_per_fpy_n_cm2")?;
    let flux = number(&result, "/coil/fast_flux_per_source_neutron_cm2")?;
    let flux_sd = number(&result, "/coil/fast_flux_sd")?;
    let fluence_sd = if flux > 0.0 {
        flux_sd / flux * fluence
    } else {
        0.0
    };
    Ok(vec![
        coverage_claim(
            "tritium-breeding-ratio",
            number(&result, "/tbr/value")?,
            number(&result, "/tbr/sd")?,
            1.0,
            "1",
        )?,
        coverage_claim(
            "coil-fast-fluence-per-fpy",
            fluence,
            fluence_sd,
            1.0,
            "n/cm2",
        )?,
        coverage_claim(
            "coil-heating",
            number(&result, "/coil/heating_W_cm3_at_wall_load")?,
            number(&result, "/coil/heating_sd_W_cm3")?,
            1.0,
            "W/cm3",
        )?,
        coverage_claim(
            "vessel-helium",
            number(&result, "/vessel/helium_appm_per_fpy")?,
            number(&result, "/vessel/helium_appm_sd")?,
            1.0,
            "appm",
        )?,
        coverage_claim(
            "vessel-dpa",
            number(&result, "/vessel/dpa_per_fpy")?,
            number(&result, "/vessel/dpa_sd")?,
            1.0,
            "dpa",
        )?,
        exact_claim("radial-build", number(&result, "/radial_build_cm")?, "cm")?,
        exact_claim("li6-enrichment", number(&result, "/li6_enrichment")?, "1")?,
        ExtractedClaim {
            output_slot: "layer-spectra".into(),
            output_id: "layer-spectra".into(),
            claim: json!({ "model": "unquantified" }),
        },
        ExtractedClaim {
            output_slot: "transport-result".into(),
            output_id: "transport-result".into(),
            claim: json!({ "model": "unquantified" }),
        },
    ])
}

fn layer_thickness(layer: &Value) -> Result<f64, String> {
    match layer.get("thickness_cm") {
        Some(Value::Number(n)) => n.as_f64().ok_or("thickness is not finite".into()),
        Some(Value::String(s)) => s
            .parse::<f64>()
            .map_err(|_| format!("thickness `{s}` is not a number")),
        _ => Err("layer has no thickness_cm".into()),
    }
}

/// Applicability facts from the staged candidate, material table and source:
/// `blanket.total_thickness` (every non-coil layer), `blanket.breeder_thickness`
/// (layers of lithium-bearing materials), `blanket.multiplier_thickness`
/// (layers whose material has role `multiplier` or contains beryllium),
/// `blanket.li6_enrichment`, `blanket.layer_count`, `source.geometry`; and the
/// candidate's `layer.N.material` attributes.
pub fn facts(
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
    let mut table: BTreeMap<String, Value> = BTreeMap::new();
    for (slot, _, _, bytes) in staged {
        if slot == "materials" {
            let document: Value = serde_json::from_slice(bytes)
                .map_err(|error| format!("materials document: {error}"))?;
            match document.get("materials") {
                Some(Value::Object(map)) => {
                    table = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                }
                Some(Value::Array(items)) => {
                    for item in items {
                        if let Some(id) = item.get("id").and_then(Value::as_str) {
                            table.insert(id.to_string(), item.clone());
                        }
                    }
                }
                _ => return Err("materials document has no `materials`".into()),
            }
        }
    }
    for (slot, _, sha256, bytes) in staged {
        match slot.as_str() {
            "source" => {
                facts.insert(
                    "source.geometry".into(),
                    json!({ "value": "plane", "source": source_of(sha256) }),
                );
                facts.insert(
                    "source.energy".into(),
                    json!({ "value": { "value": "14.1", "unit": "MeV" }, "source": source_of(sha256) }),
                );
            }
            "candidate" => {
                let document: Value = serde_json::from_slice(bytes)
                    .map_err(|error| format!("candidate document: {error}"))?;
                let layers = document
                    .get("layers")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let (mut total, mut breeder, mut multiplier) = (0.0_f64, 0.0_f64, 0.0_f64);
                let mut attributes = serde_json::Map::new();
                for (index, layer) in layers.iter().enumerate() {
                    let material = layer.get("material").and_then(Value::as_str).unwrap_or("");
                    let thickness = layer_thickness(layer)?;
                    let spec = table.get(material);
                    let role = spec
                        .and_then(|s| s.get("role"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let lithium = spec
                        .and_then(|s| s.get("lithium"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let beryllium = spec
                        .and_then(|s| s.pointer("/composition/elements/Be"))
                        .is_some();
                    if role != "coil" {
                        total += thickness;
                    }
                    if lithium {
                        breeder += thickness;
                    }
                    if role == "multiplier" || beryllium {
                        multiplier += thickness;
                    }
                    attributes.insert(format!("layer.{}.material", index + 1), json!(material));
                }
                let li6 = match document.get("li6_enrichment") {
                    Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
                    Some(Value::String(s)) => s.parse::<f64>().unwrap_or(0.0),
                    _ => 0.0,
                };
                for (name, value, unit) in [
                    ("blanket.total_thickness", total, "cm"),
                    ("blanket.breeder_thickness", breeder, "cm"),
                    ("blanket.multiplier_thickness", multiplier, "cm"),
                    ("blanket.li6_enrichment", li6, "1"),
                ] {
                    facts.insert(
                        name.into(),
                        json!({ "value": quantity(value, unit)?, "source": source_of(sha256) }),
                    );
                }
                facts.insert(
                    "blanket.layer_count".into(),
                    json!({ "value": layers.len(), "source": source_of(sha256) }),
                );
                let entry = inputs
                    .entry("candidate".to_string())
                    .or_insert_with(|| json!({ "attributes": {} }));
                if let Some(existing) = entry.get_mut("attributes").and_then(Value::as_object_mut) {
                    existing.extend(attributes);
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

    fn context() -> StepContext {
        StepContext {
            parameters: BTreeMap::from([
                (
                    "particles".to_string(),
                    json!({ "type": "integer", "value": 40000 }),
                ),
                (
                    "batches".to_string(),
                    json!({ "type": "integer", "value": 10 }),
                ),
            ]),
            seed: Some("1".into()),
        }
    }

    fn result() -> Value {
        json!({
            "schema": SCHEMA,
            "tbr": { "value": 1.4558, "sd": 0.00185 },
            "coil": { "fast_fluence_per_fpy_n_cm2": 2.5e17, "fast_flux_per_source_neutron_cm2": 1.79e-8,
                      "fast_flux_sd": 1.6e-9, "heating_W_cm3_at_wall_load": 0.00023, "heating_sd_W_cm3": 0.000015 },
            "vessel": { "helium_appm_per_fpy": 0.0033, "helium_appm_sd": 0.0007, "dpa_per_fpy": 0.005, "dpa_sd": 0.0005 },
            "radial_build_cm": 123.0,
            "li6_enrichment": 0.6
        })
    }

    #[test]
    fn arguments_bind_script_seed_and_parameters() {
        let staged: BTreeMap<String, String> = INPUT_SLOTS
            .iter()
            .map(|slot| ((*slot).to_string(), format!("in/{slot}")))
            .collect();
        let args = arguments(&staged, &context()).unwrap();
        assert_eq!(args[0], "in/script");
        assert!(args.windows(2).any(|p| p == ["--seed", "1"]));
        assert!(args.windows(2).any(|p| p == ["--particles", "40000"]));
        assert!(args.windows(2).any(|p| p == ["--groups", "in/groups"]));
    }

    #[test]
    fn claims_are_coverage_intervals_with_stated_coverage() {
        let outputs = BTreeMap::from([
            (
                "transport-result".to_string(),
                result().to_string().into_bytes(),
            ),
            (
                "layer-spectra".to_string(),
                json!({ "schema": LAYER_SPECTRA_SCHEMA, "layers": [] })
                    .to_string()
                    .into_bytes(),
            ),
        ]);
        let claims = extract_claims(&outputs, &context()).unwrap();
        assert_eq!(claims.len(), 9);
        let tbr = claims
            .iter()
            .find(|c| c.output_slot == "tritium-breeding-ratio")
            .unwrap();
        assert_eq!(tbr.claim["model"], json!("coverage_interval"));
        assert_eq!(tbr.claim["coverage"], json!("0.95"));
        assert_eq!(tbr.claim["nominal"]["value"], json!("1.4558"));
        let fluence = claims
            .iter()
            .find(|c| c.output_slot == "coil-fast-fluence-per-fpy")
            .unwrap();
        assert_eq!(fluence.claim["nominal"]["unit"], json!("n/cm2"));
        assert!(
            fluence.claim["upper"]["value"]
                .as_str()
                .unwrap()
                .starts_with("29")
        );
        let build = claims
            .iter()
            .find(|c| c.output_slot == "radial-build")
            .unwrap();
        assert_eq!(build.claim["model"], json!("exact"));
        assert_eq!(build.claim["nominal"]["value"], json!("123"));
    }

    #[test]
    fn zero_hit_tally_is_refused() {
        let mut doc = result();
        doc["coil"]["heating_W_cm3_at_wall_load"] = json!(0.0);
        doc["coil"]["heating_sd_W_cm3"] = json!(0.0);
        let outputs = BTreeMap::from([
            ("transport-result".to_string(), doc.to_string().into_bytes()),
            (
                "layer-spectra".to_string(),
                json!({ "schema": LAYER_SPECTRA_SCHEMA, "layers": [] })
                    .to_string()
                    .into_bytes(),
            ),
        ]);
        let error = extract_claims(&outputs, &context()).unwrap_err();
        assert!(error.contains("zero hits"), "{error}");
    }

    #[test]
    fn facts_measure_breeder_and_multiplier_thickness() {
        let materials = json!({ "materials": {
            "pbli": { "role": "breeder", "lithium": true, "composition": { "elements": { "Pb": "0.83", "Li": "0.17" } } },
            "beryllium_pebbles": { "role": "multiplier", "composition": { "elements": { "Be": "1" } } },
            "wc": { "role": "shield", "composition": { "elements": { "W": "0.5", "C": "0.5" } } },
            "ref_coil_pack": { "role": "coil", "composition": { "elements": { "Cu": "1" } } }
        }});
        let candidate = json!({ "li6_enrichment": "0.9", "layers": [
            { "material": "beryllium_pebbles", "thickness_cm": "5" }, { "material": "pbli", "thickness_cm": "50" },
            { "material": "wc", "thickness_cm": "20" }, { "material": "ref_coil_pack", "thickness_cm": "52.5" } ] });
        let staged = vec![
            (
                "materials".to_string(),
                "inputs/materials.json".to_string(),
                "sha256:m".to_string(),
                materials.to_string().into_bytes(),
            ),
            (
                "candidate".to_string(),
                "inputs/candidate.json".to_string(),
                "sha256:c".to_string(),
                candidate.to_string().into_bytes(),
            ),
            (
                "source".to_string(),
                "inputs/source.json".to_string(),
                "sha256:s".to_string(),
                b"{}".to_vec(),
            ),
        ];
        let mut f = serde_json::Map::new();
        let mut i = serde_json::Map::new();
        facts(&staged, "abc", &mut f, &mut i).unwrap();
        assert_eq!(f["blanket.total_thickness"]["value"]["value"], json!("75"));
        assert_eq!(
            f["blanket.breeder_thickness"]["value"]["value"],
            json!("50")
        );
        assert_eq!(
            f["blanket.multiplier_thickness"]["value"]["value"],
            json!("5")
        );
        assert_eq!(f["blanket.li6_enrichment"]["value"]["value"], json!("0.9"));
        assert_eq!(f["source.geometry"]["value"], json!("plane"));
        assert_eq!(
            i["candidate"]["attributes"]["layer.2.material"],
            json!("pbli")
        );
    }
}
