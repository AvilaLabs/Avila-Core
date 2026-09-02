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
- `cases/case-000-actinv-aftermatter/` is the first complete but still
  unqualified composed case. It pins an existing synthetic ACTINV 1.0.1 →
  Aftermatter R0 evidence chain, carries the first draft case-package manifest,
  admits its hash attestations, and deliberately returns `NOT_EVALUATED`
  because qualified review is absent.

The dose limit, unit factors, parameter domains, and every capability
description are hypothetical. They must never be used for engineering or
regulatory decision-making. Package manifests, which describe implementations
rather than types, return under ADR-0006 SC-5.

CASE-000 is more concrete than the invented shutdown-dose specimen, but Core
still does not invoke either scientific tool. `avila-core run` can independently
re-hash external artifact bytes from roots supplied by the operator and reports
omitted roots as `not_checked`; the campaign evaluator itself still consumes
recorded claims. A matching hash proves byte identity, not scientific validity.
