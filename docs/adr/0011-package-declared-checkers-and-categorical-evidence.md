# ADR-0011: Package-declared checkers and categorical evidence

- Status: accepted for the local case runner; both schemas remain drafts
- Date: 2026-09-03
- Refines: ADR-0007 and ADR-0006 SC-3/SC-11 A6
- Extraction extended by [ADR-0016](0016-external-checker-numeric-uncertainty.md)
  with interval and numeric unquantified claims; the original decision below
  records the first exact/categorical boundary.

## Context

Core's first executable cases use Rust-owned, purpose-built adapters. That is
appropriate when an adapter must derive applicability facts or transform a
domain format, but requiring new Core code for every conventional command-line
checker makes the iteration loop unnecessarily slow and couples Core releases
to project-specific tools.

NCTForge provides the immediate pressure test. It can verify its own evidence
chain and emit a deterministic JSON summary. Core should bind and run that
checker without acquiring NCTForge's domain rules. The result also contains two
different kinds of evidence: an exact count that can support an explicit
requirement and a categorical qualification that must remain visible without
being disguised as a number or process exit status.

## Decision

1. **A case may declare a narrow external checker adapter.** The package
   includes a raw-byte-hashed document with role `external_checker_adapter`.
   Its `document_id`, internal `adapter_id`, and the execution's adapter id
   must agree. Built-in adapters remain available for richer mappings.

2. **The descriptor is a closed execution map.** Schema
   `avila.core/external-checker-adapter/v0.1-draft` fixes one capability type,
   the complete set of input slots, the exact ordered argument vector, declared
   output files and media types, one timeout, claim extraction rules, and
   limitations. Its verified raw-byte SHA-256 enters the portable invocation,
   receipt verification, and memoization identity. Arguments can be only
   literal strings, runner-staged input paths, or descriptor-declared output
   paths.

3. **Core invokes a bound executable directly.** The case package binds the
   executable bytes by SHA-256. The runner uses no shell, clears the
   environment, supplies no descriptor-defined host path, confines normalized
   relative input and output paths to a fresh workspace, rejects symbolic-link
   outputs, and collects only declared regular files. Receipts and all existing
   replay rules apply unchanged.

4. **Extraction is structural, not scientific.** A checker output used for
   extraction must be authoritative JSON. The descriptor may select values by
   JSON Pointer as either:

   - `exact`: a safe JSON integer or canonical exact-number string plus a fixed
     unit; or
   - `categorical`: a string contained in a nonempty closed set fixed by the
     descriptor.

   The producing checker owns the meaning and validation of those values. Core
   checks only the declared transport, shape, identity, and type boundary.

5. **Categorical evidence stays unquantified and may be required.** The draft evidence-claims
   schema permits an optional nonempty `value` on `model: unquantified` for a
   non-quantity role. Admission rejects categorical values on quantity roles.
   Core preserves the value in generated claims, the case report, the human
   summary, and the attempt log without lowering it into the numeric verdict
   kernel. A role may declare its complete `categorical_values` vocabulary; a
   contract may then apply an explicit `equals` or `in_set` requirement to that
   role. The categorical kernel returns `PASS` for a match, `FAIL` for a
   mismatch, and `NOT_EVALUATED` for missing, quarantined, duplicate, or
   out-of-qualification evidence. This is a compatible extension of the
   unreleased `v0.2-draft`, not a claim that the draft is stable.

6. **Scientific rejection is not execution failure.** A checker that verifies
   its inputs and emits an allowed rejection category exits successfully. A
   separately extracted exact or categorical value may then produce a Core
   `FAIL` under an authored requirement. Parse errors, evidence-chain mismatches, unknown
   categories, missing outputs, nonzero exits, and timeouts remain execution or
   evidence failures and cannot masquerade as scientific verdicts.

## Boundary

This is not a general plugin system, package manager, validator language, or
sandbox. It does not provide signatures, permissions, networking, resource
accounting, dynamic output discovery, arbitrary transformations, applicability
fact extraction, preflight, or package selection. External checkers receive no
environment variables through this descriptor. A checker needing those
features still requires a purpose-built adapter or a later protocol revision.

A descriptor's hash and an executable's hash establish byte identity, not
method qualification or scientific truth. Closed categories prevent silent
vocabulary expansion; they do not make the vocabulary authoritative.

## Consequences

- A project can add a deterministic machine-facing checker and a case package
  without adding project-specific logic to Core.
- Core's external boundary remains auditable: reviewers can see the exact
  executable, argv map, files, pointers, category set, timeout, and limitations.
- Numeric and categorical results can share one output artifact while remaining
  distinct claims with distinct semantic roles.
- Large or domain-rich outputs stay in their artifacts; reports and logs carry
  only compact claim records.
- The NCTForge JEFF-4.0 evidence-aware gate is the first cross-repository
  specimen. Its expected process succeeds, its qualification category is
  `transported_photon_kerma_rejected`, and its explicit zero-finding requirement
  evaluates `FAIL` from the separately extracted count.

## Acceptance checks

- Changing the descriptor bytes or executable bytes changes the planned
  invocation identity and prevents reuse of an older receipt; stale package
  bindings are refused before execution.
- An undeclared input, output, output slot, path escape, duplicate declaration,
  invalid pointer, unbounded timeout, symlink output, unknown category, or
  non-authoritative numeric output is refused.
- The runner never invokes a shell for a package-declared checker.
- A valid checker run produces a verified receipt and generated claims under
  the same binding and replay rules as a built-in adapter.
- A categorical result survives in the generated claims and attempt log. It
  cannot satisfy a quantitative requirement, but a role-owned closed
  vocabulary can feed an explicit categorical `equals` or `in_set`
  requirement.
- A checker may report scientific rejection with exit status zero while Core
  independently derives a technical `FAIL` from an exact claim.
