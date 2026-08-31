# ADR-0002: Solver-neutral capability boundary

- Status: accepted
- Date: 2026-08-31

## Context

Avila cannot and should not reimplement the scientific software required across
engineering domains. Tying Core to one solver would weaken neutrality, external
provider participation, and customer control.

## Decision

Core integrates provider-owned tools through semantic, versioned capability
types and manifests. Implementations may use any suitable language, process,
scheduler, service, or licensed solver. Rust governs the boundary and evidence;
it is not a mandate to rewrite scientific methods.

## Consequences

- Capability type semantics and conformance become core intellectual work.
- External processes require strong isolation and output validation.
- Multiple implementations can compete under explicit policy.
- Solver licensing and data rights become preflight concerns.
- Core must expose provider and implementation identity in every plan and package.

