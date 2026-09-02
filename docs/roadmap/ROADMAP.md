# Roadmap to 1.0

This is a sequence of evidence gates, not a promised release calendar. Calendar
ranges assume a very small team and may expand materially as scientific,
security, legal, and partner requirements become clear. Releases should occur
along the way; version 1.0 is the long-term threshold described in the product
definition.

## Stage 0 — foundation and discovery

**Planning range:** now through approximately month 3–4.

Current gate evidence and unresolved work are maintained in the
[Stage 0 status ledger](STAGE_0_STATUS.md). Executable fixture counts come from
`avila-core semantic-profile`; the ledger interprets what those counts do and
do not establish.

Build:

- authoritative contract, capability, plan, evidence, and verdict models;
- normative semantic profile covering canonical values, quantity kinds, roles,
  facts, policy, admission, verdicts, and reuse;
- schemas, adversarial conformance fixtures, canonical issue reporting, and CLI
  validation;
- an intellectual-property and prior-art review before publishing enabling
  details of mechanisms that may warrant protection;
- question-first egui workbench;
- deterministic planning and explicit blocked states;
- capability protocol threat model and conformance design;
- evidence-package spike mapped to provenance/package standards; and
- interview materials and a concrete first-contract workflow map, maintained in
  the [first-pilot discovery packet](../discovery/FIRST_PILOT_PACKET.md).

Validate:

- 15–25 interviews across requesters, method owners, reviewers, and buyers;
- at least three detailed examples of an existing painful decision loop;
- current cycle time, labor, compute, review, and defect/rework baseline; and
- legal and license feasibility for candidate external tools.

Exit gate:

> One organization commits real people, representative non-sensitive data, a
> method owner, and a reviewer to a bounded design-partner pilot.

ADR-0006 remains proposed until its written semantics and fixture coverage pass
the acceptance conditions in that decision. A draft profile is not a scientific
qualification.

The current repository remains in this stage. Its draft kernel executes 90 pure
vectors, its static compiler executes 68 fixtures through R10, and its campaign
harness executes 12 fixtures over the first SC-10/SC-11 slice. That slice
performs type-level claim admission and verdict derivation. It does not perform
full package-level evidence admission: the evaluator reads no bytes;
artifact-byte and receipt verification exist only in the case runner for the
steps a case declares; package binding beyond an executable digest,
signatures, qualification and eligibility-policy evaluation, review
fulfillment, and invalidation remain absent.

The repository also carries
[CASE-000](../../examples/cases/case-000-actinv-aftermatter/README.md), a
synthetic ACTINV 1.0.1 → Aftermatter R0 composition. It is an internal pressure
test of the current boundary, not Stage 1: the case runner verifies every
declared byte, executes both computational steps through case-specific
adapters under verified execution receipts (ACTINV through Aftermatter's
frozen R0 builder, then Aftermatter over the fresh inventory), and generates
every claim from the fresh results (ADR-0007), while both requirements remain
`NOT_EVALUATED` because qualified review is absent.

## Stage 1 — vertical reference prototype

**Planning range:** approximately months 2–6.

Stage 1 implementation begins only after the Stage 0 exit gate identifies the
organization, bounded question, method owner, reviewer, representative data,
and deployment constraints. The selected vertical determines which runner,
adapter, receipt, and package surfaces are built first.

Build only the chosen evidence chain:

- canonical semantic kernel and contract compiler for the selected question;
- contract editor and semantic preflight over that same headless semantic
  kernel;
- controlled local runner with sandbox strategy;
- two or more real adapters needed for the chain;
- immutable artifact store and execution receipts;
- requirement evaluator with four-state verdict semantics;
- initial human and machine evidence export; and
- reference cases, negative cases, and fault injection.

Do not call the workflow qualified. Experimental adapters remain visibly
experimental.

Exit gate:

> The reference campaign reproduces agreed reference results, rejects known bad
> cases, preserves complete lineage, and can be reviewed end to end by the named
> domain professional.

## Stage 2 — useful design-partner alpha

**Planning range:** approximately months 5–12.

Build:

- organization-scoped identities and externally governed eligibility policies
  for requester, method owner, provider, and reviewer;
- immutable plan attestations and review records bound to the exact compiled
  dossier, with technical verdict and governance disposition kept separate;
- private capability registry snapshot;
- resumable campaigns, resource controls, and honest estimates;
- dependency-aware invalidation and selective reruns;
- package viewer and first independent verifier;
- on-premises deployment path; and
- operational logging, backups, update policy, and incident basics.

Commercial proof:

- complete at least one paid pilot;
- expose Avila labor and third-party cost explicitly;
- measure the workflow against the customer’s baseline; and
- obtain the reviewer’s written assessment of evidence usefulness.

Exit gate:

> A design partner uses Core on a real non-safety-critical decision and chooses
> to repeat the contract because measured total cycle time or cost improved
> without reducing required reviewability.

## Stage 3 — repeatable platform beta

**Planning range:** approximately months 10–24.

Build:

- stable capability SDK, compatibility rules, and conformance suite;
- a second independent implementation that reproduces normative semantic
  vectors without sharing the production kernel;
- governed contract templates and migrations;
- signed capability packages, receipts, reviews, and package roots;
- organization policy engine and separation of duties;
- private multi-organization registry and provider routing;
- a second implementation for at least one capability type;
- controlled remote/HPC backend in addition to local execution;
- cost estimation, provider metering, and settlement pilot;
- security assessment and adversarial package/runner tests; and
- documentation sufficient for a provider to integrate without Avila writing
  its adapter.

Commercial proof:

- sell the same contract class to a second organization;
- sell enterprise governance separately from custom services;
- pay one external provider through the contract economics; and
- demonstrate declining Avila labor per repeat contract.

Exit gate:

> An external provider publishes a conforming capability, an organization admits
> it through policy, and an independent reviewer verifies the resulting package
> without founder intervention.

## Stage 4 — narrow production 1.0

**Planning range:** approximately months 18–36 or longer.

Release only for a named domain, contract class, and deployment boundary.

Required product gates:

- every requirement in [Product definition](../product/PRODUCT_DEFINITION.md) is
  satisfied for the supported scope;
- the schemas and protocol have compatibility and migration commitments;
- runner isolation and evidence integrity have independent security review;
- qualification and validation packages have professional approval;
- offline verification and disaster recovery are tested;
- support, vulnerability, update, retention, and deprecation policies operate;
- customer and provider terms define evidence ownership, responsibility, and
  limits without implying certification; and
- at least two organizations have repeated the supported contract.

Required outcome gates:

- material median reduction in question-to-reviewed-answer cycle time;
- no loss of required evidence compared with the governing baseline;
- selective reruns demonstrate correct reuse after representative changes;
- reviewer clarification loops decline rather than move elsewhere;
- economics work without unpriced founder labor or compute waste; and
- users can exit Avila while retaining independently verifiable evidence.

## Stage 5 — network expansion

**Planning range:** several years; not part of 1.0.

Possible work:

- public or federated capability discovery;
- standardized commercial terms and provider settlement;
- cross-organization reputation grounded in reproducibility and service evidence;
- independent countersignature markets;
- additional contract domains; and
- recognition by major evidence consumers.

Expansion follows supply and demand in proven contract types. Core should not
launch an empty marketplace or claim a universal engineering platform.

## Release naming before 1.0

- `0.1`: contract and planner scaffold;
- `0.2`: accepted semantic profile, compiler/checker, and normative fixtures;
- `0.3`: controlled runner and artifact receipts;
- `0.4`: first experimental end-to-end reference campaign;
- `0.5`: evidence package and offline verifier;
- `0.6`: design-partner alpha and review workflow;
- later `0.x`: invalidation, SDK, qualification, security, enterprise policy,
  remote execution, and provider routing as they pass gates.

These labels are illustrative. Scientific maturity must be shown separately from
software version.
