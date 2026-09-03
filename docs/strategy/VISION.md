# Product vision

## The destination

Avila Core should become a neutral semantic, market, and trust layer for
computational engineering. People and connected agents submit evidence
contracts; capabilities compete and compose to contribute evidence; Core
applies an explicit rulebook and returns portable, independently verifiable
conclusions about what follows from that admitted evidence.

This is deliberately more ambitious than a workflow orchestrator. Orchestration
is necessary plumbing. The durable product is a mechanism through which:

- a requester can express what must be established;
- named owners can encode methods, validation evidence, and applicability
  boundaries;
- software and service providers can supply substitutable capabilities;
- Core can select and compose those capabilities without favoring its own;
- every important claim is connected to reviewable evidence; and
- an evidence consumer can verify the package without trusting an Avila-hosted
  screen.

The conclusion is deliberately conditional. Core can establish that a verdict
follows from named records, attestations, policies, and semantic rules. It cannot
establish that every premise corresponds to physical reality, that a model is
scientifically adequate outside its qualification, or that a regulator will
accept the result. That boundary is part of the product, not disclaimer text.

If this works, Core becomes the strict oracle inside autonomous research and
engineering loops—not the organization pretending to possess every specialty
or the gatekeeper demanding a professional review for each result.

## The wedge

The full product cannot be built horizontally on day one. Core begins with one
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
| Core | contract, policy, routing, orchestration, evidence, presentation, and later settlement | The scaffold has planning only; it performs no scientific work. |

The first vertical may change after benchmark and user evidence. The mechanism
is more important than preserving this illustrative chain.

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

A team using Core should move faster because it reuses versioned contracts,
methods, and evidence—not because rigor is removed. Every repeated contract
should make the next one cheaper to define, easier to route, more selective in
what must be rerun, and faster to verify.

That compounding reuse is the adoption thesis. It must be demonstrated with
measured cycle time, cost, verification burden, and defect escape—not asserted in
marketing.

## What compounds and what can be copied

The semantic specification and independent verifier should become auditable and
reimplementable after an explicit owner-recorded publication and licensing
decision. A closed
verdict algorithm would weaken the very neutrality Core needs. Consequently,
the specification by itself cannot be treated as the moat.

The compounding assets are:

- governed quantity kinds, evidence roles, and capability-type definitions;
- owned qualifications, validation evidence, and applicability boundaries;
- adapters, conformance suites, and negative-case corpora;
- versioned, tested contract templates and authority-specific policy mappings;
- dependency and reuse rules that safely avoid unnecessary reruns;
- portable evidence history usable by independent verifiers and customers; and
- enterprise operation across local, air-gapped, HPC, and organization
  environments.

The strategic objective is therefore not to make the judgment layer impossible
to copy. It is to make Core the best-supported implementation and the common
protocol through which people, agents, tools, and evidence consumers interoperate.
That position is earned through governance, coverage, and use—not declared by
publishing a schema.

## The flywheel hypothesis

```text
more valuable contracts
        ↓
more demand for qualified capabilities
        ↓
more owners and providers publish capabilities
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

1. **Attributed ownership:** a named person or organization governs each
   technical method and applicability boundary; Core does not impose a
   professional credential or reviewer.
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
   assumptions, methods, uncertainty treatment, and qualification records.
8. **Independent verification:** evidence must be usable without trusting the
   campaign operator or an Avila-hosted interface.
9. **Conditional claims:** a Core verdict says what follows inside a recorded
   boundary; it never silently expands into scientific truth, certification, or
   regulatory approval.
10. **Semantic transparency:** admissibility and verdict rules are versioned,
    testable, and separable from commercial provider ranking.

## What Core is not

- not an Avila-authored suite of every scientific solver;
- not a generic engineering consultancy billed primarily by the hour;
- not a cloud-compute reseller whose revenue rises with waste;
- not a chat interface over tools;
- not a workflow diagram marketed as scientific assurance;
- not a certification body, regulator, insurer, or substitute for one;
- not a favorable-answer machine; and
- not a claim that a technical verdict automatically supplies certification,
  regulatory acceptance, or every unstated practical requirement.

## Company fit

The strategy lets Avila Labs be a laboratory without pretending to be the
leading authority in every field. Avila investigates neglected computational
gaps, builds rigorous reusable mechanisms and creates the infrastructure
through which people and agents can explore more designs without weakening the
rules.

The company’s role is servant, mechanism builder, specification steward, and
operator of the trust boundary. That is a demanding technical role in its own
right. It requires humility about domain authority and unusual rigor about what
the software is allowed to claim.
