# Shielding configuration search

200 candidates screened; 17 passed the screen, mass, and thickness requirements; 3 finalists ran transport.

| candidate | layers | screen uSv/h | mass kg | transport uSv/h | transport verdict |
| --- | --- | ---: | ---: | ---: | --- |
| c-0066 | 100 cm polyethylene | 3.127 | 940 | [9.411, 12.04] | inconclusive (bounded.le.crossing) |
| c-0115 | 40 cm borated_polyethylene + 5 cm borated_polyethylene + 55 cm polyethylene | 3.127 | 967 | [9.044, 11.23] | inconclusive (bounded.le.crossing) |
| c-0163 | 60 cm polyethylene + 15 cm polyethylene + 25 cm water | 3.725 | 955 | [14.11, 17.27] | fail (bounded.le.exceeds) |

Transport verdicts for the 3 finalists: 1 fail, 2 inconclusive.
2 feasible candidate(s) skipped as duplicates of a design already sent to transport.
An `inconclusive` verdict means the statistical interval straddles the limit; more particles narrow it, and a candidate whose nominal sits above the limit is unlikely to pass.

Values are shown to four significant digits; the campaign log keeps every exact value and identity.

The screen's PASS is nominal and establishes nothing; only the transport verdict is bounded, and its interval is statistical only. Nothing here is qualified for any decision.

This frozen search predates the staged-agent-review slice, so its historical log contains no review records. Under the now-bound instruction to request changes unless every Core requirement is `PASS`, all three finalists would be returned (two are `INCONCLUSIVE`, one is `FAIL`) and none would enter the accountable-person queue. Current runs of `shield_search.py` record that routing step explicitly; an agent recommendation still is not approval.
