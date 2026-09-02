# CASE-000 — ACTINV to Aftermatter

CASE-000 is Avila Core's first composed internal case. It freezes the existing
synthetic Aftermatter R0 chain:

```text
ACTINV 1.0.1 inventory
        ↓
Aftermatter R0 activated-metal classification and route screen
        ↓
Avila Core v0.2-draft admission and requirement verdicts
```

This is an integration specimen, not a pilot and not evidence that the
discovery candidate has been selected. No external participant, customer,
qualified reviewer, real component, facility decision, or operational use is
represented.

## Frozen question

At the synthetic 50-year checkpoint, are both recorded Class A mixture
fractions strictly below 1 under the bounded Aftermatter R0 rule
implementation, with use withheld until qualified independent review?

The frozen Aftermatter result reports:

| Boundary | Nominal fraction | Recorded numeric-error interval | Technical comparison |
| --- | ---: | ---: | --- |
| Table 1 Class A | `0.817559455198327183456183455` | `[0.8175594551983271834561771734, 0.8175594551983271834561897366]` | upper `< 1` |
| Table 2 Class A | `0.4447498809577018478459076928` | `[0.444749880957701847845906437, 0.4447498809577018478459089486]` | upper `< 1` |

Those intervals contain only Aftermatter's recorded decimal-floor and rounding
bound. They do not contain nuclear-data, activation-model, regulatory-model,
or scenario uncertainty.

All three modeled facility/storage routes remain `unresolved` at 50 years.
The current Core language cannot evaluate categorical route states, so the
route result is admitted as an unquantified artifact and is never translated
into an invented numeric score.

## Expected Core result

Every declared input and output claim is structurally admitted under the
current type-level rules. Both source intervals have upper bounds below their
frozen limits. Nevertheless, both requirement verdicts are:

```text
NOT_EVALUATED — not_evaluated.review_pending
```

That is intentional. A qualified independent review is required by
`contract.json`, no decision is asserted in `claims.json`, and the current
campaign evaluator withholds `PASS`.

## Files

- `contract.json` — the bounded question, ACTINV → Aftermatter dataflow, two
  numeric requirements, and pending review obligation.
- `registry.json` — the research-only roles and capability types used by this
  case.
- `claims.json` — hash attestations and the two bounded numeric claims.
- `campaign-report.json` — the deterministic expected Core evaluation.
- `provenance.json` — upstream repositories, commits/releases, artifact
  hashes, observed source result, and explicit boundary.
- `reviewer-eligibility-policy.md` — the hash-bound but currently unfulfilled
  internal review policy.

The external source artifacts are not vendored here. Their recorded identities
come from Aftermatter commit
`70a1c341d478bc37bf1ed0206dad4ee507cf743d`, whose R0 inventory records
ACTINV 1.0.1 and its data/result identities. Core v0.2-draft does not yet read
or independently re-hash those bytes; this limitation is material.

## Reproduce the Core layer

From the repository root:

```bash
cargo run -p avila-core-cli -- compile \
  --contract examples/cases/case-000-actinv-aftermatter/contract.json \
  --registry examples/cases/case-000-actinv-aftermatter/registry.json

cargo run -p avila-core-cli -- evaluate \
  --contract examples/cases/case-000-actinv-aftermatter/contract.json \
  --registry examples/cases/case-000-actinv-aftermatter/registry.json \
  --claims examples/cases/case-000-actinv-aftermatter/claims.json

cargo test -p avila-core-compiler --test case_000
```

The integration test requires semantic equality with the committed campaign
report and separately asserts that every evidence record is admitted while
both verdicts remain review-blocked.
