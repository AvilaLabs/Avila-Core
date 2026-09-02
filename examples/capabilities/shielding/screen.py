#!/usr/bin/env python3
"""One-dimensional fast-neutron attenuation screen for a layered slab.

Dose rate behind the slab is estimated as the unshielded plane-source dose
rate times exp(-sum(removal_cross_section_i * thickness_i)). Removal cross
sections ignore buildup and spectrum change, so the estimate is an
unqualified guide for searching, never a bounded result. Mass and thickness
are exact arithmetic over the candidate and the material table.
"""

import argparse
import json
import math
from decimal import Decimal, getcontext

getcontext().prec = 40

SCHEMA = "avila.shielding/screen-result/v1"


def canonical(value: Decimal) -> str:
    text = format(value.normalize(), "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    if text in ("", "-0"):
        text = "0"
    return text


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--materials", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    with open(args.candidate, encoding="utf-8") as handle:
        candidate = json.load(handle)
    with open(args.materials, encoding="utf-8") as handle:
        materials = json.load(handle)["materials"]
    with open(args.source, encoding="utf-8") as handle:
        source = json.load(handle)

    if candidate.get("schema") != "avila.shielding/candidate/v1":
        raise SystemExit("candidate schema is not avila.shielding/candidate/v1")
    layers = candidate["layers"]
    if not layers:
        raise SystemExit("candidate has no layers")

    area = Decimal(source["area_cm2"])
    strength = Decimal(source["strength_n_per_s"])
    coefficient = Decimal(source["dose_coefficient_uSv_cm2"])
    unshielded_uSv_h = strength / area * coefficient * Decimal(3600)

    thickness = Decimal(0)
    mass_g = Decimal(0)
    exponent = Decimal(0)
    for layer in layers:
        material = materials[layer["material"]]
        t = Decimal(layer["thickness_cm"])
        if t < 0:
            raise SystemExit("negative layer thickness")
        thickness += t
        mass_g += Decimal(material["density_g_cm3"]) * t * area
        exponent += Decimal(material["removal_cross_section_cm_inv"]) * t
    attenuation = math.exp(-float(exponent))
    dose_rate = Decimal(repr(float(unshielded_uSv_h) * attenuation))

    result = {
        "schema": SCHEMA,
        "method": "plane-source removal-cross-section attenuation; no buildup; screening only",
        "candidate_id": candidate.get("candidate_id"),
        "layers": layers,
        "unshielded_dose_rate": {"value": canonical(unshielded_uSv_h), "unit": "uSv/h"},
        "attenuation_factor": repr(attenuation),
        "dose_rate": {"value": canonical(dose_rate), "unit": "uSv/h"},
        "mass": {"value": canonical(mass_g / Decimal(1000)), "unit": "kg"},
        "thickness": {"value": canonical(thickness), "unit": "cm"},
        "limitations": [
            "Removal cross sections are approximate fission-spectrum values applied to a 14 MeV source; buildup and spectrum softening are ignored.",
            "The dose coefficient is a single approximate value at the source energy.",
            "This result cannot establish a bounded verdict; it guides a search.",
        ],
    }
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
