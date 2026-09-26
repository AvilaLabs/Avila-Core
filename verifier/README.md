# Avila Core independent verifier (Slice I)

An independently implemented, offline reader of exported Avila Core
packages. Python 3, standard library only (`json`, `hashlib`, `fractions`,
`decimal`, `pathlib`, `argparse`, `unittest`, `re`, `unicodedata`). It
imports no Core crate; every rule it applies is re-derived from this
repository's committed *documents* (ADRs, architecture notes,
`docs/DEFINITIONS.md`) and *fixture vectors and receipts*
(`fixtures/semantic-core/`, `examples/cases/*/receipts/*.json`), not from
Rust source. See `avila_core_verify.py`'s module docstring for exactly
which rule came from which document or vector, and for the places a rule's
*byte-level shape* had no vector of its own and was learned by reading Rust
to see which fields it names — never to copy its logic — then proved
independently against the repository's own committed receipts and logs.

This is Stage 0's "Independent verification" validation gate (previously
`Open`) and the "independently implemented verifier" half of the
Evidence-package spike build gate.

## Supported profile

This file checks exactly ten things and refuses everything else by name.
See the module docstring in `avila_core_verify.py` for the full, precise
statement (each item there cites the ADR clause or fixture file it proves
agreement against); in short:

1. **Package identity** — every document and artifact a case package's
   manifest binds re-hashes to its bound SHA-256 from bytes. External
   artifact roots are supplied as `NAME=PATH`, exactly like
   `avila-core run --source-root NAME=PATH`.
2. **Canonical JSON** — reimplements Core's canonicalisation and proves
   agreement on every vector in `fixtures/semantic-core/vectors/canon.v1.json`.
3. **Execution receipts** — recomputes each receipt's `invocation_sha256`
   from its own recorded fields and compares it with the recorded value;
   verifies every declared output's digest against the package artifact it
   binds (byte-bearing states are `collected` and `partial`); shape-checks
   each output's ADR-0006 clause-9 `representation_error`/`numerical_error`
   disclosures — recorded components, never verdict inputs.
4. **Claims binding** — every claim's artifact identity binds to a
   package-declared artifact that verifies; the claims document's and
   campaign report's compiled-snapshot digests are compared for equality
   (never recompiled — this profile does not implement the compiler).
5. **Verdict re-derivation** — recomputes the four-state verdict (and an
   exact margin) for every numeric and categorical requirement in a case's
   authored contract from its admitted claims, with exact rational
   arithmetic and the kind-table unit scaling the case's own `registry.json`
   declares, and compares status/rule/canonical values against the case's
   committed `campaign-report.json`. Proves agreement on every vector in
   `fixtures/semantic-core/vectors/verdict-calculus.v1.json` and
   `unit-scaling.v1.json`, and on every fixture in
   `fixtures/semantic-core/campaigns/campaign-cases.v1.json` except three
   that need a compiler this profile does not implement (named in
   `test_verifier.py`'s `NOT_RE_DERIVABLE_CAMPAIGN_FIXTURES`). Also
   recomputes the committed report's `campaign_sha256` over its semantic
   body — the digest additionally binds the `findings` and `admissions`
   fields this profile does not individually re-derive. When the contract
   declares a `completion` block (SC-9 clause 6), the `completion.block`
   check re-derives the delivery assessment from the declared fulfilling
   verdicts and permitted inconclusive reasons — `not_evaluated` never
   fulfills — and compares it against the committed `completion` entry.
6. **Attempt lineage** — verifies a campaign/attempt JSONL log's
   parent-line SHA-256 binding, candidate-state digest, manifest/snapshot
   inheritance, and the recursive RFC 6901 `changes` diff (ADR-0014).
7. **Mutation tests** — `test_verifier.py`'s `TestMutations` corrupts one
   claim value, one receipt output digest, one manifest entry, one log
   line, one verdict margin, one signature byte, one signature's key id,
   one manifest (without re-signing it), one qualification envelope term,
   one persisted applicability-fact value, one qualification context (by
   removal), one context input identity, one context (by lifting it from a
   sibling step), four coverage declarations (a dropped bounded cover, a
   deleted stated omission, a mapping to a requirement that does not
   exist, a raised `minimum_basis` in the bound set), and one
   campaign-report `findings` entry, each in a scratch copy of a real case
   with everything *else* re-hashed to stay self-consistent, and asserts
   this verifier names exactly the corrupted layer.
8. **ADR-0015 signatures** — a from-scratch, standard-library Ed25519 (RFC
   8032), proved against every RFC 8032 section 7.1 test vector and every
   ADR-0015 signature document committed under `examples/cases/*/signatures/`.
   Verifies the manifest's requester signature, each execution receipt's
   runner signature, and any campaign log-line runner signatures present;
   reports `verified` (with the signer's key id), `invalid` (with the
   reason — a bad digest, an unlisted or wrong-role key, or a signature
   that plain does not verify), or `unsigned` per signature. `--trust-root
   FILE` supplies the accepted keys (e.g. `examples/keys/trust-root.json`);
   without it, every signature is `not_checked` and never `verified`.
9. **Qualification envelopes** (ADR-0008, S-039, ADR-0018, S-046) — a
   from-scratch Strong-Kleene predicate evaluator, proved against every
   vector in `fixtures/semantic-core/vectors/scope-predicates.v1.json`.
   For CASE-001 and 003's committed claims, independently re-evaluates
   each qualification-carrying claim's per-term scope predicate over the
   claim's persisted applicability context and compares every derived
   term result and the aggregate state against the recorded ones; checks
   each recorded term's predicate text against the bound qualification
   record's own scope; and binds the context to the step's execution
   receipt — every context input's identity equals a receipt input's,
   every fact's `plan:<invocation>` source equals the receipt's own
   invocation identity, and every fact's source identity is either
   `runner:local` or a staged input the receipt names. What it does
   **not** establish, by name: that the adapter extracted the facts
   correctly from the bytes — the context remains the producer's
   assertion about verified inputs, not independent proof of its truth.
   With `--as-of INSTANT` (ADR-0006's supplied-snapshot historical
   verification), each qualified claim additionally earns an
   informational `as_of` line: its recorded state beside the labeled
   state at the supplied instant under the supplied material — `absent`,
   `revoked`, `superseded`, `expired`, or the envelope its recorded
   facts imply, in campaign-refusal precedence. `--as-of-material DIR`
   supplies the snapshot as another case package's bound records,
   revocations, signatures, and contract policy; without it the case's
   own bound set is the material. A divergence is a datum, never a
   failure.
9.5. **Provider selection records** (ADR-0020, SC-8) — a package operating
   under declared `execution_policy` provider rules binds one
   `capability_selection` document recording which capability
   implementation serves each step. The engine verifies the recorded
   selection rather than performing one; the verifier re-derives every
   check the runner makes: the record's `registry_snapshot` equals the
   bound registry, every considered candidate carries a decision and
   reasons, criteria stay inside the closed legitimate vocabulary
   (`provider_payment` and `avila_margin` never appear), at most one
   candidate is `selected`, and the selected triple is exactly the
   capability the manifest binds (contract capability type, execution's
   capability id + adapter, the capability's pinned executable digest).
   Each declared rule is re-checked — `deny_providers`/`allow_providers`
   on the selected type's registry `owner`, `require_provider_independence`
   and `require_diverse_implementations` across steps, `maturity_floor`
   against the type's declared `maturity` (undeclared fails a declared
   floor), `forbid_self_preference` against an `avila_provided` selection's
   recorded `self_preference_check`, and `cost_cap` against the recorded
   `cost_estimate` unless `cost_confirmed_by` is present. What the runner
   refuses at bind/plan time (`CORE-P5101`–`P5602`) the verifier reports
   as `selection.*` mismatches — the same truth, checked without trusting
   the runner.
9.6. **Organization policy floors** (ADR-0023, SC-8) — a contract's
   `execution_policy` may pin a bound `organization_policy` document and
   name a `relation`. `verify_org_policies` re-derives the merge the
   compiler performed: the pin resolves to exactly one bound policy by
   canonical digest with matching `policy_id`/`policy_revision`; under
   `tightens` every declared contract field is re-checked against the
   floor's per-field order (deny superset, allow subset, grant lists
   inside the floor's, maturity ranking, cost-cap currency and ceiling,
   restrictive booleans, the inverted permissive `permit_nominal_basis`);
   `exact` tolerates no declared fields; `replaces` re-checks the pinned
   `attestation`'s binding — `policy_owner` `approves` over the policy
   (`organization_policy` subject) and contract (`target`) identities —
   and both documents' signatures under the supplied trust root's
   `policy_owner` keys (`not_checked` without one). A bound newer policy
   revision reports drift, never a mismatch. What the compiler refuses
   (`CORE-A4901`–`A4905`) the verifier reports as `organization_policy.*`
   mismatches.
10. **Requirement-set coverage** (S-024) — a case package may bind one
    `requirement_set` document and declare in its manifest which contract
    requirements cover each set entry and, for the rest, a reason and an
    accepting owner. A `run` refuses to spend evaluation on an incomplete
    declaration (an unstated `must_state` omission, or coverage only on a
    basis weaker than the entry's `minimum_basis`), so a package whose
    committed claims and campaign report exist asserts its coverage
    re-derives `complete`. The verifier re-derives the assessment from
    the committed manifest, requirement set, and contract, and reports a
    derived `incomplete` as a `mismatch`. The per-entry coverage report
    is written to the transient run workspace and never committed, so the
    derived status is the only committed observable — this check does not
    fabricate a per-entry diff. Cases with no `coverage` declaration get
    no check.

Explicitly **out of scope**, refused by name wherever the check would
otherwise silently pass or silently mismatch:

- proving a persisted applicability fact was extracted correctly from the
  staged bytes (item 9's own boundary above);
- presentation-gate / staged-review realisation or content;
- recompiling a contract + registry into a compiled-snapshot identity
  (equality is checked; recomputation is not);
- the A3 parent-admission dataflow cascade, the A6
  registry-role-permitted-claim-model check, and the SC-5-clause-6
  `partial`-vs-`permits_partial` check are re-derived through
  `avila_core_lower`'s registry index where the documents parse; a
  campaign fixture whose verdict depends on a step binding the lowerer
  cannot resolve stays named, not silently passed.

## Language evaluations — `language_verify.py`

`language_verify.py` replays the engineering-language evaluate chain
(`avila.core/language-evaluation/v0.1-draft` records) from the supplied
program, library, execution plan, and observations — an independent port
of the §7 [O1]/[O2] evidence-binding rules, §8 premise admissibility, and
§10 semantic-identity projection, without importing Rust behavior.
Checks: program/library *semantic* digests (annotations dropped, declared
sets sorted by projected bytes) against the evaluation context; the
program's library pin; plan self-identity and the plan↔context link;
every planned invocation's fields against the verifier's own binding
replay (bind/method/executable + re-staged input digests); observation
records through the full O1 chain (receipt re-hash, plan/site/executable
triple, input digest-set equality, invocation identity, completed status,
output digest, typed admission); runtime obligations re-derived
(`domain_containment`, `scope_check`, `provenance_disjoint`,
`independence`, postcondition `output = <expr>` replay under
exact-rational interval arithmetic); premise discharge cones with their
residual `conditional on` assumptions; and every requirement verdict —
`pass`/`fail`/`inconclusive`/`not_evaluated` under `bounded.ge`/
`bounded.le` — against the record's claimed status, rule, and detail.
Foreign, transplanted, duplicated, tampered, absent, or malformed
observation material stays rejected in the replay exactly as the record
must report it; `language_verify.py explain` diffs two evaluations of one
program at requirement, obligation, and observation granularity.

**Admission parity.** Admission is replayed, not trusted: the record's
`""`-vs-digest program/library identities must agree with a full
re-derivation of the admission set — document schema/profile, §11
budgets, identifier charset, sequential single-assignment references,
requirement shapes, premise attribution and `over`-member binding,
provenance-object shape (`malformed` vs the non-admission `unsupported`
certificate check), proposition vocabulary and scope params, entity
intervals, and every admission-kind finding the replay emits during the
analyze run. The library gate is the full `LibraryChecker` — method
signatures, `requires`/`ensures` declaration shape, implementation kinds,
`kind_products`. A record claiming an identity for an inadmissible
document mismatches; an honest `""` for a refused document verifies
(`fixtures/language/inadmissible-*/` pin both directions). Lifecycle
gating (`--lifecycle key=state`) and the `expired`/`withdrawn` refusal,
`contradiction` blocking, and `nominal`-claim gates are all replayed.

```bash
python3 language_verify.py verify-evaluation \
  --program P.json --library L.json --plan plan.json \
  --observations obs.json --evaluation eval.json [--analysis a.json]
python3 language_verify.py explain \
  --program P.json --evaluation-old A.json --evaluation-new B.json
python3 -m unittest test_language_verify -v   # corpus + adversarial suite
```

The committed corpus under `fixtures/language/` was generated once from
the reference implementation and held constant — the replay stays
independent of the Rust toolchain.

## Usage

```bash
# Run every applicable check against one case directory.
python3 avila_core_verify.py verify-case ../examples/cases/case-003-thermal-spreader \
  --source-root case=../examples/cases/case-003-thermal-spreader \
  --source-root thermal=../examples/capabilities/thermal

# Same, plus verify ADR-0015 signatures against the example trust root
# (CASE-001, 002, and 003 are the three signed cases).
python3 avila_core_verify.py verify-case ../examples/cases/case-003-thermal-spreader \
  --source-root case=../examples/cases/case-003-thermal-spreader \
  --source-root thermal=../examples/capabilities/thermal \
  --trust-root ../examples/keys/trust-root.json

# Same, machine-readable.
python3 avila_core_verify.py verify-case ../examples/cases/case-000-actinv-aftermatter --json

# Same, plus a historical-verification pass (ADR-0006): one informational
# [ASOF] line per qualified claim naming its recorded state beside the
# labeled state at the supplied instant under the bound material.
python3 avila_core_verify.py verify-case ../examples/cases/case-002-coupled-shield \
  --as-of 2026-09-03T02:17:52Z

# Same, against a supplied material snapshot: another case package whose
# bound records, revocations, signatures, and contract policy stand in for
# "what was known then" (requires --as-of).
python3 avila_core_verify.py verify-case ../examples/cases/case-002-coupled-shield \
  --as-of 2026-09-18 --as-of-material ../snapshots/case-002-before-revocation

# Run this file's own fixture-vector proofs as a Report (same CLI surface).
python3 avila_core_verify.py self-test
```

An omitted `--source-root` leaves that root's artifacts `not_checked`
(never `mismatch`); a supplied root whose bytes are missing or differ *is*
`mismatch`. Exit code is `0` only when nothing reports `mismatch` — a
`not_checked` entry (an unavailable external root, an out-of-profile rule)
never by itself changes the exit status.

Roots this repository can supply for its own example cases (everything
else — `nuclear-data`, `actinv-data`, `actinv-release`, `simsopt` — is an
external checkout this repository does not vendor and will correctly come
back `not_checked`):

| Root name | Path |
| --- | --- |
| `case` | the case's own directory |
| `shielding` | `examples/capabilities/shielding` |
| `coupled` | `examples/capabilities/shield-coupled` |
| `agents` | `examples/agents` |
| `thermal` | `examples/capabilities/thermal` |
| `magnetic-compliance` | `examples/capabilities/magnetic-compliance` |

## Tests

```bash
python3 -m unittest discover -s . -v      # from inside verifier/
python3 -m unittest test_verifier -v
```

Every test class cites, in its own docstring or comments, the fixture file
or committed example case it proves agreement against — see
`test_verifier.py`. Highlights:

- `TestCanonVectors`, `TestUnitScalingVectors`, `TestVerdictCalculusVectors`:
  every vector in the three named fixture files.
- `TestCampaignFixtures`: all 15 fixtures in `campaign-cases.v1.json`, 12
  matched exactly and 3 named as out of profile.
- `TestReceiptInvocationIdentity`: all 15 committed execution receipts
  reproduce their own `invocation_sha256` byte-for-byte.
- `TestAttemptLineage`: both committed `attempts.jsonl` logs (CASE-008,
  CASE-009).
- `TestMarginRendering`: every `margin` value in every committed
  `campaign-log.jsonl` / `attempts.jsonl` in this repository (several
  thousand) reproduces exactly.
- `TestPositivePathOnRealCases`: CASE-000, 001, 003, 008, 009 each verify
  with zero `mismatch` (CASE-001 and 003 additionally with
  `--trust-root`, asserting every manifest/receipt signature `verified`);
  CASE-002 is exercised separately by name — its pre-ADR-0018 claims
  report the missing qualification contexts as mismatches and nothing
  else does.
- `TestEd25519RFC8032Vectors`: all 5 RFC 8032 section 7.1 vectors (public
  key derivation, signing, and verification), plus the curve constants
  cross-checked against RFC 8032 Table 1's own literals.
- `TestSignaturesAgainstCommittedDocuments`, `TestManifestSigningDigest`:
  every ADR-0015 signature document under `examples/cases/*/signatures/`
  verifies against `examples/keys/trust-root.json`, and the manifest-signing
  digest rule reproduces every committed `signatures/manifest.sig.json`.
- `TestLogLineSignatureVerification`: no committed log carries a signed
  line (log-line signing needs a live `run --runner-key`), so this signs a
  synthetic line with the real, committed `examples/keys/runner.seed` and
  proves the verify/tamper/unsigned/mixed-file paths all report correctly.
- `TestScopePredicateVectors`: all 28 vectors in `scope-predicates.v1.json`,
  plus hand-written cases for the string/bool/integer fact and structural-
  error paths those vectors don't happen to reach.
- `TestQualificationEnvelopeConsistency`: every qualification-carrying
  claim in CASE-001, 002, and 003 re-derives from its persisted context
  and binds to its step's receipt.
- `TestCapabilitySelection`: the ADR-0020 `selection.*` checks — honest
  records verify; wrong snapshot, wrong selected triple, missing winner,
  banned criterion, undecided candidate, denied provider, maturity floor,
  unconfirmed cost cap, missing self-preference check, shared provider,
  and shared executable each report the `CORE-P5xxx` mismatch.
- `TestCoverageReDerivation`: CASE-001, 002, and 003's bound requirement
  sets each re-derive a `complete` coverage declaration; the unbound
  cases emit no coverage check.
- `TestMutations`: the eighteen corruption scenarios in item 7 above.

## What this is not

An independent verifier is not a substitute for the runner, the compiler,
or a qualified reviewer. A `verified` line here means this file's own
re-derivation from the same bytes agrees with what was committed; it says
nothing about scientific correctness, qualification, or regulatory
suitability — exactly as every execution receipt and verdict record
already says about itself. See `docs/architecture/EVIDENCE_MODEL.md`
("Independent verification": *"Package valid" must never collapse these
different checks into a single unqualified promise*), which this file's
per-check `verified` / `mismatch` / `not_checked` report is built to honor.
