# Architecture

## Architectural goal

Core must separate five kinds of authority:

1. **Contract authority:** what question and policy were approved.
2. **Semantic authority:** which versioned language rules give the records and
   derivations their meaning.
3. **Method authority:** which professional-owned capability is admissible for
   that context.
4. **Execution authority:** what inputs, software, data, environment, and process
   actually ran.
5. **Verdict authority:** how admitted evidence maps to a requirement state.

No single interface, provider, or process should be able to impersonate all five.

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
avila-core-model
    ├── avila-core-runtime   (planning only)
    ├── avila-core-evidence  (record model + hashing only)
    ├── avila-core-cli       (canonicalize, compile, validate, and plan)
    └── avila-core-app       (thin read-only specimen UI)

avila-core-kernel            (first canonical-value authority slice)
    └── avila-core-compiler  (v0.2-draft static compilation only)
```

### `avila-core-model`

Authoritative serializable types for evidence contracts, capability manifests,
requirements, qualification, and verdicts. It performs structural validation but
cannot assert scientific validity.

### `avila-core-runtime`

Validates the dependency graph, chooses a deterministic matching manifest, and
marks unavailable or inadmissible steps as blocked. It contains no process
runner, scheduler, cache, or remote backend.

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

Accepts a `v0.2-draft` contract and one explicit immutable registry snapshot,
then returns deterministic structured findings or a content-identified compiled
snapshot. The current slice resolves typed slots, derives and checks the
dependency graph, validates nominal roles and media, and lowers requirement
limits through exact kind/unit rules. It performs no package selection,
execution, evidence admission, qualification decision, or verdict. See the
[semantic compiler boundary](SEMANTIC_COMPILER.md).

### `avila-core-evidence`

Defines draft evidence records and SHA-256 content identities. It has no package
writer, signature system, lineage validator, or independent verifier yet.

### `avila-core-cli`

Provides authoritative JSON canonicalization, embedded semantic-profile and
vector-set identities, `v0.2-draft` compilation, headless `v0.1` contract
validation, and specimen plan rendering. All output explicitly distinguishes
software conformance or structural validity from scientific validity.

### `avila-core-app`

An egui client over the same model and planner. It does not perform calculations
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

The compiler, admission engine, and verdict evaluator are conceptual authority
boundaries even if an early implementation keeps them in fewer crates. Front
ends may author and explain records; only the kernel boundaries may construct
admissions and verdicts.

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

Any failure before step 13 yields no verdict. A completed method that cannot decide
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
- organization identity → provider or reviewer authority.

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
  outside the authority kernel.
- A changed input or method cannot preserve a downstream conclusion without an
  explicit reuse rule.
- The UI cannot invent placeholder numbers.
- Every external process is assumed hostile or faulty until isolated and checked.
- A verifier reports what it checked and what it did not check.
