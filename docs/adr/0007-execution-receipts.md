# ADR-0007: Execution receipts and case-specific adapters

- Status: accepted for the internal case runner; the receipt schema is a
  `v0.1-draft` record
- Date: 2026-09-02
- Refines: ADR-0002, ADR-0005, ADR-0006 (SC-5, SC-11 A1/A2/A4/A5)

## Context

Until CASE-000E, Core evaluated the ACTINV → Aftermatter chain by replaying
claims that a person had copied out of Aftermatter's frozen result. Every
hash was correct and every admission was honest, but nothing in the
repository had run the tool: the output claims were authored, not produced.
A verdict over authored claims is only as good as the author.

The Stage 0 rules forbid a horizontal runner, registry, or orchestration
layer before a pilot exists. The smallest step that removes hand-authored
output claims is therefore a case-specific adapter: Core stages the exact
bytes it already verified, runs one exact executable it can identify, and
extracts claims from the bytes that come back.

## Decision

1. **An execution receipt is a first-class evidence record**
   (`avila.core/execution-receipt/v0.1-draft`). It binds the case and
   compiled-snapshot identity, the step and capability type, the adapter, the
   exact capability (package name and executable digest, with source
   coordinates as annotations), the compiled parameters, every staged input
   by slot, evidence identifier, workspace path, media type, digest and
   length, the portable invocation (program name, relative arguments,
   working directory `.`, the environment passed, and the timeout), the
   process outcome (timestamps, duration, exit status, signal, timeout), the
   captured stdout and stderr by digest, every declared output by digest and
   length or its absence, the runner identity, and explicit limitations.
   The `invocation_sha256` identifies what was asked of the program before it
   ran and excludes results, so a rerun of the same request has the same
   invocation identity whatever it produces. Timestamps are observations and
   never enter an identity.

2. **Receipts are verified from bytes, not from memory.** The runner writes
   the receipt, then re-reads it and re-hashes every input, log, and output
   file it names, recomputes the invocation identity, and checks the process
   outcome and every identity the package and compiled snapshot fixed. A
   verified receipt establishes process provenance: a named executable ran
   over named bytes and produced named bytes. It establishes nothing about
   scientific correctness, qualification, review, or regulatory suitability,
   and it says so.

3. **The case package binds what may run.** A case package declares
   `capabilities` (exact implementations by executable digest) and
   `executions` (which compiled step runs through which adapter with which
   capability, where each bound input slot is staged, and which evidence
   identifier each produced output slot's claim receives). Every execution
   output claim must bind to an artifact identity the package already
   declares, so a fresh output that differs from the bound identity fails
   identity binding instead of silently replacing it. A committed
   `execution_receipt` document, named by step, lets a fresh run be compared
   with the run that froze the expectations.

4. **Adapters are case-specific and named.** The runner knows only the
   adapters a committed case declares, by identifier. The first is
   `avila-labs.aftermatter/evaluate@1`, which maps the compiled
   `aftermatter.activated-metal-disposition@1` slots onto Aftermatter's
   `evaluate` command line, collects `outputs/route-result.json`, and
   extracts the two Class A mixture fractions at the compiled checkpoint as
   intervals `[fraction − error_bound, fraction + error_bound]` with exact
   decimal arithmetic, plus the whole document as an unquantified artifact.
   An adapter maps slots to arguments and outputs to claims; it does not
   interpret results, and the interval it extracts is the producing tool's
   own recorded numerical bound, never an uncertainty Core invented.

5. **Execution is confined and fails closed.** Each step runs in a fresh
   workspace directory with a cleared environment, working directory set to
   the workspace, stdin closed, streams captured to files, only declared
   outputs collected, and a timeout. The runner refuses to execute over
   bytes it has not verified, over an executable whose digest differs from
   the bound identity, through an adapter whose capability type differs from
   the compiled step's, or for a step that carries a review obligation. A
   refused or failed execution stops the workflow before any claim is
   generated.

6. **Claims are generated, not authored.** Input attestations come from the
   package's artifact identities; claims for executed steps come from the
   adapters' extraction over fresh bytes and carry the producer identity;
   claims for steps that were not executed are carried from the committed
   document as recorded attestations and reported as such. The generated
   document is compared canonically with the committed `claims.json`, is
   bound to the package identities, is evaluated, and its campaign report is
   compared with the committed expectation. Any drift is a rejected run.

7. **Omission is visible; deviation is fatal.** As with artifact roots
   (S-018), an executable that is not supplied leaves the step `not_run` and
   the committed claims stand as recorded attestations, visibly. A supplied
   executable must match, run to completion, and reproduce claims that bind,
   or the run is rejected.

## Boundary

This is the first executable slice of SC-11 A1, A2, A4, and A5 in a
case-specific form. It does not implement signatures or trust roots (A2's
runner-signature half), qualification or applicability facts (A7), policy
snapshots (A8), review fulfilment (A9), invalidation (A10), sandboxing,
resource accounting, package selection, or a generic adapter protocol. The
campaign evaluator's admission semantics are unchanged; receipt checks live
in the case-runner layer. A verified receipt never turns an unqualified
method into a qualified one.

## Consequences

- CASE-000 executes both computational steps and generates every output
  claim; its `claims.json`, `campaign-report.json`, and the receipts under
  `receipts/` are emitted by `avila-core run` and committed as expectations,
  not written by hand.
- Adding an executed step means adding a named adapter and an execution
  declaration, not a runner feature. The second adapter,
  `avila-labs.aftermatter/build-r0-case@1`, runs Aftermatter's frozen R0
  builder under a digest-pinned Python interpreter with the builder script and
  the ACTINV executables as hash-bound inputs; the interpreter is pinned by
  digest like any other executable, which is exact but machine-specific until
  reproducible builds or a package identity exist.
- The receipt schema, the case-package schema, this ADR, and the adversarial
  execution tests move together.
