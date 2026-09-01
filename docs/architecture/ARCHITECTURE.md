# Architecture

## Architectural goal

Core must separate five kinds of responsibility and provenance:

1. **Contract intent:** what question and policy were approved.
2. **Semantic rules:** which versioned language gives records and derivations
   their meaning.
3. **Method qualification:** which professional-owned capability is admissible
   for a context of use.
4. **Execution provenance:** what inputs, software, data, environment, and
   process actually ran.
5. **Verdict derivation:** how admitted evidence maps to a requirement state.

No single interface, provider, reviewer, or process may impersonate all five.
In particular, the compiler checks explicit obligations; it does not decide who
deserves professional or institutional trust.

## Target system context

```text
 Requester / Engineer          Method Owner / Provider          Reviewer
          │                              │                         │
          └───────────────┬──────────────┴──────────────┬──────────┘
                          ▼                             ▼
                 ┌────────────────┐           ┌────────────────┐
                 │  Core clients  │           │Evidence viewer │
                 │ GUI / CLI / API│           │  + verifier    │
                 └───────┬────────┘           └───────┬────────┘
                         │                            │
                         ▼                            │ independent
              ┌─────────────────────┐                 │ verification
              │ Semantic compiler   │                 │
              │ + canonical profile │                 │
              └──────────┬──────────┘                 │
                         ▼                            │
              ┌─────────────────────┐                 │
              │ Contract & policy   │                 │
              │   control plane     │                 │
              └──────────┬──────────┘                 │
                         ▼                            │
              ┌─────────────────────┐                 │
              │ Deterministic plan  │                 │
              │ + capability select │                 │
              └──────────┬──────────┘                 │
                         ▼                            │
              ┌─────────────────────┐                 │
              │ Controlled runner   │                 │
              │ local / HPC / remote│                 │
              └──────────┬──────────┘                 │
                         ▼                            │
           ┌─────────────────────────────┐             │
           │ Versioned capability adapters│            │
           │ solver / data / method / review│          │
           └─────────────┬───────────────┘             │
                         ▼                             │
              ┌─────────────────────┐                  │
              │ Evidence graph and  │──────────────────┘
              │ requirement verdicts│
              └─────────────────────┘
```

Core is both local software and, later, an optional organization/network control
plane. A campaign must be able to remain inside a customer-controlled environment
while exchanging signed metadata with a registry or settlement service when
policy permits.

## Current workspace

The scaffold deliberately implements only the shaded foundation implied below:

```text
avila-core-kernel            (first canonical-value semantic slice)
    └── avila-core-compiler  (v0.2-draft document types, static compilation,
        │                     semantic IR, and the diagnostic catalog)
        ├── avila-core-cli   (canonicalize, compile, evaluate, explain)
        └── avila-core-app   (thin client rendering compile reports)

avila-core-evidence          (record model + hashing only)
```

The `v0.1` contract model and planner were retired once the compiler's
document types became the only authoritative contract representation
(decision S-016). Package selection returns as an SC-5 and SC-8 binding pass
over compiled snapshots, not as a separate planner.

### `avila-core-kernel`

Implements the first isolated portions of the draft semantic profile: canonical
decimal lowering, reduced exact rationals under an explicit fail-closed work
budget, authoritative JSON reading, duplicate-key rejection, NFC enforcement,
deterministic JCS-derived key ordering, nominal quantity-kind registries, and
exact within-kind unit scaling. It also evaluates the current bounded predicate
language with strong Kleene `true`/`false`/`unknown` semantics over explicit
contexts and source requirements, then derives four-state requirement verdicts
from admitted exact, interval, coverage-interval, worst-case, and nominal claim
specimens. Its conformance tests execute all 90 current vectors across the four
pure vector sets. It does not yet compile whole contracts, admit evidence,
invalidate dependencies, or verify packages.

### `avila-core-compiler`

Owns the authoritative `v0.2-draft` document types, including the contract's
bounded question and prose assumptions, which it carries into the compiled
boundary and never interprets. It accepts a contract and one explicit
immutable registry snapshot, validates both against the embedded schemas so
every source-layer problem is reported in one pass, then returns deterministic
structured findings or a content-identified compiled snapshot. The current
slice resolves typed slots, derives and checks the
dependency graph, validates nominal roles, claim-model sufficiency, and media,
enforces declared parameter types and domains, and lowers parameters and
requirement limits through exact kind/unit rules. It also binds each capability
type's determinism class, seed, and declared material execution factors while
leaving package-level reproducibility to future package binding. Accountable
review types lower to pending obligations containing the exact dossier,
governance dispositions, digest-pinned external eligibility policy, and
explicit independence constraints. The compiler neither evaluates that policy
nor fulfills the review. Governed nominal requirement purposes are checked
against output-level explicit exclusions without interpreting prose or inferring
purpose hierarchies. It performs no package selection, execution, qualification decision, or
verdict during compilation. A separate campaign entry point admits an
evidence-claims document against the compiled snapshot under the type-level
admission conditions and derives verdicts with the kernel; see the
[semantic compiler boundary](SEMANTIC_COMPILER.md) and
[campaign evaluation](CAMPAIGN_EVALUATION.md).

### `avila-core-evidence`

Defines draft evidence records and SHA-256 content identities. It has no package
writer, signature system, lineage validator, or independent verifier yet.

### `avila-core-cli`

Provides authoritative JSON canonicalization, embedded semantic-profile and
vector-set identities, `v0.2-draft` compilation with a nonzero exit status for
a rejected contract, campaign evaluation over a claims document, and the
diagnostic catalog through `explain`. All output
explicitly distinguishes software conformance or structural validity from
scientific validity.

### `avila-core-app`

An egui client that compiles the embedded specimen through the same compiler
and renders the report: question, contract, findings with owners and repairs,
and the compiled snapshot when one exists. It does not perform calculations
and must never grow a separate scientific state model.

## Target components

Future components should be added only behind acceptance gates:

- `avila-core-kernel`: extend the existing canonical-value slice with nominal
  quantity kinds, evidence roles, predicate evaluation, verdict derivation, and
  semantic-profile compatibility;
- `core-compiler`: authored-document lowering, static checks, deterministic
  findings, and impact analysis without authority to admit evidence or emit a
  verdict;
- `core-contracts`: template lifecycle, instantiation, approval, and amendments;
- `core-registry`: local and organization capability discovery and policy matching;
- `core-runner`: isolated lifecycle execution with immutable receipts;
- `core-artifacts`: content-addressed storage, packaging, and retention;
- `core-policy`: admissibility and separation-of-duties evaluation;
- `core-admission`: I/O-free package-level admissibility over immutable
  snapshots;
- `core-verdict`: I/O-free requirement evaluation over admitted evidence;
- `core-invalidation`: dependency and semantic change-impact engine;
- `core-signing`: identities, signatures, timestamps, and trust roots;
- `core-verify`: independent evidence-package verification;
- adapter SDKs and conformance fixtures for non-Rust providers; and
- optional enterprise and network services for private registries, routing,
  settlement, and organization governance.

Names are provisional. Separate crates are appropriate only when they enforce a
real dependency or trust boundary.

The compiler, admission engine, and verdict evaluator are separate enforcement
boundaries even if an early implementation keeps them in fewer crates. Front
ends may author and explain records; only the semantic kernel boundaries may
construct admissions and verdicts.

## Data flow

1. A client submits an authored contract and input descriptors.
2. The compiler parses and lowers it under a named semantic profile, returning
   canonical records and typed findings without silently modifying the source.
3. Generic validators establish structural properties. Domain validators and
   capability packages make separately attributed method claims.
4. The planner determines type-level satisfiability against immutable registry
   and policy snapshots.
5. Admission separates organization policy, contract policy, qualification,
   applicability facts, and implementation constraints before any optimization.
6. Selection ranks only admitted candidates using an explicit ordered policy.
7. The planner emits an immutable campaign plan with exact capability identities,
   dependencies, expected artifacts, environment policy, and cost estimate.
8. Required people approve and sign the plan.
9. The runner stages each step into a fresh controlled workspace, verifies all
   input hashes, invokes the adapter, and captures an execution receipt.
10. Output validators reject artifacts that do not satisfy the capability contract.
11. Admission A1–A10 decides whether each produced claim can enter the campaign
    evidence graph.
12. Evidence records connect outputs to inputs, process receipts, method and data
   versions, validation evidence, and reviews.
13. Verdict logic evaluates only admitted evidence, compares exact canonical
    values, and produces requirement-level states and boundary statements.
14. The packager writes a human-readable and machine-readable evidence package;
    the independent verifier checks it from the package root.

Steps 11 and 13 have a first executable slice: type-level admission over a
claims document and kernel verdicts with review asymmetry. Steps 4 to 10 and
14 do not exist yet. Any failure before step 13 yields no verdict. A completed method that cannot decide
the requirement may yield `INCONCLUSIVE` when the contract permits it.

## Execution neutrality

Rust provides Core’s authoritative model, planner, verifier, and application
shell because it supports explicit types, portable binaries, and controlled
failure behavior. Scientific software remains in its suitable ecosystem.

A capability may invoke:

- a local Rust, C, C++, Fortran, Python, or Julia executable;
- a container or batch job;
- an HPC scheduler;
- an organization service; or
- a human review step.

Core standardizes the boundary and evidence receipt. It does not rewrite a proven
solver merely to make the implementation homogeneous.

## Local-first deployment

The first trustworthy runtime should work without an Avila service:

- contract and policy files are local;
- capability packages are installed from an explicit registry snapshot;
- data stays in the configured environment;
- package verification requires no network call; and
- all external communication is policy-controlled and receipted.

Enterprise services may add identity, private registries, collaboration, managed
updates, and routing. They must not become necessary to inspect historical
customer evidence.

## Determinism and reproducibility

Core must distinguish:

- deterministic compilation under a named semantic profile;
- deterministic planning and document canonicalization;
- bitwise-reproducible execution where attainable;
- numerically reproducible results within declared tolerances; and
- scientifically credible results within a context of use.

These are not synonyms. A deterministic wrong method remains wrong. A stochastic
method can be admissible when seeds, distributions, convergence, numerical error,
and acceptance policy are recorded.

## Trust boundaries

Untrusted inputs include every contract, artifact, manifest, adapter, process
output, archive, remote receipt, and signature until verified. Important
boundaries are:

- GUI/API → authoritative model;
- manifest registry → policy engine;
- planner → runner;
- runner → external process or scheduler;
- produced artifact → output validator;
- evidence graph → verdict evaluator;
- package → independent verifier; and
- organization identity → external provider or reviewer eligibility policy.

Parsing success, process exit code zero, and a valid signature each establish
only their narrow claim.

The trusted computing base should remain small, I/O-free where practical, and
covered by normative semantics plus adversarial conformance vectors. A semantic
profile and a kernel implementation have separate identities: archived packages
name both, and a verifier must report when it cannot replay the historical
profile.

## Non-negotiable failure behavior

- Unknown schema fields are rejected at authoritative boundaries.
- Missing capability, dependency, qualification, evidence, or review blocks the
  affected claim.
- No capability can directly set the final verdict badge.
- No interface, agent, or provider can construct an admission or verdict record
  outside the semantic kernel.
- A changed input or method cannot preserve a downstream conclusion without an
  explicit reuse rule.
- The UI cannot invent placeholder numbers.
- Every external process is assumed hostile or faulty until isolated and checked.
- A verifier reports what it checked and what it did not check.
