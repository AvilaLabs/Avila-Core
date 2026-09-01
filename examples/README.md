# Specimen documents

Everything under this directory is illustrative and unqualified. The specimen
contract and registry snapshot exercise the compiler, the CLI, and the desktop
shell; they are not validated scientific methods, executable solver
integrations, safety analyses, or evidence of compliance.

- `contracts/shutdown-dose-specimen.json` is a `v0.2-draft` contract that
  leaves every parameter and seed `not_defined`, so it compiles to a rejected
  draft whose only findings are `missing` values owned by the requester.
- `registry/shutdown-dose-specimen.registry.json` declares the specimen kinds,
  purpose, roles, and capability types the contract names. Every owner is
  `specimen.unassigned` and every type carries non-claims.

The dose limit, unit factors, parameter domains, and every capability
description are hypothetical. They must never be used for engineering or
regulatory decision-making. Package manifests, which describe implementations
rather than types, return under ADR-0006 SC-5.
