# EL-01 r3 review closure

Reviewed the uncommitted r3 working tree based on `ce35f22`, following the
[r2 review](2026-09-25-engineering-language-r2-review.md). Specification byte
SHA-256 after the small clarifications described below:
`284f781645a0999b5f0befc245171da53a52199d0badabafe660c17815f282f6`.

**The six r2 findings are closed at the specification/fixture level. EL-01
is ready to proceed to EL-02's finite shared analysis and obligation
generation implementation.** This review does not establish implementation
correctness, runner integration, or independent language replay; those remain
the subsequent milestones in the [handoff](../ENGINEERING_LANGUAGE_HANDOFF.md).

## Closure evidence

1. **Operand order:** §10.1 preserves sequences and normalizes only declared
   sets. The previous subtraction reversal now gives different semantic
   hashes (`84e7dc11…` versus `3d309865…`). Reordering methods and their
   declared-set entries preserves identity. A body containing a forward
   reference is refused before the focused helper computes identity.
2. **Annotation positions:** the projection preserves declaration keys.
   Changing the scope parameters of propositions named `note`, `label`, or
   `description` changes identity; adding a kind named `label` also changes
   it. Declared method labels and kind glosses do not. An annotation placed
   inside a quantity type is refused rather than silently removed.
3. **Material propagation:** both clearance methods now require and return
   material. Independently deriving all five `ensures` expressions yields
   both their declared kinds and their complete relation maps. Following the
   actual positive clearance and heuristic arguments also retains
   `material: al-6061-t6` through the final output.
4. **Independence:** §7.E4 separates graph disjointness from measurement
   independence in both directions. The measurement library declares both
   obligations. The revised conflict fixture has a valid independence
   premise and a disjointness premise contradicted by recorded overlap;
   the latter blocks the plan without declaring statistical dependence.
5. **Nominal arithmetic:** the primitive domain is now explicitly
   exact/enclosure. The new nominal-arithmetic program attempts a kind-valid
   product with a nominal operand and specifies `unsupported`; no
   representative-selection rule or stronger claim is invented.
6. **Lifecycle scopes:** §10.2 collects both applicable scopes, accumulates
   expired/withdrawn reasons, refuses conflicting states at one key, and
   records absence explicitly. A method-level active entry cannot override
   a withdrawn library. The replay matrix includes these decisions.

The r3 lifecycle policy also explicitly treats `superseded` as a notice
rather than a refusal. This is the current experimental rule; an
implementation must follow it consistently with the specified handling of
expired, withdrawn, and absent material.

## Small clarifications made during review

- Restricted the remaining presentation-summary sentence to reordering
  **declared-set** arrays, matching §10.1.
- Explicitly classified the existing unavailable-input `binding.reason` as
  an annotation at that position. Without this clause, the strict position
  rule left the authored unavailability fixture's reason field unexplained.
- Specified that `Z` is uniform on `{0,1,2,3}` in the independence
  counterexample. Its conclusion requires that distribution.

These edits preserve the fixture calculations and library identities.

## Validation and limits

All 27 JSON documents passed the existing verifier's canonical reader:
24 programs, two libraries, and expectations. Program/expectation coverage
matches exactly. Both library document hashes, both semantic hashes, and all
24 program pins match. The thermal bounds `[3/10,2/5]` and measurement bounds
`[187/100,231/100]` were checked using exact fractions.

The focused review helper is available in the current workspace at
`/tmp/avila-core-el01-r3-review/check.py`:

```bash
python3 -B /tmp/avila-core-el01-r3-review/check.py
```

It transcribes r3's relevant schema roles for the authored examples and
exercises the previous identity counterexamples, annotation positions,
reference ordering, kind derivations, relation preservation, and arithmetic.
It is not the product's schema admission or language checker. Lifecycle,
nominal refusal, and independence closure were also checked against the
written rules and authored expectations, without claiming executions of an
unimplemented checker.

No builds, benchmarks, or engineering runs were launched. The focused script
completed in under one second. The unrelated draft case remains untouched.

## Next work for SWE-2

Commit the EL-01 specification, ADR, libraries, programs, expectations, and
all review notes together. Then implement EL-02 from the existing handoff:
one shared analysis operation, generic method application, related-type
checks, residual assumptions, generated obligations, and inspectable holes.

Make the concrete schemas and identity bodies enforce the written roles,
including the treatment of source maps and produced analysis/plan records.
Turn the identity and refusal examples into executable regression tests at
the actual shared API boundary. Keep the selected experimental profile and
the existing authority boundaries. Return the implementation for review
before progressing to EL-03 runner integration.
