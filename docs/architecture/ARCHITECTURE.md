# Architecture

## Architectural goal

Core must separate five kinds of responsibility and provenance:

1. **Contract intent:** what question and policy were approved.
2. **Semantic rules:** which versioned language gives records and derivations
   their meaning.
3. **Method qualification:** which owned and validated capability is admissible
   for a context of use.
4. **Execution provenance:** what inputs, software, data, environment, and
   process actually ran.
5. **Verdict derivation:** how admitted evidence maps to a requirement state.

No single interface, provider, agent, or process may impersonate all five. In
particular, the compiler checks explicit records; it does not invent method
authority or require a professional reviewer.

## Target system context

```text
 Requester / Agent             Method Owner / Provider      Evidence consumer
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
           │ solver / data / method / agent│           │
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
    ├── avila-core-compiler  (v0.2-draft document types, static compilation,
    │                         semantic IR, campaign evaluation, and the
    │                         diagnostic catalog)
    ├── avila-core-evidence  (record model, hashing, case-package integrity,
    │                         and execution receipts)
    └── avila-core-runner    (the case workflow: staging, execution, receipts,
        │                     reuse, claim generation, built-in and declared adapters)
        ├── avila-core-cli   (canonicalize, compile, evaluate, explain, run)
        └── avila-core-app   (thin egui workbench over the runner and compiler)
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
specimens. Its conformance tests execute all 91 current vectors across the four
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
leaving package-level reproducibility to future package binding. When
configured, an optional agent practicality type lowers to a
`presentation_gate` containing the exact dossier, closed routing dispositions,
a digest-pinned agent policy, and explicit instructions. It never participates
in a technical verdict, and omitting it is valid. Governed nominal requirement purposes are checked
against output-level explicit exclusions without interpreting prose or inferring
purpose hierarchies. It performs no package selection, execution, qualification decision, or
verdict during compilation. A separate campaign entry point admits an
evidence-claims document against the compiled snapshot under the type-level
admission conditions and derives verdicts with the kernel; see the
[semantic compiler boundary](SEMANTIC_COMPILER.md) and
[campaign evaluation](CAMPAIGN_EVALUATION.md).

### `avila-core-evidence`

Defines draft evidence records, SHA-256 content identities, the case-package
manifest with its bound capabilities and executions, the execution receipt
record with its byte-level verification (ADR-0007), and the unsigned optional
agent routing record (ADR-0010). It has no package writer, signature system,
lineage validator, or independent verifier yet.

### `avila-core-runner`

The only crate that performs I/O beyond reading documents: it re-hashes bytes
at explicitly supplied roots, stages verified inputs into a fresh workspace,
runs exact executables through named built-in or hash-bound package-declared
checker adapters, writes and verifies execution receipts, reuses steps whose committed receipts still
describe the planned invocation and names every change by class, generates
the claims document, hands evaluation to the compiler and kernel, and—only when
configured—materializes an exact content-identified post-campaign presentation
request. Its report is the single source every front end renders (ADR-0007,
ADR-0010).

### `avila-core-cli`

Provides authoritative JSON canonicalization, embedded semantic-profile and
vector-set identities, `v0.2-draft` compilation with a nonzero exit status for
a rejected contract, campaign evaluation over a claims document, the
diagnostic catalog through `explain`, and the composed case workflow through
`run`, printed as a concise staged view or as the complete JSON report. All
output explicitly distinguishes software conformance, structural validity,
and process provenance from scientific validity.

### `avila-core-app`

An egui workbench with two modes. The case workbench opens a composed case,
lists the roots and executables its package requests, runs the workflow on a
background thread through the runner crate, and renders the report stage by
stage: integrity, compilation, execution with reuse and change classes,
generated claims and binding, verdicts with their complete boundaries,
optional presentation-gate readiness and instructions, and replay. The specimen view compiles the embedded specimen and renders its
findings with owners and repairs. Every badge and number is read from a
report; the client performs no calculation and must never grow a separate
scientific state model.

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
8. The planner binds the exact plan identity; any organization-specific launch
   authorization is an external policy event, not a Core verdict rule.
9. The runner stages each step into a fresh controlled workspace, verifies all
   input hashes, invokes the adapter, and captures an execution receipt.
10. Output validators reject artifacts that do not satisfy the capability contract.
11. Admission A1–A10 decides whether each produced claim can enter the campaign
    evidence graph.
12. Evidence records connect outputs to inputs, process receipts, method and data
   versions, and validation evidence.
13. Verdict logic evaluates only admitted evidence, compares exact canonical
    values, and produces requirement-level states and boundary statements.
14. The packager writes a human-readable and machine-readable evidence package;
    the independent verifier checks it from the package root.
15. If configured, a connected agent applies the exact practical presentation
    instructions and either returns the candidate, presents it to the user, or
    abstains. This routing record remains outside steps 11 and 13.

Steps 9 to 11 and 13 have first executable slices: for the steps a committed
case declares, the runner stages verified bytes, invokes the bound executable,
and writes a receipt verified from bytes; built-in or narrowly declarative
external-checker adapters extract claims from declared outputs; type-level
admission over the generated claims and
review-independent kernel verdicts follow. Optional presentation requests are
materialized after evaluation. Steps 4 to 8, generic output
validators, and step 14 do not exist yet. Any failure before step 13 yields
no verdict. A completed method that cannot decide the requirement may yield
`INCONCLUSIVE` when the contract permits it.

## Execution neutrality

Rust provides Core’s authoritative model, planner, verifier, and application
shell because it supports explicit types, portable binaries, and controlled
failure behavior. Scientific software remains in its suitable ecosystem.

A capability may invoke:

- a local Rust, C, C++, Fortran, Python, or Julia executable;
- a container or batch job;
- an HPC scheduler;
- an organization service; or
- an optional connected-agent presentation step.

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
- organization identity → external provider or trust policy.

Parsing success, process exit code zero, and a valid signature each establish
only their narrow claim.

The trusted computing base should remain small, I/O-free where practical, and
covered by normative semantics plus adversarial conformance vectors. A semantic
profile and a kernel implementation have separate identities: archived packages
name both, and a verifier must report when it cannot replay the historical
profile.

## Non-negotiable failure behavior

- Unknown schema fields are rejected at authoritative boundaries.
- Missing capability, dependency, qualification, or evidence blocks the
  affected claim. Missing review never does.
- No capability can directly set the final verdict badge.
- No interface, agent, or provider can construct an admission or verdict record
  outside the semantic kernel.
- A changed input or method cannot preserve a downstream conclusion without an
  explicit reuse rule.
- The UI cannot invent placeholder numbers.
- Every external process is assumed hostile or faulty until isolated and checked.
- A verifier reports what it checked and what it did not check.
