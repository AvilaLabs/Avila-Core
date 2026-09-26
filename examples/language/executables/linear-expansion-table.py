#!/usr/bin/env python3
"""synthetic/linear-expansion-table@1 — the table-based implementation of
the same postcondition. Its bytes differ from linear-expansion@1 so its
executable digest differs — the observation binds to whichever identity
actually ran. Same contract: `<exe> <inputs_dir> <output_path>`.

The table resolves the product coefficient * temperature_change * length
by iterating exact rational endpoints rather than nested multiplication —
the result is the same enclosure by interval arithmetic.
"""
import json
import sys
from fractions import Fraction
from pathlib import Path


def load_staged(inputs_dir: Path, slot: str) -> tuple[Fraction, Fraction]:
    """A staged ValueDecl: `enclosure` supplies bounds; `exact` is the
    degenerate point interval [v, v]."""
    decl = json.loads((inputs_dir / f"{slot}.json").read_text(encoding="utf-8"))
    kind = decl.get("kind")
    if kind == "enclosure":
        return Fraction(decl["lower"]), Fraction(decl["upper"])
    if kind == "exact":
        v = Fraction(decl["value"])
        return v, v
    raise SystemExit(f"{slot}: expected an enclosure or exact ValueDecl")


def main() -> None:
    inputs_dir = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    ends = [
        load_staged(inputs_dir, "length"),
        load_staged(inputs_dir, "coefficient"),
        load_staged(inputs_dir, "temperature_change"),
    ]
    corners = [
        a * b * c
        for a in ends[0]
        for b in ends[1]
        for c in ends[2]
    ]
    output = {
        "kind": "enclosure",
        "lower": f"{min(corners).numerator}/{min(corners).denominator}",
        "upper": f"{max(corners).numerator}/{max(corners).denominator}",
        "unit": "mm",
    }
    output_path.write_text(
        json.dumps(output, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
