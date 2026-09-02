# User experience

## Implemented slice

The egui workbench today covers the run-and-understand half of the journey
below for a composed case: it lists the roots and executables the case
package requests, plans or runs the workflow through the same runner the CLI
uses, and renders integrity, compilation, execution (reused, executed,
planned, not run, refused, failed, with every change named by class),
generated claims, requirement verdicts with their complete boundaries, and
replay. Guided help is built in: contextual explanations per view, bundled
answers to the questions the interface itself raises, and spotlight
walkthroughs for the use cases above that dim everything but the control
being explained while leaving it live. Dark and light themes are provided.
Contract authoring, preflight editing, plan approval, and campaign
supervision beyond one run are not implemented; nothing in the interface has
semantics the runner and kernel do not.

## Design premise

Core should feel like a disciplined technical workbench, not a solver storefront
or an empty automation canvas. Its first question is:

> What do you need to establish?

The interface organizes complexity around a bounded technical question that a
person or software agent needs answered. Advanced solver details remain
available, but they are downstream of the contract.

## Primary journey

### 1. Choose or create an evidence contract

The user may start from a governed template or a blank bounded question. The
screen asks for:

- the question and system boundary;
- acceptance requirements and units;
- inputs and their identities;
- assumptions, tolerances, and uncertainty sources;
- permitted methods and data policies;
- any optional connected-agent practicality instructions; and
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
  data, environment, or qualification.

The interface never reduces all four to “error.” It shows the source location,
stable finding code, owner, and next action for each blocker. Suggested fixes are
explicit edits; the compiler does not silently change units, kinds, facts, or
policy.

### 3. Inspect the campaign plan

Before execution, the user sees:

- selected capability and provider for each step;
- qualification scope and limitations;
- dependency order and expected outputs;
- data movement and execution location;
- estimated time, compute, license, and provider cost;
- reusable evidence and work that would be rerun; and
- any optional post-campaign practicality-routing instructions.

The runner executes a specific immutable plan under the requester's configured
authorization. A person may choose to acknowledge or approve a plan under an
organization's own policy, but Core does not require that acknowledgement for a
technical verdict. Material changes create a new plan, not a silent mutation.

### 4. Run and supervise the campaign

Progress is expressed in decision-relevant states: waiting for input, running,
blocked, refused, invalidated, or complete. Logs and solver diagnostics are
accessible without becoming the primary navigation.

Core may suggest corrective actions, but cannot conceal a failed execution or
substitute a different method outside the bound contract and policy.

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
error, uncertainty treatment, and qualification boundary. Display formatting
never changes the exact canonical comparison. The result also names the
semantic profile, policy, fact providers, and rule that produced it. A simple
badge is a summary, never the evidence or an unqualified claim of physical
truth.

### 6. Inspect and export evidence

The evidence view supports both audiences:

- a readable package organized requirement by requirement; and
- a machine-readable, content-addressed package for independent verification.

An independent verifier or evidence consumer can reproduce checks and attach
separate annotations without permission to modify the original execution
records. If a connected practicality agent is configured, its separate routing
record can return the candidate to the design loop, present it to the user, or
abstain; it cannot change the technical verdict.

### 7. Change and selectively rerun

When an input, method, dataset, qualification, or policy changes, Core shows:

- exactly which evidence is invalidated;
- which conclusions are no longer usable;
- what remains reusable and why; and
- the minimum admissible rerun plan.

This view is central to the speed advantage. It must be correct before it is
optimized.

## Roles

### Requester / engineer

Creates or instantiates contracts, supplies inputs, configures execution, and
receives results. The default interface emphasizes requirements and next
actions. The requester may be represented by a software agent.

### Method owner

Defines method semantics, applicability, validation evidence, and limitations.
Approves relevant capability versions and contract templates.

### Capability provider

Packages an implementation, maintains conformance and qualification evidence,
and supplies technical support. Cannot alter the requester’s contract or verdict
policy.

### Independent verifier / evidence consumer

Optionally reproduces integrity, compilation, admission, and verdict checks and
records separate findings. This role is a portability and trust option, not a
Core verdict prerequisite. The evidence viewer should be free and read-only by
default.

### Practicality agent

Optionally applies explicit instructions to a technically evaluated finalist.
It can request another design iteration, present the candidate to the user, or
abstain. It cannot approve use or alter a Core technical verdict.

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

The desktop shell compiles the embedded specimen through the same compiler as
the CLI and demonstrates six workspaces:

1. **Overview:** the question and the first blocking finding as the next action;
2. **Contract:** inputs, workflow steps, parameters, and requirements;
3. **Findings:** every finding with class, code, owner, pointer, repair
   candidates, and the catalog's next action;
4. **Compiled snapshot:** source identities, step order, and bindings when the
   contract compiles, or the reason it did not;
5. **Results:** `NOT_EVALUATED` only; and
6. **Evidence:** disabled export and explanation of what a future package contains.

It is a product-language prototype, not an execution application. The UI remains
thin over the headless compiler and holds no scientific state of its own.
