# EXP-003 harness — local CASE-003 launch-to-report path

A reproducible release-build measurement of the **non-agent** portion of the
EXP-003 path described in [BACKLOG.md](../BACKLOG.md): integrity
verification and reuse checking, an evaluated run reusing committed
receipts, and fresh execution. It answers whether *the Core portion* of a
prepared campaign runs without manual work — not whether the agent
orchestration does.

## Repeat

```sh
experiments/exp-003/measure.sh [REPS]
```

From the repository root. It builds `target/release/avila-core`, records the
manifest (machine, build, workload identities, cache policy, repetitions,
resolved executables), times each phase `REPS` times into
`results/run-<timestamp>/result.json`, and keeps per-run stdout/stderr and a
scratch `campaign.jsonl` alongside. Nothing writes into the case directory.

## Phases and what this machine measured

| Phase | Command | What it covers |
| --- | --- | --- |
| verify | `run --plan` | integrity re-hash, compile, coverage, current reuse plan; no steps launched |
| reuse | `run` | the same plus claim generation, binding, campaign evaluation, receipt reuse |
| fresh | `run --no-reuse` | actual solver execution; **requires the pinned capability executables** |
| agent orchestration | — | EXP-003's agent/unattended portion; not part of this harness |
| manual setup | — | documented once: roots, keys, candidate selection; not timed |

## Availability on the recorded machine (2026-09-16)

- `python3` resolves to `/usr/bin/python3` (Python 3.14.4) at
  `sha256:52e0a1…30cb41`, which **does not match** the pinned
  `sha256:b8d828…99700`; `import skfem` fails. Fresh execution (`screen` and
  `fe` alike) is **UNAVAILABLE** here and recorded as such — nothing is
  substituted and no run is faked.
- Verify and reuse phases measured 22–67 ms wall per invocation on an
  Intel i3-N305 (3 repetitions each; see `results/run-*/result.json`).
- Reused steps pass the contract's `require_signatures` policy via the
  committed signed receipts; the run needs `--trust-root` and `--runner-key`
  (both under `examples/keys/`) to verify and sign its log lines.

## Limits

- Three repetitions at interactive priority is a smoke measurement, not a
  distribution. EXP-003's full question — can a *prepared campaign* run
  unattended end to end — needs the agent harness, model calls, and the
  pinned capability executables, all explicitly out of scope here.
- Wall time includes process startup; sub-100 ms readings are
  resolution-limited, not a claim about solver cost.
