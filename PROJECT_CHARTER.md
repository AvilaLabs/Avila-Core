# Project charter

**Product name:** Avila Core  
**Internal codename:** Project North Star  
**Status:** exploratory research and pre-alpha engineering  
**Charter date:** 2026-08-31

## Purpose

Avila Core exists to make rigorous computational work easier for the
professionals responsible for it. It should help them translate a technical
question into a reproducible chain of methods, evidence, review, and an explicit
decision state without replacing their scientific authority.

The north-star product is a neutral market and trust layer for computational
engineering:

> Organizations submit evidence contracts; qualified capabilities compete and
> compose to resolve them; Core returns portable, independently reviewable
> technical conclusions.

## Problem

Technical organizations already possess solvers, data, experts, scripts, and
compute. Their costly bottleneck is often the distance between those resources
and an answer that another professional can understand, reproduce, challenge,
and accept. More simulation is not automatically more certainty.

Core’s governing thesis is:

> The world should not pay for more simulation. It should pay for faster
> resolution of technical uncertainty.

This is a hypothesis to validate, not a claim of existing product-market fit.

## People Core serves

1. **Requesters and engineers** need a bounded question resolved without
   manually rebuilding every cross-tool workflow.
2. **Method owners and domain professionals** need their methods, limits, and
   judgment represented faithfully and reused without becoming invisible.
3. **Capability providers** need a neutral route to distribute and be paid for
   valuable computational methods.
4. **Reviewers and evidence consumers** need a portable package they can inspect
   without trusting Core’s interface or the requester’s summary.

No user role delegates professional responsibility to Core.

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
- The local runtime and independent verifier must remain viable product
  boundaries even if their final license is not yet decided.
- Scientific authority stays with named method owners, reviewers, and evidence
  policy—not with the graphical interface.
- Core should minimize redundant compute rather than benefit from its growth.
- Qualification scope and limitations must be explicit and inspectable.

## What success means

The first meaningful proof is one real evidence contract that a domain
professional says is materially faster to resolve in Core, with no loss of
reviewability, than through the existing workflow.

The 1.0 threshold is higher: an independent organization must be able to define,
execute, review, export, independently verify, invalidate, and selectively rerun
a supported contract using multiple interchangeable capability providers. The
exact supported domain will be narrow and named.

Scale, revenue, or number of integrations cannot substitute for those proofs.

