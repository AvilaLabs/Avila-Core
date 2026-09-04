# CASE-002 — Coupled shielding search

CASE-002 composes CASE-001's search with the two steps its requirement set
said were missing. A candidate slab is screened, transported with coupled
neutron and photon Monte Carlo, and then activated layer by layer with
ACTINV on the neutron spectrum transport tallied in each layer. Core
evaluates six requirements, reports exact margins, and appends every run to
a campaign log. The candidate is a **free input**, as in CASE-001.

```text
candidate (free input) ─┬─► screen: removal attenuation (python3, unqualified, nominal basis)
                        └─► transport: OpenMC neutron+photon slab (openmc-python, seeded_stochastic, bounded basis)
                                    │  neutron dose, photon dose, per-layer FISPACT-709 spectra
                                    ▼
                            activation: ACTINV per layer (python3 driving the hash-bound actinv executable, nominal basis)
                                    ▼
          Avila Core: six technical verdicts + exact optional presentation request
```

This is a research specimen over a synthetic plane source. No facility,
generator, occupancy, or regulatory limit is represented, the removal cross
sections are approximate, and nothing here is qualified for any decision.
The experiment this case exists for is pre-registered in
[PROTOCOL.md](PROTOCOL.md). The contract is at revision 2: revision 1's
campaign found no feasible design within 1500 kg and 100 cm (outcome F1 in
[RESULTS.md](RESULTS.md)), so the box was widened to 2000 kg and 120 cm and
the particle budget raised, as amendment A3 records. Revision 2's campaign
is a recorded search failure: 110 cm polyethylene behind 5 cm lead passes
every requirement and no arm proposed it (amendment A4, `RESULTS.md`).

## Requirements

| Id | Requirement | Basis | What can satisfy it |
| --- | --- | --- | --- |
| SHIELD-R1-screen | screen dose rate ≤ 10 uSv/h | nominal | the screen's `unquantified` claim; guides the search |
| SHIELD-R2-neutron | neutron dose rate ≤ 7 uSv/h | bounded | transport `coverage_interval` (0.95, statistical only) |
| SHIELD-R3-photon | shield-produced photon dose rate ≤ 3 uSv/h | bounded | transport `coverage_interval` (0.95, statistical only) |
| SHIELD-R4-mass | areal mass ≤ 2000 kg | bounded | the screen's `exact` mass |
| SHIELD-R5-thickness | total thickness ≤ 120 cm | bounded | the screen's `exact` thickness |
| SHIELD-R6-activation | highest layer specific activity ≤ 1 Bq/g after 30 d irradiation and 1 d cooling | nominal | ACTINV's `unquantified` total; guides the search |

The 10 uSv/h design point is allocated 7 uSv/h to neutrons and 3 uSv/h to
photons produced in the shield; Core evaluates each allocation and does not
sum claims. The activation limit is the case author's allocation. It binds
for iron placed in front of the moderator and not for iron or lead placed
behind it, which is the ordering trade-off the case exists to exercise.
`execution_policy.require_qualification` is `true`: R2, R3, R4, and R5's
admitted evidence must each carry a satisfied qualification envelope or the
requirement is refused outright (see [Qualification
envelopes](#qualification-envelopes) below).

## Reference candidate and probe

`candidates/reference.json` is 80 cm of polyethylene followed by 5 cm of
lead. Committed results:

| Requirement | Verdict | Value |
| --- | --- | --- |
| R1 screen | FAIL (nominal) | ~15.6 uSv/h, margin ~−5.6 |
| R2 neutron | FAIL (bounded) | [~57.2, ~69.6] uSv/h, margin ~−62.6 |
| R3 photon | FAIL (bounded) | [~3.15, ~3.36] uSv/h, margin ~−0.36 |
| R4 mass | PASS | 1319.5 kg, margin 680.5 |
| R5 thickness | PASS | 85 cm, margin 35 |
| R6 activation | PASS (nominal) | ~0.0006 Bq/g in the lead, margin ~0.9994 |

`candidates/iron-first.json` (10 cm iron in front of 70 cm polyethylene) is
not bound by the package; it was run once through `--input` to check that
the activation guide discriminates ordering before any campaign ran. Iron in
front halves the neutron dose to about 40 uSv/h, raises the photon dose to
about 60 uSv/h from inelastic and capture photons that nothing behind the
polyethylene stops, and activates to about 4.3 Bq/g (Fe-55, Cr-51, Mn-54),
failing the guide. The coupling is real and it runs in both directions.

## Coverage of the library requirement set

The shielding library's seven-entry set is covered as follows: neutron dose
by R2 with R1 as a guide, photon dose by R3, mass by R4, thickness by R5.
Streaming paths are omitted with a stated reason. **Activation is omitted at
the required basis**: the set demands a bounded basis and ACTINV's inventory
is nominal, so R6 guides the search and does not cover the entry, and the
package names the missing capability, a bounded activation claim.

That omission is not a design choice made in advance. The first version of
this contract mapped R6 as covering activation and Core refused to execute
it: `covered only on a basis weaker than the set's minimum Bounded; a guide
is not evidence`. The refusal was aimed at the case author.

## Qualification envelopes

Three records, all binding no validation evidence and saying so:

- `qualification-transport.json` over the coupled transport adapter: plane
  source of 0.1 to 20 MeV, at most three layers of the listed materials, at
  most 120 cm.
- `qualification-activation.json` over the activation adapter: at most three
  layers of the listed materials, irradiation at most one year, cooling at
  least one hour, with the schedule facts in seconds.
- `qualification-screen.json` over the screen adapter, scoped to its `mass`
  and `thickness` output slots only (not its `dose-rate` estimate, which
  stays an unqualified nominal guide): the same geometry and 120 cm bound as
  the coupled transport record, because mass and thickness are exact
  arithmetic over the bound materials table and the case author states no
  wider a search box than transport's own.

The verify run reports all three `[INSIDE]` for the reference candidate.

## Running it

Verify the frozen case; nothing runs and no executable is needed:

```bash
cargo run -p avila-core-cli -- run examples/cases/case-002-coupled-shield \
  --source-root case=examples/cases/case-002-coupled-shield \
  --source-root shielding=examples/capabilities/shielding \
  --source-root coupled=examples/capabilities/shield-coupled \
  --source-root agents=examples/agents \
  --source-root nuclear-data=/path/to/endfb-vii.1-hdf5 \
  --source-root actinv-release=/path/to/actinv/target/release \
  --source-root actinv-data=/path/to/actinv-data/v1.0.0
```

The `nuclear-data`, `actinv-release`, and `actinv-data` roots hold hundreds
of megabytes that this command re-hashes on every invocation; add
`--hash-cache PATH` (S-038) to skip re-reading bytes whose path, size, and
modification time still match a prior run, with a `verified_cached` state
in place of `verified` wherever it did. This verify command deliberately
does not pass it above: verifying the frozen case is exactly the moment a
full re-hash from bytes is the point, and `examples/cases/tools/bless.py`
and `rehash.py` never pass it either. Its trust boundary is in `SECURITY.md`.

Run a candidate of your own through every step (about four minutes on
eight threads at 500 000 particles):

```bash
  … --capability python3=/usr/bin/python3 \
    --capability openmc-python=/path/to/venv/bin/python3.12 \
    --env OPENMC_CROSS_SECTIONS=/path/to/endfb-vii.1-hdf5/cross_sections.xml \
    --input candidate=my-candidate.json --log campaign-log.jsonl
```

Omit the OpenMC capability and the transport step is reported `not_run`
with its claims withheld; the activation step, which consumes transport's
spectra, is then not reached.

## What the package binds

- **Capabilities:** `python3` (the system interpreter, by digest) for the
  screen and the activation driver; `openmc-python` (the OpenMC virtual
  environment's interpreter, by digest) for transport. None is a qualified
  package by itself; each carries scoped qualification records instead (see
  [Qualification envelopes](#qualification-envelopes)).
- **Artifacts:** the reference candidate, the material table, the source
  definition, the FISPACT-709 group structure, the irradiation schedule, the
  three scripts, the reviewer script, the nuclear-data index (identity of
  the index only), the ACTINV 1.0.1 executable and its activation library,
  library index, and two decay files (all re-hashed), and the reference
  candidate's expected outputs under `expected/`.
- **Executions:** `screen` through `avila-labs.shielding/screen@1`;
  `transport` through `avila-labs.shielding/slab-transport@2`, which requires
  the operator to value `OPENMC_CROSS_SECTIONS`; `activation` through
  `avila-labs.shielding/activation@1`, whose spectra input is the transport
  step's `layer-spectra` output.
- **Free input:** `candidate`.

## What building it taught Core

Four refusals, each of which became a rule or a fix rather than a workaround:

1. **A guide is not coverage.** The coverage stage refused the first
   contract because activation was mapped at a nominal basis against a
   library minimum of bounded. The entry is now a stated omission with the
   missing capability named.
2. **An output no data can produce is not a slot.** The activation adapter
   first listed ACTINV's contact-dose proxy as an optional output. The runner
   requires a package to bind exactly the slots an adapter lists, so an
   output that the bound data can never produce would fail every run at
   binding. The slot was removed.
3. **Media types are identity.** The transport adapter wrote its spectra
   artifact as `application/vnd.avila.shield-layer-spectra+json` while the
   registry declared `…shield-spectra+json`. Core quarantined the spectra
   (`CORE-E7201`) and, through the parent link, every activation claim
   (`CORE-E7103`). The registry now names the type the adapter produces.
4. **Facts belong to the adapter that produced them.** The coupled adapter
   reused v1's fact extraction, which stamped the v1 adapter id as
   validator, so the coupled qualification record saw every term as
   `unknown` and both transport requirements were `NOT_EVALUATED` although
   transport ran. The extraction now takes the validator id.
5. **A qualification can cover some of a step's outputs and not others**
   (S-039). The screen's `mass` and `thickness` are exact arithmetic over the
   bound materials table; its `dose-rate` is a different, unqualified
   estimate from the same execution. `qualification-screen.json`'s
   `covered_output_slots` names only the first two, so the runner attaches
   the envelope to those claims and leaves `dose-rate` exactly as
   unqualified as before any record existed.

Tooling added alongside the case: `examples/cases/tools/rehash.py`
recomputes every digest a package declares from supplied roots, and
`examples/cases/tools/bless.py` copies a completed fresh run's claims,
campaign report, receipts, and outputs into a case and rehashes it.
