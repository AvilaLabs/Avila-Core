# CASE-002 results, contract revision 1

**Campaign:** 2026-09-03, `run_campaign.sh`, seed 1 for every arm, contract
revision 1 (neutron 7 uSv/h, photon 3 uSv/h, activation guide 1 Bq/g,
1500 kg, 100 cm, 200 000 particles). Arms in the pre-declared order:
baselines, sweep, recovery, learning, random.
**Summary tables:** `campaign-rev1/campaign-summary.md`, derived from the
campaign logs by `summarize_campaign.py`; the logs, every candidate, and the
arms' own summaries are kept under `campaign-rev1/` with the digests listed at
the end. Nothing below was computed by hand.

## Outcome: bounded exhaustion (F1)

No arm produced a candidate that is `PASS` on every compiled requirement.
The box has no feasible design under this model:

- **Practice baselines.** All three fail. 96 cm polyethylene by the removal
  method fails neutron ([~16, ~21] uSv/h against 7) and photon (~28 against
  3); adding 5 cm lead fixes photon but is 101 cm; the iron sandwich fails
  mass, photon, and the activation guide.
- **Control sweep.** 17 points of one or two layers of polyethylene and lead
  on a 10 cm grid. None passes. 100 cm polyethylene, the most any single
  layer can be, is [~8.5, ~13.2] uSv/h neutron and ~23 uSv/h photon. Mass
  limits lead to 10 cm, and 30 cm polyethylene with 10 cm lead is still
  thousands of uSv/h.
- **Recovery run.** Confined to the sweep's space, the learning designer
  transported 8 candidates and found nothing; the recovery check is not
  applicable because the sweep has no all-`PASS` point.
- **Learning designer.** 220 screened, 11 transported, none passes. It moved
  toward the only family that approaches feasibility, borated polyethylene
  with a thin iron layer behind it, whose photon dose lands near 4 uSv/h
  without activation, and its best candidate (5 cm polyethylene, 90 cm
  borated polyethylene, 5 cm iron) came within two `INCONCLUSIVE` verdicts of
  passing: neutron [~6.2, ~18.4] and photon [~2.9, ~3.4].
- **Random search.** 121 screened, 6 transported, none passes; it stopped by
  the same saturated rule. Its transported candidates were single-material
  borated polyethylene slabs and water mixtures, none within a factor of two of
  the neutron allocation; the learning arm's finalists were closer on every
  transport it made (see the histograms in the summary).

The outcome is pre-declared: a wider box is a new contract revision,
recorded as amendment A3 below, and the protocol is amended rather than
silently rerun.

## What the campaign established anyway

- **The coupling is real and runs in both directions.** Iron in front halves
  the neutron dose and multiplies the photon dose twenty-fold; lead in front
  activates past the guide and lead behind the moderator does not;
  polyethylene alone leaks capture photons at eight times the allocation;
  borated polyethylene with iron behind it is the family that reconciles
  neutrons, photons, and activation, and it needs more than 100 cm.
- **Core refused nothing during the campaign** because every candidate was
  inside both envelopes and the coverage was fixed; the four refusals on
  record all happened during composition (see the case README).
- **The screen is not a proxy.** Its best margin saturated at 5 uSv/h while
  every transported candidate failed; the screen guided the search and
  established nothing, as the contract says.

## What the campaign found wrong with the designer, not with Core

1. **No canonical form.** The recovery run spent five of eight transports on
   90 cm of polyethylene split into two identical-material layers, which
   transport, activation, mass, and thickness cannot distinguish.
2. **The stopping rule keyed on the screen.** Both search arms stopped after
   five rounds without improvement in the best screen-level margin, which
   saturates long before the bounded requirements do, so the learning arm
   used 11 of 60 transports. The rule now follows transported margins once
   any transport has run.
3. **Absorbing shields starve the tally.** Neutron intervals behind 90 cm of
   borated polyethylene are three times wider than behind plain polyethylene
   at the same particle count, because the weight windows are built from
   removal cross sections that ignore absorption. Revision 2 raises the
   particle budget; a window built from the tallied flux is future work.

## Amendments

- **A3, 2026-09-03, after the revision 1 campaign.** Contract revision 2
  widens the box to 2000 kg and 120 cm, raises the transport particle budget
  to 500 000 so that candidates within about twenty percent of an allocation
  are decided, and caps the transport budget at 40 per search arm because
  each transport now costs about four minutes. The designer is fixed as
  above. Allocations, the activation guide, the arms, the measurements, and
  the pre-declared outcomes are unchanged.

## Record

Driver status lines: `2026-09-03T00:53:38Z` start, `2026-09-03T02:16:17Z` end, all arms `DONE`.

| file | sha256 | lines |
| --- | --- | ---: |
| `campaign-rev1/baselines/campaign-log.jsonl` | `58d2ceda185e2aeb4d4120c3caa6979b2adea2cb342996e5e760c33c33acde60` | 4 |
| `campaign-rev1/sweep/campaign-log.jsonl` | `381ab1d17406bffa0a12be403c941f0e7bdb73dcd146fc2789ea6bf057276d68` | 18 |
| `campaign-rev1/recovery/campaign-log.jsonl` | `ed2f062d5280e6e3a9bd9535b6a7eb837ffeddd3fe4e7141c58dc6eb8faa3576` | 53 |
| `campaign-rev1/learning/campaign-log.jsonl` | `9f85cd846fc231b1a004b51396c13fee429fa94efe2c0acef37bb50fba77ffed` | 232 |
| `campaign-rev1/random/campaign-log.jsonl` | `280728f57ad5d66f90be296810f47f28fee7d2e30069cfb7c4cce85ac7515c26` | 127 |
| `campaign-rev1/campaign-summary.md` | `9336afa3ae8835740dec4cd5df6853c8d1c4d330ab2c2276b5dfc973b9d79aaf` | 35 |
| `campaign-rev1/campaign-summary.json` | `e25b4bc3dae6a9d77eb866c1c5f7cb0e0a3af7890c2d40da795ac549219347aa` | 249 |
| `campaign-rev1/STATUS` | `6e5367c2941b4f570dabd5c7113239ec52284c1529e87f491bb1e23fcf34c09f` | 12 |
