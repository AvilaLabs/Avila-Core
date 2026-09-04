# CASE-000 — ACTINV to Aftermatter

CASE-000 is Avila Core's first composed internal case. It executes the
synthetic Aftermatter R0 chain end to end:

```text
ACTINV 1.0.1 inventory build through Aftermatter's frozen R0 builder (executed by Core)
        ↓
Aftermatter R0 activated-metal classification and route screen (executed by Core)
        ↓
Avila Core v0.2-draft claim generation, admission, and requirement verdicts
```

This is an integration specimen, not a pilot and not evidence that the
discovery candidate has been selected. No external participant, customer, real
component, facility decision, or operational use is represented.

## Frozen question

At the synthetic 50-year checkpoint, are both recorded Class A mixture
fractions strictly below 1 under the bounded Aftermatter R0 rule
implementation, and does the Clive route reach Aftermatter's `feasible`
state?

The Aftermatter result reports:

| Boundary | Nominal fraction | Recorded numeric-error interval | Technical comparison |
| --- | ---: | ---: | --- |
| Table 1 Class A | `0.817559455198327183456183455` | `[0.8175594551983271834561771734, 0.8175594551983271834561897366]` | upper `< 1` |
| Table 2 Class A | `0.4447498809577018478459076928` | `[0.444749880957701847845906437, 0.4447498809577018478459089486]` | upper `< 1` |

Those intervals contain only Aftermatter's recorded decimal-floor and rounding
bound. They do not contain nuclear-data, activation-model, regulatory-model,
or scenario uncertainty. Since CASE-000E they are no longer typed in by hand:
`avila-core run` extracts them from the route result Aftermatter writes
during the run, as `[fraction − error_bound, fraction + error_bound]` in exact
decimal arithmetic.

All three modeled facility/storage routes remain `unresolved` at 50 years.
Core now extracts the contract-selected Clive state from the same route-result
artifact as closed-vocabulary categorical evidence. It compares
`unresolved` directly with the required `feasible` value; it does not
translate route state into an invented numeric score. The artifact identifies
the missing Clive facts as profile approval, dose-rate characterization,
package selection, and surface-contamination characterization.

## What is executed and what is attested

| Step | Capability type | Status in CASE-000 |
| --- | --- | --- |
| `activation` | `aftermatter.r0-inventory-build@1` | Executed. Core stages Aftermatter's frozen R0 builder, the ACTINV 1.0.1 `actinv` and `dump` release builds, the frozen FNS spectrum, and the five data-release files, all verified by digest, and runs the builder under a digest-pinned Python interpreter. The builder generates the ACTINV problem, validates and runs ACTINV, and writes the normalized inventory and decay metadata. |
| `classification` | `aftermatter.activated-metal-disposition@1` | Executed. Core stages the fresh inventory and decay metadata with the five verified Aftermatter inputs, runs the bound Aftermatter executable with a cleared environment, verifies the receipt, and extracts two numeric interval claims plus the selected route-state claim. |

Two capabilities are bound in `package.json` by executable digest and must
match before anything runs: `python3` (CPython 3.14.4, the interpreter that
runs the builder) and `aftermatter-cli` 0.1.0 built from Aftermatter commit
`70a1c341d478bc37bf1ed0206dad4ee507cf743d`. The builder script and the ACTINV
executables are hash-bound inputs of the activation step, so the receipt
identifies everything that ran. Every fresh output reproduces the frozen R0
artifact byte for byte: the problem
(`sha256:010d95e2…`), the inventory (`sha256:3942d6f3…`), the decay metadata
(`sha256:d80f972f…`), and the route result (`sha256:3eeb782a…`). A different
executable, different input bytes, a nonzero exit, a missing output, or an
output that differs from the bound identity stops the workflow before any
verdict.

## Expected Core result

Every declared input and output claim is structurally admitted under the
current type-level rules. Both source intervals have upper bounds below their
frozen limits, while the Clive route is `unresolved` rather than `feasible`:

```text
PASS — CASE-000-R1 — bounded.lt.within
PASS — CASE-000-R2 — bounded.lt.within
FAIL — CASE-000-R3 — categorical.equals.mismatch
       observed: unresolved
       accepted: feasible
```

Those technical results need no human, professional, or agent review. The FAIL
is useful feedback, not an execution error: the numerical classification
screen clears while the chosen route is not yet resolved. None of the verdicts
qualifies ACTINV, Aftermatter, the nuclear data, or the case for regulatory or
operational use, and Aftermatter's `feasible` state would still not mean
facility acceptance.

## Files

- `contract.json` — the bounded question, ACTINV → Aftermatter dataflow, two
  numeric requirements, and one categorical route-state requirement.
- `registry.json` — the research-only roles and capability types used by this
  case.
- `package.json` — raw-byte identities for the case documents, bindings from
  all 20 evidence records to 18 externally resolvable artifacts, the two bound
  executable identities, and the `activation` and `classification` execution
  declarations (adapter, staging layout, and output claim identifiers).
- `claims.json` — generated by `avila-core run`, not authored: 14 input
  attestations from package identities and six claims extracted from the
  executed outputs, each with its producer identity.
- `receipts/activation.json` and `receipts/classification.json` — the
  execution receipts of the run that froze these expectations: capability,
  staged inputs, portable invocation, process outcome, logs, and output
  digests.
- `campaign-report.json` — the deterministic expected Core evaluation.
- `provenance.json` — upstream repositories, commits/releases, artifact
  hashes, observed source result, execution record, and explicit boundary.

The external source artifacts are not vendored here. Their recorded identities
come from Aftermatter commit `70a1c341d478bc37bf1ed0206dad4ee507cf743d`,
whose R0 inventory records ACTINV 1.0.1 and its data/result identities. Eleven
artifacts live in the Aftermatter checkout (source root `aftermatter`), the
five ACTINV data-release files in its ignored `.data/actinv/v1.0.0` directory
(source root `actinv-data`), and the two ACTINV 1.0.1 release builds in the
ACTINV checkout's `target/release` (source root `actinv-release`). With all
three roots supplied, all 18 artifacts and all 20 evidence records are
re-hashed.

## Reproduce the Core layer

From the repository root, with an Aftermatter checkout at the pinned commit
built in release mode, the ACTINV 1.0.1 release builds, and the pinned Python
interpreter:

```bash
cargo run -p avila-core-cli -- run \
  examples/cases/case-000-actinv-aftermatter \
  --source-root aftermatter=../project-aftermatter \
  --source-root actinv-data=../project-aftermatter/.data/actinv/v1.0.0 \
  --source-root actinv-release=../../actinv/target/release \
  --capability python3=/usr/bin/python3 \
  --capability aftermatter-cli=../project-aftermatter/target/release/aftermatter
```

The run reports six stages: package integrity, compilation, execution (with
the receipt identity and whether the fresh output reproduces the bound
artifact), claim generation (and whether the generated document matches the
committed `claims.json`), evaluation, and replay against the committed
campaign report and receipt. It writes the staged inputs, outputs, logs,
receipt, generated `claims.json`, `campaign-report.json`, and `run-report.json`
under `workspaces/CASE-000/<run>/` (gitignored) unless `--workspace` names a
fresh directory. Pass `--json` for the complete machine-readable report.

By default the runner first plans each step's invocation and compares it
with the committed receipt. When the identity matches, the receipt completed,
and every recorded output still verifies at its bound identity, the step is
`REUSED` and nothing runs, so the command above with the three roots and no
`--capability` reports both steps reused and evaluates exactly as the frozen
expectations say. Pass `--no-reuse` to execute both tools afresh, or `--plan`
to see what would rerun and why (by SC-12 change class: input bytes, input
binding, parameters, capability, invocation, receipt state) without running.

Omit a `--capability` for a step that cannot be reused and it is reported
`NOT RUN`: its committed claims are evaluated as recorded attestations,
visibly, and a later step that consumes its outputs stages the bound artifact
bytes instead. Omit a source root and its artifacts are `not_checked`; the
runner then neither executes over nor reuses those bytes. Supply a root or an
executable that does not match and the run fails closed.

To see selective rerun on this case, change one thing in a scratch copy of
the Aftermatter root, rebind that artifact's digest in `package.json`, and
run with `--plan`: a rulepack change reaches only `classification`, while a
spectrum change reaches `activation` and, because its outputs are then
compared by content, `classification` only if the inventory actually moved.

The standalone commands still work over the committed documents:

```bash
cargo run -p avila-core-cli -- compile \
  --contract examples/cases/case-000-actinv-aftermatter/contract.json \
  --registry examples/cases/case-000-actinv-aftermatter/registry.json

cargo run -p avila-core-cli -- evaluate \
  --contract examples/cases/case-000-actinv-aftermatter/contract.json \
  --registry examples/cases/case-000-actinv-aftermatter/registry.json \
  --claims examples/cases/case-000-actinv-aftermatter/claims.json

cargo test -p avila-core-compiler --test case_000
cargo test -p avila-core-cli
```

The compiler integration test requires semantic equality with the committed
campaign report and separately asserts that every evidence record is admitted,
the two numeric verdicts remain `PASS`, and the route-state verdict remains the
expected categorical `FAIL`. The CLI tests run the workflow
without external roots, run adversarial executions over a stub capability, and
execute the real chain when `AVILA_CORE_CASE_000_AFTERMATTER`,
`AVILA_CORE_CASE_000_PYTHON3`, `AVILA_CORE_CASE_000_AFTERMATTER_ROOT`,
`AVILA_CORE_CASE_000_ACTINV_DATA`, and `AVILA_CORE_CASE_000_ACTINV_RELEASE`
name the two executables and the three roots.

## Re-freezing the expectations

If a bound executable, an input, or an adapter legitimately changes, run the
case with a `--workspace`, review the differences the run reports, then copy
the workspace's `claims.json`, `campaign-report.json`, and each
`<step>/receipt.json` into this directory (the receipts under `receipts/`),
update the document digests in `package.json`, and update the bound artifact
identities if an output changed. A changed expectation is a reviewed change,
never a mechanical one.
