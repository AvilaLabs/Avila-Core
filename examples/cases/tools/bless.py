#!/usr/bin/env python3
"""Bless a case from a completed fresh run: copy the run's claims, campaign
report, receipts, and step outputs into the case directory, then recompute
every digest the package declares.

Usage: bless.py CASE_DIR WORKSPACE --root NAME=PATH ... [--manifest FULL_PACKAGE_JSON]

WORKSPACE is the directory `avila-core run --workspace` wrote. Each step's
receipt is copied to `receipts/<step>.json`; each declared output artifact
under the case root whose path starts with `expected/` is copied from the
step's `outputs/` directory by file name. With --manifest, that file replaces
the case's package.json first (used to restore a full manifest after a
bootstrap run over a stripped one). Digests are then recomputed with
rehash.py. Nothing is verified here: run the case afterwards with no
capabilities and check that every step is reused and replay matches.

This script knows nothing about ADR-0015 signature documents and never
writes or removes one. If a receipt this run overwrites already has a bound
`signature` document naming it, that signature now covers the *old* bytes
and fails internal consistency against the new ones; the case runner
already reports that honestly as an invalid signature and refuses to reuse
it (or refuses the whole run under `execution_policy.require_signatures`),
so this is not a silent gap, but it is easy to miss. This script prints a
reminder for every receipt or manifest signature it can see is now stale;
re-sign with `avila-core sign receipt` and `avila-core sign manifest`
afterward.
"""

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("case_dir")
    parser.add_argument("workspace")
    parser.add_argument("--root", action="append", default=[])
    parser.add_argument("--manifest")
    args = parser.parse_args()
    case = Path(args.case_dir)
    workspace = Path(args.workspace)
    if args.manifest:
        shutil.copy(args.manifest, case / "package.json")
    package = json.loads((case / "package.json").read_text(encoding="utf-8"))

    copied = []
    for name in ("claims.json", "campaign-report.json"):
        source = workspace / name
        if not source.is_file():
            print(f"missing in workspace: {source}")
            return 1
        shutil.copy(source, case / name)
        copied.append(name)

    (case / "receipts").mkdir(exist_ok=True)
    for document in package.get("documents", []):
        if document.get("role") != "execution_receipt":
            continue
        step = document["step_id"]
        source = workspace / step / "receipt.json"
        if not source.is_file():
            print(f"missing receipt for step {step}: {source}")
            return 1
        shutil.copy(source, case / document["path"])
        copied.append(document["path"])

    (case / "expected").mkdir(exist_ok=True)
    outputs = {}
    for step_dir in workspace.iterdir():
        out_dir = step_dir / "outputs"
        if out_dir.is_dir():
            for file in out_dir.iterdir():
                outputs.setdefault(file.name, []).append(file)
    for artifact in package.get("artifacts", []):
        path = artifact["path"]
        if artifact["source_root"] != "case" or not path.startswith("expected/"):
            continue
        name = Path(path).name
        candidates = outputs.get(name, [])
        if len(candidates) != 1:
            print(f"cannot resolve {path}: {len(candidates)} workspace outputs named {name}")
            return 1
        shutil.copy(candidates[0], case / path)
        copied.append(path)

    print("copied:", ", ".join(copied))
    reblessed_steps = {
        document["step_id"]
        for document in package.get("documents", [])
        if document.get("role") == "execution_receipt"
    }
    warn_stale_signatures(case, package, reblessed_steps)
    rehash = Path(__file__).with_name("rehash.py")
    command = [sys.executable, str(rehash), str(case)] + [f"--root={root}" for root in args.root]
    return subprocess.call(command)


def warn_stale_signatures(case: Path, package: dict, reblessed_steps: set[str]) -> None:
    """Print a reminder for every bound `signature` document (ADR-0015) whose
    target this bless just overwrote: a receipt for a step just rebuilt, or
    the manifest itself (rehash.py is about to change its documents' digests,
    which changes the manifest's own bytes and so its signature's target).
    """
    manifest_is_signed = False
    stale_receipt_steps = []
    for document in package.get("documents", []):
        if document.get("role") != "signature":
            continue
        try:
            content = json.loads((case / document["path"]).read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        signed = content.get("signed_document", {})
        if signed.get("role") == "manifest":
            manifest_is_signed = True
        elif signed.get("role") == "execution_receipt" and signed.get("document_id") in reblessed_steps:
            stale_receipt_steps.append(signed["document_id"])
    for step in sorted(stale_receipt_steps):
        print(
            f"stale signature: step `{step}`'s receipt was just overwritten; "
            f"re-sign it with `avila-core sign receipt {case} --step {step} --key RUNNER_SEED`"
        )
    if manifest_is_signed:
        print(
            f"stale signature: the manifest signature covers the pre-bless documents; "
            f"re-sign it last with `avila-core sign manifest {case} --key REQUESTER_SEED`"
        )


if __name__ == "__main__":
    raise SystemExit(main())
