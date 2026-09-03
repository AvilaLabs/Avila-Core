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
| Random search | `examples/agents/shield_search2.py --random`: uniform proposals, no mutation of the best-so-far, finalists by observed screen margin (amendment A1) | The lower bound on search quality |
| Learning designer | `examples/agents/shield_search2.py` | The arm under test |
| Control sweep | `examples/agents/control_sweep.py` over the declared sub-grid: one or two layers of polyethylene and lead on a 10 cm grid within the limits | Ground truth for the recovery check |
| Recovery run | `examples/agents/shield_search2.py --materials polyethylene lead --grid-cm 10 --max-layers 2`, at most 15 transports | The learning designer confined to the sweep's space |

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

## Amendments

- **A1, 2026-09-02, before the campaign.** The random arm is implemented as
  `shield_search2.py --random` rather than CASE-001's `shield_search.py`,
  because the latter cannot address the coupled case's additional source
  roots. Behaviour is the same: uniform random layer stacks, no mutation of
  the best-so-far, no surrogate, finalists chosen by observed screen margin.
  The sub-grid and the recovery run are declared in the arms table at the
  same time. Budgets, measurements, and outcomes are unchanged.
- **A2, 2026-09-02, before the campaign.** The whole campaign is run by
  `run_campaign.sh`, in the order baselines, sweep, recovery, learning,
  random, with the seed fixed at 1 for every arm.
- **A3, 2026-09-03, after the revision 1 campaign (outcome F1, see
  `RESULTS.md`).** Contract revision 2 widens the box to 2000 kg and 120 cm,
  raises the transport particle budget from 200 000 to 500 000 so that
  candidates within about twenty percent of an allocation are decided rather
  than left `INCONCLUSIVE`, and caps the transport budget at 40 per search
  arm because each transport now costs about four minutes. The learning
  designer is fixed in two ways the campaign exposed: adjacent identical
  layers are merged before a candidate is proposed, and the stopping rule
  follows transported margins once any transport has run. Allocations, the
  activation guide, the arms, the measurements, and the pre-declared
  outcomes are unchanged. Revision 1's results stay on record.
- **A4, 2026-09-03, after the revision 2 campaign.** A fourth outcome is
  added for future campaigns: **search failure**, no arm finds an all-`PASS`
  candidate while a feasible design is established by a candidate evaluated
  outside the arms and labelled exploratory. Revision 2 is reported under
  that name (see `RESULTS.md`): 110 cm polyethylene followed by 5 cm lead
  passes every requirement at 1601.5 kg and no arm proposed it. The campaign
  driver now runs under a suspend inhibitor. The control sweep for the next
  campaign must be declared on a grid that can express the lead thickness
  the box admits.
