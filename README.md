<p align="center">
  <img src="assets/branding/Avila_Core_Logo.png" alt="Avila Core" width="360">
</p>

<h1 align="center">Avila Core</h1>

<p align="center">
  An evidence compiler and controlled execution layer for reproducible, agent-assisted computational engineering.
</p>

<p align="center">
  <a href="https://github.com/AvilaLabs/Avila-Core/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/AvilaLabs/Avila-Core/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Status: pre-alpha" src="https://img.shields.io/badge/status-pre--alpha-f59e0b">
  <img alt="Rust 1.95+" src="https://img.shields.io/badge/rust-1.95%2B-000000?logo=rust">
  <a href="LICENSE"><img alt="License: AGPL-3.0-only" src="https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg"></a>
</p>

Avila Core turns a bounded technical question into a reproducible chain of
inputs, methods, evidence, and explicit verdicts. It is designed to give both
people and software agents a strict feedback loop without confusing a
successful process with a justified result.

> **Pre-alpha:** the schemas and interfaces are still evolving. The included
> cases are research specimens, not qualified engineering workflows.

## Why Core exists

Agents can generate candidates quickly, but engineering decisions require more
than plausible output. Core keeps the question fixed while candidates change,
executes content-identified capabilities over exact inputs, preserves receipts,
admits only evidence that matches the contract, and reports what passed, failed,
remained inconclusive, or could not be evaluated.

```text
contract + registry
        ↓
semantic compilation
        ↓
controlled capability execution
        ↓
receipts + admitted claims
        ↓
requirement verdicts + portable evidence
```

Core does not replace solvers, scientific validation, professional judgment, or
regulatory review. It makes their boundaries explicit and inspectable.

## What works today

- A versioned evidence-contract and registry model.
- Deterministic JSON, exact quantities and unit scaling, closed-set categorical
  predicates, and four-state requirement verdicts.
- Stable compiler and runtime diagnostics with source locations, owners, and
  bounded next actions.
- Controlled local execution over staged inputs with cleared environments,
  content hashes, receipts, replay checks, and selective reuse.
- Hash-bound, package-declared command-line checkers with closed numeric and
  categorical claim extraction.
- An execution policy that can require a qualification assessment behind
  every bounded or enclosure verdict, refusing unqualified evidence outright
  or, at its default, evaluating it while visibly flagging the gap. A qualification record may scope
  itself to the output slots it covers, so an unqualified estimate produced
  alongside a qualified quantity stays unqualified.
- Registry-declared structural schemas for free inputs such as a search
  candidate, so a malformed one is refused before anything is staged.
- An opt-in verified-hash cache for large, unchanging operator-supplied
  artifacts, reported as a state distinct from a fresh hash and never
  weakening the check against the manifest's bound identity.
- End-to-end case reports for integrity, compilation, execution, evidence
  binding, coverage, and verdicts.
- Identity-bound attempt lineage with automatic typed candidate diffs and
  parent-to-child verdict and exact-margin comparisons, plus scripted agent
  campaigns and engineering specimens used to test the loop.
- Signed manifests and receipts: an operator trust root (`avila-core keys
  generate`, `avila-core sign`, `run --trust-root`) that refuses a package
  whose manifest signature does not verify and excludes an unverified
  committed receipt from reuse, with a contract policy that can make an
  unsigned package an outright refusal.
- An independently implemented, standard-library-only Python verifier
  (`verifier/`) that re-derives package identity, receipt invocation
  identities, claims binding, and every requirement verdict from the same
  fixture vectors, importing no Core crate.
- A headless CLI and a thin native workbench over the same Rust implementation.
- Ten shared read-only queries for recorded results, available through a
  human-readable CLI and a local stdio MCP server. See the
  [Core tools guide](docs/product/CORE_TOOLS.md) for usage and verification boundaries.

## Quick start

Avila Core currently requires Rust 1.95.0.

```bash
git clone https://github.com/AvilaLabs/Avila-Core.git
cd Avila-Core
cargo test --workspace --all-targets

cargo run -p avila-core-cli -- semantic-profile

cargo run -p avila-core-cli -- compile \
  --contract fixtures/semantic-core/types/types.R1.resolved.pass.contract.json \
  --registry fixtures/semantic-core/types/compiler.registry.v1.json \
  --text
```

To inspect a deliberate compiler failure:

```bash
cargo run -p avila-core-cli -- compile \
  --contract examples/contracts/shutdown-dose-specimen.json \
  --registry examples/registry/shutdown-dose-specimen.registry.json \
  --text

cargo run -p avila-core-cli -- explain CORE-R3102
```

The CLI also provides `evaluate`, `run`, and `canonicalize`; use
`cargo run -p avila-core-cli -- --help` for the complete command surface.

## Repository guide

- [`crates/`](crates/) contains the kernel, compiler, evidence model, runner,
  CLI, and desktop workbench.
- [`schemas/`](schemas/) contains the current interchange-schema drafts.
- [`examples/`](examples/) contains executable specimens, capabilities, and
  recorded campaigns.
- [`fixtures/semantic-core/`](fixtures/semantic-core/) contains conformance
  vectors and negative compiler cases.
- [Architecture](docs/architecture/ARCHITECTURE.md) explains the component and
  trust boundaries.
- [Diagnostics](docs/architecture/DIAGNOSTICS.md) documents the compiler and runtime
  finding contract.
- [Contributing](CONTRIBUTING.md), [security](SECURITY.md), and the
  [research disclaimer](DISCLAIMER.md) define the current participation and
  use boundaries.

## License and mark

Except for separately identified branding assets, this repository is licensed
under the [GNU Affero General Public License v3.0
only](LICENSE) (`AGPL-3.0-only`). The license applies to Core, not automatically
to ordinary inputs or outputs created or evaluated with Core.

Included and generated third-party material is identified in the
[third-party notices](THIRD_PARTY_NOTICES.md).

The Avila Core name and logo are separately reserved; see
[`assets/branding/README.md`](assets/branding/README.md).
