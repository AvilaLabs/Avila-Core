# CASE-003 results, contract revision 2, campaign 1

**Campaign:** 2026-09-03, contract revision 2, package pinned at
`sha256:9634fcc1a39e9116e6c875c522ec8fd704dc721e5552251ea78737284158216f`
for every run of every arm (S-030). Arms per `PROTOCOL.md`: the twelve-point
control sweep and the language-model designer with the sweep as its prior,
driven through `examples/agents/shield_llm_tools.py`; the revision-1 sweep,
whose two refusals and unreachable screen guide led to amendment A1, is kept
under `campaign-1/archive/sweep-rev1/`. Every number below is copied from
Core's logs; `campaign-1/campaign-summary.md` is derived by
`summarize_campaign.py`.

## Outcome: supported on the creation criteria; the refusal criterion was not exercised

The bar, fixed by the control sweep, is 10 mm of graphite at 18.0 kg/m²:
ten of the twelve grid points pass, the hotspot being nearly flat from 10 to
30 mm for every conductive material, and the two copper plates above 10 mm
fail only on mass.

The language-model designer, given that record, screened 13 candidates and
fully evaluated 8, five of them all-`PASS`. It read the flatness of the
hotspot with thickness, proposed thinner plates than the sweep had tried,
found the cliff where lateral spreading fails between 1 and 2 mm of
graphite (1 mm: 344 K, fails by 4.4 K; 2 mm: 335 K, passes by 4.7 K), tested
whether a higher-conductivity, denser material could beat graphite at 1 mm
(aluminium missed by 0.19 K), screened and rejected a graphite-epoxy hybrid
on the screen alone, and stopped by its own judgment with 12 evaluations
unspent, giving its reasons. Lightest all-`PASS`: **2 mm graphite at
3.6 kg/m²**, five times lighter than the bar, at its sixth evaluation
against the sweep's twelve.

| design | finite-element hotspot K | mass kg/m² | all PASS |
| --- | --- | ---: | --- |
| 1 mm graphite | [344.4, 344.4] fail | 1.8 | no |
| 3 mm graphite | [331.4, 331.4] pass | 5.4 | yes |
| 5 mm graphite | [327.8, 327.8] pass | 9 | yes |
| 7 mm graphite | [326.3, 326.3] pass | 12.6 | yes |
| 9 mm graphite | [325.5, 325.5] pass | 16.2 | yes |
| 2 mm graphite | [335.3, 335.3] pass | 3.6 | yes |
| 1 mm aluminium | [340.2, 340.2] fail | 2.7 | no |
| 1 mm aluminium_nitride | [342.5, 342.5] fail | 3.26 | no |

Both creation criteria of the pre-declared "supported" outcome hold. The
refusal criterion was not exercised: no candidate in either arm lay outside
the envelope or the coverage, and the pin was never tested by this arm.
The two `rejected` runs of the revision-1 sweep were the kernel refusing a
forty-digit claim value from the screen script, a script fault, not a
shortcut.

## What this establishes

- **The mechanism transfers.** A different physics, a different tool, a
  deterministic enclosure claim instead of a statistical interval, and the
  same contract language, runner, tools, designer prompt, and record. Nothing
  in Core changed for this domain; the tools lost a shielding assumption or
  two.
- **The first validated envelope.** Every `PASS` here is under a method whose
  qualification record binds a benchmark reproduction, which is more than the
  shielding case can say.
- **The designer reasons from the record.** The winning move came from
  noticing what the sweep's numbers implied about a region the sweep had not
  visited, then bracketing the boundary in two evaluations.

## Limits of the claim

The problem is easy: with 500 W/m²/K cooling the hotspot barely depends on
thickness above a few millimetres, so mass minimisation collapses to "the
thinnest plate that still spreads", and the answer would not surprise a
thermal engineer. The screen is pessimistic by about 80 K and so can never
decide anything. Whole-millimetre candidates hide whatever lies between 1
and 2 mm. Interface and transient entries of the library set are omitted
with reasons. And the model's validation covers one benchmark geometry, not
the strip source or the materials.

## Record

| file | sha256 | lines |
| --- | --- | ---: |
| `campaign-1/sweep/campaign-log.jsonl` | `46690fb04fdea02ed986cb5c694bcc57cf20b47bffcc92d8a178fc451eb7e434` | 24 |
| `campaign-1/llm/campaign-log.jsonl` | `dc7fb562f76577b748a5ea7837d134a114a0bbc787795fb5e6d0a5e1d1737afb` | 23 |
| `campaign-1/llm/designer-notes.jsonl` | `37e3d27fcb21482bd4cba212d51c699ccce3e5b318e4bce36edbcdcbab4d84f8` | 21 |
| `campaign-1/archive/sweep-rev1/campaign-log.jsonl` | `afcc2fa65f8db0af5efe8b00e606cd8e4a76bd4033235129e123e14e3c9ceb8b` | 20 |
| `campaign-1/campaign-summary.md` | `3533070f4e1afd5281c7ed8edbb299586517629da4b8c495228d5caab2c6e5b7` | 27 |
| `campaign-1/campaign-summary.json` | `918e9f65dcc3b88210bbf8e0fdb90fdaf611fc3d851b3907665886875581e282` | 56 |
