//! Stable explanations for every finding code the compiler can emit.
//!
//! Consumers match codes, never wording. This catalog is the wording: what a
//! code means and what a bounded next action looks like. It is served by
//! `avila-core explain` and mirrored in `docs/architecture/DIAGNOSTICS.md`.

use serde::Serialize;

/// A human-readable explanation of one stable finding code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticExplanation {
    pub code: &'static str,
    pub title: &'static str,
    /// ADR-0006 clause or rule that owns the finding.
    pub rule: &'static str,
    pub meaning: &'static str,
    pub next_action: &'static str,
}

/// Every explained code, sorted by code.
pub const DIAGNOSTIC_CATALOG: &[DiagnosticExplanation] = &[
    DiagnosticExplanation {
        code: "CORE-A4201",
        title: "Nominal basis not permitted",
        rule: "SC-8 and SC-10",
        meaning: "A requirement's basis is `nominal`, which compares a nominal value and uses no uncertainty, but the contract execution policy does not permit that weakening. Every weakening is explicit in the contract.",
        next_action: "Set `permit_nominal_basis` to true in the execution policy, accepting that the verdict will visibly state that uncertainty was not used, or use a `bounded` or `enclosure` basis. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: "CORE-A4301",
        title: "Nondeterminism not permitted",
        rule: "SC-5 and SC-6 R8",
        meaning: "A step uses a nondeterministic capability type, but the contract execution policy does not list every role that type produces under `permitted_nondeterministic_roles`. Permission is scoped by role and makes the type neither deterministic nor qualified.",
        next_action: "Add each produced role to the execution policy, accepting that execution memoization stays disabled for the step, or choose a deterministic or seeded-stochastic type. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: "CORE-A4401",
        title: "Evidence outside its qualification envelope",
        rule: "SC-10 A7",
        meaning: "A bounded or enclosure requirement depends on a claim whose producing capability carries a qualification, and this run's facts fall outside that envelope or their position is unknown: a fact is missing, comes from a weaker source than the envelope requires, or a term evaluates false. The calculation may be fine; it is not qualified here, so the requirement is not evaluated on it.",
        next_action: "Bring the case inside the envelope, extend the qualification with validation evidence for the new range, or supply the missing fact from an admissible source. The owner is the method owner.",
    },
    DiagnosticExplanation {
        code: "CORE-A4402",
        title: "Unqualified evidence not permitted",
        rule: "SC-10 A7; ADR-0008 clause 5",
        meaning: "The contract's execution policy requires a qualification assessment on every bounded or enclosure requirement's evidence, and an admitted claim for this requirement's metric carries no qualification assessment at all. The calculation may be fine; nothing states where it may be trusted, so the requirement is not evaluated on it. A claim outside its stated envelope is `CORE-A4401` instead; this code is for evidence with no envelope statement whatsoever.",
        next_action: "Bind a qualification record for the producing capability, or supply evidence from a capability that already carries one. The owner is the method owner.",
    },
    DiagnosticExplanation {
        code: "CORE-A4403",
        title: "Unqualified evidence admitted",
        rule: "SC-10 A7; ADR-0008 clause 5",
        meaning: "A bounded or enclosure requirement's verdict was derived from admitted evidence that carries no qualification assessment, and the contract's execution policy does not require one. The verdict stands, but the report makes visible that the profile permitted unqualified evidence rather than silently treating it as validated.",
        next_action: "This is informational and does not block the verdict. Set `require_qualification` to true in the execution policy once the method owner has bound a qualification record, so the same gap would instead be refused. The owner is the policy owner.",
    },
    DiagnosticExplanation {
        code: "CORE-E7001",
        title: "Claims bind a different snapshot",
        rule: "SC-11",
        meaning: "The claims document names a compiled snapshot identity that differs from what the supplied contract and registry compile to, so nothing in it can be attributed to this campaign.",
        next_action: "Regenerate the claims against these documents, or supply the contract and registry the claims were produced for. The owner is the executor.",
    },
    DiagnosticExplanation {
        code: "CORE-E7002",
        title: "Claim names nothing in the snapshot",
        rule: "SC-11",
        meaning: "An attestation or claim names a contract input, workflow step, or output slot that the compiled snapshot does not have.",
        next_action: "Correct the identifier to one the compiled snapshot declares.",
    },
    DiagnosticExplanation {
        code: "CORE-E7101",
        title: "Artifact identity missing or malformed",
        rule: "SC-11 A1",
        meaning: "A contract input has no attested artifact, or an artifact identity is not a lowercase `sha256:` digest of 64 hex digits. Artifact bytes are not read in this slice; the identity is what later verification binds.",
        next_action: "Attest every contract input and give every artifact its exact digest.",
    },
    DiagnosticExplanation {
        code: "CORE-E7103",
        title: "Parent not admitted",
        rule: "SC-11 A3",
        meaning: "A claim was produced from a parent that is missing or quarantined. Admission is fail closed along the bound dataflow, so the claim is quarantined even when its own values are well formed.",
        next_action: "Admit the parent first: attest the missing input or repair the quarantined parent claim, then re-evaluate.",
    },
    DiagnosticExplanation {
        code: "CORE-E7201",
        title: "Claim rejected by type-level validation",
        rule: "SC-3 and SC-11 A6",
        meaning: "The claim's model is not permitted by the output slot, its shape does not satisfy its model, a quantity does not scale in the role's kind, a bound is inverted, a nominal lies outside its interval, a coverage is outside (0, 1], or the artifact media type differs from the declared one.",
        next_action: "Produce a claim in a model the output permits, with quantities in admitted units of the role's kind.",
    },
    DiagnosticExplanation {
        code: "CORE-E7301",
        title: "Duplicate claim for one output",
        rule: "SC-15",
        meaning: "More than one claim or attestation exists for a single output slot or contract input. A slot admits exactly one; every claim for it is quarantined rather than one being chosen.",
        next_action: "Keep exactly one claim per output slot; a rerun creates a new claim only after the previous one is withdrawn.",
    },
    DiagnosticExplanation {
        code: "CORE-R3101",
        title: "No compatible source",
        rule: "SC-6 R1",
        meaning: "A required input slot has no source carrying the same nominal role in an accepted media type; an explicit binding names a contract input that is not declared; a contract input names a role absent from the snapshot; or a step names a capability type absent from the snapshot.",
        next_action: "For a slot, apply one candidate from the `constrained_choice` repair: `declare_input:<role>` adds a contract input carrying the role, `add_step:<type>/<output>` adds a step of a capability type that produces it. For an unknown role or type, correct the reference or supply a snapshot that defines it.",
    },
    DiagnosticExplanation {
        code: "CORE-R3102",
        title: "Ambiguous source",
        rule: "SC-6 R1",
        meaning: "A required input slot has more than one compatible source. The compiler never chooses between them.",
        next_action: "Add an explicit binding for the slot naming one of the candidates in the `constrained_choice` repair.",
    },
    DiagnosticExplanation {
        code: "CORE-R3201",
        title: "Self dependency",
        rule: "SC-6 R5",
        meaning: "A step binds one of its own outputs to one of its own inputs.",
        next_action: "Bind the slot to a contract input or to another step's output.",
    },
    DiagnosticExplanation {
        code: "CORE-R3202",
        title: "Dependency cycle",
        rule: "SC-6 R5",
        meaning: "Bindings form a cycle among the primary and related steps. The compiler never repairs a graph by reordering or dropping work.",
        next_action: "Break the cycle by rebinding one of the listed steps.",
    },
    DiagnosticExplanation {
        code: "CORE-R3203",
        title: "Unknown dependency",
        rule: "SC-6 R5",
        meaning: "An explicit binding names a workflow step that does not exist, or a known step that declares no such output slot. A reference to a step whose capability type is absent from the snapshot is suppressed under that step's `CORE-R3101` instead.",
        next_action: "Correct the step id or output slot; the message lists the outputs the step declares.",
    },
    DiagnosticExplanation {
        code: "CORE-R3301",
        title: "Metric unbound",
        rule: "SC-6 R6",
        meaning: "A requirement names no metric source, or names one that cannot be resolved to a declared contract input or an existing step output.",
        next_action: "Set `metric` to a declared contract input or an existing step output.",
    },
    DiagnosticExplanation {
        code: "CORE-R3401",
        title: "Optional presentation gate incomplete",
        rule: "SC-6 R9",
        meaning: "An optional agent practicality stage or its contract binding is structurally incomplete or contradictory: it hides or makes optional a presented input, emits anything but one unquantified routing record, uses a quantitative routing role, repeats a disposition, lacks explicit instructions, pins its agent policy without a revision and lowercase `sha256:` identity, declares invalid independence constraints, or appears on a capability type that has no review declaration.",
        next_action: "Complete the declaration at the reported pointer. This only compiles an optional presentation gate; it is never a requirement-verdict input.",
    },
    DiagnosticExplanation {
        code: "CORE-R3501",
        title: "Registry snapshot incomplete",
        rule: "SC-1, SC-4, and SC-5",
        meaning: "The registry snapshot is internally inconsistent: a duplicate purpose; a role with no media type or claim model, or whose unit class does not match its kind; a slot naming an unknown role or a media type outside its role; an output claim model outside its role; an empty or non-comparable parameter or factor domain; an unknown quantity kind; or an excluded purpose that is unknown or repeated.",
        next_action: "Correct the snapshot at the reported pointer. The owner is the registry owner.",
    },
    DiagnosticExplanation {
        code: "CORE-R3601",
        title: "Unused contract input",
        rule: "SC-6 notices",
        meaning: "No step binds this contract input, so it would enter no campaign evidence. This is a notice and does not block compilation.",
        next_action: "Remove the input or bind it to a slot.",
    },
    DiagnosticExplanation {
        code: "CORE-R3602",
        title: "Unconsumed step",
        rule: "SC-6 notices",
        meaning: "This non-review step's outputs feed neither another step nor a requirement, so executing it would produce evidence nothing uses. This is a notice and does not block compilation.",
        next_action: "Remove the step, bind one of its outputs, or name one as a requirement metric.",
    },
    DiagnosticExplanation {
        code: "CORE-S1101",
        title: "Undeclared field",
        rule: "SC-2 authoritative documents; SC-6 R7 and R8",
        meaning: "A document contains a key its schema does not define, or a step supplies a parameter or material execution factor its capability type does not declare. Undeclared values never become implicit defaults.",
        next_action: "Remove the key, or declare it in the schema or capability type through a new registry snapshot.",
    },
    DiagnosticExplanation {
        code: "CORE-S1102",
        title: "Structural or canonical-value violation",
        rule: "SC-2",
        meaning: "A value violates the canonical profile or a structural rule: a binary floating-point JSON number, `null`, a non-NFC string, an unsafe integer, a non-canonical decimal or rational, an empty identifier, a zero revision, an empty workflow or requirement list, a duplicate identifier, a coverage outside `(0, 1]` or on a non-bounded basis, a negative tolerance, or a schema or semantic-profile header the compiler does not support.",
        next_action: "Fix the value at the reported pointer. When the finding carries a `mechanically_safe` repair, its single candidate is the unique canonical form; apply it verbatim.",
    },
    DiagnosticExplanation {
        code: "CORE-S1103",
        title: "Duplicate object key",
        rule: "SC-2 canonical JSON",
        meaning: "An object repeats a key. Canonical JSON forbids duplicates because a reader could not know which value was signed.",
        next_action: "Remove or rename the repeated key at the reported pointer.",
    },
    DiagnosticExplanation {
        code: "CORE-S1301",
        title: "Parameter not defined",
        rule: "SC-6 R7",
        meaning: "A required parameter is absent, or a parameter is the reserved placeholder `not_defined`. In a `draft` this is a `missing` finding; once the contract is `in_review`, `approved`, or `retired` it is `invalid`. The placeholder never enters compiled IR.",
        next_action: "Supply a value of the declared family and domain. The owner is the requester.",
    },
    DiagnosticExplanation {
        code: "CORE-T2001",
        title: "Unknown unit symbol",
        rule: "SC-1",
        meaning: "The unit symbol is not admitted for the quantity kind by the snapshot. Symbols are case-sensitive because SI prefix case changes magnitude: `Mpa` is not `MPa`, and `usv/h` is not `uSv/h`.",
        next_action: "Choose one of the admitted symbols listed in the `constrained_choice` repair. Case is corrected automatically only for a governed typo alias, which the current snapshot format does not yet carry.",
    },
    DiagnosticExplanation {
        code: "CORE-T2101",
        title: "Nominal role mismatch",
        rule: "SC-4 and SC-6 R2",
        meaning: "An explicitly bound source carries a role whose identity or major version differs from the destination slot's role. Roles are nominal: equal dimensions do not make quantities interchangeable.",
        next_action: "Bind a source carrying the required role, or route through a cross-kind conversion capability owned by a method owner.",
    },
    DiagnosticExplanation {
        code: "CORE-T2102",
        title: "Metric domain mismatch",
        rule: "SC-1 and SC-6 R6",
        meaning: "A quantitative requirement uses the wrong quantity kind or a non-quantity role. A categorical requirement uses a quantity role, a role without a closed vocabulary, or predicate values outside that vocabulary.",
        next_action: "Choose a metric with the required domain, or make the requirement match the metric role's governed quantity kind or categorical vocabulary.",
    },
    DiagnosticExplanation {
        code: "CORE-T2103",
        title: "Unit outside kind",
        rule: "SC-1 and SC-6 R6",
        meaning: "The unit is a known symbol, but it belongs to a different quantity kind than the metric kind. Exact scaling within one kind is the only implicit conversion.",
        next_action: "Use a unit of the metric kind; conversion between kinds is a separately qualified capability.",
    },
    DiagnosticExplanation {
        code: "CORE-T2104",
        title: "Equality tolerance",
        rule: "SC-6 R6 and SC-10",
        meaning: "An `equal` comparison has no tolerance quantity, so the verdict calculus could never evaluate it; or a comparison other than `equal` carries a tolerance, which would silently mean nothing.",
        next_action: "Add a nonnegative tolerance of the metric kind to the equality requirement, or remove the tolerance from the inequality.",
    },
    DiagnosticExplanation {
        code: "CORE-T2201",
        title: "Claim model insufficient",
        rule: "SC-3 and SC-6 R3",
        meaning: "No claim model the metric source may emit can satisfy the requirement's basis. A `bounded` basis needs an exact, interval, coverage-interval, or correctly sided worst-case claim; an `enclosure` basis needs an exact or interval claim; a `nominal` basis needs a claim that carries a nominal value.",
        next_action: "Choose a metric source whose type permits a sufficient model, or change the basis only if the requirement genuinely accepts a weaker claim. The package-level half of this rule is checked later at binding.",
    },
    DiagnosticExplanation {
        code: "CORE-T2203",
        title: "Claim model irreducible",
        rule: "SC-3 and SC-6 R3",
        meaning: "The metric source permits only claim models the semantic kernel cannot reduce: standard uncertainty, samples, or a distribution.",
        next_action: "Insert an uncertainty expansion or reduction capability between the source and the requirement; the `method_owner_judgment` repair names the candidate type.",
    },
    DiagnosticExplanation {
        code: "CORE-T2301",
        title: "Media type not accepted",
        rule: "SC-6 R4",
        meaning: "A contract input's media type is outside its role, or a bound source's media type is not accepted by the destination slot.",
        next_action: "Use a media type the role or slot accepts, or bind a different source.",
    },
    DiagnosticExplanation {
        code: "CORE-T2401",
        title: "Wrong value family",
        rule: "SC-6 R7 and R8",
        meaning: "A parameter or material execution factor is authored in the wrong family. The five families are boolean, signed 64-bit integer, exact-number string, text, and a quantity object with an exact string `value` and a `unit` of the declared kind. The compiler does not coerce.",
        next_action: "Author the value in the declared family; a quantity must use an admitted unit of the declared kind.",
    },
    DiagnosticExplanation {
        code: "CORE-T2402",
        title: "Value outside domain",
        rule: "SC-6 R7 and R8",
        meaning: "A parameter or material execution factor is well typed but outside its declared domain: an integer, exact-number, or quantity bound, or a finite text choice set.",
        next_action: "Choose a value inside the domain; for text, one of the candidates in the `constrained_choice` repair.",
    },
    DiagnosticExplanation {
        code: "CORE-T2501",
        title: "Reproducibility binding",
        rule: "SC-5 and SC-6 R8",
        meaning: "A seeded-stochastic step lacks a nonempty seed; a deterministic or nondeterministic step carries a seed that is not part of its invocation identity; or a type-declared material execution factor is unbound or still `not_defined`.",
        next_action: "Bind exactly the seed and factors the capability type declares. The owner is the requester.",
    },
    DiagnosticExplanation {
        code: "CORE-T2601",
        title: "Governed purpose",
        rule: "SC-4 and SC-6 R10",
        meaning: "A requirement names a purpose absent from the snapshot at that exact identity and major version, or the metric source's output explicitly excludes that purpose. Purpose identity is nominal: no prefix, hierarchy, or prose inference applies.",
        next_action: "Name a purpose the snapshot defines, or choose a metric source whose output does not exclude it. Absence of an exclusion is not a positive qualification claim.",
    },
];

/// Looks up the explanation for a stable finding code.
#[must_use]
pub fn explain(code: &str) -> Option<&'static DiagnosticExplanation> {
    DIAGNOSTIC_CATALOG.iter().find(|entry| entry.code == code)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::diagnostic::COMPILER_FINDING_CODES;

    #[test]
    fn catalog_is_sorted_unique_and_complete() {
        let codes: Vec<_> = DIAGNOSTIC_CATALOG.iter().map(|entry| entry.code).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            codes, sorted,
            "catalog must be sorted by code without repeats"
        );
        let declared: BTreeSet<_> = COMPILER_FINDING_CODES.iter().copied().collect();
        let explained: BTreeSet<_> = codes.iter().copied().collect();
        assert_eq!(
            declared, explained,
            "every declared code is explained and vice versa"
        );
        for entry in DIAGNOSTIC_CATALOG {
            assert!(
                !entry.title.is_empty() && !entry.rule.is_empty(),
                "{}",
                entry.code
            );
            assert!(
                !entry.meaning.is_empty() && !entry.next_action.is_empty(),
                "{}",
                entry.code
            );
        }
    }

    #[test]
    fn every_code_the_compiler_source_emits_is_declared() {
        let source = [
            include_str!("compile/mod.rs"),
            include_str!("compile/findings.rs"),
            include_str!("compile/notices.rs"),
            include_str!("compile/registry.rs"),
            include_str!("compile/reproducibility.rs"),
            include_str!("compile/requirements.rs"),
            include_str!("compile/resolve.rs"),
            include_str!("compile/review.rs"),
            include_str!("compile/shape.rs"),
            include_str!("compile/source.rs"),
            include_str!("compile/values.rs"),
            include_str!("campaign/mod.rs"),
            include_str!("campaign/admission.rs"),
            include_str!("campaign/verdicts.rs"),
        ]
        .concat();
        let mut referenced = BTreeSet::new();
        for (start, _) in source.match_indices("CORE_") {
            let token: String = source[start..]
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            if token.len() == "CORE_X0000".len() {
                referenced.insert(token.replacen('_', "-", 1));
            }
        }
        assert!(!referenced.is_empty());
        for code in referenced {
            assert!(
                explain(&code).is_some(),
                "{code} is emitted but not explained"
            );
        }
    }

    #[test]
    fn diagnostics_document_lists_every_code() {
        let document = include_str!("../../../docs/architecture/DIAGNOSTICS.md");
        for entry in DIAGNOSTIC_CATALOG {
            assert!(
                document.contains(entry.code),
                "{} is missing from DIAGNOSTICS.md",
                entry.code
            );
            assert!(
                document.contains(entry.title),
                "{} title is missing from DIAGNOSTICS.md",
                entry.code
            );
        }
    }
}
