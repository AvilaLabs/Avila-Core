# ADR-0013: Runtime environment confidentiality

- Status: accepted
- Date: 2026-09-03
- Refines: ADR-0007 and S-023
- Resolves: S-032

## Context

An adapter can require an operator to supply an environment value so a process
can locate external content. The first implementation copied those values into
execution receipts as non-identifying provenance. That exposed machine-specific
paths in committed specimens and would also expose a token, credential, or
license-server address if a future adapter accepted one.

The values did not participate in `invocation_sha256`. Required key names did,
while the located content was separately staged and bound by digest. Persisting
the raw value therefore added disclosure risk without strengthening replay or
receipt verification.

## Decision

1. Operator-supplied environment values exist only in runner memory while a
   fresh process is launched. Core does not serialize them into a receipt,
   campaign report, or log.
2. A portable invocation records the required environment key names. Those
   names remain part of `invocation_sha256`.
3. Content located through an environment value must be represented by a
   staged, digest-bound input when it affects the capability result. Omitting
   the raw locator is not a substitute for binding the bytes it selects.
4. Static environment declared by an adapter remains in the invocation and its
   identity. It is package content and must never contain an operator secret.
5. A fresh run is refused when a required value is absent. Receipt reuse needs
   no value because the prior invocation identity and output bytes are checked.

## Consequences

- Receipts are portable across machines and do not reveal operator paths or
  secret-bearing environment values.
- A receipt proves which required keys were present, not what raw strings the
  operator supplied. The staged input identities carry the relevant content
  provenance.
- Adapters that depend on an environment value without binding the selected
  content are invalid capability designs and must be corrected rather than
  relying on a disclosed path as evidence.
- Older draft receipts that serialized `supplied_environment` require migration
  before the strict current reader accepts them.
