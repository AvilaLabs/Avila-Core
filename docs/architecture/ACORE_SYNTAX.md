# `.acore` textual front end — exploratory note

- Status: exploratory; not scheduled and not part of ADR-0006 acceptance
- Owner: compiler front end
- Governing decision: ADR-0006 SC-16
- Non-goal: new semantics. JSON remains the canonical interchange.

## Why it may eventually exist

Humans and software tools often iterate more effectively against a compact
grammar with stable spans than against large JSON documents. A textual surface
could render the same diagnostics as the form, CLI, JSON protocol, and egui
client while making quantities, bindings, and requirements easier to read.

That is a product hypothesis. Core should not build a language merely because a
compiler-like product can have one. The textual front end proceeds only after:

1. ADR-0006 semantics and canonical JSON fixtures are stable enough to lower
   into;
2. real authoring sessions identify repeated problems that forms and JSON do not
   solve adequately; and
3. a contract subset can lower without inventing hashes, provenance, defaults,
   or scientific values.

## Required properties

1. **One semantic model.** Every supported construct maps to an existing
   canonical record. The front end never constructs an admission or verdict.
2. **Subset honesty.** A file that omits information required by canonical JSON
   is an incomplete draft. `acore lower` returns typed findings or requires an
   explicit workspace snapshot; it does not fabricate values.
3. **Source identity.** External files enter the snapshot with explicit content
   digests supplied by the workspace loader. A pure parser does not read or hash
   an ambient path.
4. **Explicit provenance.** `provided_by`, authority, and assumption ownership
   are syntax, not comments interpreted as data.
5. **First-class quantities.** A quantity includes a value and unit. Bare values
   are accepted only for parameters whose types are genuinely unitless.
6. **No implicit semantic conversion.** Exact within-kind unit scaling is the
   only coercion; cross-kind conversions remain capabilities.
7. **Source maps.** Lowering emits byte spans to canonical record paths for
   stable diagnostics and repairs.
8. **Deterministic formatting.** `acore fmt` has one style. Comments are authoring
   metadata and their effect on document identity is specified separately from
   Semantic IR identity.
9. **Editions only if needed.** Syntax migration machinery follows an actual
   compatibility requirement; it is not pre-built.

## Illustrative contract subset

This example demonstrates readability only. It does **not** yet lower to a
released JSON schema byte for byte.

```acore
contract sam.bike_hook.001 revision 2 {
  question "Does the printed PLA wall hook hold a 12 kg bicycle without yielding?"
  status draft
  template hobby.print.static-strength@2 digest "sha256:..."

  input geometry: mech.geometry.component@1 {
    artifact "bracket_v2.step"
    digest "sha256:..."
    provided_by requester
  }

  input material: core.material.definition@1 {
    artifact "pla_generic.material.json"
    digest "sha256:..."
    provided_by requester
  }

  step fea: mech.structural.static-linear@1 {
    geometry = input.geometry
    material = input.material
    mesh_refinement_levels = 3
  }

  require R1 "Peak von Mises stress is at most 25 MPa" {
    metric fea.peak_von_mises
    comparison <= 25 MPa
    basis bounded coverage 0.95
    provided_by requester
  }

  policy sam.hobby@1
  complete when pass
}
```

The corresponding canonical contract additionally includes resolved role and
type references, immutable input identities, policy identity, template digest,
and any fields required by the then-current schema. Lowering fails closed if
those cannot be supplied from the file and explicit workspace snapshot.

## Predicate sketch

Predicates are the only expression-like surface and map one-to-one to ADR-0006
SC-7. Inputs are addressed by slot, and facts name an acceptable source class.

```acore
qualification qual.actinv.activation.0008 for package "sha256:..." {
  implements nuclear.activation.inventory@1
  scope parameter cooling_time in [24 h, _)
    and input material.material_class in { steel, copper_alloy }
    and fact nuclear_data_library
      from validated_input
      equals "TENDL-2023"
  exclude input material.contains in { Li, Be }
  expires 2027-08-31
  recognized_by org:example-lab
}
```

Unbounded range endpoints are omitted when lowered; canonical records do not use
`null` as absence.

## Diagnostic rendering

Every frontend renders the same `CoreDiagnostic`. A future textual renderer may
show:

```text
error[CORE-T2001]: unknown unit symbol `Mpa`
  --> sam.bike_hook.acore:29:22
   |
29 |     comparison <= 25 Mpa
   |                      ^^^
   |
   = owner: requester
   = requires confirmation: replace with `MPa`
```

Unit-symbol case is not generically machine-fixable. An automatic correction is
offered only for a governed typo alias whose meaning and scale are unique.

## Staging trigger

If authoring evidence justifies the language, begin with contract drafts only:

1. parser, formatter, lowering, source map, and diagnostics;
2. byte-equivalence fixtures for every fully specified supported contract;
3. only then templates, policies, kinds, roles, capability types,
   qualifications, and reuse rules; and
4. LSP and migration tooling after a compatibility need exists.

Until that trigger is met, forms, JSON, and typed patch operations remain the
supported authoring surfaces.
