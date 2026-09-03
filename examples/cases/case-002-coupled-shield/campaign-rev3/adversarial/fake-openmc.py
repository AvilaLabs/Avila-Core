#!/usr/bin/env python3
"""A fake 'openmc-python' interpreter: does no transport at all, just writes
canned favorable output wherever --output / --layer-spectra-output point.
Used only to test whether Avila Core's capability-hash check can be beaten
by supplying a different executable once package.json / the qualification
record are patched to declare *this* file's digest as the trusted one.

The layer-spectra output is copied verbatim from the case's own real,
already-verified reference-candidate spectra (a genuinely low-activation
result) so the downstream activation step has something real to chew on;
only the dose-rate numbers below are fabricated.
"""
import json
import shutil
import sys
from pathlib import Path

REAL_LAYER_SPECTRA = Path(
    "/home/connoravila/Documents/north-star-worktrees/adversarial"
    "/examples/cases/case-002-coupled-shield/expected/layer-spectra.json"
)

args = sys.argv[1:]


def opt(name):
    return args[args.index(name) + 1]


output = Path(opt("--output"))
layer_spectra_output = Path(opt("--layer-spectra-output"))

transport_result = {
    "schema": "avila.shielding/transport-result/v2",
    "candidate_id": "fake-interpreter-output",
    "neutron_dose_rate": {
        "nominal": {"value": "0.5", "unit": "uSv/h"},
        "lower": {"value": "0.4", "unit": "uSv/h"},
        "upper": {"value": "0.6", "unit": "uSv/h"},
        "coverage": "0.95",
        "interpretation": "fabricated by the adversarial fake interpreter; no transport ran",
    },
    "photon_dose_rate": {
        "nominal": {"value": "0.2", "unit": "uSv/h"},
        "lower": {"value": "0.1", "unit": "uSv/h"},
        "upper": {"value": "0.3", "unit": "uSv/h"},
        "coverage": "0.95",
        "interpretation": "fabricated by the adversarial fake interpreter; no transport ran",
    },
    "limitations": ["This file was never produced by OpenMC; the adversarial arm wrote it."],
}
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(transport_result, indent=2) + "\n")

layer_spectra_output.parent.mkdir(parents=True, exist_ok=True)
shutil.copyfile(REAL_LAYER_SPECTRA, layer_spectra_output)

sys.exit(0)
