# ADR-0015: Signed manifests and receipts

- Status: accepted for the local runner, 2026-09-04 (implemented the same day
  it was proposed; see Implementation notes below for the exact mechanism and
  deviations)
- Date: 2026-09-04
- Refines: ADR-0007 (execution receipts), ADR-0014 (attempt lineage), and
  decision S-030 (the requester's manifest pin)

## Context

The adversarial designer arm of CASE-002 campaign 3 obtained undeserved
verdicts only by rewriting the package: limits, scripts, receipts and
manifests together. Every refusal that did not require rewriting the
package held. Decision S-030 answered with two things: every campaign log
line carries the identities it was evaluated under, and `run
--expect-manifest` refuses a package whose manifest digest differs from a
pin the requester holds outside the package.

The pin is an anchor, not evidence. It lives in the search tools'
configuration on the same machine as the designer, it binds only the
manifest, and a reader of an exported package cannot tell who approved the
package or whether a receipt was written by the runner or by hand. A
receipt is verified today by re-hashing what it names; a forged receipt
whose digests agree with forged outputs verifies.

## Decision

1. **Detached signatures, unchanged documents.** A signature is its own
   document with role `signature`, bound in the package like any other.
   It names the signed document's role and SHA-256, the signing key's
   identifier, the algorithm, and the signature bytes. Manifests, receipts
   and log lines keep their bytes and digests; committed history stays
   byte-compatible.
2. **Two keys, two meanings.** A *requester* key signs the manifest digest
   and, through it, every document the manifest binds: the signature says
   "this is the package the requester approved". A *runner* key signs each
   execution receipt's `invocation_sha256` together with its output
   identities, and each campaign log line: the signature says "this record
   was written by a runner that held this key". Neither signature makes a
   claim true, qualifies a method, or changes a verdict.
3. **Trust roots come from outside the package.** `run --trust-root FILE`
   supplies the requester and runner public keys the run accepts. A package
   whose manifest signature does not verify against a listed requester key
   is refused before compilation, exactly where the pin refuses today. A
   committed receipt whose signature does not verify against a listed runner
   key is not reused; the step reruns or is `not_run`, and the reason names
   the signature. Without `--trust-root`, signatures are checked for
   internal consistency only and every report says `unsigned` or
   `signature not checked` for each document, never `verified`.
4. **Keys stay out of the designer's reach.** Private keys live in an
   operator-owned directory outside every case, workspace and log
   (`$XDG_CONFIG_HOME/avila-core/keys/` by default), readable only by the
   operating-system user. The designer tools never read, pass, or print
   them. A designer that can read the runner's private key can forge
   receipts; this boundary is documented, not enforced by the runner.
5. **Algorithm and format.** Ed25519 over the raw SHA-256 digest bytes.
   Public keys are 32 bytes, private keys 32-byte seeds, both hex encoded;
   key identifier is the SHA-256 of the public key bytes, hex encoded. The
   dependency is a pure-Rust Ed25519 implementation pinned by version and
   audited once at adoption; this is the workspace's first cryptographic
   dependency and the decision record must say so.
6. **Log lines.** A signed log line is the canonical line with the
   `signature` member removed, signed, and re-serialized with the member
   appended. Lineage revalidation (ADR-0014) verifies the parent line's
   signature when a trust root is supplied and refuses a child whose parent
   line does not verify.
7. **Legacy stays visible.** Unsigned packages, receipts and logs remain
   usable and are reported as unsigned. A contract may set
   `execution_policy.require_signatures` to refuse unsigned reuse and
   unsigned packages outright; the default is off until the three loop
   cases are signed and re-blessed.

## Boundary

- A signature proves possession of a key at signing time, not the
  correctness of what was signed.
- One machine, one user: an adversary with the operating-system user's
  privileges holds both keys. The first deployment does not resist a
  compromised operator.
- No certificate chains, revocation, timestamps, or hardware keys. A key
  compromise is handled by replacing the trust root and re-blessing.
- Signed exports and checkpoints across trust boundaries are a later
  decision; this record covers the local runner only.

## Acceptance

- Adversarial tests: a rewritten manifest with the old signature, a receipt
  forged by hand, a receipt copied from a donor package, a log line edited
  after signing, a signature made with an unlisted key, and a package
  signed by the runner key instead of the requester key, are each refused
  with a named finding.
- CASE-001, CASE-002 and CASE-003 signed by a requester key and re-blessed;
  the adversarial checker (`adversarial_check.py`) verifies the signature on
  every log line it reads.
- The independent verifier (queue item 4) verifies these signatures without
  importing the runner.

## Implementation notes (2026-09-04)

The mechanism above is implemented as specified, with these precise choices
where the proposal left room:

- **Dependency.** `ed25519-dalek` 2.2.0 (default features `fast`, `std`,
  `zeroize`; no `rand_core`) plus `getrandom` 0.3.4 for key generation, both
  pinned exactly and confined to `avila-core-evidence::signature`. This is
  the workspace's first cryptographic dependency; see
  `THIRD_PARTY_NOTICES.md`.
- **The manifest-signing digest rule (clause 1).** Exactly the simplest
  sound rule the ADR proposes: the digest covers the manifest with the
  signature document's own `documents[]` entry removed, canonicalized.
  Removing an absent id is a no-op, so signing (before the entry exists)
  and verifying (after it does) compute the identical digest. One sharp
  edge this exposed: the digest must be taken from the manifest as its
  typed-struct serialization will actually render it, not from whatever
  raw bytes happen to be on disk — the first tool to touch a manifest that
  predates any struct round-trip normalizes in fields such as an empty
  `free_inputs`, and a digest taken before that normalization never
  matches one taken after. `avila-core sign manifest` and the equivalent
  test helper both digest `serde_json::to_vec(&manifest)`, never the raw
  file bytes.
- **Report and log states.** Rather than the two literal states the ADR's
  prose names ("unsigned" and "signature not checked"), the implementation
  reports four: `unsigned`, `not_checked`, `verified { signed_by }`, and
  `invalid { reason }`. `invalid` is a strict superset of what the ADR
  requires — a tampered signature is visibly wrong even without a trust
  root, since target-digest and structural consistency need no key to
  check — and `verified` never appears without a trust root, matching the
  ADR exactly.
- **Signature target for a receipt or log line is its own role name and, for
  a receipt, the step id** (stable across re-blessing) rather than the
  package's own arbitrary `document_id` for that receipt document. A log
  line's target role is `log_line`.
- **A donor-receipt gap this work found and closed.** Reusing a committed
  receipt never compared its `case_id` to the running case's, only its
  invocation identity (capability, parameters, staged inputs, arguments,
  environment). A receipt copied from a different package with
  byte-identical capability and inputs would have been silently reused,
  signed or not. `ChangeClass::DifferentCase` now refuses that,
  independent of and in addition to signature checking; it is not part of
  clause 1-7 as proposed but is required for the "donor package" acceptance
  test to mean anything, since a genuine signature travels with its bytes
  and cannot itself detect that those bytes describe a different case.
  `compiled_snapshot_sha256` is deliberately not compared: a requirement,
  registry, or review edit changes that identity without invalidating what
  a capability already ran over (three existing tests pin this).
- **`execution_policy.require_signatures` (clause 7).** Implemented as
  specified: the compiler only carries the flag through and emits a visible
  `CORE-A4404` notice (it has no receipts to check); the case runner
  refuses the run outright when the flag is set and no `--trust-root` was
  supplied, and refuses when any declared step's operative evidence
  (reused, freshly executed, or left `not_run`) is not itself verified.
- **Not implemented in this slice.** The independent offline verifier
  (queue item 4) and `CASE-002`'s `adversarial_check.py` verifying log-line
  signatures are both still open; see `docs/roadmap/STAGE_0_STATUS.md`.
  CASE-001 and CASE-002's `transport` (and CASE-002's `activation`) steps
  are signed but their reuse could not be demonstrated in the environment
  this slice was built in, because their bound inputs need external
  nuclear-data and ACTINV artifacts this sandbox does not carry and this
  task's rules forbid running OpenMC or ACTINV to obtain; CASE-003, which
  has no such external dependency, demonstrates every step reused and
  every signature verified end to end.
