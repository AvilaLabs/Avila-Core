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
by adversarial tests. Stronger isolation remains required before Core can accept
untrusted capability packages or production data.
