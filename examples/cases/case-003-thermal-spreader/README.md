# CASE-003 — Thermal spreader search

CASE-003 is the second domain for the [generative loop](../../../docs/strategy/GENERATIVE_LOOP.md):
a different physics, a different tool, the same mechanism. A designer proposes
a layered plate; Core screens it with a one-dimensional resistance estimate,
solves two-dimensional steady conduction by finite elements on request,
evaluates four requirements, reports margins, and appends the run to a
campaign log. The candidate is a **free input**, as in CASE-001 and CASE-002.

```text
candidate (free input) ─┬─► screen: 1-D series resistance, no spreading (python3, unqualified, nominal basis)
                        └─► fe: 2-D conduction, P2 finite elements, two mesh levels (thermal-python, deterministic, enclosure basis)
                                    ↓
                          Avila Core: four technical verdicts
```

The plate is 100 mm wide and heated by 50 kW/m² over a central 20 mm strip
of its top face, insulated elsewhere on that face and on its sides, and
cooled by 500 W/m²/K convection to 300 K on its far face. Materials are
nominal isotropic literature values; interfaces are perfect. This is a
research specimen; nothing here is qualified for any decision.

## Requirements

| Id | Requirement | Basis | What can satisfy it |
| --- | --- | --- | --- |
| THERM-R1-screen | screen hotspot ≤ 420 K | nominal | the screen's `unquantified` claim; pessimistic, excludes hopeless designs, establishes nothing |
| THERM-R2-hotspot | finite-element hotspot ≤ 340 K | enclosure | the finite-element `interval`, a two-level discretisation bracket |
| THERM-R3-mass | areal mass ≤ 100 kg/m² | bounded | the screen's `exact` mass |
| THERM-R4-thickness | total thickness ≤ 40 mm | bounded | the screen's `exact` thickness |

The screen applies the strip's flux straight through the stack with no
lateral spreading, so it is pessimistic: every design it passes, the
finite-element step passes too, and many it fails are fine. That is the
opposite polarity from the shielding screen, and the contract does not care.
`execution_policy.require_qualification` is `true`: R2, R3, and R4's admitted
evidence must each carry a satisfied qualification envelope or the
requirement is refused outright (see [Qualification
envelopes](#qualification-envelopes) below).

## Reference candidate and sentinels

`candidates/reference.json` is 3 mm of copper on 10 mm of aluminium.
Committed results:

| Requirement | Verdict | Value |
| --- | --- | --- |
| R1 screen | PASS (nominal) | 402.9 K, margin 17.1 |
| R2 hotspot | PASS (enclosure) | [322.820, 322.821] K, margin 17.2 |
| R3 mass | PASS | 53.88 kg/m², margin 46.1 |
| R4 thickness | PASS | 13 mm, margin 27 |

The capability's own samples under `examples/capabilities/thermal/samples/`
fix the box: 30 mm of aluminium passes at 323.9 K and 81 kg/m²; the reference
passes lighter and thinner; 10 mm of steel fails the hotspot at 360.3 K; 5 mm
of epoxy fails it by more than a thousand kelvin.

## Qualification envelopes

`qualification-fe.json` is the first record in this repository whose
`validation_evidence` is not empty. It binds, by digest,
`examples/capabilities/thermal/validation/nafems-t4.json`: the NAFEMS T4
benchmark, two-dimensional heat transfer with convection, reproduced by the
same solver code path on three mesh levels, converging to 18.2491 °C against
the published 18.25 °C. The envelope covers at most three layers of the
listed materials, a strip flux up to 100 kW/m², convection between 5 and
5000 W/m²/K, and a strip up to 100 mm; its limitations say what the
benchmark does not validate: the material data, the perfect interfaces, and
the bracket's conservatism.

`qualification-screen.json` covers the screen adapter, scoped to its
`areal-mass` and `thickness` output slots only (not its `hotspot-temperature`
estimate, which stays an unqualified nominal guide): at most three layers of
the listed materials and a total thickness of at most 50 mm, because mass
and thickness are exact arithmetic over the bound materials table and the
case author states no wider a search box than that. It binds no validation
evidence and says so in its limitations. Both records report `[INSIDE]` for
the reference candidate.

## Coverage of the library requirement set

`examples/libraries/thermal/requirement-set.json` names five things a
spreader search must address. CASE-003 covers hotspot (R2 at enclosure basis,
R1 as a guide), mass, and thickness, and states two omissions with an
accepting owner: interface temperature, because no interface limit is
declared and every interface is perfect, and transient overshoot, because the
model is steady state.

## Running it

Verify the frozen case; nothing runs:

```bash
cargo run -p avila-core-cli -- run examples/cases/case-003-thermal-spreader \
  --source-root case=examples/cases/case-003-thermal-spreader \
  --source-root thermal=examples/capabilities/thermal
```

Run a candidate of your own through both steps (a few seconds):

```bash
  … --capability python3=/usr/bin/python3 \
    --capability thermal-python=/path/to/venv/bin/python \
    --input candidate=my-candidate.json --log campaign-log.jsonl
```

The finite-element interpreter must be a real file inside its virtual
environment (`python -m venv --copies`): the case runner resolves symlinks
before launching an executable, and a symlink into a system interpreter
loses the environment's packages. The interpreter's digest is then the
system interpreter's; the result document records the finite-element
package version, and the package limitations say so.

## What the package binds

- **Capabilities:** `python3` for the screen; `thermal-python` for the
  finite-element step, both by digest. Each carries a scoped qualification
  record instead of being a qualified package by itself (see [Qualification
  envelopes](#qualification-envelopes)).
- **Artifacts:** the reference candidate, the material table, the source
  definition, both scripts, and the reference candidate's expected outputs.
- **Executions:** `screen` through `avila-labs.thermal/screen@1`; `fe`
  through `avila-labs.thermal/spreader-fe@1`.
- **Free input:** `candidate`.

## What building it taught Core

- **Symlinked environments do not survive the runner.** Recorded above and
  in the package limitations rather than patched around.
- **An artifact must bind evidence.** A package artifact with no evidence
  ids is refused; validation evidence therefore lives in the qualification
  record's digest binding, not in the manifest.
- **The enclosure basis has a home for deterministic brackets.** A plain
  `interval` claim under an `enclosure` requirement is how a discretisation
  bracket is evaluated, distinct from the statistical `coverage_interval`
  the shielding case uses.
- **A qualification can cover some of a step's outputs and not others**
  (S-039). The screen's `areal-mass` and `thickness` are exact arithmetic
  over the bound materials table; its `hotspot-temperature` is a different,
  unqualified estimate from the same execution. `qualification-screen.json`'s
  `covered_output_slots` names only the first two, so `hotspot-temperature`
  stays exactly as unqualified as before any record existed. The screen and
  the finite-element adapter now share one fact extraction (candidate layer
  count and materials, source heat flux, convection, and strip width, and a
  new total-thickness sum), each reporting under its own adapter id.
