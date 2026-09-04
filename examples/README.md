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
  unqualified composed case. It binds an existing synthetic ACTINV 1.0.1 →
  Aftermatter R0 evidence chain, carries the draft case-package manifest with
  its bound executables and execution declarations, executes the ACTINV build
  and the Aftermatter classification under verified receipts, generates every
  output claim from the fresh results, and returns two numeric `PASS` verdicts
  beside one categorical route-state `FAIL`, without any review stage.
- `cases/case-004-magnetic-compliance/` is a pre-registered, reduced-order
  stellarator-support feasibility gate. Its stable negative result is the
  intended outcome of a cheap test: four numerical and compliance checks pass,
  but positive nearest-neighbor coupling fails the reduction, nominal-load,
  and meaningful-coupling criteria, so the protocol says not to advance it.
- `cases/case-005-passive-compliance-ceiling/` tests whether CASE-004 stopped
  too early. It uses public SIMSOPT NCSX geometry and the closed-form global
  optimum over every equal-trace positive-definite stiffness matrix. The
  mathematical ceiling still misses the inherited target, so no iterative
  passive linear topology campaign is started.
- `cases/case-006-coil-support-codesign/` tries the geometry escape route from
  CASE-005 with a field-preserving Fourier near-nullspace loop. It finds a
  small valid improvement, catches coarse-grid overfitting, and still fails
  the fixed reduction target, so that local candidate family stops.
- `cases/case-007-fault-balanced-circuits/` switches mechanisms and treats
  winding-circuit assignment as a fault-design variable. Its first
  magnetic-only proposal fails the force gate; an exhaustive minimax repair
  finds a symmetry-related interleaving that passes seven exploratory gates
  and justifies—but does not perform—a coupled quench/circuit study.
- `cases/case-008-mode-selective-quench/` performs that next reduced-order
  gate. A native Core lineage records an independent-dump root that misses two
  shape gates and a one-variable passive pair-shared-resistor child that passes
  all ten gates, while keeping its normalized I²t and element-voltage proxies
  within five percent of the control.
- `cases/case-009-ncsx-copper-discharge/` corrects the historical conductor
  framing and tests that mechanism on dimensional cryoresistive-copper type
  circuits. The ratio-two root amplifies the worst field and inter-coil
  filament-force departures;
  a ratio-four child worsens both margins and creates a voltage failure, so
  further one-parameter tuning of the delayed symmetric mesh stops under the
  fixed protocol.

The dose limit, unit factors, parameter domains, and every capability
description are hypothetical. They must never be used for engineering or
regulatory decision-making. Package manifests, which describe implementations
rather than types, return under ADR-0006 SC-5.

CASE-000 is more concrete than the invented shutdown-dose specimen. `avila-core
run` re-hashes external artifact bytes from roots supplied by the operator,
reports omitted roots as `not_checked`, executes the ACTINV build and the
Aftermatter classification through case-specific adapters when their bound
executables are supplied, and extracts the output claims from the fresh
bytes; the campaign evaluator itself consumes only the generated claims
document. A matching hash proves byte identity and a verified receipt proves
process provenance, not scientific validity.
