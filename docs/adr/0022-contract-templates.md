# ADR-0022: Contract templates and instantiation records

- Status: proposed 2026-09-19

## Context

SC-9.2 and SC-9.5 ratify the template semantics:

- Template instantiation is **immutable origin metadata, not a contract
  status**. An instance pins a template digest, parameters, case inputs,
  and eligibility evaluation.
- Templates declare **typed parameters and domains, workflow,
  requirements, policy floor, eligibility, validation cases, owner,
  version, and signature**. An instance may tighten but not loosen the
  policy floor.
- SC-9.4 adds the amendment rule: a semantic edit to a non-draft
  contract produces a new draft version with a `supersedes` edge. A
  template amendment is a new template version; instances pin their
  template identity and are never rewritten.

None of it exists. A `contract_template` document type does not exist;
a contract cannot record that it was instantiated; eligibility has no
record form. Ten `lifecycle.*` fixture rows name the machinery, and
their cited diagnostic codes (`CORE-A4401`, `CORE-A4405`, `CORE-A4201`)
were aspirational — all three are allocated to other checks today, so
this ADR assigns the free `CORE-A48xx` family.

Same posture as every other record family: **the engine does not
instantiate — it verifies the recorded instantiation.** The template
declares its parameters, domains, floor, eligibility, and validation
cases; the instance pins exact identities; the compiler checks the
binding is inside every declared boundary.

## Proposed decision

### 1. A `contract_template` document declares the reusable shape

`avila.core/contract-template/v0.1-draft`. Fields, all required unless
noted:

- `template_id`, `template_revision`, `schema_version`,
  `semantic_profile`, `owner`, `status` (the same closed contract-status
  vocabulary — a template is drafted, reviewed, approved, retired under
  ADR-0021 transitions).
- `parameters`: typed parameter declarations reusing the exact
  `ParameterDefinition` machinery capability types already use
  (`parameter_id`, `required`, `value_type` with `min`/`max`/
  `allowed_values`/`kind` domains) — no second parameter vocabulary.
- `inputs`: the declared input shapes an instance must fill —
  `{ input_id, role, media_type, claim_model }`. Eligibility rules
  address these fields; the instance's own attributes and metadata are
  its own. *(Added in implementation — the draft named eligibility
  rules over "contract input" claims without declaring the inputs.)*
- `workflow`: the step template, a list of `{ step_id, capability_type,
  bindings?, parameters }` where `parameters` may bind template
  parameter references (`{"ref": "param_id"}`) as well as literals.
  Carried as raw JSON — a step cannot be typed until its references are
  substituted at materialization.
- `requirements`: the requirement template, the same shape with
  parameter references permitted in `limit` and `tolerance`.
- `policy_floor`: the `execution_policy` fields the instance must
  declare at least as strict.
- `eligibility`: an ordered list of eligibility rules — each
  `{ rule_id, input_id, predicate }` where `predicate` is a closed
  expression over the contract input's declared claim. The implemented
  grammar is `input_field_in` (`field` in `media_type | claim_model |
  role`, membership over `values`), `attribute_in` (membership over the
  input's declared `attributes` — the ADR-0025 "contract_input fact"),
  `attribute_in_range` (an exact-number `min`/`max` bound over an
  attribute), and `all`/`any`/`not` composition under strong Kleene
  truth. Eligibility is decidable: every predicate is a comparison over
  declared fields, never free text.
- `validation_cases`: a list of `{ case_id, parameters, inputs,
  expected }` — parameter bindings and input references the template
  declares must compile into a contract; `expected` names the verdict
  statuses the case should produce. A validation case is evidence the
  template's own shape compiles, not a scientific qualification.
- The template owner's signature is a package-bound `signature`
  document over the template's digest, verified under the supplied
  trust root exactly as every other record — detached, not embedded.
  *(The draft named an embedded `signature` field; detached binding is
  the committed signature discipline.)*

### 2. An `instantiation` record pins the origin

`avila.core/contract-instantiation/v0.1-draft`, carried as a package
document and named by the instantiated contract's `instantiated_from`
field (origin metadata — it never becomes a status). The pin direction
is one-way: the *record* digest-pins the contract, the contract names
the record by `instantiation_id` — a digest in both directions is a
cycle no document can satisfy. Fields:

- `instantiation_id`, `schema_version`, `semantic_profile`.
- `template`: `{ template_id, template_revision, sha256 }` — the exact
  template digest. A later template amendment does not touch the
  instance: the pin is immutable, and the compiler emits a
  `template_superseded` *notice* when a newer template revision exists —
  information, never a gate.
- `parameters`: the bound value per declared parameter id.
- `inputs`: the case-input bindings the instance fills.
- `eligibility`: the recorded per-rule evaluation
  (`{ rule_id, outcome }`, `outcome` in `eligible | ineligible |
  unknown`) plus the aggregate the rules imply.
- `contract`: `{ contract_id, revision, sha256 }` — the instantiated
  contract's identity.
- The instantiator's signature is likewise a package-bound `signature`
  document over the record's digest.

### 3. Compile-time checks against every declared boundary

A contract compiled with an `instantiated_from` record is checked
against its template:

- `CORE-A4801` — the record's `contract` identity does not match the
  contract being compiled, or the template identity/digest is wrong.
- `CORE-A4802` — a bound parameter is missing, undeclared, or outside
  its typed domain; or a parameter reference in workflow/requirements
  resolves to nothing the instance bound.
- `CORE-A4803` — the instance's `execution_policy` loosens the
  template's `policy_floor`. Tightening order is per field: a
  `deny_providers` superset tightens; `maturity_floor` higher tightens;
  `cost_cap` lower-or-equal tightens; a boolean rule turning on
  tightens; `permit_nominal_basis`/`require_qualification` weakening
  loosens. The order is mechanical and total — no judgment.
- `CORE-A4804` — an eligibility rule evaluated `ineligible` or
  `unknown`: the instance admits evidence the template did not declare
  it eligible for, or the record cannot say. `ineligible` and `unknown`
  fail identically — eligible-or-refused.
- `CORE-A4805` — a `validation_case` fails to compile its parameter
  binding, so the template's own declared shape is broken. Checked
  wherever a template is compiled — at instance compile each case is
  materialized and compiled against the instance's registry. The
  approval transition itself cannot see document bytes (a transition
  record names a subject identity), so the enforceable boundary is the
  instance compile; a template whose cases cannot compile yields
  instances that refuse. *(The draft asked for the check "at template
  approval time"; the log boundary cannot carry that.)*
- `CORE-A4806` — the `template_superseded` notice (allocated from the
  same family in implementation).

### 4. Template amendment pins, never rewrites

A template edit produces a new `template_revision`; the superseding
document carries an informational `supersedes` pin
(`{ template_id, template_revision, sha256 }`) naming the revision it
replaces — the document-level edge, since ADR-0019's log amendment
machinery is manifest-bound and templates are not campaign roots.
Instances pin `(template_id, template_revision, sha256)`: a superseded
template changes nothing the instance recorded — the
`template_superseded` notice reports drift, never invalidates.

## Boundary

- **Eligibility is not semantic admission.** An eligible instance's
  claims still face A1–A10 at campaign time; `ineligible` is a
  compile-time refusal, never a verdict state.
- **Validation cases are shape checks.** A compiling validation case
  shows the template's parameters can bind a real contract; it is not a
  scientific qualification, which remains the ADR-0008 envelope's job.
- **No template registries or marketplaces.** A template is a document
  the package names by digest; discovery and versioning policy are
  organizational, not compiled.
- **Parameter references stay inside the template.** A workflow or
  requirement cannot reference a parameter the template did not
  declare — `deny_unknown_fields` discipline at the reference layer.
- **The `tightens` order in clause 3 is scoped to `execution_policy`
  fields.** The organization-policy `tightens` lattice across contract
  documents is the ADR-0020-deferred work, not this ADR.

## Implements

- ADR-0006 SC-9.2 and SC-9.5.
- Fixture rows: `lifecycle.instantiation-is-origin`,
  `lifecycle.template.instantiate.eligible`, `lifecycle.template.ineligible`,
  `lifecycle.template.eligibility-unknown`,
  `lifecycle.template.default-provided_by`,
  `lifecycle.template.instance-pins-version`,
  `lifecycle.template.validation-cases-must-compile`,
  `lifecycle.template.policy-only-tightens`, and the
  `lifecycle.amend.*` rows through the amendment edge.

## Implementation review (for ratification)

Delivered in commit `23875e5`. `contract_template` and
`contract_instantiation` are package-bound document families with
draft schemas; the contract names its record through
`instantiated_from` (`instantiation_id`, never a digest — the record
digest-pins the contract, so a digest in both directions is a cycle
no document can satisfy; the draft was amended). Parameters reuse
`ParameterDefinition` domains; `{"ref": "<param>"}` substitution
materializes the workflow/requirements templates; `policy_floor`
applies the mechanical tighten-only order (permissive flags inverted);
eligibility is a closed grammar under strong Kleene truth —
eligible-or-refused, and a recorded outcome the fields do not imply
fails as `CORE-A4804`. `CORE-A4801` binding integrity, `A4802`
parameters/coverage, `A4803` floor order, `A4805` validation cases,
`A4806` the superseded notice. Compiler 16 tests, one runner e2e, 7
verifier-parity tests.

Divergences from the proposal text, both recorded in the draft:

- The approval-time gate the draft sketched cannot live in the
  transition log — a transition names a subject identity and never
  sees document bytes. `CORE-A4805` is enforced at instance compile:
  a broken template yields refusing instances. That is the honest
  boundary, not a weakened check.
- Template/record signatures are package-bound `signature` documents
  under the standard trust-root machinery, not embedded fields.

Ratification questions:

- Accept the compile-time enforcement point for validation cases, or
  should approval require a bound template-check record (new
  machinery)?
- Accept `instantiated_from` naming the record by id only (no
  contract-side digest — the pin cycle makes one unsatisfiable)?
- `supersedes` is informational only — not resolved against a bound
  prior revision. Acceptable, or should a bound older revision
  downgrade the notice to a finding?
