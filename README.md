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

- authoritative Rust types for contracts, capability manifests, and verdicts;
- the first semantic-kernel slice for exact bounded rationals, canonical
  decimals, authoritative JSON, quantity kinds, exact unit scaling, and
  three-valued applicability predicates;
- exact four-state verdict derivation over admitted claim specimens;
- a deterministic dependency planner that fails closed;
- a draft portable evidence model and SHA-256 utility;
- JSON Schemas and deliberately non-executable specimen documents;
- a local CLI for structural validation and planning; and
- an egui product shell showing the intended question-first experience.

The repository also contains proposed `v0.2` semantic rules and an initial
conformance-vector corpus. The Rust kernel executes all 90 current pure vectors:
12 canonical-value, 10 unit-scaling, 19 scope-predicate, 41 requirement-verdict,
and 8 aggregate-verdict cases. The wider compiler, admission, invalidation, and
package semantics remain proposed and unimplemented.

It does **not** run scientific software, calculate a physical quantity, admit
real evidence, evaluate a scientifically qualified requirement, certify a
design, or produce decision-grade evidence. The specimen campaign is blocked on
purpose.

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

cargo run -p avila-core-cli -- \
  validate-contract examples/contracts/shutdown-dose-specimen.json

cargo run -p avila-core-cli -- plan \
  --contract examples/contracts/shutdown-dose-specimen.json \
  --capability examples/capabilities/openmc-transport.specimen.json \
  --capability examples/capabilities/actinv-activation.specimen.json \
  --capability examples/capabilities/avila-dose.specimen.json \
  --capability examples/capabilities/avify-bounds.specimen.json \
  --capability examples/capabilities/core-requirement.specimen.json

cargo run -p avila-core-app
```

The plan command should return `"status": "blocked"`. That is the expected and
only honest state of the included specimen.

## Repository map

```text
crates/
  avila-core-model/       contract, capability, requirement, and verdict types
  avila-core-kernel/      exact values, predicates, and verdict derivation
  avila-core-runtime/     deterministic campaign planning; no execution yet
  avila-core-evidence/    portable evidence records and hashing
  avila-core-cli/         headless local interface
  avila-core-app/         thin egui client
docs/
  strategy/               north star, economics, and counter-positioning
  product/                product definition, evidence contracts, and UX
  architecture/           boundaries, capability protocol, and evidence model
  roadmap/                staged validation and 1.0 planning hypotheses
  adr/                    durable architectural decisions
schemas/                  machine-readable interchange drafts
examples/                 unqualified, non-executable software specimens
assets/branding/          provisional Avila Core mark
fixtures/semantic-core/   proposed semantic-profile coverage and initial vectors
```

Start with the [project charter](PROJECT_CHARTER.md), then read the
[north-star strategy](docs/strategy/NORTH_STAR.md),
[product definition](docs/product/PRODUCT_DEFINITION.md), and
[architecture](docs/architecture/ARCHITECTURE.md). The proposed language rules
are in [ADR-0006](docs/adr/0006-semantic-core.md).

## Licensing and claims

No source-code license has been selected. Until Avila Labs adopts one, access to
this repository does not grant permission to copy, modify, or redistribute its
contents. The provisional logo is separately reserved. See
[DISCLAIMER.md](DISCLAIMER.md) for the project’s scientific and regulatory
limits.
