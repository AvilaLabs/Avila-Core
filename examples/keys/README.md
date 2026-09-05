# Example signing keys (ADR-0015)

These are **public example keys that prove nothing.** The private seeds
(`requester.seed`, `runner.seed`) are committed to this repository in plain
text specifically so that anyone can reproduce, inspect, or forge a
signature over these three example cases. A signature made with a key
whose private half is published in the same repository as the evidence it
signs establishes nothing about who approved what; it exists here only to
exercise and demonstrate the mechanism ADR-0015 describes.

**Never reuse these keys, or the pattern of committing a private seed, for
anything that matters.** A real requester or runner key must be generated
with `avila-core keys generate` on a machine and account the corresponding
role controls, and its seed file must never leave that machine or be
committed to a repository.

## Contents

- `requester.seed` (32 raw bytes, mode `0600`) and `requester.pub` (hex
  public key) — signs case manifests.
- `runner.seed` and `runner.pub` — signs execution receipts and campaign
  log lines.
- `trust-root.json` — both public keys, in the format `avila-core run
  --trust-root FILE` accepts.

Key ids (SHA-256 of the public key bytes, hex):

- requester: `191c470d8848ffedf3bb54e33798e98423a5659db297877d6482a06c96149cd7`
- runner: `49104f3eb2b3489e8b26bc1ebb0934637e43aacf515719b1698758532c6daf62`

## What is signed with them

CASE-001, CASE-002, and CASE-003 each carry a `signatures/manifest.sig.json`
document (signed with `requester.seed`) and one `signatures/<step>-receipt.sig.json`
per declared execution step (signed with `runner.seed`). None of the three
cases sets `execution_policy.require_signatures`; verifying with
`--trust-root examples/keys/trust-root.json` reports every signature
`verified` but does not change any technical verdict, and running without
`--trust-root` at all still reports them `signature not checked` and works
exactly as before signing existed.
