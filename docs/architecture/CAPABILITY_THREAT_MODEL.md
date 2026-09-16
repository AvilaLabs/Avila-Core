# Capability threat model and conformance design

**Status:** Stage-0 adversarial review of the execution boundary as it exists
today — the built-in adapters, the ADR-0011/0017 external-checker adapter, and
the `case_run` staging/receipt path. The conformance *suite* this designs
remains a Stage-3 track (capability SDK and conformance tools); this document
fixes what conformance must mean so the suite cannot drift into a checkbox.

## What a receipt actually claims

An execution receipt claims exactly: *the executable whose SHA-256 is recorded
ran with this invocation identity, in this workspace, and produced these output
digests.* It does not claim the program was safe, correct, qualified, or free
of side effects. Every attack below is judged against that claim — a defense
exists only where the claim could otherwise be falsified.

## Adversary model

- A **hostile or tampered package**: wrong or repointed source bytes, a
  rewritten manifest or registry, a forged or transplanted receipt, an edited
  campaign log, changed requirement identities between attempts.
- A **wrong executable**: a plausibly named but different program at the
  supplied capability path, or bytes swapped after the check.
- A **hostile-but-pinned executable**: the declared bytes are exactly what ran
  — the digest is honest — but the program itself misbehaves: writing outside
  the workspace, exfiltrating environment values, refusing to exit.
- A **hostile input**: a supplied free input that is malformed, or an attempt
  to feed an input the package never declared free.

Core is evidence machinery, not a sandbox. The adversary model's purpose is to
keep the record honest under attack, not to make a hostile program harmless.

## The boundary as implemented

Each row names the mechanism, the falsified claim it defends, and the
adversarial test pinning it (`execute::adversarial_tests`).

| Threat | Defense | Pinned by |
| --- | --- | --- |
| Substituted executable | The declared `executable_sha256` is hashed at the supplied path before execution; a mismatch or unreadable file refuses the step | `a_different_executable_is_refused_before_it_runs`; `a_bound_plan_blocks_on_unsupplied_missing_and_wrong_capabilities` |
| Modified or unchecked input bytes | Every staged input is re-hashed against the package identity before staging; `not_checked` artifacts block the steps that consume them | `modified_input_bytes_stop_before_execution`; `unchecked_input_bytes_refuse_execution` |
| Path escape via a manifest path or symlink | `resolve_confined` validates the relative path, canonicalizes, refuses `starts_with(root)` failures and non-regular files; package documents are read confined to the package root | package tests in `avila-core-evidence` (`EscapesRoot`) |
| Environment leakage into or out of the child | The child runs under `env_clear()` plus only the adapter's fixed environment and the operator-supplied values for declared keys; adapter-fixed keys cannot be supplied | staging in `execute/mod.rs`; bound-plan environment coverage |
| Declared output is a symlink or non-regular file | Output collection refuses symlinks outright and records any other non-regular or absent output as not collected | external-checker adapter tests |
| Runaway solver tree | Timeout terminates the child's Unix process group or Windows Job Object; the receipt names the cleanup and disclaims it as sandboxing | `timeout_kills_the_whole_process_group_not_just_the_direct_child`; `windows_timeout_terminates_descendants_and_releases_their_files` |
| Malformed or undeclared free input | A supplied free input is schema-validated against its role's embedded JSON Schema before staging; an input the package does not declare free is refused | `a_malformed_free_input_is_rejected_before_staging`; `a_well_formed_free_input_satisfying_its_schema_proceeds_unchanged`; `an_input_the_package_does_not_declare_free_cannot_be_supplied` |
| Rewritten package under a pinned manifest | A manifest pin or `execution_policy.require_signatures` under `run --trust-root FILE` verifies the manifest's Ed25519 signature against a listed requester key before compilation | `a_pinned_manifest_refuses_a_rewritten_package_and_the_log_names_identities`; `a_rewritten_manifest_with_the_old_signature_is_refused` |
| Forged or transplanted receipt | Receipt reuse verifies the receipt's signature against a listed runner key, its `case_id`, its invocation identity, and its output digests | `a_hand_forged_receipt_signature_does_not_verify`; `a_receipt_copied_from_a_donor_package_is_refused`; `missing_or_edited_committed_receipts_are_detected`; `an_edited_receipt_cannot_be_reused_and_the_rerun_drifts_from_it` |
| Edited campaign log | Signed log lines re-validate on read; an edited line fails lineage revalidation | `a_log_line_edited_after_signing_fails_lineage_revalidation` |
| Reuse masking a semantic edit | Reuse is keyed on the invocation identity — registry, presentation, and qualification edits each produce the SC-12 change classes instead of a masked reuse | `a_registry_edit_narrowing_a_claim_model_is_not_masked_by_reuse`; `a_qualification_edit_narrowing_the_envelope_leaves_a_reused_step_not_evaluated`; `an_optional_review_edit_changes_nothing_in_technical_verdicts` |
| Changed goalposts between attempts | Attempt lineage binds the exact parent record and refuses a run whose fixed identities moved without an amendment | `attempt_lineage_refuses_changed_goalposts_before_execution`; `attempt_lineage_refuses_a_tampered_candidate_history` |
| Adapter bound to the wrong step kind | Adapter resolution checks the compiled step type before planning | `an_adapter_bound_to_the_wrong_step_type_is_refused` |
| Failing process laundered into a verdict | A nonzero exit, missing output, or timeout produces a failed/timed-out receipt and no verdict | `a_failing_execution_produces_a_failed_receipt_and_no_verdict`; `drifting_output_fails_binding_and_receipt_replay` |
| Plan claiming readiness it cannot bind | The bound plan decides `execute` only when capability bytes hash-verify at the supplied path and every required environment key is valued | the six `a_bound_plan_*` fixtures |

## Residual risks — named, not fixed

These are honest gaps. Each is stated in the receipt's own `limitations` where
it affects a recorded run.

- **No sandbox.** A verified executable can read anything the operator can,
  open network connections, and write outside its workspace. `env_clear`, the
  confined working directory, and process-group kill are hygiene, not
  confinement. The digest proves *which bytes* ran, never what they do.
- **Check-to-exec TOCTOU.** The executable is hashed at the supplied path and
  later executed at the same path; a concurrent swap between the two is not
  detected. Closing it needs an fd-level execution pin or platform signing.
- **Supplied environment values are outside invocation identity.** ADR-0013
  keeps operator-supplied values (which may be secrets) out of the receipt:
  identity covers the required key *names* and the adapter's fixed values only.
  The designed consequence: changing a supplied value does not invalidate a
  committed receipt, so reuse cannot see that change.
- **No resource accounting.** CPU, memory, and disk beyond the timeout are
  unbounded. Named open in the execution slice.
- **A compromised runner host.** A hostile OS or filesystem can falsify any of
  this. Core's claim is bounded at the document level; remote attestation is a
  later track.
- **Signature coverage is document-level.** ADR-0015 signs manifests, receipts,
  and log lines — not archives, capability packages, or the verifier itself.

## Conformance design

Interchangeability under ADR-0002 is established by conformance, not by sharing
a label. The design that keeps that honest:

- **A conformance vector is an ordinary case package.** It binds a candidate
  executable to a capability type's declared boundary — input slots, output
  slots, claim models, determinism class, material factors, parameter domains
  — over inputs whose expected claims are fixed by the type's owner. The
  runner needs no new machinery: `run` executes it, `run --plan`'s bound plan
  proves the supplies bound, the report's claims report extracts the outputs.
- **The suite is adversarial, not happy-path.** One vector per falsifiable
  dimension of the type contract: a wrong output slot, a malformed claim
  payload, a determinism-class violation (an executable that cannot reproduce
  a `deterministic` type's output across two runs), a parameter outside its
  declared domain, an output whose media type the type does not permit. A
  candidate passes the suite only when the good vectors evaluate and every bad
  vector fails binding or claim extraction — the same vocabulary the
  adversarial suite already pins.
- **Conformance is scoped, never global.** A vector establishes conformance to
  `type@major` under its stated envelope; qualification evidence and
  `require_qualification` stay orthogonal per ADR-0008.
- **Deliberately out of scope here:** the capability SDK, signed provider
  packages, a conformance registry, and scoring across candidates — Stage-3
  platform work. The Stage-0 obligation this discharges is fixing the
  *definition* so the suite cannot be built wrong later.

## Cross-references

- ADR-0002 — solver-neutral capability boundary (the claim conformance serves)
- ADR-0007 — execution receipts (the record under attack)
- ADR-0011 / ADR-0017 — the external-checker adapter, the first conformable
  boundary
- ADR-0013 — runtime-environment confidentiality (why env values stay outside
  identity)
- ADR-0015 — signed manifests, receipts, and log lines (the trust-root half)
- `execute/adversarial_tests.rs` — the executable pins this document cites
