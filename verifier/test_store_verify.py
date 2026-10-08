#!/usr/bin/env python3
"""Tests for the evidence-store reader (ADR-0028).

Run with: python3 -m unittest test_store_verify -v   (from verifier/)

The refusal tests build stores by hand with the standard library, so they
need no Rust binary. ``CrossImplementationTest`` packs a tree with the Rust
CLI and checks that this verifier accepts and unpacks it byte-identically; it
is skipped unless the binary is found, via ``AVILA_CORE_BIN`` or the usual
``target/{debug,release}/avila-core`` (also under ``CARGO_TARGET_DIR``).
"""

from __future__ import annotations

import hashlib
import json
import lzma
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import store_verify as sv  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def xz(data: bytes) -> bytes:
    return lzma.compress(data, format=lzma.FORMAT_XZ, preset=1)


def entry(path: str, data: bytes) -> dict:
    return {"path": path, "sha256": sha(data), "bytes": len(data)}


def index_of(trees: list) -> dict:
    return {"schema_version": sv.SCHEMA_VERSION, "codec": "xz", "trees": trees}


class StoreTestCase(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="avila-core-store-verify-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.store = self.tmp / "store"

    def write_store(self, index: dict, contents: list[bytes]) -> None:
        shutil.rmtree(self.store, ignore_errors=True)
        (self.store / "blobs").mkdir(parents=True)
        for data in contents:
            self.write_blob(sha(data), xz(data))
        (self.store / "store.json").write_text(json.dumps(index))

    def write_blob(self, digest: str, compressed: bytes) -> Path:
        path = self.store / sv.blob_relpath(digest)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(compressed)
        return path

    def sample(self) -> dict:
        self.files = {
            "package.json": b'{"package":true}\n',
            "expected/history.json": b"shared content",
            "deep/er/file.txt": b"deep",
            "empty.bin": b"",
        }
        work = {
            "log.txt": b"log\n",
            "steps/s1/history.json": b"shared content",
        }
        index = index_of(
            [
                {"name": "case", "files": [entry(p, d) for p, d in sorted(self.files.items())]},
                {"name": "work", "files": [entry(p, d) for p, d in sorted(work.items())]},
            ]
        )
        self.write_store(index, list({*self.files.values(), *work.values()}))
        return index

    def kinds(self) -> list[str]:
        return [f["kind"] for f in sv.verify_store(str(self.store))["findings"]]


class RoundTripTest(StoreTestCase):
    def test_verifies_and_unpacks_byte_identically(self) -> None:
        self.sample()
        report = sv.verify_store(str(self.store))
        self.assertEqual(report["status"], "verified", report)
        self.assertEqual((report["trees"], report["files"], report["distinct_blobs"]), (2, 6, 5))
        out = self.tmp / "out"
        result = sv.unpack_store(str(self.store), str(out))
        self.assertEqual(result["files"], 6)
        for path, data in self.files.items():
            self.assertEqual((out / "case" / path).read_bytes(), data)
        self.assertEqual((out / "work/steps/s1/history.json").read_bytes(), b"shared content")
        self.assertEqual(
            sv.read_file(str(self.store), sv.load_index(str(self.store)), "case", "deep/er/file.txt"),
            b"deep",
        )

    def test_unpack_refuses_existing_target_and_selects_trees(self) -> None:
        self.sample()
        (self.tmp / "taken").mkdir()
        with self.assertRaises(sv.StoreError):
            sv.unpack_store(str(self.store), str(self.tmp / "taken"))
        self.assertTrue((self.tmp / "taken").is_dir())
        sv.unpack_store(str(self.store), str(self.tmp / "one"), ["work"])
        self.assertTrue((self.tmp / "one/work/log.txt").is_file())
        self.assertFalse((self.tmp / "one/case").exists())
        with self.assertRaises(sv.StoreError):
            sv.unpack_store(str(self.store), str(self.tmp / "two"), ["nope"])
        self.assertFalse((self.tmp / "two").exists())


class BlobRefusalTest(StoreTestCase):
    def test_tampered_blob(self) -> None:
        self.sample()
        self.write_blob(sha(b"deep"), xz(b"DEEP"))
        self.assertEqual(self.kinds(), ["bad_blob"])
        with self.assertRaises(sv.StoreError):
            sv.unpack_store(str(self.store), str(self.tmp / "out"))
        self.assertFalse((self.tmp / "out").exists(), "failed unpack leaves no output")

    def test_bomb_is_bounded(self) -> None:
        self.sample()
        bomb = xz(bytes(64 * 1024 * 1024))
        self.assertLess(len(bomb), 64 * 1024)
        self.write_blob(sha(b"deep"), bomb)
        produced = 0
        with self.assertRaisesRegex(sv.StoreError, "more than the 4 bytes"):
            for chunk in sv.iter_blob(str(self.store), sha(b"deep"), 4):
                produced += len(chunk)
        self.assertLessEqual(produced, 5)

    def test_short_truncated_trailing_and_garbage(self) -> None:
        self.sample()
        good = xz(b"deep")
        for label, blob in [
            ("short", xz(b"dee")),
            ("truncated", good[:-4]),
            ("trailing", good + b"junk"),
            ("concatenated", good + good),
            ("garbage", b"not xz at all"),
        ]:
            with self.subTest(label):
                self.write_blob(sha(b"deep"), blob)
                self.assertEqual(self.kinds(), ["bad_blob"])

    def test_missing_blob(self) -> None:
        self.sample()
        (self.store / sv.blob_relpath(sha(b"deep"))).unlink()
        self.assertEqual(self.kinds(), ["missing_blob"])

    def test_extra_and_misnamed_blobs(self) -> None:
        self.sample()
        extra = b"not in the index"
        self.write_blob(sha(extra), xz(extra))
        self.assertEqual(self.kinds(), ["unreferenced_blob"])
        (self.store / sv.blob_relpath(sha(extra))).unlink()
        real = self.store / sv.blob_relpath(sha(b"deep"))
        (real.parent / "notes.txt").write_text("x")
        wrong = self.store / "blobs" / "00" / real.name
        wrong.parent.mkdir()
        wrong.write_bytes(real.read_bytes())
        (self.store / "README").write_text("x")
        self.assertEqual(
            sorted(self.kinds()), ["misnamed_blob", "misnamed_blob", "unexpected_entry"]
        )

    @unittest.skipIf(os.name == "nt", "symlinks need privileges on Windows")
    def test_symlink_blob_and_directory(self) -> None:
        self.sample()
        blob = self.store / sv.blob_relpath(sha(b"deep"))
        elsewhere = self.tmp / "elsewhere.xz"
        blob.rename(elsewhere)
        blob.symlink_to(elsewhere)
        self.assertIn("not_regular_blob", self.kinds())
        with self.assertRaisesRegex(sv.StoreError, "not a regular file"):
            list(sv.iter_blob(str(self.store), sha(b"deep"), 4))
        blob.unlink()
        elsewhere.rename(blob)
        fanout = blob.parent
        moved = self.tmp / "moved"
        fanout.rename(moved)
        fanout.symlink_to(moved)
        self.assertEqual(sv.verify_store(str(self.store))["status"], "failed")


class IndexRefusalTest(StoreTestCase):
    def refuses(self, index: dict, needle: str) -> None:
        self.write_store(index, [])
        with self.assertRaisesRegex(sv.StoreError, needle):
            sv.load_index(str(self.store))
        report = sv.verify_store(str(self.store))
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["findings"][0]["kind"], "invalid_index")

    def tree(self, *files: dict, name: str = "t") -> dict:
        return {"name": name, "files": list(files)}

    def test_bad_path_forms(self) -> None:
        long = "/".join(["a"] * 65)
        for path in ["../x", "a/../b", "a/./b", "a//b", "a/", "/abs", "a\\b", "a\x01b", "a\x7fb",
                     "a\x00b", "a\x85b", "", ".", long]:
            with self.subTest(path=path):
                self.refuses(index_of([self.tree(entry(path, b"x"))]), "path|component|absolute|empty")
        self.write_store(index_of([self.tree(entry("/".join(["a"] * 64), b"x"))]), [])
        sv.load_index(str(self.store))

    def test_prefix_conflict(self) -> None:
        self.refuses(
            index_of([self.tree(entry("a", b"1"), entry("a-x", b"2"), entry("a/b", b"3"))]),
            "directory prefix",
        )

    def test_unsorted_and_duplicates(self) -> None:
        self.refuses(index_of([self.tree(entry("b", b"1"), entry("a", b"2"))]), "not sorted")
        self.refuses(index_of([self.tree(entry("a", b"1"), entry("B", b"2"))]), "not sorted")
        self.refuses(index_of([self.tree(entry("a", b"1"), entry("a", b"1"))]), "more than once")
        self.refuses(index_of([self.tree(name="b"), self.tree(name="a")]), "not sorted")
        self.refuses(index_of([self.tree(name="a"), self.tree(name="a")]), "duplicate tree")

    def test_names_digests_lengths(self) -> None:
        for name in ["", ".", "..", "has space", "a/b", "n" * 129]:
            with self.subTest(name=name):
                self.refuses(index_of([self.tree(name=name)]), "tree name")
        bad = entry("a", b"x")
        bad["sha256"] = sha(b"x").upper()
        self.refuses(index_of([self.tree(bad)]), "sha256")
        bad = entry("a", b"x")
        bad["bytes"] = True
        self.refuses(index_of([self.tree(bad)]), "bytes")
        bad = entry("a", b"x")
        bad["bytes"] = sv.MAX_FILE_BYTES + 1
        self.refuses(index_of([self.tree(bad)]), "bytes")

    def test_same_digest_different_bytes(self) -> None:
        other = entry("b", b"x")
        other["bytes"] = 2
        self.refuses(index_of([self.tree(entry("a", b"x"), other)]), "two different lengths")

    def test_schema_codec_unknown_and_duplicate_keys(self) -> None:
        index = index_of([])
        index["schema_version"] = "avila.core/evidence-store/v0.2"
        self.refuses(index, "schema_version")
        index = index_of([])
        index["codec"] = "zstd"
        self.refuses(index, "codec")
        index = index_of([])
        index["extra"] = 1
        self.refuses(index, "unknown field")
        extra = entry("a", b"x")
        extra["mode"] = 420
        self.refuses(index_of([self.tree(extra)]), "unknown field")
        self.write_store(index_of([]), [])
        (self.store / "store.json").write_text(
            '{"schema_version":"%s","codec":"xz","codec":"xz","trees":[]}' % sv.SCHEMA_VERSION
        )
        with self.assertRaisesRegex(sv.StoreError, "duplicate key"):
            sv.load_index(str(self.store))

    def test_oversized_index(self) -> None:
        self.write_store(index_of([]), [])
        with open(self.store / "store.json", "wb") as handle:
            handle.truncate(sv.MAX_INDEX_BYTES + 1)
        with self.assertRaisesRegex(sv.StoreError, "larger than"):
            sv.load_index(str(self.store))


def find_binary() -> str | None:
    candidates = []
    if os.environ.get("AVILA_CORE_BIN"):
        candidates.append(Path(os.environ["AVILA_CORE_BIN"]))
    exe = "avila-core.exe" if os.name == "nt" else "avila-core"
    targets = [Path(os.environ["CARGO_TARGET_DIR"])] if os.environ.get("CARGO_TARGET_DIR") else []
    targets.append(REPO_ROOT / "target")
    for target in targets:
        for profile in ("release", "debug"):
            candidates.append(target / profile / exe)
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    return None


@unittest.skipUnless(find_binary(), "avila-core binary not built (set AVILA_CORE_BIN)")
class CrossImplementationTest(StoreTestCase):
    def cli(self, *args: str, check: bool = True) -> subprocess.CompletedProcess:
        return subprocess.run(
            [find_binary(), *args], capture_output=True, check=check, timeout=300
        )

    def make_sources(self) -> dict[str, Path]:
        case, work = self.tmp / "case-src", self.tmp / "work-src"
        for root in (case, work):
            (root / "nested/deeper").mkdir(parents=True)
        (case / "package.json").write_bytes(b'{"id":"case"}\n')
        (case / "empty.bin").write_bytes(b"")
        big = b"".join(hashlib.sha256(bytes([i % 251, i // 251])).digest() for i in range(40_000))
        (case / "nested/big.bin").write_bytes(big)
        (work / "nested/deeper/big-copy.bin").write_bytes(big)
        (work / "unicode-éè.txt").write_bytes("café\n".encode())
        return {"case": case, "work": work}

    @staticmethod
    def digests(root: Path) -> dict[str, str]:
        return {
            p.relative_to(root).as_posix(): sha(p.read_bytes())
            for p in sorted(root.rglob("*"))
            if p.is_file()
        }

    def test_rust_store_verifies_and_unpacks_here(self) -> None:
        sources = self.make_sources()
        self.cli("store", "pack", "--out", str(self.store), *[f"{n}={p}" for n, p in sources.items()])
        report = sv.verify_store(str(self.store))
        self.assertEqual(report["status"], "verified", report)
        self.assertEqual(report["distinct_blobs"], 4)

        out = self.tmp / "py-out"
        sv.unpack_store(str(self.store), str(out))
        rust_out = self.tmp / "rust-out"
        self.cli("store", "unpack", str(self.store), "--out", str(rust_out))
        for name, source in sources.items():
            self.assertEqual(self.digests(out / name), self.digests(source))
            self.assertEqual(self.digests(rust_out / name), self.digests(source))

        self.assertEqual(
            self.cli("store", "cat", str(self.store), "case", "package.json").stdout,
            sv.read_file(str(self.store), sv.load_index(str(self.store)), "case", "package.json"),
        )
        self.assertEqual(
            json.loads(self.cli("store", "verify", str(self.store)).stdout)["status"], "verified"
        )

    def test_both_implementations_reject_a_tampered_rust_store(self) -> None:
        sources = self.make_sources()
        self.cli("store", "pack", "--out", str(self.store), *[f"{n}={p}" for n, p in sources.items()])
        data = (sources["case"] / "package.json").read_bytes()
        self.write_blob(sha(data), xz(b'{"id":"evil"}\n'))
        self.assertEqual(sv.verify_store(str(self.store))["status"], "failed")
        result = self.cli("store", "verify", str(self.store), check=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["status"], "failed")


if __name__ == "__main__":
    unittest.main()
