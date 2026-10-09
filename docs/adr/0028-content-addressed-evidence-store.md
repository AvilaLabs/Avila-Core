# ADR-0028: Content-addressed evidence store

Status: accepted (2026-10-08)

## Context

A FARIS release ships four Core cases and their four run workspaces. Expanded,
they are 823 MB, more than twenty times the application. Only 296 MB of that is
distinct file content, and compressed it is about 10 MB. Two causes are Core's:

- `execute_step` copies every staged input into the step directory
  (`inputs/<slot>.json`), so a workspace holds each upstream output twice.
- Case packages bind expected outputs as ordinary files, so the same bytes
  appear again in the case.

Core's architecture already states that material objects are immutable and
content-addressed (EVIDENCE_MODEL.md) and plans an artifact store
(ARCHITECTURE.md `core-artifacts`; roadmap proposal 2026-09-04, item B, "import
immutable bytes once"). Nothing implements it.

## Decision

Core defines one on-disk format, the **evidence store**, that holds any number
of named file trees (a case package, a workspace, an export) and stores each
distinct file content once, compressed. Identity is unchanged: a file's identity
is the SHA-256 of its exact uncompressed bytes, the same digest receipts,
packages and claims already record. Receipts, invocation identities, manifests
and signatures do not change.

### Format `avila.core/evidence-store/v0.1`

A store is a directory containing exactly:

- `store.json`: the index (below).
- `blobs/`: one file per distinct content, at
  `blobs/<h0h1>/<h>.xz`, where `<h>` is the 64-character lowercase hex SHA-256
  of the uncompressed bytes and `<h0h1>` its first two characters.

`store.json` is UTF-8 JSON:

```json
{
  "schema_version": "avila.core/evidence-store/v0.1",
  "codec": "xz",
  "trees": [
    {
      "name": "case",
      "files": [
        { "path": "expected/history.json", "sha256": "<64 hex>", "bytes": 15912345 }
      ]
    }
  ]
}
```

Rules:

1. `trees` is sorted by `name`; names are unique, 1 to 128 characters from
   `[A-Za-z0-9._-]`, and not `.` or `..`.
2. Each tree's `files` is sorted by `path` (bytewise UTF-8 order); paths are
   unique within a tree. A path is relative, `/`-separated, has 1 to 64
   components, and no component is empty, `.` or `..`; it contains no `\`,
   NUL or control character and does not start with `/`. No path is a proper
   directory prefix of another path in the same tree.
3. `sha256` is the digest of the uncompressed bytes; `bytes` is their length.
   Entries with the same `sha256` must have the same `bytes`.
4. Every referenced digest has exactly one blob. A blob is a regular file (not
   a symlink) containing one xz stream (LZMA2) whose decompressed bytes have
   that digest and length. Writers use xz preset 9; readers accept any valid
   xz stream, since identity is over the uncompressed bytes.
5. Empty directories, file modes, timestamps and ownership are not
   represented. Unpacking yields regular files only.
6. Bounds: `store.json` at most 64 MiB; at most 1,024 trees, 65,536 files per
   tree, 262,144 files in total; each file at most 4 GiB uncompressed.

Reading a file means: look up its entry, open its blob, decompress with the
output bounded to `bytes` + 1, and accept the content only if the length equals
`bytes` and the SHA-256 equals `sha256`. A reader must never return unverified
content to a caller as verified, and must refuse a blob that is a symlink or
not a regular file.

Verifying a store means: the index is well-formed under rules 1 to 6, every
blob referenced by the index exists and decodes to its digest and length, and
`blobs/` contains nothing else (an unreferenced or misnamed blob is an error).

### Why xz

The Python verifier promises the standard library only. Python has `lzma` in
every supported version; `zstd` only from 3.14. xz at preset 9 is also slightly
denser than zstd for these JSON files. Decompression speed (about 100 MB/s) is
adequate for evidence trees of a few hundred MB.

### Operations

- `avila-core store pack --out STORE NAME=DIR...`: create a new store from
  directories (refusing symlinks, special files and an existing `--out`).
- `avila-core store unpack STORE --out DIR [--tree NAME]...`: materialize trees
  byte-identically as `DIR/<name>/<path>`, verifying each file.
- `avila-core store verify STORE`: the verification above; JSON report.
- `avila-core store cat STORE TREE PATH`: write one verified file to stdout.
- `avila-core store ls STORE [TREE]`: list trees or a tree's files.

The Python verifier gains the same read and verify operations with `lzma`.

## Consequences

- A case and its workspace packed into one store share every identical file:
  staged inputs, outputs and expected outputs become one blob each.
- Compression happens once at pack time; nothing about execution changes in
  this ADR.
- Follow-up (separate change): Core verification (`verify_case_package`,
  `verify_receipt`) reads store-backed trees in place, and `run` can write a
  workspace straight into a store, so evidence never needs to be expanded to
  be checked.
- The store is not a security boundary: anyone can write a store. Integrity
  comes from checking digests against receipts and packages, as before.

## Amendment 1 (2026-10-08): stores in place of directories

Measured after the format landed: three local runs of the coupled-shield case
(`case-002`) leave 516 MB workspaces each. Nothing inside one run repeats, but
every run stages the same 238 MB of nuclear data and the 9 MB ACTINV tool, and
the activation step leaves a 275 MB ACTINV preparation cache in its directory
that no output declares. The original decision removes duplication inside one
pack; this amendment lets Core work on stores directly, share them across runs,
and keep only evidence. Nothing above is changed except rule 4's preset
sentence, as stated in A2.

**A1. Appendable stores.** A store may gain trees after it is created. A writer
holds an exclusive lock, `store.lock` (created exclusively, holding the writer's
process id and host; an existing lock refuses the write and says how to clear a
stale one). New blobs are written under `tmp/` and renamed into `blobs/`; a blob
that already exists is kept after it verifies. The new index is written to
`tmp/` and renamed over `store.json`, so readers see the old or the new index,
never a partial one. Trees already in the store are never changed or removed by
adding; adding an existing tree name is refused. `store.lock` and `tmp/` are
writer state: readers ignore them, and `store verify` reports their presence
without failing. Removing trees (garbage collection) is out of scope.

**A2. Writer preset.** Rule 4's "Writers use xz preset 9" becomes: `store pack`
uses preset 9; run persistence (A4) uses preset 6. Identity is over the
uncompressed bytes, so the preset never affects verification.

**A3. Reading cases and workspaces from a store.** Wherever a command takes a
case package directory, a source root directory or a workspace directory to
read or verify, it also accepts `store:<STORE_DIR>#<TREE>`. Reading a file from
such a root is a verified store read (length and SHA-256 checked before the
bytes are used); a path that is not in the tree's index is missing. Package,
receipt and campaign verification therefore give the same results from a store
tree as from the same files on disk. Commands that write (run, export) still
write to directories unless A4 applies.

**A4. Run persistence: `run --store STORE_DIR`.** Steps still execute in a
scratch step directory, because programs need real files. After a step's
receipt verifies, Core puts the step's receipt, declared inputs, declared
outputs and logs into the store and deletes the step directory, including any
file the step did not declare (scratch). A downstream step's staged inputs are
materialized from the store with a verified read. When the run ends, Core adds
one tree, named from the case id, run timestamp and process id, laid out
exactly as the workspace would be with scratch removed (`<step>/receipt.json`,
`<step>/inputs/…`, `<step>/outputs/…`, `<step>/logs/…`, and the run's
reports), and prints its `store:` address. Identical files across runs, such as
nuclear data and tools, are stored once. `--keep-scratch` keeps each step
directory as it is today, for debugging. Without `--store`, `run` behaves
exactly as before. Receipts, invocation identities and reports are unchanged:
they record the same workspace paths and digests.

**A4 note (2026-10-08).** Putting a blob needs no lock: a blob is written under
`tmp/` and renamed to its content address, so concurrent writers of the same
content produce the same file and a reader never sees a partial blob. Only the
index update that adds a tree takes `store.lock`. A run therefore writes blobs
as its steps finish and holds the lock only for the moment it adds its tree,
so several runs can share one store. A run whose store does not exist yet
creates an empty one. A run that fails still adds its tree (failed receipts are
evidence) and keeps the failed step's directory for inspection, saying where.

**A4 note 2 (2026-10-08).** To verify one step of a stored run in place, a
store address may name a directory inside a tree: `store:<STORE_DIR>#<TREE>/<DIR>`
(tree names cannot contain `/`, so the split is unambiguous). Such a root reads
only files under `<DIR>/`, with that prefix removed, and is missing if the tree
has no file there. Adding a tree waits up to 30 seconds for a lock another
run holds, then reports it as `store.lock` always has. The tree is the files
the run put, so a run that is interrupted leaves blobs no tree references;
`store verify` reports them as unreferenced and they can be removed once
garbage collection exists. The run's JSON output carries the address in a
`store` field; `run-report.json` is written before it is known and is the same
with and without a store.

## Amendment 2 (2026-10-09): stores by default; export reports from a store

Measured before this amendment: a directory run of CASE-002 leaves 516 MB
(238 MB nuclear data and 9 MB of tool staged per run, 275 MB of undeclared
preparation cache); two directory runs take 1,081 MB, two store runs 128 MB,
and a second store run adds about 30 KB. The 1.7 GB of old run folders in the
maintainer's checkout are directory runs, most made with an explicit
`--workspace`. Opt-in persistence therefore does not reach the runs that cost
the space. FARIS verifies the export identity of each saved case by unpacking
it and running `avila-core export`, which needs scratch space for the case and
its export copy although A3 already lets `export` read from a store.

**A5. Store by default.** `avila-core run` persists every run that executes a
step into an evidence store, as A4 describes, unless `--directory` is given.
The store is `--store DIR` when given; otherwise it is the folder
`evidence-store` beside the run's workspace (the workspace's parent directory
joined with `evidence-store`), whether the workspace was named with
`--workspace` or defaulted to `workspaces/<case>/<run>`. Sibling runs therefore
share one store, and files they have in common are kept once. The workspace
stays the scratch location and must still be fresh; after a successful run it
is removed, and a failed step's directory is kept and named as A4 says. A run
that executes nothing and so creates no workspace adds no tree and creates no
store. `--directory` restores the earlier behaviour exactly: the workspace is
left as it is and no store is written; it cannot be combined with `--store` or
`--keep-scratch`. The run's JSON output carries the tree address in `store` as
before, and the human-readable output prints it. Receipts, invocation
identities and reports are unchanged by where the run is persisted. Callers
that read a run's workspace folder after the run either pass `--directory` or
read the printed `store:` address (A3).

**A6. Export reports without writing.** `avila-core export --report-only`
reads the case and its source roots exactly as `export --out` does (from
directories or `store:` addresses, every read verified), and prints the export
report `export --out` would write, byte for byte, without writing anything.
The digests it reports are those of the bytes that would be copied, which are
the verified bytes read. Exactly one of `--out` and `--report-only` is
required.
