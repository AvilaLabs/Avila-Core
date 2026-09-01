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
        code: "CORE-A4301",
        title: "Nondeterminism not permitted",
        rule: "SC-5 and SC-6 R8",
        meaning: "A step uses a nondeterministic capability type, but the contract execution policy does not list every role that type produces under `permitted_nondeterministic_roles`. Permission is scoped by role and makes the type neither deterministic nor qualified.",
        next_action: "Add each produced role to the execution policy, accepting that execution memoization stays disabled for the step, or choose a deterministic or seeded-stochastic type. The owner is the policy owner.",
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
        title: "Review obligation incomplete",
        rule: "SC-6 R9",
        meaning: "An accountable-review capability type or its contract binding is structurally incomplete or contradictory: the type is not nondeterministic, hides or makes optional a presented input, emits anything but a single `unquantified` decision record, uses a quantitative decision role, or repeats a disposition; the step lacks its review binding, pins the eligibility policy without a revision and lowercase `sha256:` identity, or declares empty or duplicated independence constraints; or a non-review type carries a review binding.",
        next_action: "Complete the declaration at the reported pointer. Even then the compiler does not decide reviewer eligibility or fulfill the review; those are later admission checks.",
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
        next_action: "Remove the key, or declare it in the schema or capability type through a new registry snapshot. For parameters and factors the owner is the requester.",
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
        title: "Kind mismatch",
        rule: "SC-1 and SC-6 R6",
        meaning: "A requirement's `limit` or `tolerance` names a quantity kind different from the metric role's kind, or the metric role is not a quantity role at all.",
        next_action: "Set the kind to the metric role's kind, or choose a quantitative metric.",
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
        let source = include_str!("compile.rs");
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
