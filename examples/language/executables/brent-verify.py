#!/usr/bin/env python3
"""synthetic/brent-verify@1 — controlled synthetic executable.

Contract: `<exe> <inputs_dir> <output_path>`. Reads the staged ValueDecl
documents `candidate_rank.json`, `parity_equations.json` from <inputs_dir>
and writes the violation-count ValueDecl as canonical JSON to
<output_path>.

This is a controlled fixture, not a real Brent parity check: the point of
the demonstration is that the record *binds* — plan, receipt, staged
inputs, and output digests — not that the arithmetic is verified. A real
verifier consumes the term list itself, which is opaque to the typed-value
boundary and would be staged as case evidence rather than a quantity.
"""
import json
import sys
from pathlib import Path


def load_staged(inputs_dir: Path, slot: str) -> None:
    decl = json.loads((inputs_dir / f"{slot}.json").read_text(encoding="utf-8"))
    if decl.get("kind") != "exact":
        raise SystemExit(f"{slot}: expected an exact ValueDecl")


def main() -> None:
    inputs_dir = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    load_staged(inputs_dir, "candidate_rank")
    load_staged(inputs_dir, "parity_equations")
    output = {"kind": "exact", "value": "0", "unit": "1"}
    output_path.write_text(
        json.dumps(output, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
