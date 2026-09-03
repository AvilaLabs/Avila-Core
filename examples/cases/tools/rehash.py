#!/usr/bin/env python3
"""Recompute every digest a case package declares.

Usage: rehash.py CASE_DIR --root NAME=PATH ... [--check]

Documents are read relative to the case directory; artifacts relative to the
named source root. Missing files are reported and leave the recorded digest
untouched, so a package can be drafted before every artifact exists. With
--check nothing is written and a non-zero exit means at least one digest
differs or a file is missing. The package is rewritten with two-space
indentation and its key order preserved.
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("case_dir")
    parser.add_argument("--root", action="append", default=[], help="NAME=PATH")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    case = Path(args.case_dir)
    roots = {}
    for spec in args.root:
        name, _, path = spec.partition("=")
        roots[name] = Path(path)
    package_path = case / "package.json"
    package = json.loads(package_path.read_text(encoding="utf-8"))
    changed = 0
    missing = 0
    for document in package.get("documents", []):
        path = case / document["path"]
        if not path.is_file():
            print(f"missing document {document['document_id']}: {path}")
            missing += 1
            continue
        digest = sha256(path)
        if document.get("sha256") != digest:
            print(f"document {document['document_id']}: {document.get('sha256')} -> {digest}")
            document["sha256"] = digest
            changed += 1
    for artifact in package.get("artifacts", []):
        root = roots.get(artifact["source_root"])
        if root is None:
            print(f"no root supplied for {artifact['artifact_id']} ({artifact['source_root']})")
            missing += 1
            continue
        path = root / artifact["path"]
        if not path.is_file():
            print(f"missing artifact {artifact['artifact_id']}: {path}")
            missing += 1
            continue
        digest = sha256(path)
        if artifact.get("sha256") != digest:
            print(f"artifact {artifact['artifact_id']}: {artifact.get('sha256')} -> {digest}")
            artifact["sha256"] = digest
            changed += 1
    for capability in package.get("capabilities", []):
        path_text = capability.get("_local_path")
        if path_text:
            path = Path(path_text)
            if path.is_file():
                digest = sha256(path)
                if capability.get("executable_sha256") != digest:
                    print(f"capability {capability['capability_id']}: {capability.get('executable_sha256')} -> {digest}")
                    capability["executable_sha256"] = digest
                    changed += 1
            else:
                print(f"missing capability executable {capability['capability_id']}: {path}")
                missing += 1
    print(f"{changed} digest(s) changed, {missing} missing")
    if args.check:
        return 1 if (changed or missing) else 0
    if changed:
        package_path.write_text(json.dumps(package, indent=2) + "\n", encoding="utf-8")
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
