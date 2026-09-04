# ADR-0012: AGPL license and work-product boundary

- Status: accepted
- Date: 2026-09-03
- Resolves: S-011 and S-013 licensing and Core patent-publication questions

## Context

Avila Core is intended to be publicly inspectable, locally usable, and capable
of supporting independent verification. It may also become a remotely operated
execution or control service. A distribution-only copyleft would allow a party
to modify Core, offer that modified version only through a network, and keep
the modifications unavailable to the people using it.

Core is also a tool for work that may remain confidential, proprietary, or
patentable. Licensing Core must not imply that ordinary inputs, generated
evidence, engineering designs, or other work products acquire Core's license
merely because Core processes them.

Avila is not holding the mechanisms published in this repository for patent
protection. That decision does not determine the intellectual-property strategy
for separate inventions developed with Core.

## Decision

1. Except where a file or directory states otherwise, this repository is
   licensed under the GNU Affero General Public License version 3 only,
   identified by SPDX as `AGPL-3.0-only`.
2. The exact unmodified license text is kept in the repository-root `LICENSE`
   file. Rust workspace and crate manifests carry the same SPDX expression.
3. Modified versions of Core that support remote network interaction must
   provide interacting users the corresponding source as required by AGPLv3
   section 13.
4. The Avila Core name and logo are not licensed under the repository's source
   license. Their reservation remains documented with the branding asset.
5. Core makes no project-policy claim over ordinary inputs or outputs merely
   because they are created, checked, or recorded with Core. Material that is
   itself copied from or based on covered Core code remains subject to the
   license's actual terms.
6. Contributions are submitted under `AGPL-3.0-only` unless a separate written
   agreement says otherwise.
7. A future standalone SDK, normative interchange specification, or exported
   evidence-package format may receive additional permissive terms when that
   materially improves independent implementation. Such a grant requires a
   separate explicit decision.

## Boundary

This decision does not create a commercial dual-license program, contributor
license agreement, trademark registration, or legal opinion about whether a
particular integration forms one covered work. It does not license confidential
or patent-sensitive details from projects that happen to use Core.

## Consequences

- Core may be used, modified, and sold under the AGPL's terms.
- A provider cannot keep modifications to a network-interactive Core fork from
  the users of that fork while relying on the AGPL for permission.
- Some organizations will decline to embed AGPL software; that adoption cost is
  accepted in exchange for the network-source obligation.
- Core-assisted engineering work does not become open source solely because it
  passed through Core.
- Release packaging must preserve the AGPL text and applicable third-party
  notices.
