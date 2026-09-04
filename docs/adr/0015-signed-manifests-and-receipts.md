# ADR-0015: Signed manifests and receipts

- Status: proposed
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
