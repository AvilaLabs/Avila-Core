# CASE-003 protocol: does the loop transfer to a second domain?

**Status:** pre-registered 2026-09-03, before any campaign on this case.
**Owner:** Connor Avila, case author. **Hypothesis under test:** the search
mechanism shown on CASE-002 transfers to a different physics and tool with
no change to Core: a designer reading only Core's verdicts and margins finds
a plate that passes every requirement and is lighter than an exhaustive grid's
best, under the package pin of S-030.

## Arms

| Arm | What proposes candidates | Purpose |
| --- | --- | --- |
| Control sweep | twelve declared points: 10, 20, and 30 mm single plates of aluminium, copper, and graphite, and 2, 3, and 5 mm of copper on 10 mm of aluminium, run through `shield_llm_tools.py` | The bar: the lightest all-`PASS` point |
| Language-model designer | `shield_llm_tools.py` with the CASE-002 prompt adapted to this case, transport budget 20, package pinned | The arm under test |

The scripted surrogate designer is not run: it still assumes the shielding
material table, and adapting it is recorded as future work rather than done
mid-campaign.

## Measurements and outcomes

As in CASE-002's protocol: per arm, candidates screened and evaluated, first
all-`PASS` and the evaluation count at which it appeared, lightest all-`PASS`
by mass, refusals with their named reason. Outcomes: **supported** (an
all-`PASS` design lighter than the sweep's best, reached in fewer evaluations
than the sweep's size, with the package pin active throughout), **F1**
bounded exhaustion, **F2** search adds nothing, **F3** practice wins, and
**search failure** as CASE-002's A4 defines it. The refusal criterion counts
any candidate refused by the envelope, the coverage, or the pin.

## Budget

Screens at most 60 and evaluations at most 20 for the designer; the sweep is
its twelve points.

## Amendments

- **A1, 2026-09-03, after the first sweep and before the designer arm.**
  The sweep under contract revision 1 showed two things: the screen script
  emitted a 40-digit repeating decimal that Core refused as exceeding its
  exact-arithmetic budget (two graphite points `rejected`; the script now
  rounds claim values to 12 significant digits), and the 340 K screen guide
  could not be met by any plate because the no-spreading estimate is about
  80 K pessimistic, so no design could be all-`PASS`. Contract revision 2
  sets the screen guide at 420 K. The revision-1 sweep is kept on record
  under `campaign-c3/sweep-rev1/`; the sweep is rerun under revision 2 and the
  designer arm runs only under revision 2. Limits on the finite-element
  hotspot, mass, and thickness are unchanged.
