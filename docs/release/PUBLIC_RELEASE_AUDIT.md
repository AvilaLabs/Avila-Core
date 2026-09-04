# Public release audit

- Audit date: 2026-09-03
- Scope: every ref reachable from the GitHub remote plus the current worktree
- Current recommendation: hold visibility change pending the two decisions
  below

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
- The GitHub description, topics, default branch, CI workflow, security policy,
  contribution guide, logo reservation, and license detection are present.

## Decisions required before publication

### 1. History boundary

The current tree is publication-clean, but the existing Git history is not a
clean public boundary:

- 15 remote-reachable commits contain the retired internal codename;
- 16 contain personal workstation paths; and
- 91 expose a personal email address in author or committer metadata.

A history rewrite would also invalidate commit references already recorded by
downstream use-case logs, and closed pull-request refs can retain old objects on
GitHub. The clean option is a new public baseline repository (or a deleted and
recreated remote) from the reviewed tree while retaining this repository as a
private archive. Preserving the existing remote is acceptable only if those
historical disclosures are intentionally accepted.

### 2. Generated neutron library

`examples/capabilities/shield-coupled/mgxs-vitamin-j-175.h5` is 23,640,864
bytes and dominates the repository. It contains an Avila-generated multigroup
library built with OpenMC and dose-response coefficients derived from ICRP
Publication 116. OpenMC is MIT-licensed and publishes the source ENDF/B data,
but ICRP's permissions page says reproduction of report material requires
permission. No permission or independent redistribution basis is recorded.

Before publication, either:

1. remove the HDF5 object from every public ref and publish only the builder and
   sidecar provenance;
2. replace the coefficient source with data whose redistribution terms are
   documented; or
3. obtain and record permission or a reviewed legal basis for distribution.

## Repository operations after the boundary is chosen

- Delete the stale remote experiment branch after preserving any desired tag
  or archive; its six changes already exist on `main` in consolidated form.
- Enable dependency alerts, secret scanning, push protection, and private
  vulnerability reporting once the repository plan and visibility expose
  those controls.
- Add branch protection or a ruleset requiring CI on `main` when the GitHub
  plan permits it.
- Create the first pre-alpha tag only from the chosen clean public baseline.
