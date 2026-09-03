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

---

# CASE-002 results, contract revision 2

**Campaign:** 2026-09-03, `run_campaign.sh`, seed 1 for every arm, contract
revision 2 (neutron 7 uSv/h, photon 3 uSv/h, activation guide 1 Bq/g,
2000 kg, 120 cm, 500 000 particles), transports capped at 40 per search arm
(amendment A3). Arms in the pre-declared order.
**Summary tables:** `campaign-rev2/campaign-summary.md`, derived from the
logs by `summarize_campaign.py`; logs, candidates, and arm summaries under
`campaign-rev2/` with the digests listed at the end.

## Outcome: no arm found an all-PASS design, and the box is not exhausted

By the letter of the pre-declared list this is F1, "no arm finds an all-PASS
candidate within the limits". By its meaning it is not: F1 was written for a
box with no feasible design, and this box almost certainly has one. The
sweep's 110 cm of polyethylene passes neutron at [~2.7, ~4.4] uSv/h and
fails only photon; the baselines' 5 cm of lead cuts the photon dose of
96 cm of polyethylene from ~27 to ~1.8 uSv/h; so 110 cm of polyethylene
followed by 5 cm of lead, at about 1600 kg and 115 cm, is expected to pass
every requirement. No arm evaluated it:

- **Practice baselines.** All three fail the neutron allocation, because the
  removal-method sizing underestimates the transported dose by about a
  factor of three. Adding 5 cm of lead does fix photons (~1.75 uSv/h).
- **Control sweep.** 31 points on a 10 cm grid of polyethylene and lead.
  None passes. This is the protocol's fault, not the model's: at 10 cm
  steps the only lead option is 10 cm, which with enough polyethylene to
  pass neutron exceeds 2000 kg, while polyethylene alone passes neutron at
  110 cm and 120 cm and fails photon by a factor of three to five.
- **Recovery run.** The confined designer screened the whole grid without
  one identical-layer split (the revision 1 fix works), transported five
  distinct designs, found nothing, and stopped when the grid was exhausted.
  The recovery check is inapplicable because the sweep has no all-PASS point.
- **Learning designer.** 120 screened, 6 transported, none passes. Every
  finalist that passed neutron put its heavy layer in front of the
  moderator (5 cm concrete, 10 cm iron, 5 cm iron ahead of 100 to 105 cm of
  polyethylene), which helps neutrons and does nothing for the capture
  photons made behind it, so photon failed by a factor of seven to nine each
  time. The surrogate had not learned ordering from six transports, and the
  stopping rule, now keyed on transported margins, ended the arm after five
  rounds with one finalist each: 34 of 40 transports unspent.
- **Random search.** 240 screened, 12 transported, none passes. Uniform
  sampling reached the neutron allocation 7 times, all with borated
  polyethylene of 105 cm or more or a water-concrete mixture, and never put
  a heavy layer behind the moderator, so photon failed every time, by a
  factor of two to five. Its best worst-case margin, about −4.4 uSv/h on
  110 cm of borated polyethylene, was nevertheless closer to feasibility
  than anything the learning arm transported.

This is a search failure, recorded as such: the designer did not propose the
design the evidence pointed at, and its stopping rule let it quit with most
of its budget unspent. Core evaluated everything it was given exactly, and
the campaign log now holds the ordering effect in numbers: heavy layer in
front, neutron pass and photon fail; heavy layer behind (revision 1's
learning arm), photon near its allocation and neutron short.

**Post-campaign exploratory check, not pre-registered.** The obvious
polyethylene-lead designs, run once each through the same contract after
the arms had finished, with their logs under `campaign-rev2/posthoc/`:

| candidate | neutron uSv/h | photon uSv/h | mass kg | cm | activation | all PASS |
| --- | --- | --- | ---: | ---: | --- | --- |
| 110 cm polyethylene + 5 cm lead | [1.85, 3.44] pass | [1.02, 1.1] pass | 1.6e+03 | 115 | pass | yes |
| 105 cm polyethylene + 5 cm lead | [2.97, 6.63] pass | [1.17, 1.28] pass | 1.55e+03 | 110 | pass | yes |
| 100 cm polyethylene + 5 cm lead | [6.57, 10] inconclusive | [1.47, 1.56] pass | 1.51e+03 | 105 | pass | no |

The box is feasible. The best design found by anyone in this campaign was
found by the case author reading the sweep, not by a search arm.

## Wall clock

One transport in the learning arm spans 05:45 to 11:00 UTC in the log. The
system journal shows a suspend at 01:48 local time and a resume near 07:00;
the campaign paused with the machine. Every other transport at 500 000
particles took four to six minutes. Future campaigns run under a suspend
inhibitor.

## What changes for revision 3

The designer, not the contract:

1. Patience counted in transports, not rounds, and at least three finalists
   per round, so a stall costs budget rather than ending the arm.
2. Ordering-aware features: the moderator thickness ahead of each heavy
   layer, and the heavy thickness behind the moderator, which is what the
   photon requirement responds to.
3. The surrogate seeded from prior campaigns' logs for the same requirement
   ids, so revision 1's 25 transports and revision 2's are training data
   rather than discarded. That is the constellation being read for the
   first time.
4. The control sweep declared on a grid that can express thin lead.

## Amendments

- **A4, 2026-09-03, after the revision 2 campaign.** A fourth outcome is
  added for future campaigns: "search failure", no all-PASS candidate found
  while a feasible design is established by an evaluated candidate outside
  the arms. Revision 2 is reported under that name with F1's letter, as
  above. The campaign driver runs under a suspend inhibitor.

## Record

Driver status lines: `2026-09-03T02:23:31Z` start, `2026-09-03T11:48:01Z` end, all arms `DONE`.

| file | sha256 | lines |
| --- | --- | ---: |
| `campaign-rev2/baselines/campaign-log.jsonl` | `102d72f196887b20256ee89e461122b14b91c47af71fd55b105a07cdf6a0c950` | 4 |
| `campaign-rev2/sweep/campaign-log.jsonl` | `f90c5fabb19293dc5faa1111683a0a2b9c4ade4ad7c6f9fa49456cc9e5339187` | 32 |
| `campaign-rev2/recovery/campaign-log.jsonl` | `09bc3da33e60deedddb7b256fcf6ce3af42aa89a572a0f609fbf94f1025278a6` | 30 |
| `campaign-rev2/learning/campaign-log.jsonl` | `2484ee29bc0d447b5f83e73ad1d9a8466be1b7dc79cc80f16022ac098076b662` | 127 |
| `campaign-rev2/random/campaign-log.jsonl` | `e3a8604d5beb2df895c4953bea953194c83b4426003f5201f3cc976d5a6713ad` | 253 |
| `campaign-rev2/campaign-summary.md` | `d9249c366211a8d59419622cdc90da02438be07e70d5ae8515660b9be88c3815` | 35 |
| `campaign-rev2/campaign-summary.json` | `ab60b5636b9bc0369195d3cb78ae92ac4dd7fc5399717db9d5e0195e581b0b1a` | 253 |
| `campaign-rev2/STATUS` | `93eb29ecad9f9b407a11a127e5ca6416748ef068fffa8f778d83548ec45c363a` | 12 |
| `campaign-rev2/posthoc/campaign-log.jsonl` | `d73960baca2c6951aabbc197dcb48ea6b626477476ea8c20ad39ebfb7dca1561` | 3 |
