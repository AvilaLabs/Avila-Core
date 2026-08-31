## Purpose

Describe the contract, capability, evidence, product, or documentation boundary
this change affects.

## Claims and limitations

- [ ] This change makes no new scientific, safety, clinical, or regulatory claim.
- [ ] Any new specimen is visibly unqualified and non-production.
- [ ] Missing evidence still fails closed.
- [ ] Schema or semantic changes include an ADR and migration analysis.

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace --all-targets`
- [ ] Relevant malformed, blocked, inconclusive, and invalidation paths are tested.

## Professional review

Name any domain method owner, reviewer, security reviewer, or legal reviewer this
change requires before it can be relied upon.

