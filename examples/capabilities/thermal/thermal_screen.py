#!/usr/bin/env python3
"""One-dimensional series-thermal-resistance screen for a layered heat spreader.

Hotspot temperature is estimated as ambient + q'' * (sum(t_i / k_i) + 1/h),
using the strip's own flux density q'' as if it passed straight through the
full stack with no lateral spreading at all -- as though adiabatic walls ran
down from the strip's edges to the cooled face. Real lateral conduction can
only open an additional path for the heat to leave by; it cannot close the
straight-through path this estimate already assumes. So this estimate can
only be pessimistic (an upper bound on the true hotspot), never optimistic:
the finite-element step's answer is never above it, and is usually well
below it whenever the layers actually spread the flux across more than the
strip's own width before it reaches the cooled face. The bound is tightest
(closest to the truth) when the strip already covers the whole face, or when
the layers conduct so poorly that little lateral spreading would happen at
this thickness scale regardless; it is loosest for a good lateral conductor
under a narrow strip on a much wider plate, which is exactly the case the
finite-element step is for. See `limitations` below for the full reasoning.

Areal mass and thickness are exact arithmetic over the candidate and the
material table.
"""

import argparse
import json
from decimal import Decimal, getcontext

getcontext().prec = 40

SCHEMA = "avila.thermal/screen-result/v1"


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

    if candidate.get("schema") != "avila.thermal/candidate/v1":
        raise SystemExit("candidate schema is not avila.thermal/candidate/v1")
    layers = candidate["layers"]
    if not layers:
        raise SystemExit("candidate has no layers")

    width_mm = Decimal(source["width_mm"])
    strip_width_mm = Decimal(source["strip_width_mm"])
    if strip_width_mm <= 0 or strip_width_mm > width_mm:
        raise SystemExit("strip_width_mm must be positive and at most width_mm")
    heat_flux = Decimal(source["heat_flux_W_m2"])
    convection = Decimal(source["convection_W_m2K"])
    if convection <= 0:
        raise SystemExit("convection_W_m2K must be positive")
    ambient = Decimal(source["ambient_K"])

    thickness_mm = Decimal(0)
    mass_kg_m2 = Decimal(0)
    resistance_m2K_W = Decimal(1) / convection
    for layer in layers:
        material = materials[layer["material"]]
        t_mm = Decimal(layer["thickness_mm"])
        if t_mm < 0:
            raise SystemExit("negative layer thickness")
        thickness_mm += t_mm
        t_m = t_mm / Decimal(1000)
        conductivity = Decimal(material["conductivity_W_mK"])
        if conductivity <= 0:
            raise SystemExit(f"material `{layer['material']}` has non-positive conductivity")
        density = Decimal(material["density_kg_m3"])
        mass_kg_m2 += density * t_m
        resistance_m2K_W += t_m / conductivity

    hotspot = ambient + heat_flux * resistance_m2K_W

    result = {
        "schema": SCHEMA,
        "method": (
            "one-dimensional series thermal resistance under the strip's own flux density, "
            "with no credit for lateral spreading; pessimistic (an upper bound on the true "
            "hotspot) because any real lateral spreading can only relieve, never worsen, the "
            "peak temperature this estimate already assumes with no relief at all"
        ),
        "candidate_id": candidate.get("candidate_id"),
        "layers": layers,
        "hotspot_temperature": {"value": canonical(hotspot), "unit": "K"},
        "areal_mass": {"value": canonical(mass_kg_m2), "unit": "kg"},
        "thickness": {"value": canonical(thickness_mm), "unit": "mm"},
        "limitations": [
            "One-dimensional series resistance under the strip's own flux density, as if the "
            "strip's column ran straight through the stack to the cooled face with adiabatic "
            "walls at the strip's edges: no lateral spreading credit at all. Real conduction "
            "can only add a path for heat to leave sideways into cooler surrounding material; "
            "it cannot remove the straight-through path this estimate already counts. So this "
            "estimate is a conservative (pessimistic) upper bound on the true hotspot, not a "
            "best estimate, and the finite-element step's answer should never exceed it.",
            "The bound is loosest exactly where lateral spreading matters most: a "
            "high-conductivity layer under a narrow strip on a much wider plate. It tightens "
            "toward the truth as the strip width approaches the full plate width (where "
            "spreading is geometrically impossible and this estimate is exact -- see the "
            "finite-element script's own full-face-flux reduction test) or as every layer's "
            "conductivity falls low enough that little lateral conduction would occur at this "
            "thickness scale regardless of geometry.",
            "Convection is a single coefficient to a fixed ambient; no radiation, no "
            "layer-to-layer contact resistance, and no heat loss from the plate's cut edges "
            "is modeled.",
            "This result cannot establish a bounded verdict; it guides a search.",
        ],
    }
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
