# Security policy

Avila Core is pre-alpha and must not receive sensitive, export-controlled,
regulated, or proprietary production data.

Please report vulnerabilities privately to Avila Labs rather than opening a
public issue. A dedicated security contact and disclosure SLA must be established
before external pilot use.

The current code does not execute capability processes. When execution is added,
the threat model must cover at least:

- untrusted capability packages and input documents;
- command, path, environment, and container injection;
- credential and license-server exposure;
- artifact substitution and provenance tampering;
- data exfiltration and cross-tenant access;
- resource exhaustion and runaway compute;
- signing-key compromise and replayed attestations; and
- unsafe interpretation of untrusted evidence in the desktop application.

No process runner may be enabled until its isolation and evidence-receipt design
has a reviewed ADR and adversarial tests.

