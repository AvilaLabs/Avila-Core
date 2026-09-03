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
