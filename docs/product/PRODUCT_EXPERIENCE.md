# Core as an engineering workspace

Recorded: 2026-09-15. Status: aspirational product direction and interaction
requirements. This describes the intended complete experience without imposing
a release schedule, selecting a frontend technology, or claiming these features
are implemented. New authoritative schemas and semantics still require ADRs.

## Product form

Core should be an installable engineering workspace with a browser-accessible
counterpart and an integrated ecosystem of workflows, requirements, methods,
datasets, and solver capabilities. Organizations can deploy the same experience
with private libraries, identities, storage, and compute. Public evidence can be
inspected without requiring an account or a paid service.

A project retains its identity across desktop, browser, and organization
deployments. Viewing location and execution location are independent: browser
users can connect a workstation runner, and desktop users can use organizational
compute. WASM is a possible implementation mechanism, not the product identity.
Local and offline operation remain first-class experiences.

## Opening Core

The home screen centers on projects, recent work, and actionable changes:
completed explorations, affected evidence, or available method updates. New
users see guided examples and workflow starters. Established users return
directly to their engineering work. Library, compute, and organization settings
are nearby, but a solver marketplace or chat window is not the mandatory entry.

Creating a project offers three paths:

- start from a published or organizational workflow;
- import an existing project or evidence package;
- assemble a new question and methods from scratch.

## Integrated library

Keep these objects distinguishable:

| Object | Responsibility |
| --- | --- |
| Requirement set | What must be established |
| Contract template | Inputs, assumptions, and assessment structure for a question |
| Capability package | A method implementation with execution and verification interfaces |
| Dataset | Versioned reference inputs and their provenance |
| Workflow starter | A compatible combination with examples and guided configuration |

Workflow starters provide the easiest entrance. Users choose an engineering
task, inspect its scope, supply their inputs, and resolve the choices that
require engineering judgment. The resulting project pins its own requirements
and versions; a library update cannot silently alter a baseline.

Support public, organization, and personal sources. Show publisher identity,
applicability, validation evidence, limitations, versions, maintenance status,
and organization acceptance separately. Reputation is useful context, not a
universal qualification badge. Label hypothetical catalog examples as such.

## Integration is part of the product

Core should discover installed capabilities, help install or connect selected
ones, resolve declared dependencies, and expose compatible execution locations.
Commercial licenses and organization-managed tools belong in that experience.
Routine work should not require hand-editing manifests or finding executable
paths. Expert overrides remain inspectable and attributable.

Scientific compatibility remains explicit: matching file formats is insufficient
without compatible units, normalization, spatial meaning, material identity,
uncertainty treatment, and scope. Unsupported combinations receive actionable
findings rather than guessed adapters.

ACTINV, OpenBNCT, OptCoil, and future Avila tools remain independently useful
products. They may supply capability packages and specialist viewers/editors to
Core. Third-party implementations use the same interfaces and admission rules.
Core does not need to absorb all solver interfaces, CAD authoring, or domain
optimization algorithms to provide a coherent workspace.

## Inside a project

| View | User purpose |
| --- | --- |
| Overview | Understand the question, current design, requirements, and open issues |
| Design | Work with parameters, materials, geometry references, and alternatives |
| Methods | Inspect calculation dependencies, implementations, and applicability |
| Explore | Compare manually, sweep parameters, optimize, or delegate an investigation |
| Evidence | Inspect claims and follow them to exact supporting records |
| History | Compare revisions, record decisions, and inspect baselines |

The design is the center of attention. Geometry may be edited in an external
tool and inspected through a domain viewer; Core need not become a universal
CAD application. Shared operations supply all authoritative values to every
client. See [design-history requirements](DESIGN_HISTORY.md) for identity,
amendment, comparison, review, and invalidation boundaries.

## Optional AI, shared work

Manual work, scripts, optimizers, and agents operate on the same project and
produce comparable records. Agents can propose alternatives, request bounded
execution, investigate failures, and return evidence-linked comparisons. Their
results appear in project views, not only in chat transcripts. Agent rationale
remains attributable, and agents cannot rewrite fixed requirements to obtain a
pass. Users who never enable AI retain the library, guided setup, execution,
comparison, and history experience.

## Experience acceptance scenarios

1. A new user selects a workflow, inspects its publisher and limits, supplies
   inputs, resolves compatible capabilities, and opens a usable project.
2. An engineer changes a design manually, sees the impact plan, runs it, compares
   results to the baseline, and exports evidence without using AI.
3. An agent explores under a fixed requirement set and budget. Its alternatives,
   failures, evidence, and rationale remain inspectable by a human afterward.
4. A colleague opens the same project in another client without changing its
   identities; data location and execution location remain explicit.
5. An installed method receives an update. The user can assess its impact while
   retaining the exact earlier baseline and independently inspectable evidence.

## Interactive product preview

`cargo run -p avila-core-app --bin avila-core-product-preview` opens a separate
egui concept application. It uses in-memory invented data and simulates project
creation, library selection, manual changes, execution, agent exploration,
comparison, and baseline acceptance. No solver runs, downloads, remote calls,
real approvals, or evidence exports occur. Closing it resets the simulation.
It is an interaction study, not an implementation of the requirements above.

The existing `avila-core-app` remains the real case workbench. The preview's
rendered figures must never be cited as engineering evidence.
