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
          Avila Core: four technical verdicts + exact optional presentation request
                                    ↓
          optional practical-review agent: request changes or present to user
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
error: that is why the loop has two fidelities. `execution_policy.require_qualification`
is `true`: R2's, R3's, and R4's admitted evidence must each carry a satisfied
qualification envelope or the requirement is refused outright rather than
merely footnoted (see [Qualification envelope](#qualification-envelope)
below).

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
  --source-root agents=examples/agents \
  --source-root nuclear-data=/path/to/endfb-vii.1-hdf5
```

Screen a candidate of your own (the transport step is reached by the supplied
input, so its committed claims are withheld and R2 is `NOT_EVALUATED`):

```bash
cargo run -p avila-core-cli -- run examples/cases/case-001-shield-search \
  --source-root case=examples/cases/case-001-shield-search \
  --source-root shielding=examples/capabilities/shielding \
  --source-root agents=examples/agents \
  --source-root nuclear-data=/path/to/endfb-vii.1-hdf5 \
  --capability python3=/usr/bin/python3 \
  --input candidate=my-candidate.json --log campaign-log.jsonl \
  --attempt candidate-001
```

For a derived candidate, choose a new attempt id and add
`--parent-attempt candidate-001`; Core derives the candidate changes and
refuses the child if the parent record or fixed question changed.

Run transport on it as well (about a minute at 1e6 particles on 8 threads):

```bash
  … --capability openmc-python=/path/to/venv/bin/python3.12 \
    --env OPENMC_CROSS_SECTIONS=/path/to/endfb-vii.1-hdf5/cross_sections.xml
```

The scripted designer in `examples/agents/shield_search.py` drives the whole
loop: it proposes candidates, screens each, keeps the ones the screen and the
exact requirements accept, and sends the ones with the most screen margin to
transport. After each finalist it gives the exact ready dossier to
`shield_review.py`, writes a staged-review record, returns candidates that need
changes, and places only candidates that pass both the technical requirements
and the authored practical instructions in the user-presentation queue. It
reads reports and never constructs or alters a verdict. Core can be used
without this agent stage by omitting the review capability from the contract.

## Coverage of the library requirement set

`requirement-set.json` is a byte-identical copy of the shielding library's
[requirement set](../../libraries/shielding/requirement-set.json): seven
things any slab-shield search must address. The package declares that the
neutron dose-rate entry is covered by R2 (bounded) with R1 as a guide below
the set's minimum basis, mass by R3, thickness by R4, and that photon dose,
shield activation, and streaming paths are omitted, each with a reason and
the case author as accepting owner; skyshine the set lets pass silently. Core
reports this after compiling and before executing anything:

```text
coverage of requirement set avila-labs.shielding/slab-shield revision 1: [COMPLETE] 3 covered, 3 omitted with a stated reason, 1 omissible, 0 unstated
```

Remove one omission from `package.json` and the run stops there with
`[UNSTATED]` on that entry. That is the point: the search above found the
best neutron shield it could inside a question that, by its own declaration,
does not ask about capture photons.

## Qualification envelope

Two qualification records are bound. `qualification.json` covers the
transport capability: this exact interpreter and script, over a plane
source of 0.1 to 20 MeV neutrons, through at most three layers of the listed
materials, at most 120 cm in total. `qualification-screen.json` covers the
screen's `mass` and `thickness` output slots only (not its `dose-rate`
estimate, which stays an unqualified nominal guide, or its `screen-result`
diagnostic): the same layer-count and material geometry, and the same
120 cm bound, because the mass and thickness are exact arithmetic over the
bound materials table and the case author states no wider a search box than
transport's own. Neither record binds validation evidence and both say so in
their limitations; they exist so that Core can refuse what lies beyond the
stated geometry and so a real qualification has a place to go. Before each
step runs, its adapter reports the source energy and geometry, the slab's
thickness and layer count, and each layer's material as facts with the
input's identity and its own adapter id as provenance (the screen and
transport read the same candidate bytes independently, so each fact names
which adapter read it), and the kernel evaluates each record's scope:

```text
envelope avila-labs.shielding/screen-arithmetic rev 1: [INSIDE] 5/5 terms hold
envelope avila-labs.shielding/slab-transport-openmc rev 1: [INSIDE] 8/8 terms hold
```

`candidates/outside-envelope.json` is 150 cm of polyethylene in three layers,
beyond both records' 120 cm bound. Running it (screen only; transport is
reached but not supplied a capability here) now refuses every bounded
requirement the screen's own output touches, not only transport's:

```text
[PASS] SHIELD-R1-screen — nominal.le.within (nominal ~0.0128 uSv/h; …)
[NOT_EVALUATED] SHIELD-R2-transport — not_evaluated.missing
   because: CORE-R3301 (owner requester)
[NOT_EVALUATED] SHIELD-R3-mass — not_evaluated.outside_qualification
   because: CORE-A4401 (owner method_owner); screen-mass: outside_qualification (avila-labs.shielding/screen-arithmetic rev 1): {"fact":{"name":"slab.total_thickness","op":"le",…"value":{"unit":"cm","value":"120"}}} -> False
[NOT_EVALUATED] SHIELD-R4-thickness — not_evaluated.outside_qualification
   because: CORE-A4401 (owner method_owner); screen-thickness: outside_qualification (avila-labs.shielding/screen-arithmetic rev 1): {"fact":{"name":"slab.total_thickness","op":"le",…"value":{"unit":"cm","value":"120"}}} -> False
```

(R2 is `not_evaluated.missing` here only because transport was not run for
this supplied candidate; running transport as well reproduces the earlier
`not_evaluated.outside_qualification` on R2 too, exactly as before.)

`candidates/outside-envelope-four-layers.json` isolates the layer-count term
from thickness: four 20 cm polyethylene layers, 80 cm total, well inside the
120 cm bound. The screen still computes a mass and a thickness; the geometry
envelope refuses both anyway because it counts four layers where the record
states at most three:

```text
[FAIL] SHIELD-R1-screen — nominal.le.exceeds (nominal ~28.2172 uSv/h; …)
[NOT_EVALUATED] SHIELD-R3-mass — not_evaluated.outside_qualification
   because: CORE-A4401 (owner method_owner); screen-mass: outside_qualification (avila-labs.shielding/screen-arithmetic rev 1): {"fact":{"name":"slab.layer_count","op":"le",…"value":3}} -> False
[NOT_EVALUATED] SHIELD-R4-thickness — not_evaluated.outside_qualification
   because: CORE-A4401 (owner method_owner); screen-thickness: outside_qualification (avila-labs.shielding/screen-arithmetic rev 1): {"fact":{"name":"slab.layer_count","op":"le",…"value":3}} -> False
```

R1, the nominal screen guide, is unaffected either way: qualification only
ever governs a bounded or enclosure requirement (ADR-0008 clause 4).

## Optional practicality presentation gate

The final `practical-review` step is an optional connected-agent capability.
Its exact dossier is the hash-bound reviewer script, candidate, screen result,
and transport result.
The contract and `agent-review-policy.json` bind four practical instructions,
including the rule that every technical requirement must already be `PASS`.
Core emits the realized request only after campaign evaluation and identifies
it canonically:

```text
[READY FOR AGENT] practical-review — optional agent practicality gate; 4/4 dossier artifacts present; request sha256:f5c42c12…
  instruction: Use Core's recorded requirement statuses and rules; never derive, edit, or override a technical verdict.
  instruction: Request changes unless every compiled requirement is PASS.
```

The compiler permits this role only `present_to_user`, `request_changes`, or
`abstain`. Campaign evaluation never reads the routing record, so the stage
cannot gate, create, or alter a technical verdict. In a connected generative
workflow, however, the surrounding agent does not show a candidate to the
user until this optional stage returns `present_to_user`. The committed
[`reviews/reference.json`](reviews/reference.json) embeds that exact request
and records the expected negative route for the reference candidate:
`request_changes`, because Core reports R2 as `FAIL`. The record is unsigned
and `unverified`; it is presentation-routing history, not technical evidence.

## What the package binds

- **Capabilities:** `python3` (the system interpreter, by digest) for the
  screen; `openmc-python` (the OpenMC virtual environment's interpreter, by
  digest) for transport. Neither package is qualified by itself; each named
  capability's mass/thickness or transport output carries a scoped
  qualification record instead (`qualification-screen.json`,
  `qualification.json`). The practical reviewer is an input artifact
  identified by its own digest, not an executable granted runner authority.
- **Artifacts:** the reference candidate, the material table, the source
  definition, both computational scripts, the reviewer script, the
  nuclear-data index (identity of the index
  only; the nuclide files it names are not re-hashed), and the reference
  candidate's expected outputs under `expected/`.
- **Optional practical review:** the agent policy and reference routing record as package
  documents; `practical-review` has no package execution because the runner
  materializes the request and external software consumes it.
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

[`search/summary.md`](search/summary.md) and
[`search/campaign-log.jsonl`](search/campaign-log.jsonl) record the first run
of the scripted designer (seed 1), with the three finalist candidates under
`search/finalists/`:

| Stage | Count |
| --- | ---: |
| candidates screened (about 0.1 s each) | 200 |
| accepted by screen, mass, and thickness | 17 |
| distinct designs sent to transport (duplicates skipped) | 3 (2) |
| transport verdicts | 2 INCONCLUSIVE, 1 FAIL, 0 PASS |

No candidate passes the bounded requirement. 100 cm of polyethylene is
`INCONCLUSIVE` at [~9.4, ~12.0] uSv/h; replacing part of it with borated
polyethylene lowers the interval to [~9.0, ~11.2], still crossing the limit; a
polyethylene-and-water layering fails at [~14.1, ~17.3]. The screen rated all
three between 3.1 and 3.7 uSv/h, about three times too optimistic, and could
not tell borated from plain polyethylene at all. Read as the loop intends:
within this material table, 100 cm, and 1500 kg, the stated requirement was
not met by anything the designer tried; the next moves belong to the designer
(more particles on the borated candidate, a less optimistic screen, materials
from the table it never combined into a feasible candidate) and to the
requirement owner, not to Core.

That frozen search predates the optional presentation-gate slice, so its historical log is
not retrofitted with routing records. Running the current designer writes a
record for every transported finalist and reports separately how many were
returned and how many entered the user-presentation queue. Since all three
historical finalists were `FAIL` or `INCONCLUSIVE`, the bound instructions
would return all three rather than present any of them.
