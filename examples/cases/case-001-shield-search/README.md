# CASE-001 — Shielding configuration search

CASE-001 is the first case built for the [generative loop](../../../docs/strategy/GENERATIVE_LOOP.md):
a designer, human or scripted, proposes a layered slab; Core screens it with
a cheap unqualified method, runs Monte Carlo transport on it when asked,
evaluates four requirements, reports margins, and appends the run to a
campaign log. The candidate is a **free input**: it may be supplied on the
command line without re-freezing the package.

```text
candidate (free input) ─┬─► screen: removal-cross-section attenuation (python3, unqualified, nominal basis)
                        └─► transport: OpenMC slab model (openmc-python, seeded_stochastic, bounded basis)
                                    ↓
             Avila Core: four requirement verdicts with margins, campaign-log line
```

This is a research specimen over a synthetic plane source. No facility,
generator, occupancy, or regulatory limit is represented; the removal cross
sections are approximate; the dose coefficient is a single number at one
energy. Nothing here is qualified for any decision.

## Requirements

| Id | Requirement | Basis | What can satisfy it |
| --- | --- | --- | --- |
| SHIELD-R1-screen | screen dose rate ≤ 10 uSv/h | nominal | the screen's `unquantified` claim; guides the search, establishes nothing |
| SHIELD-R2-transport | transport dose rate ≤ 10 uSv/h | bounded | the transport `coverage_interval` (0.95, statistical error only) |
| SHIELD-R3-mass | areal mass ≤ 1500 kg | bounded | the screen's `exact` mass |
| SHIELD-R4-thickness | total thickness ≤ 100 cm | bounded | the screen's `exact` thickness |

The contract permits the nominal basis only for R1. A candidate that passes
the screen and fails transport is the expected shape of a result, not an
error: that is why the loop has two fidelities.

## Reference candidate

`candidates/reference.json` is 90 cm of polyethylene. Committed results:

| Requirement | Verdict | Value |
| --- | --- | --- |
| R1 screen | PASS (nominal) | ~9.39 uSv/h, margin ~0.61 |
| R2 transport | FAIL (bounded) | [~25.4, ~28.8] uSv/h at 0.95 coverage, margin ~−18.8 |
| R3 mass | PASS | 846 kg, margin 654 |
| R4 thickness | PASS | 90 cm, margin 10 |

The screen is optimistic by about a factor of three here (no buildup, fission-
spectrum removal cross sections applied to 14 MeV neutrons). Core does not
know that; it reports that the unqualified method says one thing and the
bounded one says another, and it never lets the first count as the second.

## Running it

Verify the frozen case; nothing runs and no executable or environment value is
needed (about 20 ms):

```bash
cargo run -p avila-core-cli -- run examples/cases/case-001-shield-search \
  --source-root case=examples/cases/case-001-shield-search \
  --source-root shielding=examples/capabilities/shielding \
  --source-root nuclear-data=/path/to/endfb-vii.1-hdf5
```

Screen a candidate of your own (the transport step is reached by the supplied
input, so its committed claims are withheld and R2 is `NOT_EVALUATED`):

```bash
cargo run -p avila-core-cli -- run examples/cases/case-001-shield-search \
  --source-root case=examples/cases/case-001-shield-search \
  --source-root shielding=examples/capabilities/shielding \
  --source-root nuclear-data=/path/to/endfb-vii.1-hdf5 \
  --capability python3=/usr/bin/python3 \
  --input candidate=my-candidate.json --log campaign-log.jsonl
```

Run transport on it as well (about a minute at 1e6 particles on 8 threads):

```bash
  … --capability openmc-python=/path/to/venv/bin/python3.12 \
    --env OPENMC_CROSS_SECTIONS=/path/to/endfb-vii.1-hdf5/cross_sections.xml
```

The scripted designer in `examples/agents/shield_search.py` drives the whole
loop: it proposes candidates, screens each, keeps the ones the screen and the
exact requirements accept, and sends the ones with the most screen margin to
transport. It reads reports and never constructs a verdict.

## What the package binds

- **Capabilities:** `python3` (the system interpreter, by digest) for the
  screen; `openmc-python` (the OpenMC virtual environment's interpreter, by
  digest) for transport. Neither is a qualified package.
- **Artifacts:** the reference candidate, the material table, the source
  definition, both scripts, the nuclear-data index (identity of the index
  only; the nuclide files it names are not re-hashed), and the reference
  candidate's expected outputs under `expected/`.
- **Executions:** `screen` through `avila-labs.shielding/screen@1`;
  `transport` through `avila-labs.shielding/slab-transport@1`, which requires
  the operator to value `OPENMC_CROSS_SECTIONS`. The key name is invocation
  identity; the value is recorded in the receipt but is not identity, because
  the script refuses to run unless the index at that path has the digest of
  the staged index.
- **Free input:** `candidate`.

## What building it taught Core

Three things Core refused before this case was blessed, each of which became
a rule rather than a workaround:

1. **OpenMC aborted under a cleared environment.** The build in use
   initializes Open MPI, which needs a home directory. The adapter now sets
   `HOME=.` (the step directory) as static environment, so nothing outside
   the workspace is read and the receipt is the same on every machine.
2. **Transport was not byte-reproducible.** Two runs with the same seed
   differed in the last floating-point digits because OpenMC reduces
   per-thread tallies in a varying order. Core reported `DIFFERS from the
   bound artifact` and refused to bind. The capability now rounds its tally
   statistics to 8 significant digits, far above that noise and far below
   the statistical uncertainty, and the same seed reproduces the same bytes.
   Byte stability under a declared seed is the capability's job; Core's job
   is to notice when it is missing.
3. **A locator path was identity.** The first receipts carried the
   machine-specific cross-section path inside the invocation identity, so
   reuse would have failed on any other machine, and verification needed the
   value even when nothing ran. Required environment keys are now identity by
   name only.

Also found on the way: an `exact` claim under a `bounded` requirement was
refused by the kernel although ADR-0006 defines `exact` as the degenerate
interval `lo = hi = nominal`; R3 and R4 exposed it, and vector
`le.bounded.exact.within` now pins the rule.

## First search

See [`search/summary.md`](search/summary.md) and the campaign log beside it.
