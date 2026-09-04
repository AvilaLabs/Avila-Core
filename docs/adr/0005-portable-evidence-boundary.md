# ADR-0005: Portable evidence boundary

- Status: accepted in principle; format and evidence-package license remain proposed
- Date: 2026-08-31

## Context

Core cannot credibly act as a neutral trust layer if historical evidence requires
an active Avila account or proprietary viewer. Yet portability must support
confidential data, external artifacts, signatures, and established provenance
standards.

## Decision

Customer evidence will be exportable in a documented machine-readable package
with a freely usable offline verification path. The human report will be derived
from the same records. Core will map to established provenance and research-object
standards where they fit.

## Consequences

- Evidence lock-in cannot be a commercial strategy.
- Canonicalization, signatures, redaction, and archival identity are first-class.
- An independent verifier must describe checks and omissions separately.
- Repository source and schema drafts are `AGPL-3.0-only`; final package,
  standalone SDK, and interoperability licensing still require legal and
  strategic decisions before external reliance.
- Enterprise revenue must come from governance, collaboration, routing, support,
  and ongoing use—not hostage access to old evidence.
