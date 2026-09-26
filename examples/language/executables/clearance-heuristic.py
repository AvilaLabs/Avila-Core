#!/usr/bin/env python3
"""synthetic/clearance-heuristic@1 — heuristic clearance estimate.

Contract: `<exe> <inputs_dir> <output_path>`. Reads the staged ValueDecl
documents `initial_clearance.json` and `displacement.json` and writes a
`nominal` ValueDecl — the method declares no postcondition, so the output
carries an asserted claim only; downstream bounded requirements cannot
admit it.
"""
import json
import sys
from fractions import Fraction
from pathlib import Path


def load_staged(inputs_dir: Path, slot: str) -> tuple[Fraction, Fraction]:
    decl = json.loads((inputs_dir / f"{slot}.json").read_text(encoding="utf-8"))
    kind = decl.get("kind")
    if kind == "enclosure":
        return Fraction(decl["lower"]), Fraction(decl["upper"])
    if kind == "exact":
        v = Fraction(decl["value"])
        return v, v
    raise SystemExit(f"{slot}: expected an enclosure or exact ValueDecl")


def midpoint(bounds: tuple[Fraction, Fraction]) -> Fraction:
    return (bounds[0] + bounds[1]) / 2


def main() -> None:
    inputs_dir = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    initial = midpoint(load_staged(inputs_dir, "initial_clearance"))
    displacement = midpoint(load_staged(inputs_dir, "displacement"))
    estimate = initial - displacement
    output = {
        "kind": "nominal",
        "value": f"{estimate.numerator}/{estimate.denominator}",
        "unit": "mm",
    }
    output_path.write_text(
        json.dumps(output, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
