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
| S-018 | Accepted | The first runnable case workflow separates raw-byte integrity, identity binding, semantic compilation, campaign evaluation, and deterministic replay. Omitted artifact roots are `not_checked`; supplied roots with missing or different bytes fail closed. A matching digest proves identity only and does not alter qualification or review state. |
| S-019 | Accepted | `CASE-000E`: Core executes Aftermatter through a named case-specific adapter over verified bytes and an exact executable, writes and verifies an execution receipt, and generates the output claims from the fresh bytes. An omitted executable leaves a step `not_run` with its committed claims as recorded attestations; a supplied executable must match its bound identity, complete, and reproduce claims that bind, or the run is rejected. No horizontal runner or adapter protocol is built. |
| S-020 | Accepted | CASE-000 revision 2 executes ACTINV 1.0.1 as well, through Aftermatter's frozen R0 builder under a digest-pinned Python interpreter, with the builder script and the ACTINV executables as hash-bound inputs of the step. The activation step's problem, inventory, and decay metadata become executed outputs, so no output claim in the case is authored. The Aftermatter repository is not modified for this; a tool that must change to be executed is a finding, not a patch. |
| S-021 | Accepted | The case runner performs SC-12 execution memoization: a step whose planned invocation identity (capability digest, parameters, staged input identities, arguments, environment, timeout) equals a committed completed receipt's, and whose recorded outputs all verify at bound artifact identities, is reused without running and without needing the executable. Any difference is reported by typed change class (input bytes, input binding, parameters, capability, invocation, no or incomplete receipt, outputs unavailable) and reruns exactly that step; dependency follows content, so a rerun whose outputs are byte-identical leaves its consumers reused. `--plan` reports the analysis without running; `--no-reuse` forces fresh execution. |
| S-022 | Accepted | Core's intended primary use is the generative loop: an agent proposing candidates, Core as the oracle that says exactly what fails and why, every run logged into an engineering constellation, two fidelities with `PASS` reserved for qualified methods, requirements owned and reviewed by people, and accountable human review at the end. Verification of existing work is a use, not the goal. See [The generative loop](docs/strategy/GENERATIVE_LOOP.md). The first domain is shielding configuration search. |

Architecture records:

- [ADR-0001: Evidence contract as unit of work](docs/adr/0001-evidence-contract-unit-of-work.md)
- [ADR-0002: Solver-neutral capability boundary](docs/adr/0002-solver-neutral-capability-boundary.md)
- [ADR-0003: Headless Rust authority and thin egui client](docs/adr/0003-headless-rust-authority.md)
- [ADR-0004: Four-state verdict model](docs/adr/0004-four-state-verdict-model.md)
- [ADR-0005: Portable evidence boundary](docs/adr/0005-portable-evidence-boundary.md)
- [ADR-0006: Semantic core and evidence-contract language](docs/adr/0006-semantic-core.md)
- [ADR-0007: Execution receipts and case-specific adapters](docs/adr/0007-execution-receipts.md)
