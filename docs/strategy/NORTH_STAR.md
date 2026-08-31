# North-star strategy

## The destination

Avila Core should become the neutral market and trust layer for computational
engineering. Organizations submit evidence contracts; qualified capabilities
compete and compose to resolve them; Core returns portable, independently
reviewable technical conclusions.

This is deliberately more ambitious than a workflow orchestrator. Orchestration
is necessary plumbing. The durable product is a mechanism through which:

- a requester can express what must be established;
- domain professionals can encode and govern admissible methods;
- software and service providers can supply substitutable capabilities;
- Core can select and compose those capabilities without favoring its own;
- every important claim is connected to reviewable evidence; and
- an evidence consumer can verify the package without trusting an Avila-hosted
  screen.

If this works, Core becomes infrastructure beneath research and engineering
organizations—not the organization pretending to possess every specialty above
them.

## The wedge

The north star cannot be built horizontally on day one. Core begins with one
expensive, recurring, cross-tool technical question in a domain where Avila
already has useful pieces and access to practitioners.

The present candidate is a nuclear evidence chain:

```text
geometry + materials + exposure + tolerances + requirement
                            │
                            ▼
                   transport calculation
                            │
                            ▼
                  activation / inventory
                            │
                            ▼
                    shutdown-dose method
                            │
                            ▼
                 uncertainty / bound method
                            │
                            ▼
               requirement-specific verdict
                            │
                            ▼
                  portable evidence package
```

Existing Avila work suggests—but does not yet prove—the following roles:

| Project | Possible Core role | Current caution |
| --- | --- | --- |
| NCTForge | transport adapter and provenance patterns | It must not be represented as a qualified general transport service. |
| ACTINV | activation-inventory capability | Applicability, validation, interfaces, and ownership must be established. |
| Avify | bounded-uncertainty capability where mathematically valid | Soundness, tightness, and scalability remain gating research questions. |
| Core | contract, policy, routing, orchestration, evidence, review, and later settlement | The scaffold has planning only; it performs no scientific work. |

The first vertical may change after professional interviews. The mechanism is
more important than preserving this illustrative chain.

## Why this could matter

Current products already automate solvers, move compute to the cloud, connect
requirements, manage data, track workflows, and generate reports. Core is not
important merely because it combines those familiar features.

Its potential leverage comes from changing the economic and technical unit:

| Conventional unit | Core unit |
| --- | --- |
| software seat | resolved evidence contract |
| solver job | admissible capability contribution |
| compute hour | minimum sufficient computation |
| vendor report | portable evidence package |
| successful run | explicit requirement verdict |
| vendor ecosystem | neutral provider market |

A team using Core should move faster because it reuses reviewed contracts,
methods, and evidence—not because rigor is removed. Every repeated contract
should make the next one cheaper to define, easier to route, more selective in
what must be rerun, and faster to review.

That compounding reuse is the adoption thesis. It must be demonstrated with
measured cycle time, cost, review burden, and defect escape—not asserted in
marketing.

## The flywheel hypothesis

```text
more valuable contracts
        ↓
more demand for qualified capabilities
        ↓
more professionals and providers publish capabilities
        ↓
greater coverage and substitutability
        ↓
faster, cheaper, more resilient contract resolution
        ↓
more evidence consumers recognize the package
        ↓
more valuable contracts
```

The flywheel fails if capabilities are not reusable, evidence consumers do not
accept the package, providers cannot earn meaningful economics, or each contract
still requires bespoke consulting. Those are validation questions, not
implementation details.

## Strategic invariants

Core must preserve these properties while it grows:

1. **Professional authority:** a named owner governs each technical method and
   applicability boundary.
2. **Neutrality:** selection policy cannot secretly prefer an Avila capability
   or a high-margin compute route.
3. **Portability:** the customer can retain and inspect its evidence without a
   continuing Avila subscription.
4. **Fail-closed semantics:** missing, invalid, stale, or unqualified evidence
   cannot become cosmetic success.
5. **Outcome independence:** Avila and providers are paid for compliant work,
   regardless of `PASS`, `FAIL`, or `INCONCLUSIVE`.
6. **Compute minimization:** caching, invalidation, sensitivity, bounds, and
   selective reruns should reduce unnecessary computation.
7. **Explicit scope:** no verdict exists outside the stated contract,
   assumptions, methods, uncertainty treatment, and review policy.
8. **Independent review:** evidence must be useful to people who did not run the
   campaign and do not trust Core by default.

## What Core is not

- not an Avila-authored suite of every scientific solver;
- not a generic engineering consultancy billed primarily by the hour;
- not a cloud-compute reseller whose revenue rises with waste;
- not a chat interface over tools;
- not a workflow diagram marketed as scientific assurance;
- not a certification body, regulator, insurer, or substitute for one;
- not a favorable-answer machine; and
- not a claim that software can remove professional judgment.

## Company fit

The strategy lets Avila Labs be a laboratory without pretending to be the
leading authority in every field. Avila investigates neglected computational
gaps, builds rigorous reusable mechanisms with professionals, and creates the
infrastructure through which their expertise can do more work.

The company’s role is servant, mechanism builder, and steward of the trust
boundary. That is a demanding technical role in its own right. It requires
humility about domain authority and unusual rigor about what the software is
allowed to claim.

