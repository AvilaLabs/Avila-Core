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

The concrete example is Tony Stark telling his connected AI agent, "build me
an Iron Man suit." The agent can run thousands of candidate designs through
Core until the compiler, evidence, qualification, coverage, and requirement
gates pass. Core may still lack an authored requirement for an obvious
practical problem—for example, placing a lithium-ion battery near the wearer's
crotch. If the user configures it, the connected agent therefore performs one
post-campaign practicality check with explicit instructions. It either returns
the candidate to iteration or presents it to the user. There is no mandatory
professional at the end, and Core also works without an AI integration.

## What must hold for the loop to mean anything

1. **The agent searches inside fixed, attributable requirements.** The
   requester owns and versions the contract and requirement set. The designer,
   human or agent, proposes candidates and parameters. An agent must not
   silently rewrite the requirements it is trying to satisfy; any authorized
   amendment is a new, visible contract identity.
2. **The last mile is qualified.** Evaluation cost bounds brute force: a
   transport calculation is minutes to hours. The loop therefore has two
   fidelities. Cheap screening methods guide thousands of candidates, but
   their claims cannot establish `PASS`; the qualified method runs on
   finalists and produces the bounded claim that can. Admissibility precedes
   optimization: a good score never rescues an inadmissible candidate.
3. **Coverage is safety, not decoration.** A search optimizes exactly what is
   written. A missing requirement produces an "optimal" design that is wrong
   for a reason nobody stated. Coverage against a library's requirement set,
   with every omission explicitly accepted by the requester, makes the search's
   optimum meaningful without inventing a universal reviewer role.
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
6. **Presentation is separate from technical truth.** Core finishes its
   verdict before any optional practicality stage. A configured connected
   agent can apply user-authored sanity checks and route the candidate back or
   forward, but its routing record is not a claim, qualification, approval, or
   verdict. Without that integration, Core simply returns its technical result
   and evidence package to the caller.

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

On one bounded domain, an agent given an objective and Core autonomously
produces a candidate that satisfies every stated technical gate, survives an
optional instructed practicality gate, and yields a package an independent
verifier reproduces. Core must refuse at least one agent shortcut along the
way. If that cannot be shown on something the size of a shielding
configuration search, the vision is off. If it can, the rest is scale.

## First domain

Shielding configuration search: materials and thicknesses against dose-rate,
mass, and thickness requirements, with a fast one-dimensional attenuation
screen as the unqualified capability and Monte Carlo transport as the
bounded one. The executable slices, in order: free inputs so a candidate can
vary without re-freezing the package; margins and machine-readable reasons
on every verdict; a campaign log per candidate; a scripted agent that drives
the two-fidelity loop; coverage against a library requirement set;
qualification envelopes; an optional agent presentation gate; then physical
  evidence.

## What has landed (2026-09-02)

These slices exist as [CASE-001](../../examples/cases/case-001-shield-search/README.md):

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

- **Qualification envelopes.** The transport capability carries a
  qualification record whose scope is a kernel predicate over facts the
  adapter reads from the verified source and candidate: energy range, plane
  geometry, at most three layers of listed materials, at most 120 cm. The
  runner evaluates it before the step runs, `--plan` already says
  `OUTSIDE` and names the term, and a bounded requirement whose evidence
  lies outside is `NOT_EVALUATED` even though transport ran. This is the
  "beyond a validation envelope, Core says so" rule, minus the validation
  evidence itself, which the first record honestly does not bind.

- **Optional presentation gate.** CASE-001 configures a connected-agent
  practicality capability over the exact reviewer implementation, candidate,
  screen result, and transport result. The compiler requires explicit
  instructions and permits only `present_to_user`, `request_changes`, or
  `abstain`. After evaluation the runner content-identifies the realized
  dossier. The gate never changes a Core verdict and is absent entirely when a
  caller does not configure it. The committed record demonstrates the return
  path: R2 is `FAIL`, so the agent sends the candidate back with that action;
  the unit tests also exercise the presentation path after all technical and
  practical checks pass.

Not yet: validation evidence behind an envelope, enforcement of
qualification for every bounded verdict, physical evidence as a capability,
and any optimizer or constellation view over the log. The workbench exposes free inputs,
environment values, margins, coverage, and presentation requests, but cannot
yet drive a search itself.

## What has landed since (2026-09-02, later the same day)

[CASE-002](../../examples/cases/case-002-coupled-shield/README.md) closes two
of CASE-001's stated omissions: transport is now coupled neutron-photon and
reports both dose rates, and each layer's tallied spectrum feeds ACTINV so
shield activation is evaluated. The case is the first pre-registered
experiment: its `PROTOCOL.md` fixes four arms (practice baselines, random
search, a learning designer, a control sweep), the measurements, and the
supported and falsifying outcomes before any campaign runs. Composing it
produced four refusals from Core, recorded in the case README; the first was
aimed at the case author, who had mapped a nominal activation guide as
coverage of a library entry that requires a bounded basis. A surrogate-assisted
designer (`examples/agents/shield_search2.py`), three textbook practice
baselines, and a control sweep exist as scripts. The campaign has not run.

## What has landed since (2026-09-03)

Three campaigns of [CASE-002](../../examples/cases/case-002-coupled-shield/README.md)
are on record in its `RESULTS.md`. The first found the box infeasible; the
second found it feasible and the scripted designers unable to reach the
design the record already pointed at; the third put a language-model
designer, reading every prior result through
`examples/agents/shield_llm_tools.py`, against a surrogate seeded with the
same data. The model designer found an all-`PASS` design 277 kg lighter than
the exhaustive sweep's best at its second transport, stopped by its own
judgment with most of its budget unspent, and its rationales are on file;
the surrogate needed eleven transports and stopped 150 kg heavier. An
adversarial arm then showed that every refusal held except against a
designer that rewrites the package, which is answered by the identity log
and the manifest pin (S-030). [CASE-003](../../examples/cases/case-003-thermal-spreader/README.md)
carries the mechanism into a second domain with the first qualification
record that binds validation evidence.

## What CASE-007 exposed (2026-09-04)

[CASE-007](../../examples/cases/case-007-fault-balanced-circuits/README.md)
showed both the value and the present boundary of the loop. Core fixed the
question and gates, refused a malformed checker input, bound the executable
and artifacts, admitted the repaired candidate, and reproduced the result.
But the checker had supplied the useful schema mismatch only in stderr, while
Core's normalized feedback said only that the process and receipt failed.
Runtime failures now produce a bounded, redacted `CORE-X2501` stderr finding
with the exact captured-log path, so a person or agent can act on the failure
without discovering Core's workspace layout first.

Core still did not generate the CASE-007 repair or preserve the failed first
candidate and the repaired candidate as one first-class search lineage. The
agent performed that reasoning outside Core and the final checker output
described it after the fact. The next generative-loop weakness is therefore
not another domain adapter: it is a native attempt record that relates a
candidate to its parent, changed variables, Core findings, verdict margins,
and resulting artifacts without allowing the designer to rewrite the fixed
contract or gates.
