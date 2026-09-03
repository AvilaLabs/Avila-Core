# CASE-002 protocol: does the loop create a design that practice does not?

**Status:** pre-registered 2026-09-02, before any coupled result exists.
**Probe, same day, before any campaign:** the reference candidate (80 cm
polyethylene, 5 cm lead) and one iron-first probe (10 cm iron, 70 cm
polyethylene) were run once each to check that the pre-registered
activation guide discriminates ordering: 0.0006 Bq/g behind the moderator,
4.3 Bq/g in front of it, against the 1 Bq/g limit. No limit was changed.
**Owner:** Connor Avila, case author. **Hypothesis under test:** H8 in
[the validation plan](../../../docs/roadmap/VALIDATION_PLAN.md), in its
creation form: a designer searching through Core finds a slab shield that
satisfies every stated requirement and is better than conventional practice
under the same model.

This protocol is written first so that the result cannot be shaped by the
result. Anything below that changes after the first coupled run is recorded
as an amendment with its date and reason, never edited in place.

## The question

Within the contract's limits (neutron allocation 7 uSv/h, photon allocation
3 uSv/h, specific activity 1 Bq/g after 30 days at the source flux and 1 day
of cooling, 1500 kg per square metre, 100 cm), does a designer that reads only
Core's verdicts and margins find a candidate that:

1. is `PASS` on every compiled requirement, and
2. has lower areal mass than every practice baseline that is also `PASS` on
   every requirement, or, if no baseline passes, names the requirement each
   baseline fails?

Mass is the tie-breaker because it is the only requirement the designer can
trade against the dose and activation limits; a lighter all-`PASS` shield is
the ordinary engineering meaning of "better" here.

## Arms

All arms run through the same contract, package, capabilities, seed, particle
budget, and schedule. Nothing is compared across models.

| Arm | What proposes candidates | Purpose |
| --- | --- | --- |
| Practice baselines | `examples/agents/practice_baseline.py`: (i) polyethylene by the removal method with factor 2; (ii) the same plus 5 cm lead; (iii) 10 cm iron, polyethylene, 5 cm lead | What a textbook rule produces without a search |
| Random search | `examples/agents/shield_search.py` (CASE-001's designer) | The existing lower bound on search quality |
| Learning designer | `examples/agents/shield_search2.py` | The arm under test |
| Control sweep | `examples/agents/control_sweep.py` over a declared sub-grid | Ground truth for the recovery check |

## Measurements

Recorded from Core's campaign log and reports only; the designer writes no
verdict. Exact values live in the log; summaries round.

- Per arm: candidates screened, candidates transported and activated, first
  all-`PASS` candidate and the evaluation count at which it appeared, best
  all-`PASS` candidate by mass, verdict histogram per requirement, number of
  `INCONCLUSIVE` results and how the designer responded, number of
  `NOT_EVALUATED` results and their named terms.
- Recovery check: on the declared sub-grid, whether the learning designer's
  best candidate equals the sweep optimum, and the ratio of designer
  evaluations to sweep size.
- Refusals: every candidate Core refused, with the reason it named
  (qualification envelope, coverage, integrity, admission).
- Cost: wall time per transport and per activation, total wall time per arm.

## Pre-declared outcomes

- **Supported.** The learning designer produces an all-`PASS` candidate that
  is lighter than every all-`PASS` baseline, or an all-`PASS` candidate when no
  baseline passes; it reaches the sub-grid optimum in fewer evaluations than
  the sweep; Core refused at least one candidate on record. All three must hold.
- **Search adds nothing (F2).** The learning designer needs as many or more
  transport evaluations as the sweep to reach the sub-grid optimum, or does
  not beat random search on evaluations to first all-`PASS`. Core remains a
  checker with good records; the designer, not Core, is what failed.
- **Practice wins (F3).** A baseline is all-`PASS` at a mass no designer
  candidate beats. The rule of thumb was already at the optimum of this box.
- **Bounded exhaustion (F1).** No arm finds an all-`PASS` candidate within the
  limits. This is a legitimate Core result: the box has no feasible design
  under this model. It leaves the creation question untested here; a wider
  box is a new contract revision, recorded as such, and this protocol is
  amended, not silently rerun.

Any of F1 to F3 is reported as found. None is a reason to change the
requirements mid-campaign.

## Budget and stopping

- Screening: at most 2000 candidates per search arm.
- Transport and activation: at most 60 candidates per search arm.
- Stopping: budget exhausted, or no improvement in the best feasible mass over
  five consecutive rounds.
- An `INCONCLUSIVE` bounded verdict is a result, not an error. The designer may
  spend more of its transport budget on that candidate only if the contract's
  particle budget is unchanged; raising the budget is a contract revision.

## What this test cannot show

The transport envelope binds no validation evidence, so every `PASS` is a pass
under an unvalidated model, and the comparison is relative: baseline and
designer run through the same model. A design that "beats practice" here has
beaten practice inside the model. Establishing the model's bias against a
reference experiment is a separate test, and until it is done no absolute
claim about a real shield follows from this case.

## Record

Results are recorded in `RESULTS.md` beside this file with the campaign log,
the constellation summary, the baseline runs, and the sweep table, each
identified by digest. The decision on the outcome names which pre-declared
outcome occurred and cites the log lines that establish it.
