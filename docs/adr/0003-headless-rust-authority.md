# ADR-0003: Headless Rust authority and thin egui client

- Status: accepted
- Date: 2026-08-31

## Context

The same contract and evidence semantics must apply to desktop, CLI, remote, and
independent-verification use. Putting scientific state or verdict logic in a GUI
would create divergence and make headless or air-gapped operation secondary.

## Decision

Authoritative models, validation, planning, policy, execution, evidence, and
verdict logic live in headless Rust crates. The egui application is a thin client
over those crates. No scientific work runs on the UI thread.

## Consequences

- CLI and future APIs use the same types as the desktop application.
- egui can be replaced without changing evidence semantics.
- Core can operate locally without a web service.
- Application convenience code cannot manufacture results or bypass policy.
- Non-Rust scientific adapters remain supported behind the capability protocol.

