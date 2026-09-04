# Project charter

**Product name:** Avila Core  
**Status:** exploratory research and pre-alpha engineering  
**Charter date:** 2026-08-31

## Purpose

Avila Core exists to let people and software agents attack difficult
engineering problems through rigorous, repeatable search. It translates a
bounded technical question into a reproducible chain of methods, evidence, and
explicit verdicts, then tells an iterating agent exactly what failed and why.

The long-term product is a neutral semantic, market, and trust layer for
computational engineering:

> People or agents submit evidence contracts; capabilities compete and compose
> to contribute evidence; Core returns portable, independently verifiable
> conclusions about what follows under the declared rules.

## Problem

Technical organizations already possess solvers, data, scripts, and compute.
Their costly bottleneck is often turning those pieces into a strict oracle that
can explore thousands of candidates without moving the requirements, losing
provenance, or mistaking a successful process for a satisfactory design. More
simulation is not automatically more certainty.

Core’s governing thesis is:

> The world should not pay for more simulation. It should pay for faster
> resolution of technical uncertainty.

This is a hypothesis to validate, not a claim of existing product-market fit.

## People Core serves

1. **Requesters and engineers** need a bounded question resolved without
   manually rebuilding every cross-tool workflow.
2. **Connected agents and designers** need machine-readable failures, margins,
   and evidence so they can iterate without inventing verdicts.
3. **Method owners** need their methods, limits, validation records, and
   applicability boundaries represented faithfully and reused without becoming
   invisible.
4. **Capability providers** need a neutral route to distribute and be paid for
   valuable computational methods.
5. **Evidence consumers** need a portable package they can inspect without
   trusting Core’s interface or the requester’s summary.

No Core verdict requires a professional reviewer. External organizations may
add their own acknowledgement, approval, or regulatory process without turning
it into a kernel rule.

## Product boundary

Core may:

- formalize questions, requirements, assumptions, and admissibility policy;
- discover and compose qualified capabilities;
- run approved methods locally, remotely, or in controlled environments;
- capture immutable provenance and execution evidence;
- evaluate explicitly encoded requirements;
- report `PASS`, `FAIL`, `INCONCLUSIVE`, or `NOT_EVALUATED`; and
- export evidence that survives outside Avila’s systems.

Core may not, by itself:

- invent scientific authority;
- convert a valid derivation into an unqualified claim about physical truth;
- turn an unqualified method into a qualified one;
- imply certification, regulatory acceptance, or safety approval;
- conceal model limitations behind a simple status badge;
- reward a provider for producing a favorable result;
- trap customer evidence in a proprietary-only format; or
- require Avila-hosted compute merely to preserve commercial leverage.

## Strategic commitments

- The unit of work is the evidence contract, not the solver job.
- The customer owns the evidence generated from its work.
- `INCONCLUSIVE` is a legitimate and billable resolution state.
- Core is solver-, provider-, language-, and infrastructure-neutral.
- The local runtime and independent verifier must remain viable open-source
  product boundaries under the repository license.
- Method boundaries stay with named owners and validation records—not with the
  graphical interface, generative agent, or semantic compiler.
- Every Core verdict remains conditional on the exact records, trust policy,
  semantic profile, and limitations named in it.
- Core should minimize redundant compute rather than benefit from its growth.
- Qualification scope and limitations must be explicit and inspectable.

## What success means

The first meaningful proof is one bounded evidence contract for which an agent
uses Core to find a candidate that passes all stated gates, while Core refuses
at least one invalid shortcut and preserves independently reproducible evidence.

The 1.0 threshold is higher: an independent user or implementation must be able
to define, execute, iterate, export, independently verify, invalidate, and
selectively rerun a supported contract using multiple interchangeable
capability providers. A configured connected-agent practicality gate may route
finalists before presentation, but Core must work without it. The exact
supported domain will be narrow and named.

Scale, revenue, or number of integrations cannot substitute for those proofs.
