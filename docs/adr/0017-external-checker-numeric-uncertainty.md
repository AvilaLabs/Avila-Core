# ADR-0017: Numeric uncertainty from package-declared checkers

- Status: accepted for the local case runner; schemas remain drafts
- Date: 2026-09-08
- Refines: ADR-0011 extraction; uses the existing ADR-0006 uncertainty models

## Context

The verdict kernel and claims document already support interval-valued and
numeric unquantified evidence. Package-declared external checkers can currently
extract only exact quantities and categorical values. This disconnect forces a
provider of an estimated quantity to implement a purpose-built runner adapter
before it can preserve its numerical uncertainty through controlled execution.

Serializing a computed estimate as a canonical decimal does not make the
quantity exact. An `exact` claim reduces to a zero-width interval, so that
substitution can change a requirement outcome. A categorical provider verdict
would also bypass the quantitative requirement boundary.

## Decision

Extend the closed external-checker descriptor with two numeric extraction forms:

| Descriptor model | Required fields beyond output slot/id | Generated claim |
|---|---|---|
| `interval` | `lower_pointer`, `upper_pointer`, `unit`; optional `nominal_pointer` | Existing `interval` model with the selected bounds and optional nominal quantity |
| `unquantified` | `pointer`, `unit` | Existing `unquantified` model with a nominal quantity and no bounds |

Every pointer is a nonempty RFC 6901 pointer, validated like existing exact and
categorical pointers. An omitted nominal pointer stays omitted in the claim;
an explicit null pointer is invalid. Core does not invent a midpoint. Each
selected number uses the existing authoritative-number boundary: a safe JSON
integer or a canonical exact-number string, including reduced rational strings.
Ordinary floating-point JSON, noncanonical strings and nonnumeric values are
refused. The entire extraction document must remain authoritative JSON.

The fixed unit applies to every quantity within an extracted interval. Core's
existing admission and kernel checks own permitted claim models, quantity-role
and unit compatibility, ordered bounds, nominal containment, qualification and
requirement evaluation. Extraction transports the provider's assertion; it does
not establish a numerical error bound, probability, physical truth or method
qualification. An unquantified nominal remains a nominal guide and cannot
establish an enclosure merely because a qualification record is present.

The existing `categorical` descriptor continues to produce an unquantified
**string value** for a non-quantity role. The new numeric `unquantified` form
produces a **nominal quantity** instead. No conversion between these meanings
is implicit.

Execution paths, executable binding, argument mapping, cleared environment,
timeouts, output confinement and receipt verification are unchanged. The
descriptor's byte digest already enters invocation identity. The runner's
SC-12 change comparison must also compare that digest before selecting reuse;
the earlier comparison omitted it. This change closes that gap, so changing a
bound or nominal pointer invalidates receipt reuse. Reuse re-extracts claims
from verified output bytes under the current descriptor; bounds must survive
both fresh execution and reuse.

## Compatibility and migration

This refines the unreleased `external-checker-adapter/v0.1-draft` schema in
place, as ADR-0011 refined the unreleased evidence-claims schema. Existing exact
and categorical descriptors need no edits and retain their byte identities and
behavior. The claims schema, semantic profile and kernel models do not change.

Older runners reject the new descriptor model names rather than silently
lowering them to exact quantities. Packages using these forms must pin a runner
revision that implements this ADR. Switching a package from an exact claim to
an interval also requires its registry/output model declarations to permit that
claim, a newly bound descriptor and any appropriate method qualification.
Old receipts are not reused across changed descriptor identities.
Rust clients that exhaustively match the public `ExternalClaim` enum must
handle its two new variants when updating their pinned source dependency.

Coverage intervals, one-sided bound extraction, numeric uncertainty estimation,
arbitrary claims-file ingestion and domain applicability fact extraction remain
outside this change. They require their own scoped decisions.

## Acceptance evidence

`fixtures/external-checkers/numeric-uncertainty.adapter.json` and its output
are synthetic transport fixtures, not a validated scientific benchmark.
Portable tests exercise exact interval endpoints, optional nominal omission,
numeric unquantified rational values, unknown fields, malformed/missing pointers
and unsafe numbers. Existing exact/categorical tests retain their coverage.

On Unix, a synthetic external executable copies a bound input into a declared
output. Case-runner tests exercise fresh execution, verified receipts,
generated claims, qualification, requirement evaluation and logging:

- intervals entirely below, crossing and above a threshold produce the existing
  PASS, INCONCLUSIVE and FAIL states;
- missing/outside qualification, reversed bounds and an out-of-bounds nominal
  retain the existing NOT_EVALUATED boundary;
- a numeric unquantified estimate remains distinct from an exact result;
- committed-receipt reuse preserves the interval, a deliberate fresh repeat
  executes, and a changed extraction pointer invalidates reuse.

These are software boundary checks. The synthetic qualification record names
no scientific validation evidence and establishes no physical applicability.
