//! The case-specific adapter for Aftermatter's frozen R0 inventory builder,
//! which drives ACTINV 1.0.1.
//!
//! The executable is a Python interpreter. The builder script, the ACTINV
//! engine, and its decay-dump helper are hash-bound staged inputs, as are the
//! frozen FNS spectrum and the five ACTINV data-release files. The builder
//! validates and runs ACTINV, then writes the problem it generated, the
//! normalized inventory, and the decay metadata at fixed repository-relative
//! paths, so the staging layout must be exactly the one the builder expects.
//! The adapter checks that layout and extracts each output as an
//! unquantified artifact; it does not interpret the inventory.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

use super::{AdapterOutput, ExtractedClaim};

pub const ADAPTER_ID: &str = "avila-labs.aftermatter/build-r0-case@1";
pub const CAPABILITY_TYPE_ID: &str = "aftermatter.r0-inventory-build";
pub const CAPABILITY_TYPE_MAJOR: u64 = 1;
pub const INPUT_SLOTS: &[&str] = &[
    "builder",
    "actinv-executable",
    "actinv-dump-helper",
    "spectrum",
    "activation-library",
    "library-index",
    "decay-primary",
    "decay-fallback",
    "data-notice",
];
pub const OUTPUTS: &[AdapterOutput] = &[
    AdapterOutput {
        output_id: "problem",
        workspace_path: "cases/r0/input/actinv-problem.json",
        media_type: "application/vnd.actinv.problem+json",
    },
    AdapterOutput {
        output_id: "inventory",
        workspace_path: "cases/r0/reference/actinv-result.json",
        media_type: "application/vnd.aftermatter.inventory+json",
    },
    AdapterOutput {
        output_id: "decay-metadata",
        workspace_path: "cases/r0/reference/decay-half-lives.json",
        media_type: "application/vnd.aftermatter.decay-metadata+json",
    },
];
pub const OUTPUT_SLOTS: &[&str] = &["problem", "inventory", "decay-metadata"];
pub const TIMEOUT: Duration = Duration::from_secs(1_800);

/// The builder locates itself, the spectrum, and its outputs relative to the
/// directory above `tools/`, and it expects the data release at fixed names
/// beneath one root.
pub const BUILDER_PATH: &str = "tools/build_r0_case.py";
pub const SPECTRUM_PATH: &str = "cases/r0/input/fns-spectrum.json";
pub const DATA_LAYOUT: &[(&str, &str)] = &[
    (
        "activation-library",
        "activation/tendl-2025-neutron-709g.npz",
    ),
    (
        "library-index",
        "activation/tendl-2025-neutron-709g_index.json",
    ),
    ("decay-primary", "decay/endf-b-viii-0_decay.dat"),
    ("decay-fallback", "decay/jeff-3-3_decay.dat"),
    ("data-notice", "ACTINV-DATA-NOTICE.md"),
];
/// ACTINV prepares its activation library under this directory; the runner
/// sets it inside the workspace so nothing outside is read or written.
pub const CACHE_DIR: &str = ".cache/actinv";

pub fn environment() -> BTreeMap<String, String> {
    BTreeMap::from([("ACTINV_CACHE_DIR".to_string(), CACHE_DIR.to_string())])
}

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
    let builder = path("builder")?;
    if builder != BUILDER_PATH {
        return Err(format!(
            "the builder must be staged at `{BUILDER_PATH}`, not `{builder}`; it locates the workspace from its own path"
        ));
    }
    let spectrum = path("spectrum")?;
    if spectrum != SPECTRUM_PATH {
        return Err(format!(
            "the frozen spectrum must be staged at `{SPECTRUM_PATH}`, not `{spectrum}`"
        ));
    }
    let data_root = data_root(staged)?;
    Ok(vec![
        builder,
        "--actinv".into(),
        path("actinv-executable")?,
        "--dump".into(),
        path("actinv-dump-helper")?,
        "--data-root".into(),
        data_root,
    ])
}

/// The single directory beneath which every data-release file is staged at
/// the name the builder pins.
fn data_root(staged: &BTreeMap<String, String>) -> Result<String, String> {
    let mut root: Option<String> = None;
    for (slot, relative) in DATA_LAYOUT {
        let staged_path = staged
            .get(*slot)
            .ok_or_else(|| format!("input slot `{slot}` is not staged"))?;
        let Some(prefix) = staged_path.strip_suffix(relative) else {
            return Err(format!(
                "input slot `{slot}` must be staged as `<data-root>/{relative}`, not `{staged_path}`"
            ));
        };
        let Some(prefix) = prefix.strip_suffix('/') else {
            return Err(format!(
                "input slot `{slot}` must be staged beneath a data root, not at `{staged_path}`"
            ));
        };
        match &root {
            None => root = Some(prefix.to_string()),
            Some(existing) if existing == prefix => {}
            Some(existing) => {
                return Err(format!(
                    "data-release files are staged under both `{existing}` and `{prefix}`; the builder needs one root"
                ));
            }
        }
    }
    root.filter(|root| !root.is_empty())
        .ok_or_else(|| "the data-release root must be a directory beneath the workspace".into())
}

pub fn extract_claims(
    outputs: &BTreeMap<String, Vec<u8>>,
    _parameters: &BTreeMap<String, Value>,
) -> Result<Vec<ExtractedClaim>, String> {
    let mut claims = Vec::with_capacity(OUTPUTS.len());
    for (output, key, expected) in [
        ("problem", "spec", "actinv-spec-1"),
        ("inventory", "schema", "aftermatter-inventory-1"),
        ("decay-metadata", "schema", "aftermatter-decay-metadata-1"),
    ] {
        let bytes = outputs
            .get(output)
            .ok_or_else(|| format!("output `{output}` was not collected"))?;
        let document: Value = serde_json::from_slice(bytes)
            .map_err(|error| format!("output `{output}` is not valid JSON: {error}"))?;
        if document.get(key).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "output `{output}` does not declare `{key}` = `{expected}`"
            ));
        }
        claims.push(ExtractedClaim {
            output_slot: output.into(),
            output_id: output.into(),
            claim: serde_json::json!({ "model": "unquantified" }),
        });
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> BTreeMap<String, String> {
        let mut staged = BTreeMap::from([
            ("builder".to_string(), BUILDER_PATH.to_string()),
            ("actinv-executable".to_string(), "tools/actinv".to_string()),
            ("actinv-dump-helper".to_string(), "tools/dump".to_string()),
            ("spectrum".to_string(), SPECTRUM_PATH.to_string()),
        ]);
        for (slot, relative) in DATA_LAYOUT {
            staged.insert(
                (*slot).to_string(),
                format!(".data/actinv/v1.0.0/{relative}"),
            );
        }
        staged
    }

    #[test]
    fn arguments_pin_the_builder_layout() {
        let arguments = arguments(&layout()).unwrap();
        assert_eq!(arguments[0], BUILDER_PATH);
        assert_eq!(arguments.last().unwrap(), ".data/actinv/v1.0.0");
        assert!(arguments.iter().all(|arg| !arg.starts_with('/')));
        assert_eq!(environment()["ACTINV_CACHE_DIR"], CACHE_DIR);
    }

    #[test]
    fn a_wrong_layout_is_refused() {
        let mut staged = layout();
        staged.insert("spectrum".into(), "in/spectrum.json".into());
        assert!(arguments(&staged).unwrap_err().contains("frozen spectrum"));

        let mut staged = layout();
        staged.insert(
            "decay-fallback".into(),
            "elsewhere/decay/jeff-3-3_decay.dat".into(),
        );
        assert!(arguments(&staged).unwrap_err().contains("one root"));

        let mut staged = layout();
        staged.insert("library-index".into(), "index.json".into());
        assert!(arguments(&staged).unwrap_err().contains("<data-root>"));
    }

    #[test]
    fn extraction_checks_each_document_kind() {
        let outputs = BTreeMap::from([
            (
                "problem".to_string(),
                br#"{"spec":"actinv-spec-1"}"#.to_vec(),
            ),
            (
                "inventory".to_string(),
                br#"{"schema":"aftermatter-inventory-1"}"#.to_vec(),
            ),
            (
                "decay-metadata".to_string(),
                br#"{"schema":"aftermatter-decay-metadata-1"}"#.to_vec(),
            ),
        ]);
        let claims = extract_claims(&outputs, &BTreeMap::new()).unwrap();
        assert_eq!(claims.len(), 3);
        assert!(
            claims
                .iter()
                .all(|claim| claim.claim["model"] == "unquantified")
        );

        let mut wrong = outputs.clone();
        wrong.insert("inventory".into(), br#"{"schema":"other"}"#.to_vec());
        assert!(
            extract_claims(&wrong, &BTreeMap::new())
                .unwrap_err()
                .contains("inventory")
        );
    }
}
