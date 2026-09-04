# ADR-0014: Identity-bound attempt lineage

- Status: accepted for the local runner; schemas remain drafts
- Date: 2026-09-04
- Refines: ADR-0006 SC-8, ADR-0007, and the generative loop

## Context

The run-attempt log already records inputs, findings, verdict margins,
receipts, and artifacts, but it treats every line independently. CASE-007 made
the missing relationship concrete: an initial candidate failed one coupled
objective and a repaired candidate passed, yet Core preserved only the final
candidate as a governed result. The designer's after-the-fact explanation was
the only connection between them.

Putting proposal strategy or an optimizer inside the semantic kernel would
cross Core's authority boundary. Leaving parentage and variable changes only
in agent prose, however, prevents the log from becoming reliable iteration
memory.

## Decision

1. A caller may add `--attempt ID` to `avila-core run`, optionally with
   `--parent-attempt ID`. Lineage requires `--log FILE` and one supplied input
   nominated by `--candidate-input NAME` (`candidate` by default).
2. The nominated input must be canonical-profile JSON no larger than 1 MiB.
   Core records both its raw artifact digest and its canonical state digest.
   Nominating it explicitly opts that complete state into the report and log.
3. A root has generation zero and no changes. A child binds the SHA-256 of the
   exact parent JSONL line and inherits the parent's manifest identity,
   compiled snapshot identity, and candidate-input name.
4. Core derives changes; the designer does not assert them. Object members and
   equal-length arrays are compared recursively by RFC 6901 JSON Pointer.
   Added, removed, and replaced values retain their JSON types. An array shape
   change is one replacement so index shifting cannot imply false element
   ancestry.
5. Branches are allowed. Attempt identifiers are unique within a log, parents
   must precede children, and every existing parent link, generation, candidate
   state digest, and derived change set is revalidated before a new attempt is
   admitted.
6. A manifest or compiled-snapshot change cannot enter an existing lineage.
   Core emits `CORE-X1201` and stops before capability execution. A deliberate
   question change starts a new root.
7. The attempt record is included in case-run report v0.4 and run-attempt log
   v0.2. The surrounding record remains authoritative for that candidate's
   findings, verdicts, receipts, and output artifacts.

## Boundary

Lineage records what was tried; it does not choose the parent, candidate, or
next action. The optimizer remains outside Core and cannot construct a verdict.
The JSONL chain is digest-linked but unsigned, and the current local appender
assumes one writer. It detects prior-record changes when history is read and
rechecks the selected parent immediately before append, but it is not a
transactional multi-process database.

Candidate state is intentionally visible in the log. Secret or very large
inputs must not be nominated; their artifact identities can remain in the
ordinary run record without embedding their contents.

## Consequences

- An agent can consume exact parentage, typed changes, Core findings, margins,
  and resulting artifacts without reconstructing candidate files from paths
  that may have moved.
- Rewriting requirements during search becomes a refused identity change, not
  an undocumented optimization tactic.
- CASE-008's coupled circuit/quench iteration exercises this layer: one root
  misses two fixed gates, one identity-bound child changes a single physical
  parameter and passes all ten, and the unchanged manifest and compiled
  snapshot are recorded on both lines.
- A future constellation view or optimizer can read this record without
  acquiring authority over compilation, evidence admission, or verdicts.
