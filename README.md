# Avila Core

> Internal codename: Project North Star

Avila Core is an early research project exploring a neutral semantic and trust
layer for computational engineering. A person or connected agent states what
must be established as an **evidence contract**; capabilities contribute
evidence; Core determines what follows under explicit rules and returns a
portable, independently verifiable package.

The long-term aim is not to sell another solver, workflow canvas, or block of
compute. It is to let agents explore difficult engineering spaces against a
strict evidence compiler until the stated gates pass, without making every
campaign depend on professional review.

## Current status

This repository is a **pre-alpha scaffold**. It currently provides:

- authoritative document types for `v0.2-draft` evidence contracts, which
  carry their bounded question, and immutable registry snapshots;
- the first semantic-kernel slice for exact bounded rationals, canonical
  decimals, authoritative JSON, quantity kinds, exact unit scaling, and
  three-valued applicability predicates;
- exact four-state verdict derivation over admitted claim specimens;
- a first `v0.2-draft` semantic compiler that resolves typed dataflow,
  enforces typed parameter domains, diagnoses graph and binding failures, and
  checks type-level determinism, seeds, and material execution factors; when
  configured, it also compiles an optional connected-agent presentation gate
  bound to an exact evidence dossier, policy identity, practical instructions,
  and closed routing dispositions, while governed
  requirement purposes prevent outputs from serving uses they explicitly
  exclude; then emits an immutable snapshot with canonical configuration and
  exact requirement limits;
- a diagnostic catalog explaining every finding code, with typed repair
  candidates and accountable owners on every finding, and a mutation harness
  that proves every finding is anchored and every mechanical repair works;
- the first executable campaign slice: an evidence-claims document is admitted
  against the compiled snapshot under the type-level admission conditions, and
  the kernel derives one `PASS`, `FAIL`, `INCONCLUSIVE`, or `NOT_EVALUATED`
  verdict per requirement, independent of any review or presentation state,
  with a content-identified report;
- the first composed internal case, `CASE-000`, which binds an existing
  synthetic ACTINV 1.0.1 → Aftermatter R0 chain, executes both computational
  steps (ACTINV through Aftermatter's frozen R0 builder, then Aftermatter over
  the fresh inventory), generates every output claim from the fresh results,
  admits 14 source attestations and 6 output claims, and returns technical
  `PASS` for both bounded requirements without a review stage;
- a case-package workflow that re-hashes package documents, re-hashes
  external artifact bytes only from explicitly supplied roots, executes the
  steps the package declares through named case-specific adapters over exact
  executables in a fresh workspace with a cleared environment, writes an
  execution receipt per step and verifies it from bytes, reuses a step whose
  committed receipt matches the planned invocation and whose outputs still
  verify, names every change that forces a rerun by SC-12 class, generates
  the claims document from package identities and fresh or reused outputs,
  binds those identities to the claims and any optional agent policy, compiles and
  evaluates the case, and replays its committed claims, receipts, and
  campaign report without collapsing an unchecked artifact or an unsupplied
  executable into a success state;
- the first generative-loop case, `CASE-001`, a shielding configuration
  search: a package may declare free inputs, `run --input` supplies a
  candidate that invalidates every step it reaches, an unqualified screen
  satisfies only the nominal-basis requirement while OpenMC transport
  satisfies the bounded one, every verdict carries its exact margin, and
  `--log` appends one line per run to a campaign log that a scripted
  designer in `examples/agents/` drives through the two-fidelity loop; its
  optional hash-bound practicality agent receives Core's exact post-campaign
  dossier and can only return work, present it to the user, or abstain; it
  never changes the technical verdict;
- the first coupled case, `CASE-002`, which adds coupled neutron-photon
  transport with per-layer spectra and ACTINV activation per layer to the
  search, evaluates six requirements with exact margins, and carries a
  pre-registered experimental protocol; its activation guide is a stated
  coverage omission at the library's required basis because Core refused the
  contract that counted it as coverage;
- the second domain, `CASE-003`, a strip-heated heat spreader: a
  one-dimensional resistance screen guides, two-dimensional finite-element
  conduction decides the hotspot as an `interval` under the `enclosure`
  basis, and the finite-element qualification record is the first to bind
  validation evidence, a NAFEMS T4 reproduction;
- an identity log and a manifest pin: every campaign log line names the
  manifest, compiled snapshot, and document digests it was evaluated under,
  and `run --expect-manifest` refuses a package whose manifest differs from
  the requester's pin, the response to an adversarial designer arm that
  obtained undeserved verdicts only by rewriting the package;
- qualification envelopes: a method owner's record binds an exact executable
  and adapter to a kernel applicability predicate over facts the adapter
  reads from verified inputs; the runner evaluates it before a step runs and
  attaches it to the step's claims, and a bounded requirement whose evidence
  lies outside the envelope is `NOT_EVALUATED` with the failed term named,
  even though the calculation ran;
- requirement-set coverage: a library states what any contract in its domain
  must address (`examples/libraries/`), the case declares what covers each
  entry and why the rest are omitted with an accepting owner, and `run`
  refuses to execute over an unstated omission or a weaker-than-required
  basis, so a search cannot optimize an incomplete question;
- a draft portable evidence model, case-package manifest, execution-receipt
  and optional presentation-routing records, and SHA-256 utility;
- JSON Schemas that the compiler embeds and enforces as its source layer, and
  a deliberately non-executable specimen contract and registry snapshot;
- a local CLI for canonicalization, compilation, campaign evaluation, the
  diagnostic catalog, and the case workflow; and
- an egui workbench that runs a composed case through the same runner and
  renders integrity, compilation, execution with reuse and change classes,
  claims, verdicts with their boundaries, optional presentation-gate readiness and
  instructions, and replay, plus the original specimen compiler view. It
  computes nothing itself.

The repository also contains proposed `v0.2` semantic rules and an initial
conformance-vector corpus. The Rust kernel executes all 91 current pure vectors:
12 canonical-value, 10 unit-scaling, 19 scope-predicate, 42 requirement-verdict,
and 8 aggregate-verdict cases. A separate compiler harness executes 68 current
type fixtures across five pinned registry snapshots, and a campaign harness
executes 12 claim-admission and verdict fixtures. The first R1–R10 static
compiler frontier and a type-level SC-10/SC-11 campaign slice are implemented,
but package-level rule halves, other normative fixture families, full
package-level admission, invalidation, and package semantics remain proposed
and incomplete.

It does **not** calculate a physical quantity, select or bind capability
packages beyond an executable digest, verify signatures, evaluate a
scientifically qualified requirement, certify a design, or produce
decision-grade evidence. The standalone `compile` and `evaluate` commands do
not read artifact bytes. The `run` command re-hashes bytes at explicitly
resolved roots and, where a case declares it, runs exact executables through
case-specific adapters; a matching hash establishes identity only, and a
verified receipt establishes process provenance only. In CASE-000 both the
ACTINV build and the Aftermatter classification are executed and every output
claim is extracted from the fresh results. Their two technical PASS verdicts
need no professional review; the case still claims no scientific qualification.

## Core objects

| Object | Meaning |
| --- | --- |
| Contract | The bounded question, requirements, assumptions, inputs, and evidence policy. |
| Capability | A versioned method or adapter with an explicit provider, execution boundary, qualification scope, and limitations. |
| Campaign | A planned or executed dependency graph resolving one contract. |
| Evidence | Immutable records connecting inputs, capabilities, executions, outputs, and claims; optional presentation routing stays separate. |
| Verdict | `PASS`, `FAIL`, `INCONCLUSIVE`, or `NOT_EVALUATED` for a specific requirement and boundary. |

A verdict means that the recorded state follows from admitted evidence under a
named semantic profile. It is not an unqualified claim that an external model is
correct or a design is certified.

## Try the scaffold

Requirements: Rust 1.95.0 and the native libraries required by `eframe`.

```bash
cargo test --workspace --all-targets

cargo run -p avila-core-cli -- semantic-profile

cargo run -p avila-core-cli -- canonicalize path/to/authoritative.json

cargo run -p avila-core-cli -- compile \
  --contract fixtures/semantic-core/types/types.R1.resolved.pass.contract.json \
  --registry fixtures/semantic-core/types/compiler.registry.v1.json

cargo run -p avila-core-cli -- explain CORE-R3102

cargo run -p avila-core-cli -- evaluate \
  --contract fixtures/semantic-core/types/types.R1.resolved.pass.contract.json \
  --registry fixtures/semantic-core/types/compiler.registry.v1.json \
  --claims fixtures/semantic-core/campaigns/campaign.le.within.pass.claims.json

cargo run -p avila-core-cli -- compile \
  --contract examples/contracts/shutdown-dose-specimen.json \
  --registry examples/registry/shutdown-dose-specimen.registry.json

cargo run -p avila-core-cli -- evaluate \
  --contract examples/cases/case-000-actinv-aftermatter/contract.json \
  --registry examples/cases/case-000-actinv-aftermatter/registry.json \
  --claims examples/cases/case-000-actinv-aftermatter/claims.json

cargo run -p avila-core-cli -- run \
  examples/cases/case-000-actinv-aftermatter \
  --source-root aftermatter=../project-aftermatter \
  --source-root actinv-data=../project-aftermatter/.data/actinv/v1.0.0 \
  --source-root actinv-release=../../actinv/target/release \
  --capability python3=/usr/bin/python3 \
  --capability aftermatter-cli=../project-aftermatter/target/release/aftermatter

cargo run -p avila-core-cli -- run \
  examples/cases/case-001-shield-search \
  --source-root case=examples/cases/case-001-shield-search \
  --source-root shielding=examples/capabilities/shielding \
  --source-root agents=examples/agents \
  --source-root nuclear-data=/path/to/endfb-vii.1-hdf5 \
  --capability python3=/usr/bin/python3 \
  --input candidate=my-candidate.json --log campaign-log.jsonl

cargo run -p avila-core-app -- \
  --case examples/cases/case-000-actinv-aftermatter \
  --source-root aftermatter=../project-aftermatter \
  --source-root actinv-data=../project-aftermatter/.data/actinv/v1.0.0 \
  --source-root actinv-release=../../actinv/target/release
```

The workbench opens on the case: roots and capabilities the package requests
are listed for you to point at local paths, `Plan` reports what would be
reused or rerun and why, and `Run` performs the workflow and renders every
stage. The `?` button (or F1) opens contextual help, bundled answers, and
spotlight walkthroughs for five use cases: verifying a frozen case, seeing
what a change would rerun, executing the tools afresh, reading a verdict, and
compiling the specimen. The header switches between dark and light mode.
`--auto-run` or `--auto-plan` starts immediately, `--tour NAME` starts a
walkthrough, `--light` starts in light mode, and `--screenshot PNG` saves the
rendered window and closes, which is how the view is reviewed without a hand.
Dependencies are optimized even in `cargo run`'s dev profile, so re-hashing
the bound data releases takes a fraction of a second; a release build is not
needed for the workbench to feel immediate.

Compiling the specimen returns `"status": "rejected"` with only `missing`
findings, one for each value the draft declares `not_defined`. That is the
expected and only honest state of the included specimen; the app renders the
same report.

The compile command prints a JSON report and exits 0 when the contract
compiled, possibly with notices; 1 when it was rejected; and 2 when the tool
could not run. Every finding carries a stable code, a class, an owner, a JSON
Pointer location, and typed repair candidates where a bounded repair exists;
`--text` renders the same findings with `document:line:column` locations and
the underlined source line, and `explain` prints the
[catalog entry](docs/architecture/DIAGNOSTICS.md) for a code, or the whole
catalog with `--all`. The evaluate command admits a claims
document against the compiled snapshot and prints one verdict per requirement;
see the [campaign evaluation boundary](docs/architecture/CAMPAIGN_EVALUATION.md).
The `run` command is the concise end-to-end view: integrity, compiled
workflow, execution with a verified receipt, generated claims, identity
binding, admissions, verdicts, optional exact presentation-gate requests, and replay
against the committed claims, receipt, and campaign report. Omit
`--source-root` to see every external
artifact reported as `not_checked`. With the roots supplied and nothing
changed since the committed receipts, both steps are `REUSED` and nothing
runs, executables or not; add `--no-reuse` to execute afresh, `--plan` to see
what would rerun and why without running, and `--json` for the complete
machine-readable run report. A supplied root or executable that does not
match fails closed. For a case that declares free inputs, `--input NAME=PATH`
supplies one: the steps it reaches rerun or are reported not run with their
committed claims withheld, `--env KEY=VALUE` values a key an adapter requires
only when that step actually runs, and `--log FILE` appends the run's
supplied inputs, step states, verdicts, and margins as one JSON line.

## Repository map

```text
crates/
  avila-core-kernel/      exact values, predicates, and verdict derivation
  avila-core-compiler/    draft static compiler, document types, and semantic IR
  avila-core-evidence/    evidence records, case-package integrity, and
                          execution receipts
  avila-core-runner/      the case workflow: staging, execution, receipts,
                          reuse, claim generation, and the case-specific
                          adapters
  avila-core-cli/         headless local interface
  avila-core-app/         thin egui workbench over the runner and compiler
docs/
  strategy/               north star, economics, and counter-positioning
  product/                product definition, evidence contracts, and UX
  architecture/           boundaries, capability protocol, evidence model, and
                          the diagnostic catalog
  discovery/              sanitized first-pilot discovery record and workflow map
  roadmap/                staged validation and 1.0 planning hypotheses
  adr/                    durable architectural decisions
schemas/                  machine-readable interchange drafts
examples/                 unqualified documents: composed CASE-000 to CASE-003,
                          the shielding capability scripts, the first library
                          requirement set, and the scripted designer that
                          drives the search
assets/branding/          provisional Avila Core mark
fixtures/semantic-core/   proposed semantic-profile coverage, vectors, and campaigns
```

Start with the [project charter](PROJECT_CHARTER.md), the
[generative loop](docs/strategy/GENERATIVE_LOOP.md) that states what Core is
for, and the [definitions](docs/DEFINITIONS.md) of the words this project
uses, then read the
[north-star strategy](docs/strategy/NORTH_STAR.md),
[product definition](docs/product/PRODUCT_DEFINITION.md), and
[architecture](docs/architecture/ARCHITECTURE.md). The live milestone boundary
is in the [Stage 0 status ledger](docs/roadmap/STAGE_0_STATUS.md), and the next
validation work is captured in the
[first-pilot discovery packet](docs/discovery/FIRST_PILOT_PACKET.md). The
proposed language rules are in [ADR-0006](docs/adr/0006-semantic-core.md);
execution receipts and case-specific adapters are decided in
[ADR-0007](docs/adr/0007-execution-receipts.md).
The technical-verdict/presentation boundary is fixed by
[ADR-0010](docs/adr/0010-technical-verdicts-and-optional-presentation-gates.md).

## Licensing and claims

No source-code license has been selected. Until Avila Labs adopts one, access to
this repository does not grant permission to copy, modify, or redistribute its
contents. The provisional logo is separately reserved. See
[DISCLAIMER.md](DISCLAIMER.md) for the project’s scientific and regulatory
limits.
