# EL-05 assessment — transfer and what the finite language established

EL-05 asked for two things: a second synthetic method library exercising
different composition over the same primitives with no compiler branches
keyed to its domain, and an architecture assessment backed by measured
costs. Both are in hand; this note is the assessment. It stays inside the
charter's boundary — the evidence below is from a 24-program finite
corpus on one host, not a claim that broad infrastructure gates passed.

## Transfer evidence

`examples/language/libraries/measurement-scaling.v1.json` is the second
library. It composes the *same* primitives differently: `scaled-sum`
declares `provenance_disjoint` over its two reading inputs plus an
`independence` obligation that no amount of edge bookkeeping discharges —
only an attested premise closes it. `invalid-shared-source-independence`
demonstrates the required negative direction: shared calibration edges
refute disjointness (CORE-E8018) while independence stays an unmet
obligation (CORE-E8017); `positive-measurement-pass` discharges both via
premise `p-independence` whose own support (`separate-instrumentation`)
remains residual on `combined_rate` — the conditional-on grammar the
thermal programs also carry.

The compiler contains zero branches keyed to a library name, method id,
or domain (`grep` for `thermal-expansion`, `measurement-scaling`,
`scaled-sum`, `linear-expansion` in `crates/avila-core-compiler/src/language/`
returns nothing). All 24 programs in `examples/language/expectations.json`
reproduce their pinned analysis/plan rows through
`tests/language_fixtures.rs`, and the four execution/evaluation verdict
programs plus `positive-measurement-pass` replay through EL-03/EL-04
independently — including the Python verifier's byte-exact verdicts over
the measurement corpus. The "second library installed" adversarial row
is closed by demonstration, not by construction claim.

`scaled-sum`'s postcondition is `output = calibration * (reading_a +
reading_b)` — interval arithmetic only. No statistical combination rule
was implemented to close the independence example, per the brief.

## What the common rules established

- **One admission set governs identity.** A document's semantic identity
  exists exactly when the admission set (schema, profile, budgets,
  identifiers, sequential binding, vocabulary, declarations) passes;
  refused documents publish `""`. Every downstream record binds that
  identity, so mixed-context forgery fails at digest comparison, not at
  policy review. The independent verifier re-derives the same set and
  agrees in both directions (`verifier/fixtures/language/inadmissible-*`).
- **Obligations are derived, not declared.** `requires` and `ensures`
  produce obligation rows whose states (`discharged`/`open`/`refuted`/
  `runtime`) come from replayable checks — a record cannot claim
  obligation satisfaction, it can only *report* what replay derives.
- **Provenance is a graph, not a label.** Edges propagate through
  imports, inference, and application; `provenance_disjoint` checks the
  static edge sets, `independence` requires attested premises whose own
  assumptions join the conclusion's residual cone. An attestation's
  scope is semantic, not string-matched.
- **Verdicts have four states and stay that way.** `pass`, `fail`,
  `inconclusive`, `not_evaluated` are derived by the same comparator
  vocabulary in the runner and the replay; `not_evaluated` carries the
  blocking rule name (`obligation_unmet`, `lifecycle_refused`,
  `contradiction`, …) instead of collapsing into failure.
- **Lifecycle is supplied context.** `expired`/`withdrawn` refuse at use
  time; the record keeps both the refusal and the actual states read.

## What still requires trusted assertions

Replay checks *claims bind to identities*, not that claims are true.

- **`assertion`/`declared` provenance** is self-reported: a `party` name
  and an `edge` identifier the author typed. The compiler verifies shape
  and propagation, never correspondence to a real lab certificate.
- **`certificate` provenance** is replayable only for the profile's
  supported checks (`interval_arithmetic` today). Everything else is a
  digest pin over opaque bytes — the certificate's content is trusted.
- **Executables** are digest-bound; the receipt proves which bytes ran,
  not that they implement the method's declared semantics. A malicious
  but correctly-pinned executable is undetectable at this layer.
- **Authored entity data** (material applicability intervals, scenario
  operating domains, geometry) is input — trusted as written.
- **`--lifecycle` material** is supplied at evaluation time; the record
  binds it but cannot attest it.

## Which declarations were burdensome

- The `over` namespaces differ: `requires.over` names *method input
  slots* while premise `arguments.over` names *program bindings*. Both
  resolve through the application, but the asymmetry has already caused
  one porting error each in the analyzer and the verifier.
- `import.assumptions` is mandatory — an absent field is
  `malformed_import`, not an empty declaration. The distinction is
  intentional (absence is not vacuity) but is a guaranteed authoring
  surprise.
- Premise `at` maps key by proposition parameter names; a builtin like
  `independent` silently widens to the three relation names. Authors
  restate scope in proposition vocabulary rather than entity terms.
- Relation bookkeeping is triple-stated: `variables` declares the
  var→relation map, each slot restates `relation: var`, and `over`
  restates slot names. Dropping any one is a different finding kind.

## What failed to generalize

- **One output per method.** Observation binding looks up
  `output_id == "output"`; a method with two outputs cannot be observed.
- **Closed vocabularies everywhere** — relations
  (`geometry`/`scenario`/`material`), scopes (`steady-state`/`transient`/
  `any`), claims (`exact`/`enclosure`/`nominal`), comparators
  (`bounded.ge`/`bounded.le`), source kinds, obligation kinds,
  `SUPPORTED_CHECKS = {interval_arithmetic}`. Each is a deliberate
  grammar boundary; none is extensible without a schema revision.
- **`domain_containment` hardcodes two field paths**
  (`scenario.operating_domain`, `material.applicability`). A domain
  check over a third relation needs a language change.
- **`infer` knows exactly three primitive rules.** Everything else is an
  `external` method or unsupported.
- **Premise matching is scope-equality, not unification** — a witness at
  `geometry=bracket@1` does not witness `geometry=bracket@2`.
- **Lifecycle keys exist only at library/method scope**; an input or
  premise cannot carry a lifecycle.

## Migration under a separate experimental profile

The profile boundary today is one string gate
(`avila.core/language/0.1-draft`, refused otherwise — verified: a
`0.2-draft` program returns `malformed` at admission). Parameterizing it
migrates cleanly because admission and semantics are already separate
passes:

*Would migrate* — the profile-parameterized surface: `LANGUAGE_PROFILE`,
`SUPPORTED_CHECKS`, the §11 budgets, schema version strings, and the
closed vocabularies (claims, scopes, relations, comparators, source
kinds, obligation kinds). A profile tightening `nominal` out or adding a
check name changes only this list plus its tests.

*Would remain shared* — the machinery: canonical projection and identity
computation, the expression evaluator and kind products, provenance-edge
propagation, obligation derivation and the verdict comparator loop,
plan/execute/evaluate staging and the receipt-binding chain, the
admission-finding taxonomy, and the Python verifier's replay structure.
A second profile is a vocabulary-and-budget document, not a second
checker.

*Already divergent risk* — the finding-kind → admission mapping is baked
into `ADMISSION_KINDS`; a profile admitting different kinds (e.g. making
`unsupported` blocking) needs that taxonomy parameterized too, in both
implementations.

## Measured costs

Host: Intel Core i3-N305, x86_64, rustc 1.95.0 (59807616e), Python
3.14.4, workspace release build 3m26s cold. All numbers are wall-clock
per CLI invocation including process startup; the semantic work inside
is sub-millisecond for these programs.

| Operation | Cost | Workload |
| --- | --- | --- |
| `language analyze` | 7–10 ms | per program, both libraries |
| `language plan` | 9 ms | clearance-pass (1 external site) |
| `language execute` | 48 ms | dominated by spawning the pinned Python executable |
| `language evaluate` | 11 ms | clearance-pass, one observation doc |
| Python `verify-evaluation` | ~138 ms | per fixture (interpreter startup dominates) |

Documents: programs 2.1–5.8 KB, analyses 2.2–7.9 KB, plans 1.9 KB,
observations 3.2–6.2 KB, evaluations 2.2–2.7 KB. Implementation size:
`language/` Rust ≈ 7.8k LoC (analyze 5.8k, eval 0.5k, document 0.5k,
projection 0.4k, execution 0.4k, model 0.2k); `language_verify.py`
≈ 3.4k LoC stdlib-only. The Rust suite (23 test binaries) and the
57-test verifier suite both pass; CI run 36249705294 is green on
`8632b88`.

## Assessment

The transfer claim holds: two libraries, one pipeline, no domain
branches, independent replay agreement. The language's cost is
declarative density — three bookkeeping namespaces per relation and a
mandatory-assumptions discipline — and its boundary is the closed
vocabularies: extension is a schema revision, not a config option. That
is the right boundary for a validity layer. The concrete next decision
is the profile experiment: parameterize the vocabulary/budget lists and
the admission-kind taxonomy (the migratable surface above) rather than
forking the checker, and revisit single-output methods only when a real
method needs two.
