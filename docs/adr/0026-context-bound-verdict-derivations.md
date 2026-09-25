# ADR-0026: Context-bound observations, checked admissions, and replayable verdict derivations

- Status: accepted 2026-09-24; implemented for the campaign evaluation path
- Implements: RA-01 through RA-06 of `docs/roadmap/RUST_ARCHITECTURE_HANDOFF.md`
- Develops: ADR-0006 (SC-10/SC-11), ADR-0007, ADR-0008, ADR-0018; the
  Rust-informed architecture proposal dated 2026-09-24.

## Context

The compiler already compiles authored documents into a checked IR, admits
claims against a compiled snapshot under the SC-11 type-level rules, and
derives the four-state kernel verdict. The evidence crate already verifies a
case package's manifest, documents, artifacts, and receipts byte-for-byte.
The runner already binds supplied inputs, qualification envelopes, and reuse
rules to a run. What the current representation does not do:

- `CompiledContract` is ordinary data with public mutable fields. Any client
  can construct, mutate, or deserialize one and present it where a compiled
  result is expected.
- `verify_case_package` returns a `VerifiedCasePackage` whose integrity may
  be `partial` or `failed`: the type name asserts more than the value
  guarantees.
- `evaluate_campaign_with_artifacts` takes a `BTreeSet<String>` of digests —
  a caller-supplied assertion that hashing occurred, not evidence that it
  did.
- Admission and verdicts are computed, but the rule applications that
  produced them are discarded. A report records outcomes, not the premises
  they followed from, so a changed input or a tampered record cannot be
  answered except by re-running everything.
- No identity distinguishes two evaluations of the same documents under
  different supplied observations or context material.

This ADR defines the slice that fixes those boundaries in place: one
synthetic conformance workflow exercised end to end, with each premise
established by an existing rule and each checked value represented so only
the boundary that performed the check can produce it.

## The slice

The conformance slice is the existing fixture family under
`fixtures/semantic-core/`:

- contract `types.R1.resolved.pass.contract.json` — a parent step
  `calculate` (`fixture.calculate_dose@1`, unquantified nominal output) and a
  child step `bound` (`fixture.bound_dose@1`, interval output) over registry
  `compiler.registry.v1.json`;
- requirement `R-001` — `bounded_dose_rate ≤ 100 uSv/h`, bounded basis,
  canonical limit `1/36000000 Sv/s`;
- claims `campaign.le.within.pass.claims.json` and siblings — an interval
  claim `[90, 99] uSv/h` on `bound`, plus the existing negative family
  (duplicate claim, inverted bounds, undeclared slot, unknown input,
  model mismatch, parent missing, snapshot mismatch, every qualification
  lifecycle state, `require_qualification` on and off);
- `types.R1.resolved.require-qualification.contract.json` exercises the
  `require_qualification` policy over the same workflow;
- receipts and the runner's qualification binding are exercised by the
  existing runner test harness with synthetic executables.

Baseline identities captured through the current operations (compiler
`avila.core/compiler-rust@0.1.0`, profile `avila.core/semantic/0.2-draft`):

| Record | Identity |
| --- | --- |
| compiled snapshot (pass contract) | `sha256:cc802164995a2a326855de6414b175e2fb750072711fcebc175957540904bd3d` |
| contract bytes (canonical) | `sha256:db6d2b9317abc3e205a4f269b92810a6308fc8146b602190a86bfb15b50b2ed3` |
| registry bytes (canonical) | `sha256:d1d52d46cafb7d0930a43beef9da6a312ca78dcaeb19ced0405584e91632c0e7` |
| claims `le.within.pass` (canonical) | `sha256:074a0c0c71cbacd11fa436431c1ee2a5a74862285df2e97e71a82d1e1cf46bf2` |
| campaign report, digest-only | `sha256:2ba7e525f4f55c8c437b387311eaf94a505bde1d14532eb431b30a703dea43ed` |
| campaign report, one artifact supplied | `sha256:955ecc501a4ad3dab06e96e25f27096185e4b6a4b85dec375198eee03d36d178` |

Representative behavior to preserve: verdict `pass` under rule
`bounded.le.within` with canonical values `lower 1/40000000`,
`upper 11/400000000`, `limit 1/36000000` in `Sv/s`; rejection `CORE-E7001`
for a claims document naming another snapshot; `not_evaluated` for missing
parents and refused qualifications; incomplete drafts continue to compile
with unresolved parameters. The campaign fixture suite pins all 25 outcomes.

## Decision

### 1. Checked values are distinct from their serializable reports

A checked semantic value is one the checking boundary produced. Rust
enforces the boundary: private fields, no `Deserialize`, no `Default`, and
no constructor outside the owning crate. Reports and documents stay plain
serializable data — they may be read and printed anywhere, but no
authoritative API accepts them as a checked witness.

| Checked value | Producing boundary | What it proves |
| --- | --- | --- |
| `CheckedCompilation` / `CompiledContract` | `compile_documents*` (compiler) | The named contract+registry bytes passed every compile pass; snapshot identity is computed over the lowered IR. |
| `CanonicalTypedQuantity` | compiler lowering | A quantity that scaled exactly in its declared kind; the value is an `ExactNumber`, not a string to re-parse. |
| `VerifiedCasePackage` | `verify_case_package` (evidence) | The manifest's documents hashed to their declared digests; artifact checks are recorded per outcome. |
| `ArtifactObservations` | caller-side hashing at `check_bytes` | Every recorded digest was produced by hashing supplied bytes this call. |
| `EvaluationContext` | `EvaluationContext::bind` (compiler) | A named set of bound identities: compiled snapshot, registry, claims, policy, qualification material, profile, evaluator. |
| `CheckedAdmissions` | `EvaluationContext::admit` | The SC-11 admission rules ran under one bound context. |
| `VerdictDerivation` (constructed) | `evaluate_campaign_in_context` | The admission and verdict rule applications were recorded against one context identity. |

#### Compilation outcome

`compile_documents` and `compile_documents_with_material` return a closed
outcome:

```rust
pub enum Compilation {
    Compiled(CheckedCompilation),
    Rejected(CompileReport),
}
```

`CheckedCompilation` wraps the `CompileReport` and upholds the invariant
`status == Compiled && compiled.is_some()`; `contract()` yields the checked
`CompiledContract`. `Compilation::report()`/`into_report()` recover the
wire report for printing and storage, so the existing report bytes —
including the embedded `compiled` snapshot — are unchanged.

`CompiledContract`'s fields become crate-private; public accessors return
shared references and slices. It remains `Clone` (cloning a checked value
cannot weaken it — it is immutable) but cannot be constructed, deserialized,
or mutated outside the compiler. The nested IR records (`CompiledStep`,
`CompiledRequirement`, `ResolvedBinding`, …) keep their public fields:
they are reachable only by shared reference from a checked contract and no
API accepts them detached, so fabrication buys nothing.

`CanonicalTypedQuantity` stores `value: ExactNumber` (it already serializes
as the canonical rational string, so compiled snapshot identities and every
pinned fixture hash are preserved). `CompiledParameterValue::ExactNumber`
and `::Quantity` retain `ExactNumber` values for the same reason. Verdict
evaluation consumes the `ExactNumber` directly instead of re-parsing the
canonical string.

#### Package verification outcome

`verify_case_package` returns:

```rust
pub enum PackageVerification {
    Complete(VerifiedCasePackage),
    Partial(VerifiedCasePackage),
    Failed(VerifiedCasePackage),
}
```

`Failed` still carries the package so callers can report what failed and
verify the manifest signature; it must be matched explicitly — there is no
path that treats a failed or partial package as `Complete`. The runner
keeps its current gate: `Failed` stops the run before compilation or
execution, `Partial` runs with `not_checked` artifact records.

`VerifiedCasePackage`'s `manifest` and `integrity` fields become private
with `manifest()`/`integrity()` accessors. The one legitimate mutation —
substituting a supplied free input's bytes — becomes
`VerifiedCasePackage::supply_free_input`, which canonicalizes the path and
hashes the bytes itself, so the `supplied` provenance and `verified` check
state can only be recorded for bytes that were actually hashed.

### 2. An immutable evaluation context binds every authoritative input

`EvaluationContext` is created only by `EvaluationContext::bind`, which
requires:

- a checked `CompiledContract` (moved into the context — evaluation can
  only ever read the contract the context was bound to);
- the registry bytes, which must hash to the contract's recorded
  `registry_sha256` (`ContextError::RegistryMismatch` otherwise — this is
  the mixed-context refusal at bind time);
- the claims bytes, parsed authoritatively and hashed (`claims_sha256`);
- `ArtifactObservations`, the byte-check witnesses for this evaluation.

The context record is:

```json
{
  "schema_version": "avila.core/evaluation-context/v0.1-draft",
  "semantic_profile": "avila.core/semantic/0.2-draft",
  "evaluator": "avila.core/kernel-rust@<version>",
  "compiled_snapshot_sha256": "sha256:…",
  "registry_id": "…", "registry_revision": 1, "registry_sha256": "sha256:…",
  "claims_sha256": "sha256:…",
  "execution_policy": { …compiled policy… },
  "qualifications": ["<qualification_id>@<revision>:<sha256>", …]
}
```

`context_sha256` is the canonical SHA-256 of that record. `qualifications`
lists the sorted identities of every `ClaimQualification` attached to the
bound claims — the revocation/supersession/expiry facts already riding the
claims, made visible as bound material. There is no ambient clock: the
profile's evaluation-time rule uses the signed receipt instant already
carried on each claim's qualification record, so the context binds no time
of its own. Artifact observations are bound by inclusion: the context owns
the `ArtifactObservations` set, and every `admission.*` application records
the check outcome for its artifact.

`ArtifactObservations` replaces the `BTreeSet<String>` parameter of
`evaluate_campaign_with_artifacts`. Its only constructor of a verified
digest is `check_bytes(&mut self, bytes) -> String`, which hashes the bytes
and records the digest it computed. An artifact attested in the claims but
never observed hashes to `not_checked`; a digest the supplied bytes did not
produce cannot be recorded at all. An empty set is the digest-only
evaluation — semantically unchanged: no `artifact` field is emitted, exactly
as today.

Context equality is by `context_sha256`, never by pointer or lifetime. Two
contexts may be simultaneously live; `derive_verdicts` refuses
`CheckedAdmissions` minted under a different context identity
(`EvaluationError::ContextMismatch`, surfaced as `CORE-E7401`).

### 3. Admission and verdict emit explicit rule applications

Admission returns `CheckedAdmissions` — opaque records plus the rule
applications that produced them. Verdict derivation returns
`DerivedVerdicts`. Both are constructed only inside the campaign
evaluation; each carries its `context_sha256`.

Each rule application records:

```json
{
  "rule": "admission.claim",
  "subject": "bounded",
  "premises": [
    {"kind": "workflow_step", "id": "bound", "state": "declared"},
    {"kind": "output_slot", "id": "bound.bounded_dose_rate", "state": "declared"},
    {"kind": "slot_cardinality", "id": "bound.bounded_dose_rate", "state": "unique"},
    {"kind": "parent_admission", "id": "calculate.dose_rate", "state": "admitted"},
    {"kind": "artifact_identity", "id": "sha256:abab…", "state": "well_formed"},
    {"kind": "artifact_observation", "id": "sha256:abab…", "state": "not_checked"},
    {"kind": "media_type", "id": "application/vnd.fixture.quantity+json", "state": "match"},
    {"kind": "claim_model", "id": "interval", "state": "permitted"},
    {"kind": "claim_shape", "id": "bounded", "state": "valid"}
  ],
  "conclusion": "admitted",
  "reasons": []
}
```

Rules recorded: `admission.input`, `admission.claim` (conclusion
`admitted`/`quarantined`/`missing`, with one premise per check actually
performed — missing or contradicted premises are recorded with their
refusal state, not omitted), `qualification.envelope` per admitted claim
carrying qualification material (conclusion `inside`/`outside`/`unknown`/
`expired`/`not_recognized`/`revoked`/`superseded`), `qualification.required`
per requirement whose evidence lacked a required assessment, and one
application per requirement whose `rule` is the verdict rule the kernel
reported (`bounded.le.within`, `not_evaluated.no_admitted_evidence`, the
categorical rules, …) with `conclusion` the four-state status. Premise
kinds name the check class, never free text: `contract_input`,
`input_attestation`, `artifact_identity`, `artifact_observation`,
`media_type`, `slot_cardinality`, `parent_admission`, `claim_model`,
`claim_shape`, `unit_scaling`, `partial_result`, `workflow_step`,
`output_slot`, `qualification`, `execution_policy`, `requirement`,
`evidence`, `canonical_value`, `evaluation_context`, `report`.

Record size is bounded: `MAX_RULE_APPLICATIONS` caps the applications one
evaluation may emit; exceeding it is a bounded refusal — the campaign
report is `rejected` with `CORE-E7501` and the derivation carries a
`campaign.bound` application with conclusion `refused`. No rule can loop:
the application list is linear in the bound inputs, claims, and
requirements.

### 4. The derivation is a separate optional artifact

`VerdictDerivation` (`avila.core/verdict-derivation/v0.1-draft`) carries the
context record, `context_sha256`, the ordered `applications`, the
`campaign_sha256` it belongs to, and its own `derivation_sha256` over the
canonical body (the `derivation_sha256` field itself excluded — the same
self-excluding convention the campaign report uses). It is emitted beside
`campaign-report.json`, never inside it: existing canonical reports, signed
packages, and claims documents acquire no new bytes.

The shared operation is
`evaluate_campaign_in_context(contract, registry, claims, observations)`,
which compiles, binds, admits, derives, and returns a `CampaignEvaluation`
carrying the `CampaignReport`, the `EvaluationContext`, and the
`VerdictDerivation`. `evaluate_campaign` keeps its signature and delegates
with `ArtifactObservations::none()`; `evaluate_campaign_with_artifacts` is
replaced by the in-context operation (its digest-set parameter is exactly
the unforgeable-witness problem). The CLI gains `evaluate --derivation FILE`
to write the artifact and `derivation diff BEFORE AFTER` to explain changed
uses; the runner evaluates in context and writes `derivation.json` into the
run workspace beside `campaign-report.json`. Unsupported schema versions are
a mismatch, never a silent read.

The runner evaluates with `ArtifactObservations::none()`: the campaign
report it writes stays byte-identical to the committed replay comparison
(`replay_expected` is full-document equality), and the package-integrity
boundary already records which artifact bytes the run hashed. Supplying
real observations changes the emitted `artifact` check fields and the
`campaign_sha256`; enabling that inside `run` is a separate versioned
decision about the replay comparison, not part of this slice.

### 5. Independent replay and changed-use explanation

The Python verifier gains a `verify-derivation` subcommand, a stdlib-only
replay: it recomputes the context identity from the embedded record,
re-checks every bound digest against the supplied documents, replays each
supported rule application from the bound contract/registry/claims (the
same rules `verify_case_verdicts` re-derives), and compares conclusions
and premise states — a tampered conclusion is rejected even when the outer
`derivation_sha256` is honestly recomputed, because the replayed inference
does not produce it. `--artifact` supplies the byte-check witnesses whose
digests the context binds; `--campaign-report` recomputes the recorded
`campaign_sha256` against the emitted report. Material the record binds
but the caller did not supply is `not_checked`, never `verified`.

`explain_derivation_changes(before, after)` in the compiler and the
verifier's matching check answer "which uses changed" from the two
derivation documents: applications are matched by `(rule, subject)` and
compared premise-by-premise; a changed conclusion names the premise kinds
that differ (`requirement`, `evidence`, `qualification`, `execution_policy`,
`evaluation_context`). The runner's existing plan/impact machinery remains
the authority on which *steps* re-execute for a changed input; the
derivation diff explains which *uses* (admissions, gates, verdicts) the
change invalidated, and the two agree because both are computed over the
same bound edges.

## Consequences

- A client crate cannot construct `CompiledContract`,
  `CanonicalTypedQuantity`, `CheckedAdmissions`, `ArtifactObservations`
  entries, or a `VerifiedCasePackage`; the doctests pin these as
  `compile_fail` at the public boundary.
- Identical verdicts under different contexts have different
  `context_sha256`/`derivation_sha256` identities; equal `PASS` results do
  not conflate premises.
- The claims-only evaluation keeps its limited meaning: it asserts nothing
  about bytes, receipts, or signatures, and an `ArtifactObservations` set
  says exactly which supplied files were hashed.
- All existing canonical identities — compiled snapshots, campaign reports,
  package/receipt digests — are unchanged; the derivation is additive.
- New finding codes `CORE-E7401` (context mismatch) and `CORE-E7501`
  (derivation bound exhausted) are catalogued with bounded next actions.

## Limitations

- The derivation records the checks this profile actually performs; it does
  not make claims about receipts, signatures, or executable identity that
  the campaign evaluator never established. Those premises remain at their
  own boundaries (evidence crate, runner).
- `explain_derivation_changes` compares two derivations; it does not decide
  whether a new execution is required — that stays with the runner's plan.
- The verifier replays the rules the slice uses; an application naming a
  rule outside its vocabulary is reported `not_checked`, and a derivation
  whose required material is unavailable cannot verify.
- This is not a general obligation engine: premises are recorded, not
  solved. Session stores, query engines, and scheduling remain under the
  S-034 gates in the handoff's follow-on table.
