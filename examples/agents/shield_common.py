#!/usr/bin/env python3
"""Shared plumbing for the shielding-search agents.

`shield_search.py` and `shield_review.py` (frozen, do not modify) invoke Core
and interpret its reports inline. This module factors the parts that
`shield_search2.py`, `practice_baseline.py`, and `control_sweep.py` all need
so they stay small and so the interpretation of Core's report shape lives in
exactly one place: requirement ids, verdict statuses, limits, and margins are
always read from a `--json` run report or a `--log` campaign-log line, never
invented here.

Nothing in this module constructs or alters a verdict. It reads them.
"""

import hashlib
import json
import subprocess
import sys
from decimal import Decimal
from fractions import Fraction
from pathlib import Path

SCHEMA_CANDIDATE = "avila.shielding/candidate/v1"

# The four verdict statuses the kernel emits (avila.core/semantic/0.2-draft).
# See docs/DEFINITIONS.md: PASS / FAIL / INCONCLUSIVE / NOT_EVALUATED.
STATUS_PASS = "pass"
STATUS_FAIL = "fail"
STATUS_INCONCLUSIVE = "inconclusive"
STATUS_NOT_EVALUATED = "not_evaluated"
KNOWN_STATUSES = {STATUS_PASS, STATUS_FAIL, STATUS_INCONCLUSIVE, STATUS_NOT_EVALUATED}


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def canonical_decimal(value):
    """Exact decimal string for a Decimal, matching screen.py/transport.py's canonical()."""
    text = format(Decimal(value).normalize(), "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    if text in ("", "-0"):
        text = "0"
    return text


def number(text):
    """Core reports exact canonical values: integers, decimals, or rationals."""
    return Fraction(text)


def show(value, digits=4):
    """A handful of significant digits for people; callers keep the exact value."""
    if value is None:
        return "-"
    return f"{float(Fraction(value)):.{digits}g}"


def load_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def load_materials(path):
    return load_json(path)["materials"]


def load_source(path):
    return load_json(path)


def write_candidate(path, candidate_id, layers, description=None, schema=None, thickness_key=None):
    """Write a candidate in the frozen `avila.shielding/candidate/v1` schema,
    byte-for-byte in the same shape `shield_search.py` writes (schema,
    candidate_id, layers; description optional)."""
    candidate = {"schema": schema or SCHEMA_CANDIDATE, "candidate_id": candidate_id}
    if description:
        candidate["description"] = description
    key = thickness_key or "thickness_cm"
    candidate["layers"] = [{"material": m, key: t} for m, t in layers]
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(candidate, indent=2) + "\n")
    return candidate


class CoreRunError(SystemExit):
    pass


def run_core(core, case, candidate_path, *, source_roots, capabilities, environment=None,
             log=None, extra_args=None, workspace=None, attempt_id=None,
             parent_attempt_id=None, candidate_input="candidate"):
    """Run `avila-core run --json` over `case` with `candidate_path` as the
    free `candidate` input. `source_roots` and `capabilities` are
    `{name: path}` dicts; `environment` is `{KEY: VALUE}`. Returns the parsed
    run report. Raises if Core produced no report on stdout (a crash before
    any report could be printed); a report with `status: "rejected"` is
    still returned, exactly as `shield_search.py` treats it.
    """
    command = [str(core), "run", str(case), "--json",
               "--input", f"candidate={candidate_path}"]
    for name, path in source_roots.items():
        command += ["--source-root", f"{name}={path}"]
    for name, path in capabilities.items():
        command += ["--capability", f"{name}={path}"]
    for key, value in (environment or {}).items():
        command += ["--env", f"{key}={value}"]
    if workspace is not None:
        command += ["--workspace", str(workspace)]
    if log is not None:
        command += ["--log", str(log)]
    if attempt_id is not None:
        command += ["--attempt", str(attempt_id),
                    "--candidate-input", str(candidate_input)]
        if parent_attempt_id is not None:
            command += ["--parent-attempt", str(parent_attempt_id)]
    elif parent_attempt_id is not None:
        raise ValueError("parent_attempt_id requires attempt_id")
    command += list(extra_args or [])
    completed = subprocess.run(command, capture_output=True, text=True)
    if not completed.stdout.strip():
        raise CoreRunError(
            f"core produced no report for {candidate_path}:\n{completed.stderr}"
        )
    return json.loads(completed.stdout)


def margin(report, requirement_id):
    """The `VerdictMargin` entry for one requirement from a `--json` report, or None."""
    for entry in report.get("margins", []):
        if entry.get("requirement_id") == requirement_id:
            return entry
    return None


def all_margins(report):
    return report.get("margins", [])


def metric_sources(report):
    """`{requirement_id: {"step_id":..., "output_slot":...}}` from the full
    campaign verdicts, when present. This is how a requirement's evidence is
    traced back to the step that produced it, without guessing from names.
    """
    campaign = report.get("campaign") or {}
    sources = {}
    for verdict in campaign.get("verdicts", []):
        requirement_id = verdict.get("requirement_id")
        metric = verdict.get("metric") or {}
        if requirement_id and metric:
            sources[requirement_id] = {
                "step_id": metric.get("step_id"),
                "output_slot": metric.get("output_slot"),
            }
    return sources


# ----------------------------------------------------------------------
# Generic, data-driven classification of a requirement's physical role by
# the unit Core reports for it. Never assume a requirement id: the coupled
# case reuses R1-R4's ids for different things and adds R5/R6.
# ----------------------------------------------------------------------

MASS_UNITS = {"kg", "g", "lb", "lbm"}
LENGTH_UNITS = {"cm", "m", "mm", "in"}


def unit_role(unit):
    """"mass", "length", "dose_rate" (any unit naming sievert), or "other"."""
    if unit is None:
        return None
    if unit in MASS_UNITS:
        return "mass"
    if unit in LENGTH_UNITS:
        return "length"
    if "Sv" in unit:
        return "dose_rate"
    return "other"


def is_bounded_rule(rule):
    return isinstance(rule, str) and rule.startswith("bounded.")


def is_nominal_rule(rule):
    return isinstance(rule, str) and rule.startswith("nominal.")


def discover_limits(report):
    """`{"mass": limit_or_None, "length": ..., "dose_rate": ...}`: the most
    conservative (minimum) limit Core reports for each physical role in one
    run, identified by unit rather than by requirement id, so a candidate
    can be sized or bounded without hard-coding which id is which.
    """
    limits = {"mass": None, "length": None, "dose_rate": None}
    for entry in all_margins(report):
        role = unit_role(entry.get("unit"))
        if role in limits and entry.get("limit") is not None:
            value = float(Fraction(entry["limit"]))
            limits[role] = value if limits[role] is None else min(limits[role], value)
    return limits


def layer_signature(layers):
    """A candidate's layer stack as a hashable, order-preserving tuple, used
    to recognize "the same design" independent of candidate_id or file
    formatting."""
    return tuple((layer["material"], str(layer["thickness_cm"])) for layer in layers)


def classify_requirements(rows):
    """From any list of parsed --log rows (or a live report's `margins`
    wrapped as one such row), `{requirement_id: {"unit":...,
    "dose_rate_bounded": bool, "mass": bool, "length": bool}}`, found by
    unit and rule prefix rather than assumed by id.
    """
    seen = {}
    for row in rows:
        for entry in row.get("verdicts", row.get("margins", [])):
            requirement_id = entry["requirement_id"]
            record = seen.setdefault(requirement_id, {"unit": None, "seen_bounded": False})
            if entry.get("unit"):
                record["unit"] = entry["unit"]
            if is_bounded_rule(entry.get("rule")):
                record["seen_bounded"] = True
    result = {}
    for requirement_id, record in seen.items():
        role = unit_role(record["unit"])
        result[requirement_id] = {
            "unit": record["unit"],
            "dose_rate_bounded": role == "dose_rate" and record["seen_bounded"],
            "mass": role == "mass",
            "length": role == "length",
        }
    return result


def build_signature_index(directories):
    """Scan directories of candidate JSON files and map each file's real
    sha256 (the identity Core stages and logs) to its layer signature, so a
    campaign-log row (which records only a sha256 and a since-possibly-moved
    path) can be matched back to a design without trusting the logged path.
    """
    index = {}
    for directory in directories:
        directory = Path(directory)
        if not directory.exists():
            continue
        for path in sorted(directory.glob("*.json")):
            try:
                candidate = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                continue
            if candidate.get("schema") != SCHEMA_CANDIDATE or not candidate.get("layers"):
                continue
            index[sha256_file(path)] = {
                "signature": layer_signature(candidate["layers"]),
                "path": str(path),
                "candidate_id": candidate.get("candidate_id"),
            }
    return index


def read_jsonl(path):
    lines = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                lines.append(json.loads(line))
    return lines


def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)
