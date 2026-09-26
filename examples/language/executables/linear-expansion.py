#!/usr/bin/env python3
"""synthetic/linear-expansion@1 — controlled synthetic executable.

Contract: `<exe> <inputs_dir> <output_path>`. Reads the staged ValueDecl
documents `length.json`, `coefficient.json`, `temperature_change.json`
from <inputs_dir> and writes the output ValueDecl as canonical JSON —
sorted keys, minimal separators — to <output_path>.

output = coefficient * temperature_change * length over exact rational
enclosures (the postcondition the method declares).
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


def mul(a: tuple[Fraction, Fraction], b: tuple[Fraction, Fraction]) -> tuple[Fraction, Fraction]:
    corners = (a[0] * b[0], a[0] * b[1], a[1] * b[0], a[1] * b[1])
    return min(corners), max(corners)


def main() -> None:
    inputs_dir = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    length = load_staged(inputs_dir, "length")
    coefficient = load_staged(inputs_dir, "coefficient")
    temperature_change = load_staged(inputs_dir, "temperature_change")
    lower, upper = mul(mul(coefficient, temperature_change), length)
    output = {
        "kind": "enclosure",
        "lower": f"{lower.numerator}/{lower.denominator}",
        "upper": f"{upper.numerator}/{upper.denominator}",
        "unit": "mm",
    }
    output_path.write_text(
        json.dumps(output, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
