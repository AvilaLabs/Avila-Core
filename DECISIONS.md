# Decision index

This file records accepted strategic decisions and links to architectural
decision records (ADRs). “Proposed” items are intentionally reversible.

| ID | Status | Decision |
| --- | --- | --- |
| S-001 | Accepted | Public product name is **Avila Core**; repository folder codename is **project-north-star**. |
| S-002 | Accepted | Core begins with a technical question and evidence contract, not a solver catalog or blank workflow canvas. |
| S-003 | Accepted | Core is infrastructure for professionals; it does not present Avila Labs as the domain authority for every supported method. |
| S-004 | Accepted | Public positioning centers on rigorous computational evidence, not implementation trends. |
| S-005 | Accepted | `PASS`, `FAIL`, `INCONCLUSIVE`, and `NOT_EVALUATED` are distinct first-class states. |
| S-006 | Accepted | Avila should be paid for admissibly resolving a contract, not for returning `PASS`. |
| S-007 | Accepted | Customer evidence must be portable and independently inspectable. |
| S-008 | Accepted | Scientific adapters may wrap existing tools in any suitable language; Rust is the authority and orchestration boundary, not a mandate to rewrite solvers. |
| S-009 | Accepted | The initial application is local-first and compatible with private or air-gapped deployment. |
| S-010 | Accepted | Specimen and unavailable capabilities must fail closed and never create cosmetic success states. |
| S-011 | Proposed | A free/open local runtime and verifier become the adoption surface; enterprise policy and provider routing are commercial layers. Final licensing remains unresolved. |
| S-012 | Proposed | The first vertical proof composes transport, activation, shutdown dose, uncertainty treatment, and requirement evaluation. Domain professionals must confirm it before implementation. |
| S-013 | Proposed | The semantic profiles, conformance vectors, package format, and verifier interface should be independently auditable and reimplementable; licensing and publication follow an IP review so public disclosure does not accidentally waive protection options. |
| S-014 | Proposed | JSON remains the canonical interchange. A textual `.acore` front end is built only after semantic fixtures stabilize and measured authoring evidence justifies it. |
| S-015 | Accepted | A Core verdict is a conditional derivation from admitted records under named rules and authorities. It is not, by itself, scientific truth, certification, or regulatory approval. |
| S-016 | Accepted | The `v0.1` contract model, capability manifests, and planner are retired. The `v0.2-draft` contract, which carries its bounded question, and the registry snapshot compiled by `avila-core-compiler` are the only authoritative document forms; package selection returns as an SC-5 and SC-8 binding pass over compiled snapshots. |
| S-017 | Accepted | `CASE-000` is the first composed internal integration specimen: the frozen ACTINV 1.0.1 inventory feeds Aftermatter R0, and Core admits the recorded claims while withholding `PASS` for absent qualified review. It does not select the first external vertical, close discovery, qualify either tool, or count as pilot evidence. |

Architecture records:

- [ADR-0001: Evidence contract as unit of work](docs/adr/0001-evidence-contract-unit-of-work.md)
- [ADR-0002: Solver-neutral capability boundary](docs/adr/0002-solver-neutral-capability-boundary.md)
- [ADR-0003: Headless Rust authority and thin egui client](docs/adr/0003-headless-rust-authority.md)
- [ADR-0004: Four-state verdict model](docs/adr/0004-four-state-verdict-model.md)
- [ADR-0005: Portable evidence boundary](docs/adr/0005-portable-evidence-boundary.md)
- [ADR-0006: Semantic core and evidence-contract language](docs/adr/0006-semantic-core.md)
