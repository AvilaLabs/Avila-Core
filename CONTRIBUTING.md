# Contributing

Avila Core is not yet accepting general production use or scientific claims.
Early contributions should strengthen the contract, capability, evidence,
security, and test boundaries without widening the claimed scope.

## Development rules

- Keep scientific authority out of the UI.
- Reject unknown or malformed fields at trust boundaries.
- Default to blocked, inconclusive, or not evaluated when evidence is missing.
- Never add a specimen that looks like a validated benchmark.
- Every future execution must identify exact inputs, capability version,
  environment, outputs, and parent evidence.
- Prefer small, provider-neutral interfaces over dependencies on one solver.
- Do not commit customer data, controlled information, credentials, proprietary
  solver files, or exported campaign evidence.
- Add tests for every state transition and invalidation rule.
- Compare authoritative quantities exactly; presentation rounding must never
  alter a verdict.
- Give every new finding code a catalog entry with a bounded next action and a
  fixture that emits it; consumers match codes, never wording.
- Treat provider-reported facts as assertions with provenance, not as truth
  promoted by a runner signature.
- Do not call a finite vector corpus the complete specification; update written
  semantics, schemas, and fixtures together.
- Flag a new enabling technical mechanism for IP review before placing it in a
  public issue, pull request, release, or design document.

Before submitting a change:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Changes to schemas or core semantics require an ADR. Changes that could be
interpreted as a scientific, safety, or regulatory claim also require review by
the appropriate domain professional.
