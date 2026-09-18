//! `fixtures-check`: the coverage counting rule from
//! `fixtures/semantic-core/README.md` made executable. The README's
//! required-fixture tables are the ADR-0006 acceptance plan; this command
//! parses them, indexes every committed fixture and vector identity, and
//! reports per family which required names exist. A name is `covered`
//! when its exact identity is in the corpus, `named` when the plan row
//! references a vector set that exists, `unbounded` when it is a pattern
//! that needs an enumerated domain (`*`, `<each>`, `..` outside a numeric
//! range), and otherwise `absent`. Fixtures that reference a diagnostic
//! code defined by neither catalog are always reported and fail the
//! check; absent or unbounded names fail only under `--strict`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::Path;

use avila_core_compiler::DIAGNOSTIC_CATALOG;
use avila_core_runner::RUNTIME_DIAGNOSTIC_CATALOG;
use serde::Serialize;

const README: &str = include_str!("../../../fixtures/semantic-core/README.md");

const VECTOR_SETS: [&[u8]; 4] = [
    include_bytes!("../../../fixtures/semantic-core/vectors/canon.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/unit-scaling.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/scope-predicates.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/vectors/verdict-calculus.v1.json"),
];

const FIXTURE_SETS: [&[u8]; 8] = [
    include_bytes!("../../../fixtures/semantic-core/types/compiler-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/types/compiler-parameter-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/types/compiler-reproducibility-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/types/compiler-review-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/types/compiler-purpose-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/campaigns/campaign-cases.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/defects/defects.v1.json"),
    include_bytes!("../../../fixtures/semantic-core/authority/authority-cases.v1.json"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Coverage {
    Covered,
    Named,
    Unbounded,
    Absent,
}

#[derive(Debug, Serialize)]
struct RequiredFixture {
    family: String,
    name: String,
    coverage: Coverage,
    covered_by: Option<String>,
}

#[derive(Debug, Serialize)]
struct FamilyReport {
    family: String,
    covered: usize,
    named: usize,
    unbounded: usize,
    absent: usize,
}

#[derive(Debug, Serialize)]
struct FixturesCheckReport {
    notice: &'static str,
    families: Vec<FamilyReport>,
    absent: Vec<String>,
    unbounded: Vec<String>,
    undefined_codes: Vec<String>,
    totals: FamilyReport,
}

/// One entry of the plan a row names; a single README cell can expand to
/// several (ranges `A1..A10`, alternatives `a/b`, suffixes `pass/fail`,
/// and shorthand follow-ups like `` `types.R8.seeded-bound.pass` /
/// `missing-seed.fail` `` where the second name inherits the first's
/// stem prefix).
fn expand_names(cell: &str) -> Vec<String> {
    const FAMILY_PREFIXES: [&str; 16] = [
        "kinds",
        "numerics",
        "uncertainty",
        "roles",
        "types",
        "scope",
        "policy",
        "lifecycle",
        "verdict",
        "admission",
        "change",
        "campaign",
        "authority",
        "ownership",
        "canon",
        "scenarios",
    ];
    let mut names = Vec::new();
    let mut prefix: Option<String> = None;
    for quoted in cell.split('`').skip(1).step_by(2) {
        let quoted = quoted.trim();
        if quoted.is_empty() || quoted.contains(char::is_whitespace) {
            continue;
        }
        let quoted = if let Some(prefix) = &prefix
            && !FAMILY_PREFIXES.contains(&quoted.split('.').next().unwrap_or(""))
        {
            format!("{prefix}.{quoted}")
        } else {
            quoted.to_string()
        };
        for name in expand_suffixes(&quoted) {
            names.extend(expand_ranges(&name));
        }
        if let Some(first) = names.first()
            && let Some(dot) = first.rfind('.')
            && let Some(stem_dot) = first[..dot].rfind('.')
        {
            prefix = Some(first[..stem_dot].to_string());
        }
    }
    names
}

/// `foo.pass/fail` → `foo.pass`, `foo.fail`; `a.b.x/y.pass` expands the
/// stem's alternatives and shares the suffix.
fn expand_suffixes(name: &str) -> Vec<String> {
    let Some(dot) = name.rfind('.') else {
        return expand_stem_alternatives(name);
    };
    let (stem, suffix) = (&name[..dot], &name[dot + 1..]);
    if !stem.contains('/') && !suffix.contains('/') {
        return vec![name.to_string()];
    }
    let mut out = Vec::new();
    for stem in expand_stem_alternatives(stem) {
        for option in suffix.split('/') {
            out.push(format!("{stem}.{option}"));
        }
    }
    out
}

/// `a.b.x/y/z` → `a.b.x`, `a.b.y`, `a.b.z`: non-first segments share the
/// first segment's dotted prefix.
fn expand_stem_alternatives(stem: &str) -> Vec<String> {
    if !stem.contains('/') {
        return vec![stem.to_string()];
    }
    let parts: Vec<&str> = stem.split('/').collect();
    let prefix = parts[0]
        .rfind('.')
        .map(|dot| &parts[0][..=dot])
        .unwrap_or("");
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if index == 0 {
                (*part).to_string()
            } else {
                format!("{prefix}{part}")
            }
        })
        .collect()
}

/// `admission.A1..A10.pass` → `admission.A1.pass` … `admission.A10.pass`.
/// A `..` whose endpoints do not share a label is not a numeric range;
/// the name is left alone so it reports as unbounded.
fn expand_ranges(name: &str) -> Vec<String> {
    let Some(range) = name.find("..") else {
        return vec![name.to_string()];
    };
    let start = name[..range]
        .rfind(|c: char| !(c.is_ascii_digit() || c.is_ascii_uppercase()))
        .map(|index| index + 1)
        .unwrap_or(0);
    let from_token = &name[start..range];
    let after = &name[range + 2..];
    let to_len = after
        .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit()))
        .unwrap_or(after.len());
    let to_token = &after[..to_len];
    let tail = &after[to_len..];
    let label = |token: &str| {
        token
            .chars()
            .take_while(|c| c.is_ascii_uppercase())
            .collect::<String>()
    };
    let from_label = label(from_token);
    if from_label.is_empty() || !to_token.starts_with(&from_label) {
        return vec![name.to_string()];
    }
    let (Ok(from), Ok(to)) = (
        from_token[from_label.len()..].parse::<u64>(),
        to_token[from_label.len()..].parse::<u64>(),
    ) else {
        return vec![name.to_string()];
    };
    let width = from_token.len() - from_label.len();
    (from..=to)
        .map(|number| {
            format!(
                "{}{}{:0width$}{}",
                &name[..start],
                from_label,
                number,
                tail,
                width = width
            )
        })
        .collect()
}

/// Parse the README's `### family/` sections: each table row's first cell
/// carries the required names; the second column's backtick-quoted
/// `*.v1` references name an existing vector set.
fn parse_plan() -> Vec<(String, String, String)> {
    let mut rows = Vec::new();
    let mut family = String::new();
    for line in README.lines() {
        let line = line.trim();
        if line.starts_with("## ") && !line.starts_with("### ") {
            family.clear();
            continue;
        }
        if let Some(header) = line.strip_prefix("### ") {
            family = header
                .split(['/', ' '])
                .next()
                .unwrap_or_default()
                .to_string();
            continue;
        }
        if !line.starts_with('|') || family.is_empty() {
            continue;
        }
        let cells: Vec<&str> = line
            .split('|')
            .skip(1)
            .map(str::trim)
            .filter(|cell| !cell.is_empty())
            .collect();
        if cells.len() < 2 || cells[0].starts_with("---") || cells[0] == "Fixture" {
            continue;
        }
        rows.push((
            family.clone(),
            cells[0].to_string(),
            cells.get(1).copied().unwrap_or_default().to_string(),
        ));
    }
    rows
}

/// The committed corpus: every fixture identity, vector identity and set
/// name, every test file a row may credit instead of a corpus fixture,
/// and every diagnostic code a fixture references.
struct Corpus {
    fixture_ids: BTreeSet<String>,
    vector_ids: BTreeSet<String>,
    vector_sets: BTreeSet<String>,
    test_ids: BTreeSet<String>,
    referenced_codes: BTreeSet<String>,
}

fn corpus() -> Result<Corpus, Box<dyn Error>> {
    let mut index = Corpus {
        fixture_ids: BTreeSet::new(),
        vector_ids: BTreeSet::new(),
        vector_sets: BTreeSet::new(),
        test_ids: BTreeSet::new(),
        referenced_codes: BTreeSet::new(),
    };
    // Test files can pin a row the corpus cannot express as data — a
    // workspace-level check like the verdict-construction boundary is a
    // test, not a fixture. Referencing one by file stem credits the row
    // `named` (not `covered` — coverage still means a corpus identity).
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
    for crate_dir in fs::read_dir(&tests_dir)? {
        let crate_dir = crate_dir?.path();
        let tests = crate_dir.join("tests");
        if tests.is_dir() {
            for file in fs::read_dir(&tests)? {
                let file = file?.path();
                if file.extension().and_then(|e| e.to_str()) == Some("rs")
                    && let Some(stem) = file.file_stem().and_then(|s| s.to_str())
                {
                    index.test_ids.insert(stem.to_string());
                }
            }
        }
        // Unit tests inside `src/` pin rows too — behavior-level checks
        // that need the crate's internals. Index each `#[test]` fn by name
        // so a plan row can credit it the same way it credits a file stem.
        let src = crate_dir.join("src");
        if src.is_dir() {
            index_test_fns(&src, &mut index.test_ids)?;
        }
    }
    // The independent Python verifier pins rows too — index each
    // `def test_*` in `verifier/test_verifier.py` by name.
    let verifier_tests =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../verifier/test_verifier.py");
    if verifier_tests.is_file() {
        let text = fs::read_to_string(&verifier_tests)?;
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("def test_")
                && let Some(name) = rest.split('(').next()
            {
                index.test_ids.insert(format!("test_{}", name.trim()));
            }
        }
    }
    for bytes in VECTOR_SETS {
        let document: serde_json::Value = serde_json::from_slice(bytes)?;
        index.vector_sets.insert(
            document["vector_set"]
                .as_str()
                .ok_or("vector set is missing vector_set")?
                .to_string(),
        );
        for key in ["vectors", "aggregation_vectors", "categorical_vectors"] {
            for vector in document
                .get(key)
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(id) = vector["id"].as_str() {
                    index.vector_ids.insert(id.to_string());
                }
            }
        }
    }
    for bytes in FIXTURE_SETS {
        let document: serde_json::Value = serde_json::from_slice(bytes)?;
        for fixture in document["fixtures"]
            .as_array()
            .ok_or("fixture set is missing fixtures")?
        {
            if let Some(id) = fixture["fixture_id"].as_str() {
                index.fixture_ids.insert(id.to_string());
            }
            collect_codes(&fixture["expected"], &mut index.referenced_codes);
        }
    }
    Ok(index)
}

/// Index `#[test]` fn names under a crate's `src/` recursively, so a plan
/// row can credit a behavior-level unit test by name.
fn index_test_fns(dir: &Path, test_ids: &mut BTreeSet<String>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            index_test_fns(&path, test_ids)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let mut is_test = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[test]") {
                is_test = true;
                continue;
            }
            if is_test && trimmed.starts_with("fn ") {
                let name = trimmed[3..]
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or_default();
                if !name.is_empty() {
                    test_ids.insert(name.to_string());
                }
                is_test = false;
            } else if trimmed.starts_with("#[") || trimmed.is_empty() {
                continue;
            } else {
                is_test = false;
            }
        }
    }
    Ok(())
}

/// Every `CORE-Xnnnn` string in a fixture's expected body — finding codes,
/// verdict reasons, notes.
fn collect_codes(value: &serde_json::Value, codes: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::String(text) => {
            for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
                if word.starts_with("CORE-")
                    && word.len() == 10
                    && word.chars().nth(5).is_some_and(|c| c.is_ascii_uppercase())
                {
                    codes.insert(word.to_string());
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_codes(item, codes);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_codes(item, codes);
            }
        }
        _ => {}
    }
}

/// Plan-row names that map to vector identities by dropping a family
/// prefix: `verdict.X` → vector `X`, `scope.pred.X` → vector `X`.
fn vector_name(name: &str) -> Option<&str> {
    for prefix in ["verdict.", "scope.pred.", "canon."] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    None
}

fn is_unbounded(name: &str) -> bool {
    name.contains('*') || name.contains("<each>") || name.contains("..") || name.ends_with('.')
}

pub fn fixtures_check(strict: bool, json_output: bool) -> Result<i32, Box<dyn Error>> {
    let index = corpus()?;
    let plan = parse_plan();

    let catalog: BTreeSet<&str> = DIAGNOSTIC_CATALOG
        .iter()
        .chain(RUNTIME_DIAGNOSTIC_CATALOG.iter())
        .map(|entry| entry.code)
        .collect();
    let undefined_codes: Vec<String> = index
        .referenced_codes
        .iter()
        .filter(|code| !catalog.contains(code.as_str()))
        .cloned()
        .collect();

    let mut required: Vec<RequiredFixture> = Vec::new();
    for (family, cell, expected) in &plan {
        for name in expand_names(cell) {
            if is_unbounded(&name) {
                required.push(RequiredFixture {
                    family: family.clone(),
                    name,
                    coverage: Coverage::Unbounded,
                    covered_by: None,
                });
                continue;
            }
            let covered_by = if index.fixture_ids.contains(&name) {
                Some(name.clone())
            } else if let Some(rest) = vector_name(&name)
                && index.vector_ids.contains(rest)
            {
                Some(rest.to_string())
            } else {
                None
            };
            let named = expected
                .split('`')
                .skip(1)
                .step_by(2)
                .map(|set| {
                    let set = set.strip_suffix(".json").unwrap_or(set);
                    set.strip_suffix(".v1").unwrap_or(set)
                })
                .map(|reference| reference.strip_suffix(".rs").unwrap_or(reference))
                .any(|reference| {
                    index.vector_sets.contains(reference)
                        || index.vector_ids.contains(reference)
                        || index.fixture_ids.contains(reference)
                        || index.test_ids.contains(reference)
                });
            let coverage = if covered_by.is_some() {
                Coverage::Covered
            } else if named {
                Coverage::Named
            } else {
                Coverage::Absent
            };
            required.push(RequiredFixture {
                family: family.clone(),
                name,
                coverage,
                covered_by,
            });
        }
    }

    let mut families: BTreeMap<String, FamilyReport> = BTreeMap::new();
    let mut totals = FamilyReport {
        family: "all".to_string(),
        covered: 0,
        named: 0,
        unbounded: 0,
        absent: 0,
    };
    for entry in &required {
        let report = families
            .entry(entry.family.clone())
            .or_insert_with(|| FamilyReport {
                family: entry.family.clone(),
                covered: 0,
                named: 0,
                unbounded: 0,
                absent: 0,
            });
        let count = match entry.coverage {
            Coverage::Covered => &mut report.covered,
            Coverage::Named => &mut report.named,
            Coverage::Unbounded => &mut report.unbounded,
            Coverage::Absent => &mut report.absent,
        };
        *count += 1;
        match entry.coverage {
            Coverage::Covered => totals.covered += 1,
            Coverage::Named => totals.named += 1,
            Coverage::Unbounded => totals.unbounded += 1,
            Coverage::Absent => totals.absent += 1,
        }
    }

    let absent: Vec<String> = required
        .iter()
        .filter(|entry| entry.coverage == Coverage::Absent)
        .map(|entry| entry.name.clone())
        .collect();
    let unbounded: Vec<String> = required
        .iter()
        .filter(|entry| entry.coverage == Coverage::Unbounded)
        .map(|entry| entry.name.clone())
        .collect();
    let report = FixturesCheckReport {
        notice: "The ADR-0006 acceptance plan is not met until every required name is covered and no name is absent; `named` and `unbounded` credit plan references only.",
        families: families.into_values().collect(),
        absent,
        unbounded,
        undefined_codes,
        totals,
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "{:<14} {:>8} {:>6} {:>9} {:>7}",
            "family", "covered", "named", "unbounded", "absent"
        );
        for family in &report.families {
            println!(
                "{:<14} {:>8} {:>6} {:>9} {:>7}",
                family.family, family.covered, family.named, family.unbounded, family.absent
            );
        }
        println!(
            "{:<14} {:>8} {:>6} {:>9} {:>7}",
            "all",
            report.totals.covered,
            report.totals.named,
            report.totals.unbounded,
            report.totals.absent
        );
        if !report.absent.is_empty() {
            println!("\nabsent:");
            for name in &report.absent {
                println!("  {name}");
            }
        }
        if !report.undefined_codes.is_empty() {
            println!("\nundefined codes referenced by fixtures:");
            for code in &report.undefined_codes {
                println!("  {code}");
            }
        }
    }

    if !report.undefined_codes.is_empty() || (strict && report.totals.absent > 0) {
        return Ok(1);
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_names_parses_every_plan_shorthand() {
        assert_eq!(
            expand_names("`types.R3.worst-case-side-sufficient.pass/fail`"),
            vec![
                "types.R3.worst-case-side-sufficient.pass",
                "types.R3.worst-case-side-sufficient.fail"
            ]
        );
        assert_eq!(
            expand_names("`admission.A1..A3.pass`"),
            vec![
                "admission.A1.pass",
                "admission.A2.pass",
                "admission.A3.pass"
            ]
        );
        assert_eq!(
            expand_names("`uncertainty.reduce.exact/interval.pass`"),
            vec![
                "uncertainty.reduce.exact.pass",
                "uncertainty.reduce.interval.pass"
            ]
        );
        // A shorthand follow-up inherits the first name's stem prefix.
        assert_eq!(
            expand_names("`types.R8.seeded-bound.pass` / `missing-seed.fail`"),
            vec!["types.R8.seeded-bound.pass", "types.R8.missing-seed.fail"]
        );
    }

    #[test]
    fn fixtures_check_parses_the_plan_and_reports_coverage() {
        let index = corpus().unwrap();
        let plan = parse_plan();
        assert!(!plan.is_empty());
        assert!(!index.fixture_ids.is_empty());
        assert!(!index.vector_ids.is_empty());
        // Every diagnostic code a fixture references is defined by one of
        // the two catalogs.
        let catalog: BTreeSet<&str> = DIAGNOSTIC_CATALOG
            .iter()
            .chain(RUNTIME_DIAGNOSTIC_CATALOG.iter())
            .map(|entry| entry.code)
            .collect();
        for code in &index.referenced_codes {
            assert!(catalog.contains(code.as_str()), "undefined code {code}");
        }
    }
}
