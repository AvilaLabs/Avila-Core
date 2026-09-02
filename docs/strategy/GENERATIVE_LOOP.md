# The generative loop

**Status:** stated intent of the founder, recorded 2026-09-02; the executable
slices that serve it are listed at the end. Read this before proposing work
on Core, and read [DEFINITIONS.md](../DEFINITIONS.md) for the words.

## What Core is for, in the founder's words

Core is not meant to be only a verifier of work people already do. It is
meant to be the engine by which incredibly difficult engineering problems
are, in effect, brute forced: an agent iterates through candidate designs,
a compiler tells it quickly and exactly what fails and why, every run is
logged as part of an engineering constellation so the map of how variables
and requirements relate grows over time, and the designer, human or AI, uses
that map to search further. The hope is that this combination, a rigorous
evidence compiler with an iterating agent, can discover designs in complex
areas that have not received a solution because nobody has combined the two
before. Core is not at that vision yet; it is where Core is going.

The example given: a designer with an AI agent says "here is what I want";
the agent, working within Core, runs through thousands of iterations until
one satisfies every condition; before the result reaches the designer, a
staged review with specific practical instructions can send it back; and a
person makes the accountable review at the end.

## What must hold for the loop to mean anything

1. **The agent searches inside requirements it does not own.** The
   requester owns the contract and its requirement set, and a person has
   reviewed it. The designer, human or agent, proposes candidates and
   parameters. An agent that writes the requirements it then satisfies moves
   the goalposts; Core's roles exist to prevent that.
2. **The last mile is qualified.** Evaluation cost bounds brute force: a
   transport calculation is minutes to hours. The loop therefore has two
   fidelities. Cheap screening methods guide thousands of candidates, but
   their claims cannot establish `PASS`; the qualified method runs on
   finalists and produces the bounded claim that can. Admissibility precedes
   optimization: a good score never rescues an inadmissible candidate.
3. **Coverage is safety, not decoration.** A search optimizes exactly what is
   written. A missing requirement produces an "optimal" design that is wrong
   for a reason nobody stated. Coverage against a library's requirement set,
   reviewed by a person, is what makes the search's optimum meaningful.
4. **Beyond a validation envelope, Core says so.** Where no qualified method
   exists, a requirement stays `NOT_EVALUATED` with the missing capability
   named. For a search that is a result, not a dead end: it names the
   experiment that would establish the claim, and physical evidence enters
   Core as a capability like any other.
5. **Every iteration is a record.** Each candidate's parameters, verdicts,
   margins, and receipts are appended to the campaign log. That log is the
   raw material of the engineering constellation and of any optimizer that
   learns from it. Reuse keeps the cost of a one-parameter change at the
   steps it reaches.

## Why this is not fantasy, and where the honest limits are

Systematic search with a strict oracle has found designs before, wherever the
space is large and the constraints are many and coupled, which is exactly
where people struggle. What is new here is that the oracle is a typed,
evidence-producing pipeline whose failures come back as structured findings
an agent can act on without re-reading, and that the history of every run is
kept in a form the next search can use. What does not change: Core makes no
calculation physical; a method is trusted only inside the envelope its
validation evidence covers, and outside it Core's answer is the experiment
that is missing. Novelty comes from combinations nobody checked rigorously,
not from computation escaping physics.

## The falsifiable milestone

On one bounded domain, an agent given an objective and Core produces a
candidate whose evidence package a domain reviewer accepts without redoing
the work, and Core refuses at least one agent shortcut along the way. If that
cannot be shown on something the size of a shielding configuration search,
the vision is off. If it can, the rest is scale.

## First domain

Shielding configuration search: materials and thicknesses against dose-rate,
mass, and thickness requirements, with a fast one-dimensional attenuation
screen as the unqualified capability and Monte Carlo transport as the
bounded one. The executable slices, in order: free inputs so a candidate can
vary without re-freezing the package; margins and machine-readable reasons
on every verdict; a campaign log per candidate; a scripted agent that drives
the two-fidelity loop; coverage against a library requirement set; then
qualification envelopes, physical evidence, and staged agent review.

## What has landed (2026-09-02)

The first four slices exist as [CASE-001](../../examples/cases/case-001-shield-search/README.md):

- **Free inputs.** A package may declare inputs free; `run --input NAME=PATH`
  supplies one, hashes it, and attests it in the claims. Every step the
  supplied input reaches is invalidated: its committed claims are withheld,
  it reruns if its executable is supplied and is reported `not_run` with the
  claims withheld otherwise, and its fresh outputs bind by receipt rather
  than by a package-declared identity. Replay against committed expectations
  is reported `not applicable` rather than as a mismatch.
- **Margins.** Every verdict with numbers carries the exact distance to the
  limit on the decisive bound (upper for `≤`, lower for `≥`), in the kernel's
  canonical unit; the human summary rounds, the report does not.
- **Campaign log.** `--log FILE` appends one JSON line per run: supplied
  inputs and their identities, step states, every verdict with its numbers
  and margin, and the campaign identity. That file is the raw material of the
  engineering constellation; nothing reads it yet.
- **Scripted designer.** `examples/agents/shield_search.py` proposes layered
  slabs, screens them, and sends the feasible candidates with the most screen
  margin to transport. It reads reports and cannot construct a verdict.
- **Two fidelities in one contract.** The screen's claim is `unquantified`
  and can only satisfy the requirement whose basis the contract weakened to
  nominal; the transport claim is a coverage interval and satisfies the
  bounded one. The reference candidate passes the first and fails the
  second, which is the loop's point.

The first search (200 candidates, 3 distinct finalists) found nothing that
passes the bounded requirement: two `INCONCLUSIVE` intervals crossing the
limit and one `FAIL`, with the screen about three times optimistic. That is
a result, recorded in the case, not a failure of the loop: the oracle said
exactly what fell short and by how much, and the log holds every candidate.

- **Coverage against a library requirement set.** The shielding library's
  first requirement set names seven things any slab-shield search must
  address. CASE-001 covers three, omits three with a stated reason and an
  accepting owner (photon dose, activation, streaming paths), and one is
  omissible. The runner assesses this after compiling and before executing;
  an unstated omission stops the run, and coverage on a weaker basis than
  the set's minimum (the screen's nominal basis, for example) does not count.
  The search's optimum is now qualified by what it did not ask.

Not yet: qualification envelopes, physical evidence as a capability, staged
agent review, and any optimizer or constellation view over the log. The
workbench exposes free inputs, environment values, margins, and coverage,
but cannot yet drive a search itself.
