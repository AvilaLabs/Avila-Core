# Definitions

What we mean, precisely, when we say a word in this project. Where a word is
easy to overload, the entry also says what it is not. Terms that the semantic
rules govern cite their clause in [ADR-0006](adr/0006-semantic-core.md); terms
that describe the long-term vision are marked as such and are not yet backed
by executable rules.

## The product

**Avila Core (Core).** A neutral semantic and trust layer for computational
engineering. An organization states what it needs to establish as an evidence
contract; qualified capabilities contribute evidence; Core determines what
follows under explicit rules and returns a portable, independently reviewable
package. Core is not a solver, a workflow canvas, a marketplace of compute, or
a scientific authority. It never decides who deserves professional trust.

**Semantic profile.** The named, versioned set of rules under which records get
their meaning: canonical values, kinds and units, claim models, admission
conditions, and the verdict calculus. Today: `avila.core/semantic/0.2-draft`.
A verdict is meaningful only under the profile it names. The profile and any
kernel that implements it are versioned separately, so an archived package can
be replayed years later by a kernel that still implements its profile.

**Kernel.** The I/O-free authority that implements a semantic profile: exact
arithmetic, canonical JSON, predicate evaluation, and verdict derivation. Only
the kernel constructs admissions and verdicts. Interfaces, agents, and
providers propose; the kernel decides what follows.

**Compiler.** The pass that resolves an authored contract against one
immutable registry snapshot and either returns a content-identified compiled
snapshot or a set of findings. It performs no execution, admission, or
verdict. It is the point where errors move to compile time.

**Runner.** The only component that performs I/O beyond reading documents: it
re-hashes bytes at supplied roots, stages verified inputs into a fresh
workspace, runs exact executables through adapters, writes and verifies
receipts, reuses steps whose receipts still hold, and generates the claims
document before handing evaluation to the compiler and kernel, then
materializes exact content-identified requests for compiled review stages.

**Workbench.** The desktop application. It renders the runner's report and
the compiler's findings, requests plans and runs, and holds no semantics of
its own. Every badge and number on it is read from a report.

**Verifier.** Software that replays a package's checks without trusting the
producer: integrity, identity binding, admission, and verdict derivation,
reporting separately what it checked, what it recorded but did not
re-perform, what was unavailable, and what it refused (SC-17). The independent
verifier is a stated product boundary, not yet a separate implementation.

## Records and identity

**Canonical record.** A JSON document under the canonical profile (SC-2):
sorted keys, no binary floating point, exact decimal or rational strings,
absent optional fields rather than `null`, NFC strings, no duplicate keys.
Identity is the SHA-256 of the canonical bytes.

**Identity, digest.** `sha256:` followed by 64 lowercase hex digits. A
matching digest proves that two byte sequences are the same. It proves
nothing about whether the bytes are correct, qualified, or reviewed.

**Artifact.** Bytes plus a media type and an identity. An artifact becomes
evidence only when Core also knows who produced it, from which admitted
parents, under which method and policy, which validator accepted it, and
whether the required reviews admit it.

**Evidence.** An immutable, content-addressed record connected by typed
relationships to inputs, capabilities, executions, outputs, reviews, and
claims. Evidence is admitted, quarantined, missing, or invalidated; it is
never edited in place.

**Evidence package.** The portable export of a campaign: contract, registry
snapshot, inputs and their identities, plan, invocations, receipts, outputs,
claims, reviews, verdicts, and the boundary statements, arranged so an
independent verifier can replay every check it is able to. A package is valid
for what it verifies, never as a single all-clear.

## The contract language

**Evidence contract (contract).** The unit of work: a bounded question, the
requirements that resolve it, the assumptions accepted without being
established, the declared inputs, the workflow of steps, and the review
obligations. A contract is authored, compiled against a registry snapshot,
and then executed as campaigns. It is not a solver job and not a document
that summarizes results.

**Bounded question.** The question the contract exists to resolve, in prose
the compiler carries into the compiled boundary and never interprets. It
scopes what a verdict is about.

**Requirement.** One statement that must be established: a metric source, a
comparison, an exact limit with kind and unit, a basis, and a governed
purpose. It resolves to exactly one verdict. A requirement is not a design
goal or a preference; it is a testable condition with an owner.

**Assumption.** A condition accepted without being established by the
campaign, stated in prose and carried into every verdict's boundary. Typed
facts with provenance are a separate record.

**Input.** A declared artifact role the contract consumes, with its media
type and claim model. Inputs are attested by identity when a campaign runs.

**Step.** One application of a capability type inside the workflow, with
bindings from its input slots to contract inputs or other steps' outputs,
typed parameters, a reproducibility declaration, and, for review types, a
review binding.

**Binding.** The explicit resolution of an input slot to a single source.
Resolution is exact (SC-6 R1): an ambiguous slot is a finding, never a guess.

**Kind.** A governed quantity type with a canonical unit and an exact unit
class (SC-1). Equal dimensions do not make quantities comparable; absorbed
dose is not dose equivalent.

**Role.** A nominal evidence type: identity, version, quantity kind where
applicable, permitted claim models, media types, validator, and owner (SC-4).
Slots are satisfied by the same role identity, never by a similar name.

**Purpose.** A governed nominal identity for an intended use. An output that
excludes a purpose cannot serve a requirement declared for it (SC-6 R10).
Names and prose never imply purpose compatibility.

**Claim model.** How a claimed quantity's uncertainty is expressed: `exact`,
`interval`, `coverage_interval`, `worst_case`, or `unquantified` (SC-3). The
kernel reduces these and nothing else; statistical combination belongs to a
qualified capability.

**Basis.** How a requirement is compared: `bounded` uses the admitted
interval, `enclosure` requires an admitted enclosure claim, and `nominal`
compares a nominal value while visibly stating that uncertainty was not used.
A nominal basis needs explicit policy permission.

**Execution policy.** The contract's explicit weakenings: which
nondeterministic roles are permitted and whether a nominal basis is allowed.
Every weakening is visible in the affected verdicts.

**Review obligation.** A compiled, pending request binding a reviewer role, a
named external eligibility policy, an exact presented dossier,
governance-only dispositions, independence constraints, and any instructions
(SC-6 R9). An `accountable_person` obligation must be fulfilled before `PASS`
can be established. An `agent` obligation is a routing stage and never gates
or constructs a technical verdict. Compilation never fulfills either kind.

**Eligibility policy.** The external, organization-owned rule for who may
review, pinned by identity and digest. For an agent stage it binds the
permitted software identity and practical instructions; that does not turn
software into an eligible person. Core stores and binds the policy; it does
not decide that a person is qualified or that an agent's advice is correct.

**Staged agent review.** A compiled review stage before accountable human
review in which identified software receives an exact, content-identified
dossier and explicit practical instructions. It may recommend accountable
review, request changes, or abstain. It can never approve or reject a result
for use, fulfill an accountable-person obligation, or change a technical
verdict. Its current record is unsigned and unverified.

**Finding.** One compiler or admission result: a stable code, a class, an
owner, a JSON Pointer location, related locations, typed repair candidates,
and a message. Consumers match codes, classes, owners, and locations, never
wording.

**Finding class.** `missing`, `invalid`, `unsatisfied`, `inadmissible`, or
`notice`. The four blocking classes call for different people and actions
and are never collapsed to a single `error`.

**Repair.** A bounded edit the compiler can name, tagged `mechanically_safe`,
`constrained_choice`, or `method_owner_judgment`, in RFC 6902 patch form
where the exact bytes are known. The compiler never applies a repair itself.

## Capabilities, registry, and libraries

**Capability.** A versioned, provider-owned contribution with named input and
output slots, an explicit method and qualification boundary, and a receipt
for every execution. A capability may wrap software in any language, a data
access, a calculation, a physical measurement, or a professional review.

**Capability type.** The semantic signature of a capability: slots, roles,
permitted output claim models, typed parameters and domains, determinism
class, governed purpose exclusions, non-claims, and owner (SC-5). A type is a
contract, not an executable name.

**Capability package (package, implementation).** An exact implementation of
a capability type, identified by content digest: today an executable's SHA-256
plus source coordinates as annotations; eventually a signed manifest with
environment, validators, preflight facts, permissions, maturity, and
qualification evidence roots.

**Adapter.** The case-specific code that maps a compiled step's slots onto a
program's command line and layout, collects its declared outputs, and
extracts claims from them. An adapter maps and extracts; it does not
interpret results scientifically.

**Registry snapshot.** The immutable set of kinds, purposes, roles, and
capability types a contract is compiled against. Compilation and admission
always name the snapshot they used.

**Library.** A versioned, owned bundle of registry content and its
companions: kinds, roles, purposes, capability types, contract templates with
their requirement sets, qualification records and validation cases,
applicability predicates, review policies, and the adapters or packages that
implement the types, for one domain such as activated-metal disposition or
shielding margins. A library is what a domain professional publishes so that
others can compile contracts in that domain without reinventing its
vocabulary. Libraries may be open or commercial; either way every entry has
an owner and every claim in it carries its qualification state. A library is
not a pile of documents mined from the internet; it is curated content with
accountable owners.

**Requirement set.** A library's owned, versioned list of what any contract
in its domain must address: for each entry, the quantity kind, comparison,
weakest acceptable basis, and whether omitting it requires a stated reason.
Schema `avila.core/requirement-set/v0.1-draft`.

**Coverage.** The runner's assessment of a compiled contract against a
requirement set under the case's declaration: which contract requirements
cover each entry (and on what basis), which entries are omitted with a reason
and an accepting owner, which the set lets pass silently, and which are
unstated. An unstated omission, or coverage only on a basis weaker than the
set's minimum, makes coverage incomplete and stops the run before execution.
Coverage is a report about the contract, never a verdict about a design.

**Template.** A contract with typed parameters and domains, workflow,
requirements, policy floor, eligibility rules, validation cases, owner, and
version (SC-9). An instance pins the template digest and its parameters and
may tighten but never loosen the policy floor.

**Qualification.** A method owner's scoped, evidenced claim that a specific
package is fit for a context of use: predicates over parameters, inputs,
environment, and facts, with validation evidence, exclusions, uncertainty
limits, reviewers, and lifecycle (SC-7). Qualification is never a global
"verified" badge, and Core does not create it by storing it.

**Qualification record.** A method owner's document binding one adapter and
one exact executable to an applicability predicate (its envelope) over
facts, with the validation evidence it rests on and its limitations. Bound
by a case package as a `qualification` document. Schema
`avila.core/qualification/v0.1-draft`.

**Envelope.** The qualification record's scope evaluated over one run's
facts: `inside`, `outside`, or `unknown`, with each top-level term's result.
Recorded on the step and on every claim it produces. Outside or unknown
evidence cannot establish a bounded requirement.

**Applicability predicate.** A closed, non-Turing-complete expression over
parameters, input attributes, environment, and sourced facts that evaluates
to `true`, `false`, or `unknown`. `unknown` never counts as `true`.

**Fact.** A typed value with provenance: source class, source identity,
validator, and receipt reference. A runner signature proves a fact was
recorded, not that its provider had authority to assert it.

**Maturity, qualification, admission.** Three different things: maturity is
the provider's own implementation-state declaration; qualification is a
method owner's evidenced claim; admission is an organization's recorded
policy judgment for one contract. None implies another.

## Execution

**Case package.** The manifest of a composed case: hashed documents, external
artifacts bound to evidence identifiers under named source roots, the bound
capabilities, the executions declared for compiled steps, and limitations. It
is the bound plan of the case.

**Source root.** A named directory under which the case package's artifacts
are found and re-hashed. An omitted root is reported `not_checked`; a
supplied root whose bytes are missing or different fails closed.

**Workspace.** A fresh directory in which one step executes: staged verified
inputs at the layout the tool expects, an output directory, captured logs, and
the receipt. The runner never reuses a workspace.

**Invocation identity.** The digest of what was asked of a program before it
ran: capability digest, parameters, staged input identities, arguments,
environment, working directory, and timeout. It excludes results and
timestamps, so the same request always has the same identity.

**Execution receipt (receipt).** The record of one step execution: case and
snapshot identity, step, capability type, adapter, capability identity,
parameters, staged inputs by slot and digest, invocation and its identity,
process outcome, logs, produced outputs by digest, runner identity, and
limitations. A verified receipt proves that a named executable ran over
named bytes and produced named bytes. It proves nothing about scientific
correctness, qualification, review, or regulatory suitability.

**Reuse, execution memoization.** Skipping a step because its committed
receipt carries the same invocation identity, completed with exit status
zero, and every output it recorded still verifies at a bound identity
(SC-12.5). Reuse is exactly what a `deterministic` declaration permits and
nothing more; it does not catch a program that depends on undeclared state.

**Change class.** The typed reason a step must rerun: input bytes, input
binding, parameters, capability, invocation, or a missing, incomplete, or
output-less receipt. A rerun propagates by content: a consumer whose inputs
are byte-identical to what its receipt recorded stays reused.

**Free input.** A contract input the case package declares may vary without
re-freezing the package. Supplying one hashes and attests it, invalidates
every step it reaches, and makes replay against committed expectations not
applicable.

**Withheld evidence.** Committed claims of a step a supplied free input
reaches that did not run: absent by design, because the reference input's
results cannot speak for another input. Distinct from `not_checked`.

**Receipted evidence.** A fresh output of a step a free input reaches, bound
by the identity its receipt recorded rather than by a package-declared one.

**Required environment.** Keys an adapter requires the operator to value
because they locate content (a data-library index). Identity by name,
receipt provenance by value; the located content is bound as a staged input.

**Margin.** The exact distance from a verdict's decisive bound to its limit,
in the canonical unit: positive when satisfied, negative when not. Reported
beside every verdict that has numbers.

**Campaign log.** An append-only file of one JSON line per run: supplied
inputs and identities, step states, verdicts with numbers and margins, and
the campaign identity. The raw material of the engineering constellation.

**Plan.** A run that performs the change analysis and stops before executing.

**Fresh run.** A run with reuse disabled: every declared step executes in a
new workspace.

## Campaigns and verdicts

**Campaign.** One planned or executed run of a contract's workflow: the
attested inputs, the claims produced, the review decisions, the admissions,
and the verdicts, identified as a whole (SC-13).

**Attestation.** An identity asserted for a contract input or a recorded
output claim without the runner having produced it in this run. Attestations
are admitted at type level and are visibly weaker than receipted outputs.

**Claim.** A statement that an output artifact carries a value in one of the
claim models, attributed to a producer identity. Generated from executed or
reused outputs, or carried as an attestation for steps that did not run.

**Admission.** The decision that an artifact may enter the evidence graph
for a role at a step (SC-11): identity, admitted parents, presence, permitted
model and shape, media, cardinality, and eventually receipts, packages,
qualification, policy, reviews, and invalidation. States are `admitted`,
`quarantined`, or `missing`; quarantine is terminal and cascades to
descendants.

**Verdict.** One of exactly four states per requirement: `PASS`, `FAIL`,
`INCONCLUSIVE`, or `NOT_EVALUATED`, with the rule that decided it and the
complete boundary it holds under (SC-10). A verdict is a conditional
derivation from admitted records under named rules. It is never scientific
truth, certification, or approval by itself.

**PASS / FAIL / INCONCLUSIVE / NOT_EVALUATED.** Established within the
limit; established beyond the limit; admitted evidence that neither
establishes nor contradicts, such as an interval crossing the limit; nothing
admissible to decide on, including a pending required review. `PASS` requires
every required review; `FAIL` may be emitted with reviews outstanding.

**Boundary.** What a verdict carries with it: semantic profile, compiler and
evaluator identities, compiled snapshot and claims identities, review
attestation status, and the contract's assumptions. Outside its boundary a
verdict says nothing.

**Replay.** Re-deriving a committed expectation from the same records and
comparing: the generated claims against the committed claims document, a
fresh receipt against the committed receipt, the generated campaign report
against the committed report. Any drift is a rejected run and a reviewed
decision to re-freeze.

**Invalidation.** Marking evidence and verdicts as no longer current because
something they depended on changed: inputs, parameters, methods, data,
environment, qualification, policy, review, or a discovered defect (SC-12).
Default invalidation follows typed dependency edges and fails closed.

**Reuse rule.** An authorized, signed non-dependence claim with scope,
justification, validation evidence, and expiry, under which changed objects
are declared not to affect particular evidence. The kernel checks its
authority and applicability; it does not prove the non-dependence. "The files
look similar" is not a reuse rule.

## People and roles

**Requester.** Whoever poses the contract and owns its authored content.

**Executor.** Whoever runs a campaign and is accountable for the records it
produces, including attestations.

**Method owner.** The professional who owns a capability type or
qualification and answers for its applicability boundary.

**Capability provider.** Whoever publishes and supports an implementation.

**Reviewer.** A person eligible under the named external policy who examines
the exact presented dossier and records a governance disposition:
approve for use, reject for use, request changes, or abstain.

**Designer.** Whoever proposes work: a person, or an AI agent acting for one.
Designers author contracts, propose candidates, and iterate; they never
construct an admission or a verdict, and an agent never acts as the
accountable reviewer.

**Agent.** Software that authors and drives campaigns through Core's
interfaces. An agent is a designer, an executor, or a non-accountable
reviewer, always with a recorded identity, never a source of authority.

**Independence, separation of duties.** Explicit constraints that a reviewer
be a different person or organization from the requester, method owner,
provider, or executor, evaluated over signed role assertions, with waivers
recorded in every affected verdict.

## Vision terms

The following describe where the project is going. They are not yet backed
by executable rules and must not be read as claims about today's software.

**Engineering constellation (EC).** A machine-readable network of engineering
claims, requirements, evidence, methods, dependencies, and capabilities that
describes what must be established for an engineering objective, and how
those conclusions depend on one another. In Core's terms: the cross-campaign
accumulation of templates, requirement sets, kinds, roles, capability types,
qualification records, and typed dependencies, with every claim carrying its
provenance and qualification state. A constellation for one objective is
instantiated from libraries and grows as campaigns run against it. It is a
map of what has to be true and how each thing can be tested, never a store
of assertions without owners.

**Coverage.** How much of a constellation's requirement set a given contract
actually establishes, reported so a reviewer's question becomes "what is
missing from this list" rather than "is any of this right". Coverage does
not certify completeness; no system can.

**Iteration loop.** A designer, typically an agent, proposing many
candidates and running each as a campaign. Cheap screening methods may guide
the search; a requirement is `PASS` only when a qualified method establishes
it inside its validation envelope, with receipts. Admissibility precedes
optimization (SC-8): a good score never rescues an inadmissible candidate.

**Physical evidence.** A measurement, test, or inspection treated as a
capability whose receipt is the measurement record. This is how the loop
between computation and the physical world closes inside Core: experimental
results are evidence records like any other, with their own qualification.

**Optimizer.** Any process that chooses which candidates or methods to try
next, possibly learning from accumulated campaigns. It lives outside the
kernel and may consult evidence; it cannot construct evidence, admissions,
or verdicts.
