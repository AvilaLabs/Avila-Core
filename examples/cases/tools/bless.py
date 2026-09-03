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
    rehash = Path(__file__).with_name("rehash.py")
    command = [sys.executable, str(rehash), str(case)] + [f"--root={root}" for root in args.root]
    return subprocess.call(command)


if __name__ == "__main__":
    raise SystemExit(main())
