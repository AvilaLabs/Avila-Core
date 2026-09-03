# Shielding-search agents

Scripts that drive Avila Core's shielding configuration search from the
outside. None of them constructs or alters a verdict; they propose
candidates, run Core, and read its `--json` report and `--log` campaign
log back. Requirement ids, limits, units, and verdicts are always read from
Core, never hard-coded, so every script here runs unchanged on CASE-001
today and on the composed coupled case later.

- **`shield_search.py`** (frozen) — the original scripted designer: proposes
  uniformly random layered slabs, screens all of them, and sends the
  feasible candidates with the most screen margin to transport.
- **`shield_review.py`** (frozen) — the optional practicality presentation
  gate's reviewer implementation.
- **`shield_common.py`** — shared plumbing the scripts below use: invoking
  Core and parsing its report, canonical decimals, loading the materials
  table and source, classifying a requirement's physical role (mass,
  length, a dose rate) by the *unit* Core reports for it rather than by
  requirement id, and matching a campaign-log row's candidate back to a
  design by hashing candidate files on disk.
- **`shield_search2.py`** — a surrogate-assisted designer. Instead of
  sampling uniformly, it fits a small ridge-regression model per
  requirement over features of the candidate geometry (thickness per
  material, layer count, first/last-layer material, and a few ordering
  features), with a bootstrap ensemble for uncertainty. Each round it
  proposes a batch that maximizes the predicted minimum margin across every
  requirement Core currently reports, plus an exploration bonus from the
  bootstrap spread; screens the batch (python3 only); and sends the
  best predicted-feasible candidates to transport (both capabilities).
  Where a requirement's own data is thin (transport-fed ones especially,
  since transport only runs on finalists), it borrows a sibling
  requirement's prediction — found by matching Core's own
  `metric.output_slot`, e.g. the screen and transport dose-rate
  metrics — as a prior and corrects it with a residual model once some of
  its own data exists. The search stops when its screen budget is
  exhausted or its best observed worst-case margin stops improving for a
  configurable number of rounds. It writes `summary.md` (as
  `shield_search.py` does) plus `constellation.json` / `constellation.md`:
  how often each requirement was the binding constraint, the surrogate's
  sensitivity of each requirement's predicted margin to each material's
  thickness, the Pareto set of mass versus the primary (worst bounded dose)
  metric among feasible candidates, and the best feasible candidate(s).
  `NOT_EVALUATED` (outside a qualification envelope, or a step that did not
  run) and `INCONCLUSIVE` (a bounded interval straddling the limit) are
  counted separately from `PASS`/`FAIL` throughout, never folded into either.
  **The surrogate only proposes; Avila Core alone decides every verdict.**

- **`practice_baseline.py`** — three conventional-practice candidates, sized
  by the same removal-cross-section attenuation model `screen.py` uses, as
  the comparison arms a learned designer has to beat:

  1. **`practice-removal-poly`** — polyethylene alone, thickness solved by
     the removal-cross-section method at a **design factor of 2** (sized to
     half the dose limit, the conventional first-pass margin for a
     calculation with no buildup factor in it).
  2. **`practice-removal-poly-pb`** — the identical polyethylene thickness
     as (1), plus a fixed 5 cm lead layer after it: the usual
     **capture-gamma rule**, since a fast-neutron-only removal calculation
     says nothing about the ~2.2 MeV neutron-capture gammas the
     hydrogenous moderator itself generates.
  3. **`practice-fe-poly-pb`** — a classic 10 cm iron / polyethylene / 5 cm
     lead sandwich, all three sized together by the same removal method and
     design factor: iron first for cheap inelastic removal of the highest-
     energy neutrons, polyethylene for whatever attenuation iron and lead
     do not already supply, lead last for the same capture-gamma rule as
     (2).

  `--run` sends each candidate through Core and records its verdicts
  (screen only, or screen and transport if the OpenMC capability and cross
  sections are given); without `--run` it only writes the candidate JSON
  and the sizing rationale.

- **`control_sweep.py`** — an explicit grid sweep (`sweep` subcommand;
  default one or two layers, two materials, 10 cm steps, bounded by the
  mass/thickness limits a live Core report exposes), run through Core with
  transport on *every* point, as ground truth within the swept region: no
  proposing, only enumerating and asking Core. Its
  `evaluations-to-optimum` subcommand takes a designer's `--log` campaign
  log plus its candidates directory and reports how many evaluations the
  designer needed before it evaluated the same design as the grid's best
  feasible point (by matching candidate files' real sha256 to a layer
  signature, not by trusting a possibly-stale logged path). The full
  default grid is not meant to run casually — transport is slow until the
  faster transport capability lands — so `--limit` caps how many grid
  points actually execute; see the module docstring for the shared machine
  constraints.

## Tests

`test_shield_search2.py` (unittest) covers feature construction, the ridge
fit and its bootstrap uncertainty on synthetic data, acquisition ordering
(including the sibling/residual correction), the stopping rule, the
constellation summary on a synthetic campaign log, and that no output file
contains an invented `status` or `verdict` field — every one present is
copied from a value the synthetic log itself recorded.
