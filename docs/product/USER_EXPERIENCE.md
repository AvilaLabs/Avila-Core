# User experience

## Design premise

Core should feel like a disciplined technical workbench, not a solver storefront
or an empty automation canvas. Its first question is:

> What do you need to establish?

The interface organizes complexity around a professional decision. Advanced
solver details remain available, but they are downstream of the contract.

## Primary journey

### 1. Choose or create an evidence contract

The user may start from a governed template or a blank bounded question. The
screen asks for:

- the question and system boundary;
- acceptance requirements and units;
- inputs and their identities;
- assumptions, tolerances, and uncertainty sources;
- permitted methods and data policies;
- required reviewers; and
- the intended decision context.

Core uses ordinary language alongside precise fields. It explains why a field is
needed and who owns it. GUI fields, imported JSON, CLI input, and any future
textual syntax lower to the same canonical records; no front end has separate
verdict semantics.

### 2. Complete preflight

Core separates four kinds of problem:

- **missing:** a required item was not supplied;
- **invalid:** a document violates a schema or semantic rule; and
- **unsatisfied:** the requested capability graph has no well-typed composition;
  and
- **inadmissible:** the evidence policy does not permit the available method,
  data, environment, or reviewer.

The interface never reduces all four to “error.” It shows the source location,
stable finding code, owner, and next action for each blocker. Suggested fixes are
explicit edits; the compiler does not silently change units, kinds, facts, or
policy.

### 3. Review the campaign plan

Before execution, the user sees:

- selected capability and provider for each step;
- qualification scope and limitations;
- dependency order and expected outputs;
- data movement and execution location;
- estimated time, compute, license, provider, and review cost;
- reusable evidence and work that would be rerun; and
- required human approvals.

The user approves a specific immutable plan. Material changes create a new plan,
not a silent mutation.

### 4. Run and supervise the campaign

Progress is expressed in decision-relevant states: waiting for input, awaiting
approval, running, blocked, rejected, invalidated, or complete. Logs and solver
diagnostics are accessible without becoming the primary navigation.

Core may suggest corrective actions, but cannot conceal a failed execution or
substitute a different method without policy and approval.

### 5. Understand the result

The result page leads with each requirement:

```text
R-017        INCONCLUSIVE
Reason       admitted upper bound crosses the requirement limit
Limit        10.00 mSv
Bound        9.41–10.83 mSv
Boundary     named configuration, tolerances, data, and cooling time
Next action  reduce uncertainty in material B or modify the design margin
```

Every value links to its producing capability, configuration, inputs, numerical
error, uncertainty treatment, and reviews. Display formatting never changes the
exact canonical comparison. The result also names the semantic profile, policy,
fact providers, and rule that produced it. A simple badge is a summary, never
the evidence or an unqualified claim of physical truth.

### 6. Review and export evidence

The evidence view supports both audiences:

- a readable package organized requirement by requirement; and
- a machine-readable, content-addressed package for independent verification.

The reviewer can annotate, reject, request information, or countersign without
needing permission to modify the original execution records.

### 7. Change and selectively rerun

When an input, method, dataset, policy, or review changes, Core shows:

- exactly which evidence is invalidated;
- which conclusions are no longer usable;
- what remains reusable and why; and
- the minimum admissible rerun plan.

This view is central to the speed advantage. It must be correct before it is
optimized.

## Roles

### Requester / engineer

Creates or instantiates contracts, supplies inputs, reviews plans, and receives
results. The default interface emphasizes requirements and next actions.

### Method owner

Defines method semantics, applicability, validation evidence, and limitations.
Approves relevant capability versions and contract templates.

### Capability provider

Packages an implementation, maintains conformance and qualification evidence,
and supplies technical support. Cannot alter the requester’s contract or verdict
policy.

### Reviewer

Inspects lineage and method applicability, records findings, and countersigns
where authorized. The evidence viewer should be free and read-only by default.

### Organization administrator

Controls identity, registries, admissibility policy, environments, retention,
keys, and separation of duties. Does not acquire scientific approval authority
merely by being an administrator.

## Interaction principles

- Question first, tool second.
- Show the boundary before the answer.
- Put owner and next action beside every blocker.
- Never use green to mean “the program ran” when the requirement is unevaluated.
- Keep `INCONCLUSIVE` visually distinct from both failure and system error.
- Make limitations easier to find than marketing claims.
- Preserve expert access to configuration and raw evidence.
- Do not make conversational interaction the primary or only path.
- Do not generate values for empty product states.
- Use the same authoritative model in GUI, CLI, and service interfaces.

## Current egui scaffold

The initial desktop shell demonstrates five workspaces:

1. **Overview:** the question and next blocked action;
2. **Contract:** inputs, requirement, and evidence policy;
3. **Execution plan:** selected specimen capabilities and honest blockers;
4. **Results:** `NOT_EVALUATED` only; and
5. **Evidence:** disabled export and explanation of what a future package contains.

It is a product-language prototype, not an execution application. The UI remains
thin over the headless Rust model and planner.
