#!/usr/bin/env python3
"""Black-box tests for activate.py against the synthetic fixture.

Runs the real, local ACTINV executable over the real, local ACTINV data
release (the same paths the coupled-shield spec declares) through the
shipped `/usr/bin/python3` interpreter, exactly as the Rust adapter will
invoke it. There is no mock ACTINV here: the point of this suite is to prove
the script's contract (schema, key order, exact-decimal formatting, byte
stability) against the real tool, not against a stand-in.

Set `ACTINV_BIN` and `ACTINV_DATA_ROOT` to exercise a local ACTINV build and
data release. If either is unavailable, every test in this module is skipped
rather than failed, since none of that is this script's own responsibility.

Run with:
    /usr/bin/python3 -m unittest examples/capabilities/shield-coupled/test_activate.py -v
"""
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import unittest
from decimal import Decimal
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "activate.py"
SPECTRA = HERE / "fixtures" / "layer-spectra.synthetic.json"
SCHEDULE = HERE / "fixtures" / "schedule.json"
MATERIALS = HERE.parent / "shielding" / "materials.json"

ACTINV = Path(os.environ.get("ACTINV_BIN") or shutil.which("actinv") or ".missing/actinv")
DATA_ROOT = Path(os.environ.get("ACTINV_DATA_ROOT", ".missing/actinv-data"))
ACTIVATION_LIBRARY = DATA_ROOT / "activation" / "tendl-2025-neutron-709g.npz"
ACTIVATION_INDEX = DATA_ROOT / "activation" / "tendl-2025-neutron-709g_index.json"
DECAY_PRIMARY = DATA_ROOT / "decay" / "endf-b-viii-0_decay.dat"
DECAY_FALLBACK = DATA_ROOT / "decay" / "jeff-3-3_decay.dat"

LOCAL_RESOURCES = [SCRIPT, SPECTRA, SCHEDULE, MATERIALS, ACTINV, ACTIVATION_LIBRARY,
                   ACTIVATION_INDEX, DECAY_PRIMARY, DECAY_FALLBACK]

CANONICAL_DECIMAL = re.compile(r"^-?(0|[1-9][0-9]*)(\.[0-9]*[1-9])?$")

# Memory ceiling for each ACTINV run, matching the operational limit used
# throughout this repository's development on the shared machine.
ULIMIT_V_KB = 12_000_000


def missing_resources():
    return [str(path) for path in LOCAL_RESOURCES if not path.exists()]


def run_activate(output_path: Path) -> subprocess.CompletedProcess:
    command = [
        "/usr/bin/python3",
        str(SCRIPT),
        "--spectra", str(SPECTRA),
        "--materials", str(MATERIALS),
        "--schedule", str(SCHEDULE),
        "--actinv", str(ACTINV),
        "--activation-library", str(ACTIVATION_LIBRARY),
        "--activation-index", str(ACTIVATION_INDEX),
        "--decay-primary", str(DECAY_PRIMARY),
        "--decay-fallback", str(DECAY_FALLBACK),
        "--output", str(output_path),
    ]
    # A plain shell prefix, matching how every heavy process in this
    # repository's development has been run: bound memory on a machine
    # shared with other work.
    wrapped = ["bash", "-c", f"ulimit -v {ULIMIT_V_KB}; exec \"$@\"", "--", *command]
    return subprocess.run(wrapped, capture_output=True, text=True)


def parse_wall_times(stderr_text: str) -> dict:
    times = {}
    for line in stderr_text.splitlines():
        match = re.match(
            r"layer (\d+) \(([^)]+)\): actinv wall time ([0-9.]+) s", line
        )
        if match:
            index, material, seconds = match.groups()
            times[(int(index), material)] = float(seconds)
    return times


@unittest.skipIf(missing_resources(), f"local ACTINV/data not present: {missing_resources()}")
class ActivateFixtureTest(unittest.TestCase):
    """One real run, shared by every check that only reads its output."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="activate-test-")
        output_path = Path(cls.tmp) / "activation-result.json"
        started = time.monotonic()
        completed = run_activate(output_path)
        cls.wall_time_s = time.monotonic() - started
        cls.completed = completed
        cls.output_path = output_path
        if completed.returncode == 0:
            cls.raw_bytes = output_path.read_bytes()
            cls.document = json.loads(cls.raw_bytes)
        else:
            cls.raw_bytes = b""
            cls.document = None

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_run_succeeds(self):
        self.assertEqual(
            self.completed.returncode, 0,
            msg=f"stdout={self.completed.stdout!r} stderr={self.completed.stderr!r}",
        )

    def test_reports_wall_time_per_layer(self):
        times = parse_wall_times(self.completed.stderr)
        self.assertEqual(
            set(times), {(0, "polyethylene"), (1, "iron")},
            msg=f"stderr was: {self.completed.stderr!r}",
        )
        for key, seconds in times.items():
            self.assertGreater(seconds, 0.0, key)
            self.assertLess(seconds, 600.0, key)
        print(f"\nACTINV wall time per layer: {times}")
        print(f"end-to-end activate.py wall time: {self.wall_time_s:.3f} s")

    def test_schema_and_top_level_shape(self):
        document = self.document
        self.assertEqual(document["schema"], "avila.shielding/activation-result/v1")
        self.assertEqual(document["candidate_id"], "synthetic-fixture-polyethylene40-iron10")
        for key in ("schedule", "actinv", "layers", "totals", "limitations"):
            self.assertIn(key, document)
        self.assertEqual(len(document["layers"]), 2)
        self.assertEqual(document["schedule"], {
            "irradiation_s": "2592000",
            "cooling_s": "86400",
            "flux_scale": "1",
        })

    def test_actinv_identity_block(self):
        actinv = self.document["actinv"]
        self.assertEqual(actinv["version"], "1.0.1")
        for key in ("executable_sha256", "library_sha256", "library_index_sha256"):
            self.assertRegex(actinv[key], r"^sha256:[0-9a-f]{64}$")
        self.assertRegex(actinv["decay"]["primary_sha256"], r"^sha256:[0-9a-f]{64}$")
        self.assertRegex(actinv["decay"]["fallback_sha256"], r"^sha256:[0-9a-f]{64}$")
        # Identities the runner's own staged inputs are declared against, per
        # the coupled-shield spec's local resources.
        self.assertEqual(
            actinv["library_sha256"],
            "sha256:ec4c72bf598dc8ad3d533d9cfafdcf493e2d1f949a3e4db6251495659b68cc44",
        )
        self.assertEqual(
            actinv["executable_sha256"],
            "sha256:46cbe5ccb0cbcab1bf9a27a9bc83b8455027a02b14339a429b61805147b34426",
        )

    def test_key_order_is_sorted(self):
        # Re-serializing a loaded document with sort_keys should be a no-op
        # on the bytes actually written, at every nesting level.
        resorted = json.dumps(self.document, indent=2, sort_keys=True) + "\n"
        self.assertEqual(resorted.encode("utf-8"), self.raw_bytes)

    def test_every_claim_value_is_an_exact_decimal_string(self):
        checked = 0

        def check_quantity(document, pointer):
            nonlocal checked
            value = document["value"]
            self.assertIsInstance(value, str, pointer)
            # No exponent notation, no leading zeros (beyond a bare "0" or
            # "0.xxx"), no trailing fractional zeros, optional leading "-":
            # exactly the shape `canonical()` in activate.py produces.
            self.assertRegex(value, CANONICAL_DECIMAL, f"{pointer}: {value!r}")
            # Parses as an exact decimal (never raises, never silently
            # accepts binary-float noise like "1.0000000000000002").
            Decimal(value)
            checked += 1

        for layer in self.document["layers"]:
            check_quantity(layer["specific_activity"], f"layers[{layer['index']}].specific_activity")
            check_quantity(layer["decay_heat"], f"layers[{layer['index']}].decay_heat")
            if "contact_dose_rate" in layer:
                check_quantity(layer["contact_dose_rate"], f"layers[{layer['index']}].contact_dose_rate")
            self.assertRegex(layer["mass_kg"], CANONICAL_DECIMAL, layer["mass_kg"])
            for nuclide in layer["dominant_nuclides"]:
                self.assertRegex(
                    nuclide["activity_Bq_per_g"], CANONICAL_DECIMAL, nuclide
                )
        for key, quantity in self.document["totals"].items():
            check_quantity(quantity, f"totals.{key}")
        self.assertGreaterEqual(checked, 4)  # 2 layers x 2 + totals, at least

    def test_mass_matches_density_thickness_area(self):
        # Fixture-specific: 40 cm polyethylene (0.94 g/cm3) and 10 cm iron
        # (7.87 g/cm3) over the shared 10000 cm^2 convention.
        layers = {layer["material"]: layer for layer in self.document["layers"]}
        self.assertEqual(layers["polyethylene"]["mass_kg"], "376")
        self.assertEqual(layers["iron"]["mass_kg"], "787")

    def test_totals_are_consistent_with_layers(self):
        layers = self.document["layers"]
        max_activity = max(Decimal(layer["specific_activity"]["value"]) for layer in layers)
        self.assertEqual(
            Decimal(self.document["totals"]["max_specific_activity"]["value"]),
            max_activity,
        )
        # decay_heat is W/kg; multiplying by each layer's mass in kg gives
        # watts directly (no further unit scaling).
        expected_heat = sum(
            Decimal(layer["decay_heat"]["value"]) * Decimal(layer["mass_kg"])
            for layer in layers
        )
        reported_heat = Decimal(self.document["totals"]["total_decay_heat"]["value"])
        # Both sides are independently rounded to 8 significant figures;
        # allow that rounding's own tolerance rather than demand bit equality.
        if expected_heat == 0:
            self.assertEqual(reported_heat, 0)
        else:
            relative_error = abs(reported_heat - expected_heat) / expected_heat
            self.assertLess(relative_error, Decimal("1e-6"))

    def test_contact_dose_rate_is_absent_without_a_photon_response_table(self):
        # This deployment has no actinv-photon-response-1 table staged, so
        # ACTINV computes no contact-dose response; the claim and the total
        # must both be genuinely absent, not null or zero.
        for layer in self.document["layers"]:
            self.assertNotIn("contact_dose_rate", layer)
            self.assertTrue(
                any("contact gamma dose-rate proxy unavailable" in note
                    for note in layer["omissions"])
            )
        self.assertNotIn("max_contact_dose_rate", self.document["totals"])

    def test_limitations_are_present_and_nonempty_strings(self):
        limitations = self.document["limitations"]
        self.assertGreaterEqual(len(limitations), 5)
        for item in limitations:
            self.assertIsInstance(item, str)
            self.assertGreater(len(item), 0)


@unittest.skipIf(missing_resources(), f"local ACTINV/data not present: {missing_resources()}")
class ActivateByteStabilityTest(unittest.TestCase):
    """A separate, from-scratch double run: the definitive byte-stability
    check the task requires, independent of the shared single run above.
    """

    def test_two_fresh_runs_are_byte_identical(self):
        with tempfile.TemporaryDirectory(prefix="activate-stability-a-") as tmp_a, \
             tempfile.TemporaryDirectory(prefix="activate-stability-b-") as tmp_b:
            output_a = Path(tmp_a) / "activation-result.json"
            output_b = Path(tmp_b) / "activation-result.json"
            first = run_activate(output_a)
            second = run_activate(output_b)
            self.assertEqual(first.returncode, 0, first.stderr)
            self.assertEqual(second.returncode, 0, second.stderr)
            bytes_a = output_a.read_bytes()
            bytes_b = output_b.read_bytes()
            self.assertEqual(len(bytes_a), len(bytes_b))
            self.assertEqual(bytes_a, bytes_b)


if __name__ == "__main__":
    unittest.main()
