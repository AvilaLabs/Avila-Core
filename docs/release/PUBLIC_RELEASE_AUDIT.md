# Public release audit

- Audit date: 2026-09-03
- Scope: every ref reachable from the GitHub remote plus the current worktree
- Current recommendation: hold visibility change pending the history-boundary
  decision below

## Checks completed

- The current tree contains no retired internal codename.
- Gitleaks 8.30.1 scanned all reachable history and found no secrets.
- A separate high-confidence credential scan found no private keys, GitHub or
  AWS tokens, or secret-like filenames.
- The root license is the canonical AGPLv3 text, GitHub detects AGPL-3.0, and
  every workspace crate declares `AGPL-3.0-only`.
- All 448 locked Rust dependencies declare license metadata. `cargo audit`
  found no known vulnerabilities. It reported one informational maintenance
  warning for `ttf-parser 0.25.1`, reached through the desktop UI's Wayland
  decoration stack.
- Formatting, clippy, all Rust tests, and JSON parsing pass locally.
- Operator-supplied environment values are no longer serialized into receipts
  or reports. Current executable examples contain no personal workstation
  paths; published campaign configs mark and use environment references for
  their machine-local values.
- The generated multigroup HDF5 artifact is absent from and ignored by the
  current tree. Its builder and reference provenance sidecar remain.
- The GitHub description, topics, default branch, CI workflow, security policy,
  contribution guide, logo reservation, and license detection are present.

## Decision required before publication

### History boundary

The current tree is publication-clean, but the existing Git history is not a
clean public boundary:

- 15 remote-reachable commits contain the retired internal codename;
- 16 contain personal workstation paths; and
- 91 expose a personal email address in author or committer metadata;
- the generated multigroup HDF5 artifact remains reachable from historical
  commits; and
- pull request 1 retains head and merge refs whose trees contain that artifact.

A same-repository history rewrite can remove the artifact from branches and
tags while retaining the repository's settings and identity. It changes every
commit hash after the artifact's introduction, invalidates commit references
already recorded by downstream use-case logs, and breaks affected historical
pull-request diffs. GitHub's retained pull-request refs and cached views are not
removed by a force-push, so complete server-side expungement may also require
GitHub Support.

Publishing the existing history without that purge intentionally accepts the
historical naming, path, and email disclosures, but it would also publish the
generated artifact whose redistribution basis is unresolved. That artifact
must not remain reachable when visibility changes.

## Current-tree resolution: generated neutron library

The deleted `examples/capabilities/shield-coupled/mgxs-vitamin-j-175.h5` was
23,640,864 bytes and dominated the repository. It contained an Avila-generated
multigroup library built with OpenMC and dose-response coefficients derived
from ICRP Publication 116. OpenMC is MIT-licensed and publishes the source
ENDF/B data, but ICRP's permissions page says reproduction of report material
requires permission. No permission or independent redistribution basis is
recorded.

The current tree therefore retains only the builder and reference sidecar. The
canonical generated filename is ignored to prevent accidental recommit. Its
remaining historical reachability is part of the boundary decision above.

## Repository operations after the boundary is chosen

- Delete or rewrite the stale remote experiment branch; its six changes already
  exist on `main` in consolidated form.
- Enable dependency alerts, secret scanning, push protection, and private
  vulnerability reporting once the repository plan and visibility expose
  those controls.
- Add branch protection or a ruleset requiring CI on `main` when the GitHub
  plan permits it.
- Create the first pre-alpha tag only from the chosen clean public baseline.
