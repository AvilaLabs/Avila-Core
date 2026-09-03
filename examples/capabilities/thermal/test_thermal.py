#!/usr/bin/env python3
"""Black-box tests for thermal_screen.py, thermal_fe.py, and
validation/nafems_t4.py: each is run as a subprocess through its own
declared interpreter, exactly as the Rust adapters invoke it (screen under
the system python3, the finite-element scripts under the thermal virtual
environment's interpreter), and only the produced JSON is inspected. There
are no mocks and no direct imports of solver internals here (the analytic
and NAFEMS cross-checks live partly in this file's own arithmetic and
partly in `validation/nafems_t4.py` itself, which imports `thermal_fe.py`'s
functions directly -- see that script for the "reuse, don't duplicate"
solver path).

If the thermal virtual environment is not present at the declared path (a
different machine), every finite-element test in this module is skipped
rather than failed; the screen tests need only the system interpreter and
always run.

Run with:
    /usr/bin/python3 -m unittest examples/capabilities/thermal/test_thermal.py -v
or equivalently under the thermal venv -- this file itself imports nothing
beyond the standard library, so either interpreter runs it the same way.
"""
import json
import subprocess
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCREEN_SCRIPT = HERE / "thermal_screen.py"
FE_SCRIPT = HERE / "thermal_fe.py"
NAFEMS_SCRIPT = HERE / "validation" / "nafems_t4.py"
NAFEMS_OUTPUT = HERE / "validation" / "nafems-t4.json"
MATERIALS = HERE / "materials.json"

SYSTEM_PYTHON3 = Path("/usr/bin/python3")
THERMAL_PYTHON = Path("/home/connoravila/.venvs/thermal/bin/python")

# Memory ceiling for each subprocess, matching the operational limit used
# throughout this repository's development on a machine shared with other
# work (see test_activate.py's ULIMIT_V_KB; this solve is far lighter, so a
# smaller ceiling is enough).
ULIMIT_V_KB = 4_000_000


def missing_thermal_venv():
    return [] if THERMAL_PYTHON.exists() else [str(THERMAL_PYTHON)]


def run(interpreter: Path, script: Path, args: list) -> subprocess.CompletedProcess:
    command = [str(interpreter), str(script), *args]
    wrapped = ["bash", "-c", f'ulimit -v {ULIMIT_V_KB}; exec "$@"', "--", *command]
    return subprocess.run(wrapped, capture_output=True, text=True)


def write_json(directory: Path, name: str, document: dict) -> Path:
    path = directory / name
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(document, handle)
    return path


def candidate_document(candidate_id: str, layers: list) -> dict:
    return {
        "schema": "avila.thermal/candidate/v1",
        "candidate_id": candidate_id,
        "layers": [{"material": material, "thickness_mm": thickness_mm} for material, thickness_mm in layers],
    }


def source_document(width_mm, strip_width_mm, heat_flux_W_m2, convection_W_m2K, ambient_K) -> dict:
    return {
        "schema": "avila.thermal/source/v1",
        "width_mm": width_mm,
        "strip_width_mm": strip_width_mm,
        "heat_flux_W_m2": heat_flux_W_m2,
        "convection_W_m2K": convection_W_m2K,
        "ambient_K": ambient_K,
    }


class ScreenArithmeticTest(unittest.TestCase):
    """The screen's series-resistance formula, checked against numbers
    chosen to make the hand calculation exact and easy to re-derive:
    hotspot = ambient + q'' * (sum(t_i / k_i) + 1/h)."""

    def run_screen(self, materials: dict, candidate: dict, source: dict) -> dict:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            materials_path = write_json(tmp_path, "materials.json", {"schema": "avila.thermal/materials/v1", "materials": materials})
            candidate_path = write_json(tmp_path, "candidate.json", candidate)
            source_path = write_json(tmp_path, "source.json", source)
            output_path = tmp_path / "screen-result.json"
            result = run(
                SYSTEM_PYTHON3,
                SCREEN_SCRIPT,
                [
                    "--candidate", str(candidate_path),
                    "--materials", str(materials_path),
                    "--source", str(source_path),
                    "--output", str(output_path),
                ],
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            with open(output_path, encoding="utf-8") as handle:
                return json.load(handle)

    def test_single_layer_hand_calculation(self):
        # t/k = 0.02/10 = 0.002; 1/h = 1/100 = 0.01; sum = 0.012.
        # hotspot = 290 + 1000 * 0.012 = 302 K exactly. mass = 5000*0.02 = 100 kg/m2.
        materials = {"test_material": {"conductivity_W_mK": "10", "density_kg_m3": "5000"}}
        candidate = candidate_document("hand-calc-1", [("test_material", "20")])
        source = source_document("50", "10", "1000", "100", "290")
        document = self.run_screen(materials, candidate, source)
        self.assertEqual(document["schema"], "avila.thermal/screen-result/v1")
        self.assertEqual(document["hotspot_temperature"], {"value": "302", "unit": "K"})
        self.assertEqual(document["areal_mass"], {"value": "100", "unit": "kg"})
        self.assertEqual(document["thickness"], {"value": "20", "unit": "mm"})

    def test_two_layer_hand_calculation(self):
        # Layer 1: t/k = 0.005/25 = 0.0002. Layer 2: t/k = 0.015/3 = 0.005.
        # 1/h = 1/200 = 0.005. sum = 0.0002 + 0.005 + 0.005 = 0.0102.
        # hotspot = 250 + 2000*0.0102 = 250 + 20.4 = 270.4 K.
        # mass = 4000*0.005 + 1000*0.015 = 20 + 15 = 35 kg/m2. thickness = 5+15 = 20 mm.
        materials = {
            "material_a": {"conductivity_W_mK": "25", "density_kg_m3": "4000"},
            "material_b": {"conductivity_W_mK": "3", "density_kg_m3": "1000"},
        }
        candidate = candidate_document("hand-calc-2", [("material_a", "5"), ("material_b", "15")])
        source = source_document("80", "16", "2000", "200", "250")
        document = self.run_screen(materials, candidate, source)
        self.assertEqual(document["hotspot_temperature"], {"value": "270.4", "unit": "K"})
        self.assertEqual(document["areal_mass"], {"value": "35", "unit": "kg"})
        self.assertEqual(document["thickness"], {"value": "20", "unit": "mm"})

    def test_pessimistic_relative_to_reference_finite_element_result(self):
        # The screen's own reasoning (see thermal_screen.py's docstring) says
        # it can only over-estimate the true hotspot. Check that against the
        # committed reference sample, which was produced by both scripts
        # from the same candidate/materials/source.
        with open(HERE / "samples" / "reference" / "screen-result.json", encoding="utf-8") as handle:
            screen = json.load(handle)
        with open(HERE / "samples" / "reference" / "fe-result.json", encoding="utf-8") as handle:
            fe = json.load(handle)
        self.assertGreaterEqual(
            Decimal(screen["hotspot_temperature"]["value"]),
            Decimal(fe["hotspot_temperature"]["upper"]),
        )


@unittest.skipIf(missing_thermal_venv(), f"thermal virtual environment not present: {missing_thermal_venv()}")
class FiniteElementAnalyticReductionTest(unittest.TestCase):
    """When the strip covers the entire face, there is nothing left for the
    layers to spread heat into sideways: every quantity is uniform in x, the
    problem collapses to the 1-D series-resistance case, and the
    finite-element answer should equal that closed form -- this is the same
    formula the screen computes, so a full-face-flux candidate is the point
    where the pessimistic screen bound and the finite-element answer meet."""

    def test_full_face_flux_matches_series_resistance_formula(self):
        materials = {
            "copper": {"conductivity_W_mK": "400", "density_kg_m3": "8960"},
            "aluminium": {"conductivity_W_mK": "200", "density_kg_m3": "2700"},
        }
        candidate = candidate_document("full-face-flux", [("copper", "3"), ("aluminium", "10")])
        # strip_width_mm == width_mm: the flux covers the whole heated face.
        source = source_document("100", "100", "50000", "500", "300")
        analytic = Decimal("300") + Decimal("50000") * (
            Decimal("3") / Decimal(1000) / Decimal("400")
            + Decimal("10") / Decimal(1000) / Decimal("200")
            + Decimal(1) / Decimal("500")
        )

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            materials_path = write_json(tmp_path, "materials.json", {"schema": "avila.thermal/materials/v1", "materials": materials})
            candidate_path = write_json(tmp_path, "candidate.json", candidate)
            source_path = write_json(tmp_path, "source.json", source)
            output_path = tmp_path / "fe-result.json"
            result = run(
                THERMAL_PYTHON,
                FE_SCRIPT,
                [
                    "--candidate", str(candidate_path),
                    "--materials", str(materials_path),
                    "--source", str(source_path),
                    "--output", str(output_path),
                ],
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            with open(output_path, encoding="utf-8") as handle:
                document = json.load(handle)

        lower = Decimal(document["hotspot_temperature"]["lower"])
        upper = Decimal(document["hotspot_temperature"]["upper"])
        self.assertLessEqual(lower, analytic)
        self.assertLessEqual(analytic, upper)
        # The bracket should be tight around the analytic value: no lateral
        # variation exists for any mesh level to disagree about.
        self.assertLess(upper - lower, Decimal("0.01"))
        self.assertAlmostEqual(float(lower), float(analytic), places=2)


@unittest.skipIf(missing_thermal_venv(), f"thermal virtual environment not present: {missing_thermal_venv()}")
class ByteStabilityTest(unittest.TestCase):
    """Two runs over identical inputs must produce identical bytes -- no
    randomness, and the significant-digit rounding absorbs any last-bit
    float noise from the linear solve."""

    def test_finite_element_output_is_byte_stable(self):
        materials = {"aluminium": {"conductivity_W_mK": "200", "density_kg_m3": "2700"}}
        candidate = candidate_document("byte-stability", [("aluminium", "20")])
        source = source_document("100", "20", "50000", "500", "300")

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            materials_path = write_json(tmp_path, "materials.json", {"schema": "avila.thermal/materials/v1", "materials": materials})
            candidate_path = write_json(tmp_path, "candidate.json", candidate)
            source_path = write_json(tmp_path, "source.json", source)
            outputs = []
            for index in (1, 2):
                output_path = tmp_path / f"fe-result-{index}.json"
                result = run(
                    THERMAL_PYTHON,
                    FE_SCRIPT,
                    [
                        "--candidate", str(candidate_path),
                        "--materials", str(materials_path),
                        "--source", str(source_path),
                        "--output", str(output_path),
                    ],
                )
                self.assertEqual(result.returncode, 0, msg=result.stderr)
                outputs.append(output_path.read_bytes())
            self.assertEqual(outputs[0], outputs[1])

    def test_screen_output_is_byte_stable(self):
        materials = {"aluminium": {"conductivity_W_mK": "200", "density_kg_m3": "2700"}}
        candidate = candidate_document("byte-stability", [("aluminium", "20")])
        source = source_document("100", "20", "50000", "500", "300")

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            materials_path = write_json(tmp_path, "materials.json", {"schema": "avila.thermal/materials/v1", "materials": materials})
            candidate_path = write_json(tmp_path, "candidate.json", candidate)
            source_path = write_json(tmp_path, "source.json", source)
            outputs = []
            for index in (1, 2):
                output_path = tmp_path / f"screen-result-{index}.json"
                result = run(
                    SYSTEM_PYTHON3,
                    SCREEN_SCRIPT,
                    [
                        "--candidate", str(candidate_path),
                        "--materials", str(materials_path),
                        "--source", str(source_path),
                        "--output", str(output_path),
                    ],
                )
                self.assertEqual(result.returncode, 0, msg=result.stderr)
                outputs.append(output_path.read_bytes())
            self.assertEqual(outputs[0], outputs[1])


@unittest.skipIf(missing_thermal_venv(), f"thermal virtual environment not present: {missing_thermal_venv()}")
class NafemsReproductionTest(unittest.TestCase):
    """`validation/nafems_t4.py` reproduces NAFEMS T4 on the same solver
    code path `thermal_fe.py` uses for candidates; the finest of its three
    mesh levels must land within 0.1% of the published reference."""

    def test_finest_level_within_tolerance(self):
        result = run(THERMAL_PYTHON, NAFEMS_SCRIPT, [])
        self.assertEqual(result.returncode, 0, msg=result.stdout + result.stderr)
        with open(NAFEMS_OUTPUT, encoding="utf-8") as handle:
            document = json.load(handle)
        self.assertEqual(document["schema"], "avila.thermal/validation/v1")
        self.assertEqual(document["reference_C"], "18.25")
        finest = document["levels"][-1]
        deviation = abs(Decimal(finest["relative_deviation"]))
        self.assertLessEqual(deviation, Decimal("0.001"), msg=f"finest level deviated {deviation}")
        self.assertTrue(document["within_tolerance"]["finest_level"])


if __name__ == "__main__":
    unittest.main()
