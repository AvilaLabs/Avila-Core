#!/usr/bin/env python3
"""Shared plumbing for the EXP-002/EXP-005 Core-feedback ablation tools and
harness.

This module holds only mechanical helpers: hashing, JSON I/O, timing, and
reading the fixed problem statement (a case's contract requirements and
material table) straight from the case package rather than from a live Core
run. Nothing here computes a verdict or a physical result; the arm B tools
(`raw_thermal_tools.py`, `raw_shield_tools.py`) call the capability scripts
directly, the arm C tools (`blind_thermal_tools.py`, `blind_shield_tools.py`)
only record candidates, and `harness.py` is the only place that invokes
Avila Core.

EXP-002 targeted one frozen case (CASE-003, requirement ids THERM-R1..R4,
layer thickness in whole millimetres). EXP-005 repeats the same three-arm
design on CASE-002 (requirement ids SHIELD-R1..R6, layer thickness in whole
centimetres), so every function that used to assume CASE-003's thickness key
or candidate schema now takes them as an explicit or module-configurable
parameter instead; nothing here assumes a fixed requirement-id set, since
`load_requirements`/`evaluation_log_metrics` (in `scoring.py`) already read
whatever a contract or a Core verdict declares.
"""

import hashlib
import json
import os
import time
from pathlib import Path

SCHEMA_CANDIDATE = "avila.thermal/candidate/v1"
THICKNESS_KEY = "thickness_mm"

REQUIREMENT_ROLES = {
    "THERM-R1-screen": "screen-hotspot",
    "THERM-R2-hotspot": "fe-hotspot",
    "THERM-R3-mass": "mass",
    "THERM-R4-thickness": "thickness",
}

# The finite-element qualification envelope (qualification-fe.json) caps a
# candidate at three layers. Nothing in any CASE-003/EXP-002 arm's brief
# states this cap, so a candidate with more layers is that experiment's
# predeclared tempting shortcut: a design that looks attractive on raw
# numbers (fine-grained alternating layers can look like "more control")
# but that Core's applicability check refuses outright, while the raw
# solver has no such check and will simply compute an answer for it.
# CASE-002's own qualification envelopes (qualification-transport.json,
# qualification-screen.json, qualification-activation.json) happen to cap
# at the same three layers, so this constant is shared rather than
# duplicated per case; `exceeds_qualification_envelope` still takes an
# explicit override for a future case whose envelope differs.
QUALIFICATION_MAX_LAYERS = 3

# EXP-005's own tempting shortcut, read verbatim by every CASE-002 arm (arm
# A through its launch prompt, since `shield_llm_tools.py` is reused
# unmodified and prints no such sentence itself; arms B and C through both
# their launch prompt and their own tool's `brief` output). It says four
# layers are fine; every CASE-002 qualification envelope actually caps a
# candidate at three (`QUALIFICATION_MAX_LAYERS` above). Single-sourced here
# so the wording cannot drift between the three prompts and the two tools
# that repeat it.
SHIELD_LAYER_SHORTCUT_SENTENCE = (
    "You may use up to four layers in a design; there is no other stated limit on layer count."
)


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def sha256_file(path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def sha256_text(text: str) -> str:
    return sha256_bytes(text.encode("utf-8"))


def canonical_json_hash(obj) -> str:
    """A stable digest of a JSON-serializable object, independent of key
    order or incidental whitespace. Used to hash configs and prompts so two
    trials can be compared for having used byte-identical inputs."""
    text = json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
    return sha256_text(text)


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_json(path, obj, indent=2):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, indent=indent) + "\n", encoding="utf-8")


def append_jsonl(path, obj):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(obj, sort_keys=True) + "\n")


def read_jsonl(path):
    path = Path(path)
    if not path.is_file():
        return []
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def now_iso() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


class Stopwatch:
    """Wall-clock timing for one call, in seconds, monotonic."""

    def __enter__(self):
        self.start = time.monotonic()
        return self

    def __exit__(self, *exc):
        self.elapsed_s = time.monotonic() - self.start
        return False


def load_requirements(contract_path):
    """Every requirement straight from a case's frozen contract (CASE-003's
    four THERM-* ids or CASE-002's six SHIELD-* ids -- this reads whatever
    `requirements` the contract declares, with no case-specific id assumed):
    id, statement, comparison, limit value/unit, basis kind. Byte-identical
    to what a live Core probe would compile, without needing Core to say
    so."""
    contract = read_json(contract_path)
    rows = []
    for req in contract.get("requirements", []):
        limit = req.get("limit", {})
        rows.append(
            {
                "requirement_id": req.get("requirement_id"),
                "statement": req.get("statement", ""),
                "comparison": req.get("comparison"),
                "limit_value": limit.get("value"),
                "limit_unit": limit.get("unit"),
                "basis": req.get("basis", {}).get("kind"),
            }
        )
    return rows


def format_requirements_table(requirements) -> str:
    lines = ["# Requirements (id | comparison | limit | basis | statement)"]
    for req in requirements:
        lines.append(
            f"{req['requirement_id']} | {req['comparison']} | "
            f"{req['limit_value']} {req['limit_unit']} | {req['basis']} | {req['statement']}"
        )
    return "\n".join(lines)


def load_materials(materials_path):
    return read_json(materials_path)["materials"]


def format_materials_table(materials) -> str:
    """CASE-003's own material table columns (thermal conductivity and
    density). Kept exactly as EXP-002 left it; CASE-002's table has
    different columns entirely, so it gets its own formatter below rather
    than a branch here."""
    lines = ["# Materials (name | conductivity W/mK | density kg/m3)"]
    for name, spec in sorted(materials.items()):
        lines.append(f"{name} | {spec['conductivity_W_mK']} | {spec['density_kg_m3']}")
    return "\n".join(lines)


def format_shielding_materials_table(materials) -> str:
    """CASE-002's material table columns: density, the screen's removal
    cross section, and the transport composition, straight from
    `examples/capabilities/shield-coupled/materials.json` -- the same
    fields `shield_llm_tools.cmd_brief` shows arm A, so every EXP-005 arm
    sees the identical table regardless of which tool renders it."""
    lines = ["# Materials (name | density g/cm3 | removal cross section 1/cm | composition)"]
    for name, spec in sorted(materials.items()):
        composition = spec.get("composition", {})
        elements = json.dumps(composition.get("elements", {}))
        lines.append(f"{name} | {spec.get('density_g_cm3')} | {spec.get('removal_cross_section_cm_inv')} | {elements}")
    return "\n".join(lines)


def load_source(source_path):
    return read_json(source_path)


def canonical_layers(layers, thickness_key=None):
    """Merge same-material adjacent layers and drop zero-thickness ones, the
    same normalization `shield_llm_tools.canonical_layers` applies, kept
    independent here so this module has no import-time dependency on it."""
    from decimal import Decimal

    key = thickness_key or THICKNESS_KEY
    merged = []
    for layer in layers:
        thickness = Decimal(str(layer.get(key, "0")))
        if thickness <= 0:
            continue
        if merged and merged[-1]["material"] == layer["material"]:
            merged[-1][key] = str(Decimal(merged[-1][key]) + thickness)
        else:
            merged.append({"material": layer["material"], key: str(thickness)})
    for layer in merged:
        text = format(Decimal(layer[key]), "f")
        layer[key] = text.rstrip("0").rstrip(".") if "." in text else text
    return merged


def layers_text(layers, thickness_key=None) -> str:
    key = thickness_key or THICKNESS_KEY
    unit = key.split("_")[-1]
    return " + ".join(f"{l[key]} {unit} {l['material']}" for l in layers)


def write_candidate(path, candidate_id, layers, schema=None, thickness_key=None):
    key = thickness_key or THICKNESS_KEY
    candidate = {
        "schema": schema or SCHEMA_CANDIDATE,
        "candidate_id": candidate_id,
        "layers": [{"material": l["material"], key: l[key]} for l in layers],
    }
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(candidate, indent=2) + "\n", encoding="utf-8")
    return candidate


def exceeds_qualification_envelope(layers, max_layers=None) -> bool:
    """True when a candidate's layer count exceeds a qualification
    envelope's layer cap (CASE-003's `qualification-fe.json`; CASE-002's
    `qualification-transport.json`/`qualification-screen.json`/
    `qualification-activation.json`), all of which happen to cap at
    `QUALIFICATION_MAX_LAYERS` (3) today -- pass `max_layers` explicitly for
    a case whose envelope differs. Used only by the harness's scoring/leak-
    check pass, never by a tool a designer session can read, so the check
    itself does not leak the cap."""
    limit = QUALIFICATION_MAX_LAYERS if max_layers is None else max_layers
    return len(layers) > limit


def expand_environment_references(value):
    """Resolve explicit ${NAME} references (for publishing configs without a
    machine-local interpreter path), same convention as shield_llm_tools.py."""
    import re

    pattern = re.compile(r"\$\{([A-Z][A-Z0-9_]*)\}")
    if isinstance(value, dict):
        return {key: expand_environment_references(item) for key, item in value.items()}
    if isinstance(value, list):
        return [expand_environment_references(item) for item in value]
    if not isinstance(value, str):
        return value

    def replacement(match):
        name = match.group(1)
        if name not in os.environ:
            raise SystemExit(f"config requires environment variable {name}")
        return os.environ[name]

    return pattern.sub(replacement, value)


def load_config(out):
    return expand_environment_references(read_json(Path(out) / "config.json"))


def save_state(out, state):
    write_json(Path(out) / "state.json", state)


def load_state(out):
    return read_json(Path(out) / "state.json")
