# Avila Core

> Internal codename: Project North Star

Avila Core is an early research project exploring a neutral semantic and trust
layer for computational engineering. An organization states what it needs to
establish as an **evidence contract**; qualified capabilities contribute
evidence; Core determines what follows under explicit rules and returns a
portable, independently reviewable package.

The long-term aim is not to sell another solver, workflow canvas, or block of
compute. It is to shorten the path from an important technical question to an
admissible answer while preserving the authority of domain professionals.

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
  checks type-level determinism, seeds, and material execution factors before
  compiling accountable review steps into pending obligations bound to an
  exact evidence dossier, external eligibility-policy identity, explicit
  independence rules, and governance-only dispositions, while governed
  requirement purposes prevent outputs from serving uses they explicitly
  exclude; then emits an immutable snapshot with canonical configuration and
  exact requirement limits;
- a diagnostic catalog explaining every finding code, with typed repair
  candidates and accountable owners on every finding, and a mutation harness
  that proves every finding is anchored and every mechanical repair works;
- the first executable campaign slice: an evidence-claims document is admitted
  against the compiled snapshot under the type-level admission conditions, and
  the kernel derives one `PASS`, `FAIL`, `INCONCLUSIVE`, or `NOT_EVALUATED`
  verdict per requirement, with review asymmetry and a content-identified
  report;
- a draft portable evidence model and SHA-256 utility;
- JSON Schemas that the compiler embeds and enforces as its source layer, and
  a deliberately non-executable specimen contract and registry snapshot;
- a local CLI for canonicalization, compilation, campaign evaluation, and the
  diagnostic catalog; and
- an egui shell that compiles the embedded specimen through the same compiler
  and renders its findings, owners, and repairs.

The repository also contains proposed `v0.2` semantic rules and an initial
conformance-vector corpus. The Rust kernel executes all 90 current pure vectors:
12 canonical-value, 10 unit-scaling, 19 scope-predicate, 41 requirement-verdict,
and 8 aggregate-verdict cases. A separate compiler harness executes 68 current
type fixtures across five pinned registry snapshots, and a campaign harness
executes 12 claim-admission and verdict fixtures. The first R1–R10 static
compiler frontier and a type-level SC-10/SC-11 campaign slice are implemented,
but package-level rule halves, other normative fixture families, full
package-level admission, invalidation, and package semantics remain proposed
and incomplete.

It does **not** run scientific software, calculate a physical quantity, select
or bind capability packages, read artifact bytes, verify receipts or
signatures, evaluate a scientifically qualified requirement, certify a
design, or produce decision-grade evidence. The specimen contract is an
incomplete draft on purpose.

## Core objects

| Object | Meaning |
| --- | --- |
| Contract | The bounded question, requirements, assumptions, inputs, and evidence policy. |
| Capability | A versioned method or adapter with an explicit provider, execution boundary, qualification scope, and limitations. |
| Campaign | A planned or executed dependency graph resolving one contract. |
| Evidence | Immutable records connecting inputs, capabilities, executions, outputs, reviews, and claims. |
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

cargo run -p avila-core-app
```

Compiling the specimen returns `"status": "rejected"` with only `missing`
findings, one for each value the draft declares `not_defined`. That is the
expected and only honest state of the included specimen; the app renders the
same report.

The compile command prints a JSON report and exits 0 when the contract
compiled, possibly with notices; 1 when it was rejected; and 2 when the tool
could not run. Every finding carries a stable code, a class, an owner, a JSON
Pointer location, and typed repair candidates where a bounded repair exists;
`explain` prints the [catalog entry](docs/architecture/DIAGNOSTICS.md) for a
code, or the whole catalog with `--all`. The evaluate command admits a claims
document against the compiled snapshot and prints one verdict per requirement;
see the [campaign evaluation boundary](docs/architecture/CAMPAIGN_EVALUATION.md).

## Repository map

```text
crates/
  avila-core-kernel/      exact values, predicates, and verdict derivation
  avila-core-compiler/    draft static compiler, document types, and semantic IR
  avila-core-evidence/    portable evidence records and hashing
  avila-core-cli/         headless local interface
  avila-core-app/         thin egui client over the compiler
docs/
  strategy/               north star, economics, and counter-positioning
  product/                product definition, evidence contracts, and UX
  architecture/           boundaries, capability protocol, evidence model, and
                          the diagnostic catalog
  discovery/              sanitized first-pilot discovery record and workflow map
  roadmap/                staged validation and 1.0 planning hypotheses
  adr/                    durable architectural decisions
schemas/                  machine-readable interchange drafts
examples/                 an unqualified specimen contract and registry snapshot
assets/branding/          provisional Avila Core mark
fixtures/semantic-core/   proposed semantic-profile coverage, vectors, and campaigns
```

Start with the [project charter](PROJECT_CHARTER.md), then read the
[north-star strategy](docs/strategy/NORTH_STAR.md),
[product definition](docs/product/PRODUCT_DEFINITION.md), and
[architecture](docs/architecture/ARCHITECTURE.md). The live milestone boundary
is in the [Stage 0 status ledger](docs/roadmap/STAGE_0_STATUS.md), and the next
validation work is captured in the
[first-pilot discovery packet](docs/discovery/FIRST_PILOT_PACKET.md). The
proposed language rules are in [ADR-0006](docs/adr/0006-semantic-core.md).

## Licensing and claims

No source-code license has been selected. Until Avila Labs adopts one, access to
this repository does not grant permission to copy, modify, or redistribute its
contents. The provisional logo is separately reserved. See
[DISCLAIMER.md](DISCLAIMER.md) for the project’s scientific and regulatory
limits.
