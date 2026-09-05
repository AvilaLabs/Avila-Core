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

This file checks exactly seven things and refuses everything else by name.
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
   binds.
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
   `test_verifier.py`'s `NOT_RE_DERIVABLE_CAMPAIGN_FIXTURES`).
6. **Attempt lineage** — verifies a campaign/attempt JSONL log's
   parent-line SHA-256 binding, candidate-state digest, manifest/snapshot
   inheritance, and the recursive RFC 6901 `changes` diff (ADR-0014).
7. **Mutation tests** — `test_verifier.py`'s `TestMutations` corrupts one
   claim value, one receipt output digest, one manifest entry, one log
   line, and one verdict margin, each in a scratch copy of a real case with
   everything *else* re-hashed to stay self-consistent, and asserts this
   verifier names exactly the corrupted layer.

Explicitly **out of scope**, refused by name wherever the check would
otherwise silently pass or silently mismatch:

- signed manifests/receipts (ADR-0015 is `proposed`, not implemented in any
  committed package this profile reads);
- qualification envelope *predicate* re-evaluation (a claim's recorded
  `qualification.state` is read as given, exactly as the real evaluator
  does per `CAMPAIGN_EVALUATION.md`);
- coverage-set evaluation against a `requirement_set` document;
- presentation-gate / staged-review realisation or content;
- recompiling a contract + registry into a compiled-snapshot identity
  (equality is checked; recomputation is not);
- the whole-report `campaign_sha256` content identity;
- the A3 parent-admission dataflow cascade and the A6
  registry-role-permitted-claim-model check (both need a compiled
  dataflow graph / role table this profile does not build — three
  campaign fixtures need one of these and are named, not silently passed).

## Usage

```bash
# Run every applicable check against one case directory.
python3 avila_core_verify.py verify-case ../examples/cases/case-003-thermal-spreader \
  --source-root case=../examples/cases/case-003-thermal-spreader \
  --source-root thermal=../examples/capabilities/thermal

# Same, machine-readable.
python3 avila_core_verify.py verify-case ../examples/cases/case-000-actinv-aftermatter --json

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
- `TestPositivePathOnRealCases`: CASE-000, 001, 002, 003, 008, 009 each
  verify with zero `mismatch`.
- `TestMutations`: the five corruption scenarios in item 7 above.

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
