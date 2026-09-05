# Security policy

Avila Core is pre-alpha and must not receive sensitive, export-controlled,
regulated, or proprietary production data.

Please report vulnerabilities through this repository's private vulnerability
reporting channel rather than opening a public issue. If that channel is not
available, contact Avila Labs privately before disclosing details.

The runner can execute exact, content-identified capability processes with a
cleared environment and staged inputs. This is an evidence boundary, not a
sandbox or containment boundary. Run only capabilities you trust on a machine
appropriate for their risk.

Operator-supplied environment values are passed only to a fresh process and are
not serialized into receipts or reports. Static adapter environment is public
package content and must never contain credentials or other secrets.

`avila-core run --hash-cache PATH` is an explicit, opt-in, off-by-default
cache of verified digests for large operator-supplied artifacts resolved
under a `--source-root`. An entry is keyed by a file's canonical absolute
path, size, modification time, and (where the platform exposes them) device
and inode. On a hit, the recorded digest is used in place of re-hashing the
bytes and is still compared to the manifest's bound identity exactly as an
uncached digest would be, so a stale or malicious cache entry that disagrees
with the manifest still fails closed. Package documents and any artifact that
resolves inside the case package directory are never eligible for the cache,
regardless of this setting. **The cache trusts that operator-owned artifact
roots are not modified while preserving a file's size and modification
time.** An actor with write access to a cached root who can reproduce a
file's original size and modification time while changing its bytes is not
caught by a cache hit; this is a documented, deliberate limitation, not a
defect, and it is exercised by an adversarial test
(`crates/avila-core-evidence/src/package.rs`). The requester's manifest pin
(S-030) is a separate mechanism and is unaffected by this cache: a rewritten
package manifest is still refused before anything is compiled or executed.

`avila-core run --trust-root FILE` verifies Ed25519 signatures against an
operator-supplied list of requester and runner public keys (ADR-0015). A
package's manifest signature must verify against a listed requester key or
the run is refused before compilation; a committed receipt is reused under
SC-12 only when its signature verifies against a listed runner key. Without
`--trust-root`, every signature is checked for internal consistency only
(a well-formed signature whose recorded target digest matches
recomputation) and reported `unsigned` or `signature not checked`, never
`verified`. Private keys (`avila-core keys generate`) are 32-byte seed
files the tool writes with mode `0600`; the runner never reads, transmits,
or logs one, and only ever signs a digest the caller already computed. A
signature proves possession of a key at signing time, not the correctness,
qualification, or regulatory suitability of what was signed. There are no
certificate chains, revocation, timestamps, or hardware keys, and a key
compromise is handled by replacing the trust root and re-blessing, not by
anything this runner automates. `examples/keys/` deliberately commits both
the public and private halves of its example keys, stated plainly in its
own README: they prove nothing and must never be reused for anything real.

The threat model includes:

- untrusted capability packages and input documents;
- command, path, environment, and container injection;
- credential and license-server exposure;
- artifact substitution and provenance tampering;
- data exfiltration and cross-tenant access;
- resource exhaustion and runaway compute;
- signing-key compromise and replayed attestations; and
- unsafe interpretation of untrusted evidence in the desktop application.

The current receipt and execution design is recorded in ADR 0007 and exercised
by adversarial tests. Signed manifests and receipts are recorded in ADR 0015.
Stronger isolation remains required before Core can accept untrusted
capability packages or production data.
