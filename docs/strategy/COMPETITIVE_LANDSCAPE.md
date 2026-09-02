# Competitive landscape

**Research snapshot:** 2026-08-31. Product offerings change; verify the linked
primary sources before using this document for a commercial decision.

Core enters a crowded environment. Many organizations already solve parts of
workflow automation, simulation access, requirements integration, provenance,
model exchange, uncertainty, and review. The opportunity cannot rest on claiming
that these parts do not exist.

## Commercial engineering platforms

### Rescale

Rescale describes a digital engineering platform spanning cloud HPC, modeling
and simulation, workflow orchestration, data, and automated engineering. It
advertises a catalog of 1,250+ applications and customized platform editions,
infrastructure, support, and professional services.

Sources: [platform](https://rescale.com/platform/),
[pricing and offerings](https://rescale.com/pricing/).

**Implication for Core:** “bring engineering tools together on scalable compute”
is already a mature category. Core must differentiate at the evidence-contract,
neutral adjudication, independent-verification, and contract-economics layers.
It should interoperate with platforms like Rescale rather than assume it can
outbuild their infrastructure catalog.

### SimScale

SimScale offers cloud simulation across several physics domains, with free,
professional, and enterprise tiers. Its pricing page ties plans to analysis
coverage, private projects, computing quota, support, and higher-tier automated
engineering features.

Source: [plans and pricing](https://www.simscale.com/product/pricing/).

**Implication for Core:** browser-accessible simulation and easier solver use are
not sufficient differentiation. Core’s billable object should remain the
resolved contract rather than a subscription-defined simulation quota.

### Ansys ModelCenter

Ansys ModelCenter connects requirements to engineering analyses, automates
multi-tool workflows, executes user and third-party tools, supports trade
studies, and returns requirement-conformance information to model-based systems
engineering workflows.

Source: [Ansys ModelCenter](https://ansys.synopsys.com/en-gb/products/connect/ansys-modelcenter).

**Implication for Core:** neither a drag-and-drop workflow nor “requirements plus
simulation” is novel. Core must prove provider-neutral qualification, explicit
uncertainty and inconclusive semantics, portable evidence, and an economic market
for independent capabilities.

## Research workflow and provenance systems

### AiiDA and Common Workflow Language

AiiDA is an open computational-science infrastructure centered on automated
workflows, data provenance, and reproducibility. The Common Workflow Language
defines portable, reusable descriptions for command-line dataflow workflows and
explicit inputs, outputs, dependencies, and execution models.

Sources: [AiiDA](https://aiida.net/),
[CWL features](https://www.commonwl.org/features/).

**Implication for Core:** workflow execution and provenance should reuse or map
to established ideas. Core’s additional object is the evidence contract and its
policy-governed path to a requirement verdict. Interoperability should be
preferred over inventing an incompatible general workflow language.

## Domain-specific nuclear platforms

The U.S. Department of Energy’s NEAMS program develops individual and coupled
nuclear modeling tools, common multiphysics infrastructure, qualified benchmark
datasets, workflow support, and the NEAMS Workbench. Its scope includes
neutronics, fuel performance, thermal fluids, materials, chemistry, and
uncertainty-informed predictive capability.

Source: [NEAMS technical areas](https://neams.inl.gov/technical-areas/).

**Implication for Core:** Avila should not claim to originate the nuclear
multiphysics platform category. A credible wedge must address a concrete
cross-tool evidence and verification problem that domain programs and existing
workbenches do not already resolve satisfactorily for the chosen user.

## Model and workflow interchange standards

The Functional Mock-up Interface defines a free container and interface for
exchanging dynamic simulation models and is supported by hundreds of tools. W3C
PROV provides a family of provenance specifications, while RO-Crate specifies a
way to package research data and contextual metadata.

Sources: [FMI](https://fmi-standard.org/),
[W3C PROV](https://www.w3.org/TR/prov-overview/),
[RO-Crate](https://www.researchobject.org/ro-crate/specification.html).

**Implication for Core:** portability requires semantic mappings and adoption,
not merely publishing a new JSON file. Core should profile or reuse established
standards where they fit and document every intentional incompatibility.

## Computational credibility and assurance

Regulators and assurance practitioners already treat credibility as more than a
successful execution. For example, FDA’s 2023 guidance recommends a risk-informed
credibility-assessment framework for physics-based and mechanistic computational
models used in medical-device submissions.

Source: [FDA computational modeling and simulation credibility guidance](https://www.fda.gov/regulatory-information/search-fda-guidance-documents/assessing-credibility-computational-modeling-and-simulation-medical-device-submissions).

**Implication for Core:** hashes and provenance are necessary but not sufficient.
Credibility depends on context of use, risk, validation, uncertainty, and
applicability. Core may represent organization-specific assurance processes
without making them universal verdict gates or claiming that its schema creates
regulatory acceptance.

## Competitive conclusion

Core’s open space is not “all engineering tools in one platform.” It is a
specific combination that still requires proof:

1. begin with an evidence contract rather than a tool;
2. route it across qualified, potentially competing providers;
3. pay for admissible resolution rather than a favorable answer or consumed
   compute;
4. preserve complete, portable, independently inspectable evidence;
5. treat `INCONCLUSIVE` and invalidation as first-class outcomes; and
6. let evidence consumers and attributable method owners govern trust without
   making a professional reviewer mandatory.

Every element exists somewhere. The strategic question is whether this complete
mechanism solves a sufficiently expensive recurring problem and whether Avila’s
economic commitments make the whole difficult for existing vendors to copy.
