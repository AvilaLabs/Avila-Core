# CASE-010 — matmul tensor rank: exact GF(2) Brent-equation verification

Research specimen: an exact bilinear decomposition claim checked by replaying
the cubic Brent parity equations over GF(2), not by trusting a status flag.

## Question

Is the submitted term list an exact bilinear decomposition of the declared
matrix-multiplication tensor format over GF(2), and does its rank meet the
declared bound?

## Method

- `contract.json` binds one step, `verify`, to capability `matmul.brent-verify@1`
  via adapter `avila-labs.matmul/brent-verify@1`.
- The verify-script input is `capability/brent_verify.py` (digest-pinned in the
  package): it reads the candidate term list `candidates/npz-333.json` —
  23 terms for `<3,3,3>` in `ct` convention as GF(2) bitmasks — and checks all
  `n·m·p²` cubic Brent parity equations exactly.
- The run is executed by a pinned `python3` capability
  (`org.python/cpython@3.14.4`), which the operator supplies at run time.
- `requirement-set.json` names what any rank claim must address:
  `brent-validity`, `rank-bound`, `adds-bound`.

## Recorded result (receipt `receipts/verify.json`)

| requirement | verdict | evidence |
|---|---|---|
| MM-R1-fails | pass | `fails = 0` — every Brent equation satisfied exactly |
| MM-R1-valid | pass | `valid = pass` — categorical report field agrees |
| MM-R2-rank-bound | pass | `rank = 23 ≤ 23` — at the declared bound |
| MM-R3-adds | **fail** | `adds = 68 > 57` — the greedy pair-CSE addition count exceeds the declared record ask |

The candidate is a verified rank-23 decomposition of the 3x3x3 tensor over
GF(2). The additions bound is genuinely unmet — the failing verdict is the
answer, not a packaging defect, and the requirements were not weakened to
obtain it.

## Reproducing

```sh
avila-core run examples/cases/case-010-matmul-rank \
    --source-root matmul=examples/cases/case-010-matmul-rank/capability \
    --source-root case=examples/cases/case-010-matmul-rank \
    --capability python3=/usr/bin/python3
```

The committed receipt makes the verify step a reuse — the run re-verifies
digest bindings and replays claims rather than re-executing. Pass
`--no-reuse` to execute `brent_verify.py` afresh; the emitted report
reproduces `expected/npz-333-report.json` byte-for-byte.

## Honest limits

- The search provenance of `npz-333` (who produced the term list, under what
  method) is not recorded inside the package — `input:candidate` binds the
  bytes, not the history. The `matmul-rank` engineering-language library
  (`examples/language/`) models this boundary explicitly:
  `provenance_disjoint` refuses a verification whose equation system shares
  the search's own provenance edge.
- GF(2) only; no minimality claim — the package states this in
  `limitations`.
