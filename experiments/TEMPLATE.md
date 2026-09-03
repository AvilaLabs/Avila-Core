# EXP-NNN — Short title

**Status:** Draft  
**Priority:** P0 / P1 / P2  
**Owner:**  
**Created:** YYYY-MM-DD  
**Scored runs began:** Not started

## Question

State one question the experiment can answer.

## Hypothesis and causal claim

State the predicted result and the exact causal claim the controls are meant
to isolate. If the experiment is descriptive rather than causal, say so.

## System under test

- Core commit:
- Case and revision:
- Package manifest:
- Orchestrator model:
- Worker model(s):
- Model versions and sampling settings:
- Prompt and tool-policy identities:
- Solver/capability identities:
- Environment:

If agent identity or configuration is not machine-bound, label it
operator-reported.

## Arms and controls

| Arm | Feedback available | Purpose |
| --- | --- | --- |
| | | |

List the variables that must remain fixed across arms.

## Candidate space and prior information

Declare allowed variables, bounds, discretization, starting records, and
anything the designer is told before the first scored proposal.

## Budget and stopping rules

Declare screen, full-evaluation, token, wall-time, retry, and human-intervention
budgets. State success, exhaustion, and early-stop rules.

## Measurements

At minimum, consider:

- fraction of trials that find an all-PASS candidate;
- full evaluations to first all-PASS candidate;
- best objective value at the fixed budget;
- invalid, refused, inconclusive, and out-of-envelope proposals;
- wall time, compute time, model time, cost, retries, and human interventions;
- completeness of the resulting evidence record.

## Pre-registration and amendments

Record when the protocol was frozen. Append amendments after scored work begins;
do not silently edit the original rules.

## Results

Link raw logs and derived summaries. Distinguish copied measurements from
interpretation.

## What this demonstrates

State only conclusions directly supported by the controls.

## What this does not demonstrate

List important alternative explanations, external-validity limits, and claims
that require a different experiment.

## Decision and follow-up

Record what changes, stops, or becomes the next experiment because of the
result.
