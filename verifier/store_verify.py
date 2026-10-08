#!/usr/bin/env python3
"""Avila Core evidence-store reader and verifier (ADR-0028; offline, stdlib-only).

Reads a store in format ``avila.core/evidence-store/v0.1`` with ``lzma``,
``hashlib`` and ``json`` only, applying the ADR's rules independently of the
Rust implementation:

  * the index (``store.json``) obeys rules 1-6: sorted unique tree names and
    paths, path form, no file that is also a directory, one length per
    digest, size and count bounds, exact schema and codec, no unknown or
    duplicate keys;
  * every referenced blob is a regular file (not a symlink) holding one xz
    stream whose output, decompressed with a bound of ``bytes`` + 1, has the
    indexed length and SHA-256 and is followed by no other data;
  * ``blobs/`` holds nothing else (an unreferenced or misnamed file is an
    error), and the store directory holds only ``store.json`` and ``blobs/``
    (plus writer state, ``store.lock`` and ``tmp/``, which readers ignore and
    ``store-verify`` reports as information in ``writer_state``).

Content is returned or written only after its length and digest match.

Usage:
  python3 store_verify.py store-verify STORE
  python3 store_verify.py store-unpack STORE OUT [--tree NAME]...
  python3 store_verify.py store-ls STORE [TREE]
  python3 store_verify.py store-cat STORE TREE PATH
"""

from __future__ import annotations

import argparse
import hashlib
import json
import lzma
import os
import re
import shutil
import stat
import sys
import unicodedata
from typing import Any, Iterator, Optional

SCHEMA_VERSION = "avila.core/evidence-store/v0.1"
CODEC = "xz"

MAX_INDEX_BYTES = 64 * 1024 * 1024
MAX_TREES = 1_024
MAX_FILES_PER_TREE = 65_536
MAX_FILES_TOTAL = 262_144
MAX_FILE_BYTES = 4 * 1024 * 1024 * 1024
MAX_TREE_NAME_CHARS = 128
MAX_PATH_COMPONENTS = 64
DECODER_MEMORY_LIMIT = 1 << 30
MAX_REPORTED_FINDINGS = 1_000
CHUNK = 64 * 1024

_TREE_NAME = re.compile(r"[A-Za-z0-9._-]+\Z")
_SHA256 = re.compile(r"[0-9a-f]{64}\Z")
_FANOUT = re.compile(r"[0-9a-f]{2}\Z")


class StoreError(Exception):
    """A store, its index, or one of its blobs does not meet the format."""


# ---- index ------------------------------------------------------------------


def _no_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise StoreError(f"duplicate key `{key}` in store.json")
        result[key] = value
    return result


def _is_uint(value: Any) -> bool:
    return type(value) is int and value >= 0


def _check_keys(obj: Any, keys: set[str], what: str) -> None:
    if not isinstance(obj, dict):
        raise StoreError(f"{what} must be an object")
    unknown = set(obj) - keys
    missing = keys - set(obj)
    if unknown:
        raise StoreError(f"{what} has unknown field `{sorted(unknown)[0]}`")
    if missing:
        raise StoreError(f"{what} is missing field `{sorted(missing)[0]}`")


def validate_tree_name(name: Any) -> None:
    if not isinstance(name, str) or not 1 <= len(name) <= MAX_TREE_NAME_CHARS:
        raise StoreError(f"tree name must be 1 to {MAX_TREE_NAME_CHARS} characters")
    if name in (".", "..") or not _TREE_NAME.match(name):
        raise StoreError(f"tree name `{name}` is not allowed (A-Z a-z 0-9 . _ - only)")


def validate_path(path: Any) -> None:
    if not isinstance(path, str) or path == "":
        raise StoreError("path is empty or not a string")
    try:
        path.encode("utf-8")
    except UnicodeEncodeError:
        raise StoreError(f"path {path!r} is not valid UTF-8") from None
    if path.startswith("/"):
        raise StoreError(f"path `{path}` is absolute")
    for char in path:
        if char == "\\" or unicodedata.category(char) == "Cc":
            raise StoreError(f"path {path!r} contains a backslash or control character")
    components = path.split("/")
    if len(components) > MAX_PATH_COMPONENTS:
        raise StoreError(f"path `{path}` has more than {MAX_PATH_COMPONENTS} components")
    if any(part in ("", ".", "..") for part in components):
        raise StoreError(f"path `{path}` has an empty, `.` or `..` component")


def validate_index(index: Any) -> None:
    _check_keys(index, {"schema_version", "codec", "trees"}, "store index")
    if index["schema_version"] != SCHEMA_VERSION:
        raise StoreError(f"schema_version must be `{SCHEMA_VERSION}`")
    if index["codec"] != CODEC:
        raise StoreError(f"codec must be `{CODEC}`")
    trees = index["trees"]
    if not isinstance(trees, list):
        raise StoreError("trees must be an array")
    if len(trees) > MAX_TREES:
        raise StoreError(f"more than {MAX_TREES} trees")
    total = 0
    lengths: dict[str, int] = {}
    previous_name: Optional[str] = None
    for tree in trees:
        _check_keys(tree, {"name", "files"}, "tree")
        name = tree["name"]
        validate_tree_name(name)
        if previous_name is not None:
            if name == previous_name:
                raise StoreError(f"duplicate tree name `{name}`")
            if name < previous_name:
                raise StoreError(f"trees are not sorted by name (`{name}` after `{previous_name}`)")
        previous_name = name
        files = tree["files"]
        if not isinstance(files, list):
            raise StoreError(f"tree `{name}` files must be an array")
        if len(files) > MAX_FILES_PER_TREE:
            raise StoreError(f"tree `{name}` has more than {MAX_FILES_PER_TREE} files")
        total += len(files)
        if total > MAX_FILES_TOTAL:
            raise StoreError(f"more than {MAX_FILES_TOTAL} files in total")
        paths: set[str] = set()
        previous_key: Optional[bytes] = None
        for entry in files:
            _check_keys(entry, {"path", "sha256", "bytes"}, "file entry")
            path = entry["path"]
            try:
                validate_path(path)
            except StoreError as error:
                raise StoreError(f"tree `{name}`: {error}") from None
            key = path.encode("utf-8")
            if previous_key is not None:
                if key == previous_key:
                    raise StoreError(f"tree `{name}` lists `{path}` more than once")
                if key < previous_key:
                    raise StoreError(f"tree `{name}` files are not sorted by path (at `{path}`)")
            previous_key = key
            paths.add(path)
            sha, size = entry["sha256"], entry["bytes"]
            if not isinstance(sha, str) or not _SHA256.match(sha):
                raise StoreError(f"tree `{name}` file `{path}`: sha256 must be 64 lowercase hex characters")
            if not _is_uint(size) or size > MAX_FILE_BYTES:
                raise StoreError(f"tree `{name}` file `{path}`: bytes must be an integer 0..{MAX_FILE_BYTES}")
            if lengths.setdefault(sha, size) != size:
                raise StoreError(f"digest {sha} is recorded with two different lengths")
        for path in paths:
            parts = path.split("/")
            for end in range(1, len(parts)):
                if "/".join(parts[:end]) in paths:
                    raise StoreError(
                        f"tree `{name}`: `{'/'.join(parts[:end])}` is a file and also a directory prefix of `{path}`"
                    )


def load_index(store: str) -> dict[str, Any]:
    if not os.path.isdir(store):
        raise StoreError(f"`{store}` is not a directory")
    path = os.path.join(store, "store.json")
    try:
        info = os.lstat(path)
    except OSError as error:
        raise StoreError(f"cannot read store.json: {error}") from None
    if not stat.S_ISREG(info.st_mode):
        raise StoreError("store.json is not a regular file")
    with open(path, "rb") as handle:
        raw = handle.read(MAX_INDEX_BYTES + 1)
    if len(raw) > MAX_INDEX_BYTES:
        raise StoreError(f"store.json is larger than {MAX_INDEX_BYTES} bytes")
    try:
        index = json.loads(raw.decode("utf-8"), object_pairs_hook=_no_duplicate_keys)
    except (UnicodeDecodeError, ValueError) as error:
        raise StoreError(f"store.json is not valid JSON: {error}") from None
    validate_index(index)
    return index


# ---- blobs ------------------------------------------------------------------


def blob_relpath(sha: str) -> str:
    return os.path.join("blobs", sha[:2], sha + ".xz")


def _blob_path(store: str, sha: str) -> str:
    """The blob's path, after checking `blobs/` and its fan-out directory are
    real directories and the blob is a regular file, not a symlink."""
    for directory in (os.path.join(store, "blobs"), os.path.join(store, "blobs", sha[:2])):
        try:
            info = os.lstat(directory)
        except FileNotFoundError:
            raise StoreError(f"blob {sha}: the blob file is missing") from None
        if not stat.S_ISDIR(info.st_mode):
            raise StoreError(f"blob {sha}: `{directory}` is not a directory")
    path = os.path.join(store, blob_relpath(sha))
    try:
        info = os.lstat(path)
    except FileNotFoundError:
        raise StoreError(f"blob {sha}: the blob file is missing") from None
    if not stat.S_ISREG(info.st_mode):
        raise StoreError(f"blob {sha}: the blob is not a regular file")
    return path


def iter_blob(store: str, sha: str, nbytes: int) -> Iterator[bytes]:
    """Yield a blob's decompressed content in chunks.

    Output is bounded to ``nbytes`` + 1. The length and SHA-256 are checked
    when the stream ends, so the chunks are provisional until the generator
    is exhausted without raising ``StoreError``.
    """
    path = _blob_path(store, sha)
    decoder = lzma.LZMADecompressor(format=lzma.FORMAT_XZ, memlimit=DECODER_MEMORY_LIMIT)
    digest = hashlib.sha256()
    produced = 0
    with open(path, "rb") as handle:
        try:
            while not decoder.eof:
                data = handle.read(CHUNK)
                if not data:
                    raise StoreError(f"blob {sha}: the xz stream is truncated")
                while True:
                    out = decoder.decompress(data, max_length=min(CHUNK, nbytes + 1 - produced))
                    data = b""
                    if out:
                        produced += len(out)
                        if produced > nbytes:
                            raise StoreError(
                                f"blob {sha}: decompresses to more than the {nbytes} bytes the index records"
                            )
                        digest.update(out)
                        yield out
                    if decoder.eof or decoder.needs_input:
                        break
        except lzma.LZMAError as error:
            raise StoreError(f"blob {sha}: xz decoding failed: {error}") from None
        if decoder.unused_data or handle.read(1):
            raise StoreError(f"blob {sha}: data follows the end of the xz stream")
    if produced != nbytes:
        raise StoreError(f"blob {sha}: decompressed to {produced} bytes, index records {nbytes}")
    if digest.hexdigest() != sha:
        raise StoreError(f"blob {sha}: decompressed content hashes to {digest.hexdigest()}")


def _lookup(index: dict[str, Any], tree: str, path: str) -> dict[str, Any]:
    for candidate in index["trees"]:
        if candidate["name"] == tree:
            for entry in candidate["files"]:
                if entry["path"] == path:
                    return entry
            raise StoreError(f"tree `{tree}` has no file `{path}`")
    raise StoreError(f"store has no tree `{tree}`")


def read_file(store: str, index: dict[str, Any], tree: str, path: str) -> bytes:
    """One whole file, returned only after its length and digest match."""
    entry = _lookup(index, tree, path)
    return b"".join(iter_blob(store, entry["sha256"], entry["bytes"]))


# ---- verification -----------------------------------------------------------


def writer_state(store: str) -> list[str]:
    """Writer state a reader ignores: a lock file or a `tmp/` directory."""
    notes: list[str] = []
    lock = os.path.join(store, "store.lock")
    if os.path.lexists(lock):
        try:
            with open(lock, "rb") as handle:
                holder = handle.read(256).decode("utf-8", "replace").strip()
        except OSError:
            holder = "unreadable"
        notes.append(f"store.lock is present ({holder}); a writer may be running or may have crashed")
    tmp = os.path.join(store, "tmp")
    if os.path.lexists(tmp):
        try:
            count = len(os.listdir(tmp))
        except OSError:
            count = -1
        notes.append(f"tmp/ is present ({count} entries); leftover from a writer, ignored")
    return notes


def _check_layout(store: str, referenced: set[str], findings: list[dict[str, str]]) -> None:
    def add(kind: str, path: str, detail: str) -> None:
        findings.append({"kind": kind, "path": path, "detail": detail})

    for name in sorted(os.listdir(store)):
        if name not in ("store.json", "blobs", "store.lock", "tmp"):
            add("unexpected_entry", name, "only store.json and blobs/ may appear in a store")
    blobs = os.path.join(store, "blobs")
    try:
        info = os.lstat(blobs)
    except FileNotFoundError:
        return
    if not stat.S_ISDIR(info.st_mode):
        add("layout", "blobs", "blobs is not a directory")
        return
    for fanout in sorted(os.listdir(blobs)):
        relative = os.path.join("blobs", fanout)
        fanout_path = os.path.join(blobs, fanout)
        if not _FANOUT.match(fanout) or not stat.S_ISDIR(os.lstat(fanout_path).st_mode):
            add("misnamed_blob", relative, "blobs/ may contain only two-character lowercase hex directories")
            continue
        for name in sorted(os.listdir(fanout_path)):
            blob_relative = os.path.join(relative, name)
            digest = name[:-3] if name.endswith(".xz") else ""
            if not _SHA256.match(digest) or digest[:2] != fanout:
                add("misnamed_blob", blob_relative, "a blob must be named <h0h1>/<sha256>.xz")
            elif digest not in referenced:
                add("unreferenced_blob", blob_relative, "no entry in the index references this blob")
            elif not stat.S_ISREG(os.lstat(os.path.join(fanout_path, name)).st_mode):
                add("not_regular_blob", blob_relative, "the blob is not a regular file")


def verify_store(store: str) -> dict[str, Any]:
    """Verify a whole store and return a report; ``status`` is ``verified``
    only if the index, every blob, and the directory layout all pass."""
    report: dict[str, Any] = {
        "store": store,
        "status": "failed",
        "trees": 0,
        "files": 0,
        "distinct_blobs": 0,
        "uncompressed_bytes": 0,
        "stored_bytes": 0,
        "finding_count": 0,
        "findings": [],
        "writer_state": [],
    }
    try:
        index = load_index(store)
    except StoreError as error:
        report["finding_count"] = 1
        report["findings"] = [{"kind": "invalid_index", "path": "store.json", "detail": str(error)}]
        return report
    findings: list[dict[str, str]] = []
    referenced: dict[str, int] = {}
    for tree in index["trees"]:
        report["files"] += len(tree["files"])
        for entry in tree["files"]:
            referenced[entry["sha256"]] = entry["bytes"]
            report["uncompressed_bytes"] += entry["bytes"]
    _check_layout(store, set(referenced), findings)
    for sha, nbytes in sorted(referenced.items()):
        try:
            for _ in iter_blob(store, sha, nbytes):
                pass
        except StoreError as error:
            kind = "missing_blob" if "is missing" in str(error) else "bad_blob"
            findings.append({"kind": kind, "path": blob_relpath(sha), "detail": str(error)})
        else:
            report["stored_bytes"] += os.stat(os.path.join(store, blob_relpath(sha))).st_size
    report.update(
        trees=len(index["trees"]),
        distinct_blobs=len(referenced),
        finding_count=len(findings),
        findings=findings[:MAX_REPORTED_FINDINGS],
        status="verified" if not findings else "failed",
        writer_state=writer_state(store),
    )
    return report


# ---- unpacking --------------------------------------------------------------


def _destination(root: str, relative: str) -> str:
    destination = root
    for component in relative.split("/"):
        if os.path.splitdrive(component)[0] or os.sep in component or (
            os.altsep and os.altsep in component
        ):
            raise StoreError(f"path `{relative}` has a component this platform cannot confine: `{component}`")
        destination = os.path.join(destination, component)
    return destination


def unpack_store(store: str, out: str, trees: Optional[list[str]] = None) -> dict[str, Any]:
    """Write trees byte-identically under a new directory ``out``, verifying
    each file as it is written. ``out`` must not exist; on failure it is removed."""
    index = load_index(store)
    by_name = {tree["name"]: tree for tree in index["trees"]}
    selected = []
    for name in trees or [tree["name"] for tree in index["trees"]]:
        if name not in by_name:
            raise StoreError(f"store has no tree `{name}`")
        if by_name[name] not in selected:
            selected.append(by_name[name])
    if os.path.lexists(out):
        raise StoreError(f"`{out}` already exists; unpacking only writes to a new directory")
    os.makedirs(out)
    files = 0
    total = 0
    written: dict[str, str] = {}
    try:
        for tree in selected:
            tree_root = os.path.join(out, tree["name"])
            os.mkdir(tree_root)
            for entry in tree["files"]:
                destination = _destination(tree_root, entry["path"])
                os.makedirs(os.path.dirname(destination), exist_ok=True)
                first = written.get(entry["sha256"])
                if first is not None:
                    shutil.copyfile(first, destination)
                else:
                    with open(destination, "xb") as handle:
                        for chunk in iter_blob(store, entry["sha256"], entry["bytes"]):
                            handle.write(chunk)
                    written[entry["sha256"]] = destination
                files += 1
                total += entry["bytes"]
    except BaseException:
        shutil.rmtree(out, ignore_errors=True)
        raise
    return {"out": out, "trees": len(selected), "files": files, "bytes": total}


# ---- command line -----------------------------------------------------------


def _cmd_verify(args: argparse.Namespace) -> int:
    report = verify_store(args.store)
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "verified" else 1


def _cmd_unpack(args: argparse.Namespace) -> int:
    print(json.dumps(unpack_store(args.store, args.out, args.tree or None), indent=2))
    return 0


def _cmd_ls(args: argparse.Namespace) -> int:
    index = load_index(args.store)
    if args.tree is None:
        listing: Any = {
            "trees": [
                {"name": t["name"], "files": len(t["files"]), "bytes": sum(f["bytes"] for f in t["files"])}
                for t in index["trees"]
            ]
        }
    else:
        matches = [t for t in index["trees"] if t["name"] == args.tree]
        if not matches:
            raise StoreError(f"store has no tree `{args.tree}`")
        listing = matches[0]
    print(json.dumps(listing, indent=2))
    return 0


def _cmd_cat(args: argparse.Namespace) -> int:
    index = load_index(args.store)
    sys.stdout.buffer.write(read_file(args.store, index, args.tree, args.path))
    return 0


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="store_verify", description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("store-verify", help="Verify an evidence store; exit 1 on failure")
    p.add_argument("store")
    p.set_defaults(func=_cmd_verify)
    p = sub.add_parser("store-unpack", help="Unpack trees into a new directory, verifying each file")
    p.add_argument("store")
    p.add_argument("out")
    p.add_argument("--tree", action="append", help="Unpack only this tree (repeatable)")
    p.set_defaults(func=_cmd_unpack)
    p = sub.add_parser("store-ls", help="List trees, or the files of one tree")
    p.add_argument("store")
    p.add_argument("tree", nargs="?")
    p.set_defaults(func=_cmd_ls)
    p = sub.add_parser("store-cat", help="Write one verified file to standard output")
    p.add_argument("store")
    p.add_argument("tree")
    p.add_argument("path")
    p.set_defaults(func=_cmd_cat)
    return parser


def main(argv: Optional[list[str]] = None) -> int:
    args = build_arg_parser().parse_args(argv)
    try:
        return args.func(args)
    except StoreError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
