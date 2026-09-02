#!/usr/bin/env python3
"""Monte Carlo transport of a layered slab with OpenMC.

Geometry: layers along x starting at x = 0, a vacuum gap in front, reflective
lateral boundaries so the plane source is effectively infinite, and a thin
detector cell of air behind the last layer. The plane source is isotropic and
uniform over the slab face. The flux tally in the detector is folded with the
ICRP-116 anterior-posterior neutron dose coefficients shipped with OpenMC and
scaled to the declared source strength over the declared area. The reported
interval is the statistical estimate at the declared coverage; it contains no
nuclear-data, geometry, or model uncertainty.
"""

import argparse
import hashlib
import json
import os
import sys
from decimal import Decimal, getcontext

getcontext().prec = 40
SIGNIFICANT_DIGITS = 8


def stable(value: float) -> Decimal:
    """Round a threaded tally statistic to SIGNIFICANT_DIGITS significant digits.

    OpenMC reduces per-thread tallies in an order that varies from run to run,
    which perturbs the last binary digits of the mean and standard deviation.
    Those digits carry no information (the statistical uncertainty is orders
    of magnitude larger) but they would change the output bytes, and Avila
    Core binds outputs by their bytes. Rounding removes them so the same seed
    reproduces the same document.
    """
    decimal = Decimal(repr(value))
    if decimal == 0:
        return Decimal(0)
    quantum = Decimal(1).scaleb(decimal.adjusted() - SIGNIFICANT_DIGITS + 1)
    return decimal.quantize(quantum)

SCHEMA = "avila.shielding/transport-result/v1"
COVERAGE = "0.95"
K_95 = 1.959964


def sha256_of(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


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
    parser.add_argument("--cross-sections-index", required=True)
    parser.add_argument("--particles", type=int, required=True)
    parser.add_argument("--batches", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    library = os.environ.get("OPENMC_CROSS_SECTIONS")
    if not library:
        raise SystemExit("OPENMC_CROSS_SECTIONS is not set")
    staged_digest = sha256_of(args.cross_sections_index)
    library_digest = sha256_of(library)
    if staged_digest != library_digest:
        raise SystemExit(
            f"the data library in use ({library_digest}) is not the attested index ({staged_digest})"
        )

    import openmc  # noqa: E402  (imported after the environment checks)
    import openmc.data  # noqa: E402

    with open(args.candidate, encoding="utf-8") as handle:
        candidate = json.load(handle)
    with open(args.materials, encoding="utf-8") as handle:
        table = json.load(handle)["materials"]
    with open(args.source, encoding="utf-8") as handle:
        source = json.load(handle)
    if candidate.get("schema") != "avila.shielding/candidate/v1":
        raise SystemExit("candidate schema is not avila.shielding/candidate/v1")

    lateral = float(source["lateral_cm"])
    half = lateral / 2.0
    detector_thickness = float(source["detector_thickness_cm"])
    energy_eV = float(source["energy_MeV"]) * 1.0e6
    strength = Decimal(source["strength_n_per_s"])
    area = Decimal(source["area_cm2"])
    if Decimal(repr(lateral * lateral)) != area:
        raise SystemExit("source area must equal the square of the lateral extent for this slab model")

    materials = []
    cells = []
    x_gap = openmc.XPlane(-1.0, boundary_type="vacuum")
    ymin = openmc.YPlane(-half, boundary_type="reflective")
    ymax = openmc.YPlane(half, boundary_type="reflective")
    zmin = openmc.ZPlane(-half, boundary_type="reflective")
    zmax = openmc.ZPlane(half, boundary_type="reflective")
    lateral_region = +ymin & -ymax & +zmin & -zmax
    previous = openmc.XPlane(0.0)
    cells.append(openmc.Cell(name="gap", region=+x_gap & -previous & lateral_region))
    position = 0.0
    for index, layer in enumerate(candidate["layers"]):
        spec = table[layer["material"]]
        thickness = float(layer["thickness_cm"])
        if thickness <= 0:
            continue
        material = openmc.Material(name=f"layer-{index}-{layer['material']}")
        material.set_density("g/cm3", float(spec["density_g_cm3"]))
        basis = spec["composition"]["basis"]
        for element, fraction in spec["composition"]["elements"].items():
            material.add_element(element, float(fraction), basis)
        materials.append(material)
        position += thickness
        boundary = openmc.XPlane(position)
        cells.append(openmc.Cell(name=material.name, fill=material, region=+previous & -boundary & lateral_region))
        previous = boundary
    air = openmc.Material(name="detector-air")
    air.set_density("g/cm3", 0.0012)
    air.add_element("N", 0.79)
    air.add_element("O", 0.21)
    materials.append(air)
    detector_far = openmc.XPlane(position + detector_thickness)
    detector = openmc.Cell(name="detector", fill=air, region=+previous & -detector_far & lateral_region)
    cells.append(detector)
    tail_far = openmc.XPlane(position + detector_thickness + 1.0, boundary_type="vacuum")
    cells.append(openmc.Cell(name="tail", region=+detector_far & -tail_far & lateral_region))

    openmc.Materials(materials).export_to_xml()
    openmc.Geometry(cells).export_to_xml()

    plane = openmc.IndependentSource()
    plane.space = openmc.stats.Box((-0.5, -half, -half), (-0.5, half, half))
    plane.angle = openmc.stats.Isotropic()
    plane.energy = openmc.stats.Discrete([energy_eV], [1.0])
    settings = openmc.Settings()
    settings.run_mode = "fixed source"
    settings.source = plane
    settings.particles = args.particles
    settings.batches = args.batches
    settings.seed = args.seed
    settings.output = {"summary": False, "tallies": False}
    settings.export_to_xml()

    energies, coefficients = openmc.data.dose_coefficients("neutron", geometry="AP")
    tally = openmc.Tally(name="dose")
    tally.filters = [openmc.CellFilter(detector), openmc.EnergyFunctionFilter(energies, coefficients)]
    tally.scores = ["flux"]
    openmc.Tallies([tally]).export_to_xml()

    executable = os.path.join(os.path.dirname(sys.executable), "openmc")
    threads = int(os.environ.get("OMP_NUM_THREADS", "0")) or None
    openmc.run(openmc_exec=executable, output=False, threads=threads)

    statepoint = openmc.StatePoint(f"statepoint.{args.batches}.h5")
    result = statepoint.get_tally(name="dose")
    mean = stable(float(result.mean.ravel()[0]))
    std_dev = stable(float(result.std_dev.ravel()[0]))
    volume = lateral * lateral * detector_thickness
    # flux score is track length per source particle [particle-cm]; divide by
    # the detector volume for fluence per particle [1/cm2], times the dose
    # coefficient [pSv cm2] already folded in, times source strength [1/s],
    # times 3600 s/h, over 1e6 pSv/uSv.
    scale = Decimal(3600) * strength / Decimal(repr(volume)) / Decimal(1_000_000)
    nominal = mean * scale
    sigma = std_dev * scale
    halfwidth = sigma * Decimal(repr(K_95))
    lower = max(Decimal(0), nominal - halfwidth)
    upper = nominal + halfwidth

    document = {
        "schema": SCHEMA,
        "candidate_id": candidate.get("candidate_id"),
        "engine": {
            "name": "openmc",
            "version": openmc.__version__,
            "executable_sha256": sha256_of(executable),
            "python": sys.version.split()[0],
        },
        "data_library": {"index_sha256": library_digest, "note": "identity of the index only; the nuclide files it names are not re-hashed"},
        "run": {"particles": args.particles, "batches": args.batches, "seed": args.seed, "threads": threads},
        "tally": {
            "mean_pSv_cm_per_particle": canonical(mean),
            "std_dev": canonical(std_dev),
            "significant_digits": SIGNIFICANT_DIGITS,
            "detector_volume_cm3": repr(volume),
        },
        "dose_rate": {
            "nominal": {"value": canonical(nominal), "unit": "uSv/h"},
            "lower": {"value": canonical(lower), "unit": "uSv/h"},
            "upper": {"value": canonical(upper), "unit": "uSv/h"},
            "coverage": COVERAGE,
            "interpretation": "normal approximation of the tally statistics at the stated coverage; statistical error only",
        },
        "limitations": [
            "The interval bounds Monte Carlo statistical error only; nuclear-data, geometry, and model uncertainty are not evaluated.",
            "The slab is laterally infinite by reflection; a finite shield would leak around its edges.",
            "Tally statistics are rounded to 8 significant digits so that the thread-dependent reduction order of the Monte Carlo sums does not change the output bytes; the discarded digits are far below the statistical uncertainty.",
            "Not qualified for any regulatory, safety, or operational decision.",
        ],
    }
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
