# Review of the 2026-09-04 architecture and execution proposal

**Reviewed document:** [proposals/2026-09-04-architecture-and-execution-roadmap.md](../proposals/2026-09-04-architecture-and-execution-roadmap.md),
pinned to commit `3be5902`.
**Review basis:** the same commit. Ten of the proposal's repository
observations were each checked independently against the source by a
read-only auditor that could execute the debug binary; four further
assessments read the whole proposal against the repository's own ledger,
decisions, experiments and branches. The lead added two measurements of the
unmerged solver-acceleration branch. Debug-build timings are indicative only
and are labelled as such.
**Outcome:** the architecture is kept as reference; its sequencing is not
adopted. Decision [S-034](../../../DECISIONS.md).

## Observations checked

| # | Proposal observation | Finding at `3be5902` | Measured | Already recorded by | Response adopted |
| --- | --- | --- | --- | --- | --- |
| 1 | Every case execution re-verifies the package and recompiles the contract | Accurate. `execute_case_inner` calls `verify_case_package` and `compile_documents` unconditionally before any SC-12 reuse decision; one OS process per candidate | CASE-003 verify-only run 10–40 ms. CASE-002 with all roots 0.31 s, of which almost all is re-hashing ~250 MB of static nuclear and ACTINV artifacts; a fresh screen adds ~30 ms | Performance baseline gate was open | Opt-in verified-hash cache for operator artifact roots, reported as a distinct integrity state. No prepared-session facade yet |
| 2 | Attempt lineage re-reads and revalidates the whole history on every run | Accurate. `read_attempts` parses every line, `validate_history` re-canonicalizes every candidate state and recomputes every parent diff | 0.36 ms per log line plus 18 ms fixed; 126 ms at 300 attempts; one real CASE-008 attempt 8.16 s of which 8.12 s was the checker | ADR-0014 boundary section | In-process single read per invocation when it becomes measurable. No database |
| 3 | JSONL append is two writes with validation before, not with, the append | Accurate as stated. Two `write_all` calls under `O_APPEND`, no lock; every campaign driver runs one Core process at a time | Not a cost; a latent race | ADR-0014 boundary section | One buffer, one write, advisory lock across revalidation and append |
| 4 | Each step copies and re-hashes staged inputs into a fresh directory | Accurate. Applies only on an SC-12 miss | CASE-002 activation staging copy plus hash 1.1–1.5 s against a 12.4 s step; no other case stages large inputs | Artifact store listed as unbuilt | Deferred; the hash cache covers verification, staging stays as is |
| 5 | Timeout polls at 20 ms and kills only the direct child | Accurate. No process group; a python adapter's solver child survives a timeout | Poll cost under 1 percent of the shortest real job (8.5 s) | Execution row, open items | Process-group kill on timeout. The poll stays |
| 6 | A missing qualification record is not universally rejected | Accurate. `quarantined_by_qualification` only inspects claims that carry an assessment; CASE-000 obtains bounded PASS with no qualification record | Not a cost; a soundness boundary | ADR-0008 clause 5, S-025, queue item 4 | Contract-level `require_qualification` mirroring `permit_nominal_basis`; legacy mode carries a visible reason |
| 7 | Candidate inputs are opaque to the compiler | Accurate. A registry role's `validator` field is checked for non-emptiness and never invoked; malformed candidates surface as adapter tracebacks or silently defaulted facts | Not a cost; a feedback-quality gap | None | Role input schemas validated by the existing embedded validator before staging, with pointer-anchored findings. The typed design boundary grows from here |
| 8 | No sandbox, resource accounting or signatures | Accurate; the receipt says so itself | The designer cannot reach the executable boundary: it submits candidate JSON through a fixed, pinned package (S-030) | SECURITY.md, CAPABILITY_PROTOCOL.md, ADR-0011, S-030 | Signing next (queue item 4). Sandboxing when executables come from a third party |
| 9 | Roadmap and strategy documents mix historical and current status | Partly. `STAGE_0_STATUS.md` is already the single current ledger with an update rule and matches the binary's counts; one row and the queue were behind the cases landed on 2026-09-03 | None | The ledger itself | Ledger rows corrected; a banner on the generative-loop record points to the ledger |
| 10 | Selective rerun should become a tested invalidation service | Partly. SC-12 reuse detects eight change classes and never memoizes compilation or evaluation, so requirement and qualification changes already produce fresh verdicts over reused evidence; registry and policy edits follow the same path but lack a pinned test | None | S-021, ADR-0007, EVIDENCE_MODEL.md | Add the missing adversarial tests when the runner is next touched |

## Assessment of the milestone plan

- **Diagnosis right, sequence inverted.** The experiment backlog's ordering
  rule puts "does Core add value beyond raw tool access" first; the proposal
  schedules that ablation (EXP-002) in its last milestone, after twelve to
  twenty-two weeks of session, store, scheduler and obligation-graph work
  that EXP-002 does not need.
- **The ledger's queue was dropped without saying why.** Signing, queue
  item 1, moved to the fifth milestone; the speed tier (S-031) and its
  branch are not mentioned; CASE-004 through CASE-007 are absent.
- **The generality test it schedules has already been run.** CASE-003,
  CASE-007, CASE-008 and CASE-009 each added a domain with package changes
  and a small, demonstrated kernel extension.
- **Remedies exceed measured costs.** Every gap above has a fix of one to
  three focused days that keeps the interchange formats unchanged. None of
  the proposed dependencies (rayon, SQLite, Salsa, a locking crate) exists in
  the workspace today; each would be a first adoption decision.
- **Its best risk control is not in its own graph.** The proposal names
  "infrastructure absorbs research time" and prescribes gating each phase on
  a short measurable outcome, but no milestone before the last is gated on
  one.

## What is adopted from the proposal

- The eight guarantees in its section 3 as specification obligations, each
  needing a rule, vectors and a derivation format before it is claimed.
- The separation of compile, execution and admission caches as the design
  rule behind any future reuse work.
- Process-tree cancellation, the strict qualification profile, explicit
  assumptions, and the typed design boundary as directions.
- Benchmark discipline: release builds, a frozen workload and machine,
  cold and warm separated, machine time separated from model time, and no
  redefinition of a workload after seeing results.
- A go/no-go gate: the session, store, scheduler and obligation-graph
  milestones are considered only after the scored EXP-002 result and a
  release-build baseline show a campaign that needs them.

## Sequence adopted

1. EXP-002 with matched arms on CASE-003, the shortcut-refusal criterion
   folded in, transcripts and model identities bound to the record.
2. Strict qualification and candidate validation at the boundary.
3. Process-group timeout, locked single-write append, opt-in hash cache,
   extraction of rendering and logging from the case runner.
4. Signed receipts and manifests, then the independent verifier.
5. Constellation queries over the logs, a harder coupled search, and a fresh
   speed-tier attempt gated on agreement with the S-031 solver.

## Solver-acceleration branch

The unmerged branch `coupled/slab-sn-dsa` (commit `bd0a03e`) implemented a
Newton-GMRES acceleration of the thermal iteration rather than diffusion
synthetic acceleration. Its unit tests pass. On the real VITAMIN-J-175
library and an 85 cm three-layer sample candidate it did not meet any target
and changed the answer:

| solve | unaccelerated (main) | branch |
| --- | ---: | ---: |
| forward wall time | 51 s | 57 s |
| adjoint wall time | not run | did not finish in 900 s |
| neutron dose rate | 43.16 uSv/h | 84.32 uSv/h |

The branch is discarded and the result recorded as S-033. A future attempt
starts from the verified S-031 solver with an agreement test against it as
its first gate.
