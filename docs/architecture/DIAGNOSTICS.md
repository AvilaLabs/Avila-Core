# Diagnostic catalog

Every finding Core can emit carries a stable code. Consumers match the code,
class, owner, location, and repair applicability, never the wording. The
catalog is embedded in Core and served by `avila-core explain <CODE>`;
`avila-core explain --all` returns both compiler and composed-runner entries.
A test keeps this page, the compiler catalog, and the codes the compiler source
references in agreement.

Codes are grouped by family: `S` for source and schema, `T` for typing, `R`
for resolution, graph, requirement, review, and registry checks, `A` for
admissibility, `E` for evidence admission during campaign evaluation, and `X`
for composed-runner stages. Only `CORE-R3601` and `CORE-R3602` are compiler
notices; every other compiler code blocks compilation.

## Human rendering

`avila-core compile --text` and `avila-core evaluate --text` render the same
findings the JSON report carries in the shape a compiler user expects, and
the case runner and the workbench show the same rendering for a rejected
compilation:

```text
missing[CORE-S1301]: parameter `histories` remains explicitly not defined in this draft
  --> contract:19:131  /workflow/0/parameters/histories
   |
19 |     { "step_id": "transport", ... "parameters": { "histories": "not_defined" }, ... },
   |                                                                 ^^^^^^^^^^^^^
   = owner: requester
   = next: Supply a value of the declared family and domain. The owner is the requester.
```

The first word is the finding's class, never collapsed to `error`, because a
missing value, an invalid one, an unsatisfiable composition, and an
inadmissible record call for different people and actions. The location
resolves the JSON Pointer to a line and column in the supplied document
bytes; a pointer that does not exist yet, as for a missing member, resolves
to its nearest existing ancestor and says so. Related locations, every
repair alternative with its applicability, and the catalog's next action
follow. The rendering is presentation only: consumers keep matching codes,
classes, owners, pointers, and repair applicability.

## Composed-runner findings

`avila-core run` adds a top-level `findings` array to its JSON report. It
normalizes compiler and campaign diagnostics with runtime failures so an
iterating person or agent has one stage-ordered queue of actionable feedback.
Each entry carries `code`, `class`, `stage`, `owner`, an optional `step_id`, a
primary source location, related locations, bounded repairs, `message`, and
`next_action`. The detailed integrity, coverage, execution, receipt, binding,
and campaign reports remain the evidence behind that index.

When `--log PATH` is supplied, every returned report is appended to PATH as an
`avila.core/run-attempt/v0.1-draft` JSON line. Failures that occur before a case
report can be constructed are also appended with status `error` and
`CORE-X9001`. A log-write failure is returned to the caller; Core does not
silently claim an attempt was recorded.

| Code | Title | Stage |
| --- | --- | --- |
| `CORE-X1001` | Package byte identity failed | package integrity |
| `CORE-X1002` | Package manifest pin differs | package integrity |
| `CORE-X1101` | Requirement-set coverage incomplete | coverage |
| `CORE-X2001` | Execution declaration invalid | execution planning |
| `CORE-X2101` | Execution input unavailable or unchecked | execution planning |
| `CORE-X2201` | Invocation could not be planned | execution planning |
| `CORE-X2301` | Qualification facts could not be derived | execution planning |
| `CORE-X2401` | Capability identity unavailable | execution planning |
| `CORE-X2402` | Required execution environment missing | execution planning |
| `CORE-X2501` | Capability execution failed | execution |
| `CORE-X2601` | Execution receipt failed verification | receipt verification |
| `CORE-X2701` | Claims could not be extracted | claim generation |
| `CORE-X2801` | Adapter output contract differs | claim generation |
| `CORE-X3001` | Evidence identity binding failed | evidence binding |
| `CORE-X3101` | Committed claims drifted | replay |
| `CORE-X3201` | Committed receipt drifted | replay |
| `CORE-X3301` | Committed campaign result drifted | replay |
| `CORE-X9001` | Runner could not produce a case report | infrastructure |

The full meaning and next action for each runtime code live in the embedded
catalog and are available through `avila-core explain`, just like compiler
codes.

| Code | Title | Rule | Meaning | Next action |
| --- | --- | --- | --- | --- |
| `CORE-A4201` | Nominal basis not permitted | SC-8 and SC-10 | A requirement's basis is `nominal`, which compares a nominal value and uses no uncertainty, but the contract execution policy does not permit that weakening. Every weakening is explicit in the contract. | Set `permit_nominal_basis` to true in the execution policy, accepting that the verdict will visibly state that uncertainty was not used, or use a `bounded` or `enclosure` basis. The owner is the policy owner. |
| `CORE-A4401` | Evidence outside its qualification envelope | SC-10 A7 | A bounded or enclosure requirement depends on a claim whose producing capability carries a qualification, and this run's facts fall outside that envelope or their position is unknown. The calculation may be fine; it is not qualified here, so the requirement is not evaluated on it. | Bring the case inside the envelope, extend the qualification with validation evidence, or supply the missing fact from an admissible source. The owner is the method owner. |
| `CORE-A4301` | Nondeterminism not permitted | SC-5 and SC-6 R8 | A step uses a nondeterministic capability type, but the contract execution policy does not list every role that type produces under `permitted_nondeterministic_roles`. Permission is scoped by role and makes the type neither deterministic nor qualified. | Add each produced role to the execution policy, accepting that execution memoization stays disabled for the step, or choose a deterministic or seeded-stochastic type. The owner is the policy owner. |
| `CORE-E7001` | Claims bind a different snapshot | SC-11 | The claims document names a compiled snapshot identity that differs from what the supplied contract and registry compile to, so nothing in it can be attributed to this campaign. | Regenerate the claims against these documents, or supply the contract and registry the claims were produced for. The owner is the executor. |
| `CORE-E7002` | Claim names nothing in the snapshot | SC-11 | An attestation or claim names a contract input, workflow step, or output slot that the compiled snapshot does not have. | Correct the identifier to one the compiled snapshot declares. |
| `CORE-E7101` | Artifact identity missing or malformed | SC-11 A1 | A contract input has no attested artifact, or an artifact identity is not a lowercase `sha256:` digest of 64 hex digits. Artifact bytes are not read in this slice; the identity is what later verification binds. | Attest every contract input and give every artifact its exact digest. |
| `CORE-E7103` | Parent not admitted | SC-11 A3 | A claim was produced from a parent that is missing or quarantined. Admission is fail closed along the bound dataflow, so the claim is quarantined even when its own values are well formed. | Admit the parent first: attest the missing input or repair the quarantined parent claim, then re-evaluate. |
| `CORE-E7201` | Claim rejected by type-level validation | SC-3 and SC-11 A6 | The claim's model is not permitted by the output slot, its shape does not satisfy its model, a quantity does not scale in the role's kind, a bound is inverted, a nominal lies outside its interval, a coverage is outside (0, 1], or the artifact media type differs from the declared one. | Produce a claim in a model the output permits, with quantities in admitted units of the role's kind. |
| `CORE-E7301` | Duplicate claim for one output | SC-15 | More than one claim or attestation exists for a single output slot or contract input. A slot admits exactly one; every claim for it is quarantined rather than one being chosen. | Keep exactly one claim per output slot; a rerun creates a new claim only after the previous one is withdrawn. |
| `CORE-R3101` | No compatible source | SC-6 R1 | A required input slot has no source carrying the same nominal role in an accepted media type; an explicit binding names a contract input that is not declared; a contract input names a role absent from the snapshot; or a step names a capability type absent from the snapshot. | For a slot, apply one candidate from the `constrained_choice` repair: `declare_input:<role>` adds a contract input carrying the role, `add_step:<type>/<output>` adds a step of a capability type that produces it. For an unknown role or type, correct the reference or supply a snapshot that defines it. |
| `CORE-R3102` | Ambiguous source | SC-6 R1 | A required input slot has more than one compatible source. The compiler never chooses between them. | Add an explicit binding for the slot naming one of the candidates in the `constrained_choice` repair. |
| `CORE-R3201` | Self dependency | SC-6 R5 | A step binds one of its own outputs to one of its own inputs. | Bind the slot to a contract input or to another step's output. |
| `CORE-R3202` | Dependency cycle | SC-6 R5 | Bindings form a cycle among the primary and related steps. The compiler never repairs a graph by reordering or dropping work. | Break the cycle by rebinding one of the listed steps. |
| `CORE-R3203` | Unknown dependency | SC-6 R5 | An explicit binding names a workflow step that does not exist, or a known step that declares no such output slot. A reference to a step whose capability type is absent from the snapshot is suppressed under that step's `CORE-R3101` instead. | Correct the step id or output slot; the message lists the outputs the step declares. |
| `CORE-R3301` | Metric unbound | SC-6 R6 | A requirement names no metric source, or names one that cannot be resolved to a declared contract input or an existing step output. | Set `metric` to a declared contract input or an existing step output. |
| `CORE-R3401` | Optional presentation gate incomplete | SC-6 R9 | An optional agent practicality stage or its contract binding is structurally incomplete or contradictory: it hides or makes optional a presented input, emits anything but one unquantified routing record, uses a quantitative routing role, repeats a disposition, lacks explicit instructions, pins its agent policy without a revision and lowercase `sha256:` identity, declares invalid independence constraints, or appears on a capability type that has no review declaration. | Complete the declaration at the reported pointer. This only compiles an optional presentation gate; it is never a requirement-verdict input. |
| `CORE-R3501` | Registry snapshot incomplete | SC-1, SC-4, and SC-5 | The registry snapshot is internally inconsistent: a duplicate purpose; a role with no media type or claim model, or whose unit class does not match its kind; a slot naming an unknown role or a media type outside its role; an output claim model outside its role; an empty or non-comparable parameter or factor domain; an unknown quantity kind; or an excluded purpose that is unknown or repeated. | Correct the snapshot at the reported pointer. The owner is the registry owner. |
| `CORE-R3601` | Unused contract input | SC-6 notices | No step binds this contract input, so it would enter no campaign evidence. This is a notice and does not block compilation. | Remove the input or bind it to a slot. |
| `CORE-R3602` | Unconsumed step | SC-6 notices | This non-review step's outputs feed neither another step nor a requirement, so executing it would produce evidence nothing uses. This is a notice and does not block compilation. | Remove the step, bind one of its outputs, or name one as a requirement metric. |
| `CORE-S1101` | Undeclared field | SC-2 authoritative documents; SC-6 R7 and R8 | A document contains a key its schema does not define, or a step supplies a parameter or material execution factor its capability type does not declare. Undeclared values never become implicit defaults. | Remove the key, or declare it in the schema or capability type through a new registry snapshot. |
| `CORE-S1102` | Structural or canonical-value violation | SC-2 | A value violates the canonical profile or a structural rule: a binary floating-point JSON number, `null`, a non-NFC string, an unsafe integer, a non-canonical decimal or rational, an empty identifier, a zero revision, an empty workflow or requirement list, a duplicate identifier, a coverage outside `(0, 1]` or on a non-bounded basis, a negative tolerance, or a schema or semantic-profile header the compiler does not support. | Fix the value at the reported pointer. When the finding carries a `mechanically_safe` repair, its single candidate is the unique canonical form; apply it verbatim. |
| `CORE-S1103` | Duplicate object key | SC-2 canonical JSON | An object repeats a key. Canonical JSON forbids duplicates because a reader could not know which value was signed. | Remove or rename the repeated key at the reported pointer. |
| `CORE-S1301` | Parameter not defined | SC-6 R7 | A required parameter is absent, or a parameter is the reserved placeholder `not_defined`. In a `draft` this is a `missing` finding; once the contract is `in_review`, `approved`, or `retired` it is `invalid`. The placeholder never enters compiled IR. | Supply a value of the declared family and domain. The owner is the requester. |
| `CORE-T2001` | Unknown unit symbol | SC-1 | The unit symbol is not admitted for the quantity kind by the snapshot. Symbols are case-sensitive because SI prefix case changes magnitude: `Mpa` is not `MPa`, and `usv/h` is not `uSv/h`. | Choose one of the admitted symbols listed in the `constrained_choice` repair. Case is corrected automatically only for a governed typo alias, which the current snapshot format does not yet carry. |
| `CORE-T2101` | Nominal role mismatch | SC-4 and SC-6 R2 | An explicitly bound source carries a role whose identity or major version differs from the destination slot's role. Roles are nominal: equal dimensions do not make quantities interchangeable. | Bind a source carrying the required role, or route through a cross-kind conversion capability owned by a method owner. |
| `CORE-T2102` | Metric domain mismatch | SC-1 and SC-6 R6 | A quantitative requirement uses the wrong quantity kind or a non-quantity role. A categorical requirement uses a quantity role, a role without a closed vocabulary, or predicate values outside that vocabulary. | Choose a metric with the required domain, or make the requirement match the metric role's governed quantity kind or categorical vocabulary. |
| `CORE-T2103` | Unit outside kind | SC-1 and SC-6 R6 | The unit is a known symbol, but it belongs to a different quantity kind than the metric kind. Exact scaling within one kind is the only implicit conversion. | Use a unit of the metric kind; conversion between kinds is a separately qualified capability. |
| `CORE-T2104` | Equality tolerance | SC-6 R6 and SC-10 | An `equal` comparison has no tolerance quantity, so the verdict calculus could never evaluate it; or a comparison other than `equal` carries a tolerance, which would silently mean nothing. | Add a nonnegative tolerance of the metric kind to the equality requirement, or remove the tolerance from the inequality. |
| `CORE-T2201` | Claim model insufficient | SC-3 and SC-6 R3 | No claim model the metric source may emit can satisfy the requirement's basis. A `bounded` basis needs an exact, interval, coverage-interval, or correctly sided worst-case claim; an `enclosure` basis needs an exact or interval claim; a `nominal` basis needs a claim that carries a nominal value. | Choose a metric source whose type permits a sufficient model, or change the basis only if the requirement genuinely accepts a weaker claim. The package-level half of this rule is checked later at binding. |
| `CORE-T2203` | Claim model irreducible | SC-3 and SC-6 R3 | The metric source permits only claim models the semantic kernel cannot reduce: standard uncertainty, samples, or a distribution. | Insert an uncertainty expansion or reduction capability between the source and the requirement; the `method_owner_judgment` repair names the candidate type. |
| `CORE-T2301` | Media type not accepted | SC-6 R4 | A contract input's media type is outside its role, or a bound source's media type is not accepted by the destination slot. | Use a media type the role or slot accepts, or bind a different source. |
| `CORE-T2401` | Wrong value family | SC-6 R7 and R8 | A parameter or material execution factor is authored in the wrong family. The five families are boolean, signed 64-bit integer, exact-number string, text, and a quantity object with an exact string `value` and a `unit` of the declared kind. The compiler does not coerce. | Author the value in the declared family; a quantity must use an admitted unit of the declared kind. |
| `CORE-T2402` | Value outside domain | SC-6 R7 and R8 | A parameter or material execution factor is well typed but outside its declared domain: an integer, exact-number, or quantity bound, or a finite text choice set. | Choose a value inside the domain; for text, one of the candidates in the `constrained_choice` repair. |
| `CORE-T2501` | Reproducibility binding | SC-5 and SC-6 R8 | A seeded-stochastic step lacks a nonempty seed; a deterministic or nondeterministic step carries a seed that is not part of its invocation identity; or a type-declared material execution factor is unbound or still `not_defined`. | Bind exactly the seed and factors the capability type declares. The owner is the requester. |
| `CORE-T2601` | Governed purpose | SC-4 and SC-6 R10 | A requirement names a purpose absent from the snapshot at that exact identity and major version, or the metric source's output explicitly excludes that purpose. Purpose identity is nominal: no prefix, hierarchy, or prose inference applies. | Name a purpose the snapshot defines, or choose a metric source whose output does not exclude it. Absence of an exclusion is not a positive qualification claim. |

Owners are product roles: `requester` for the contract and its values,
`policy_owner` for execution and optional presentation-policy bindings, and `registry_owner`
for the snapshot. Finding classes are `missing`, `invalid`, `unsatisfied`,
`inadmissible`, and `notice`. The class is decided per finding, not per code: a
required property absent from a document is `CORE-S1102` as `missing`, `CORE-S1301` is
`missing` in a draft and `invalid` afterwards, and `CORE-T2104` is `missing`
when a tolerance is absent and `invalid` when one is misplaced.

Repair applicability is typed. `mechanically_safe` means the single candidate
is the unique correct value and may be applied verbatim; `constrained_choice`
means one of the listed candidates must be chosen by the contract author;
`method_owner_judgment` means the named method owner must decide, typically by
adding or qualifying a capability; Core infers no credential requirement from
that repair class. Each alternative carries its RFC 6902 JSON
Patch under `edits` when the compiler can state the exact bytes, index-aligned
with `candidates`; an alternative the compiler can only name has an empty
patch. The compiler never applies a repair itself.
