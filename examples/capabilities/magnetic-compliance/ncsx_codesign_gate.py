#!/usr/bin/env python3
"""Gate a field-preserving NCSX coil/support co-design candidate.

The candidate is a low-order Fourier perturbation selected by an exploratory
near-nullspace loop.  This checker does not rerun that search.  It binds its
record, reconstructs both the raw and repaired candidates, and evaluates them
on reference and fine grids.  Every reduction uses the original NCSX
uncoupled support as the denominator and the original NCSX field magnitude as
the normalization, so changing the candidate cannot move the control.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np


MODEL_SCHEMA = "avila.magnetic-compliance/ncsx-codesign-gate-model/v1"
SEARCH_SCHEMA = "avila.magnetic-compliance/ncsx-codesign-search-record/v1"
RESULT_SCHEMA = "avila.magnetic-compliance/ncsx-codesign-gate-result/v1"


class CodesignError(ValueError):
    """An input is malformed or inconsistent with the declared experiment."""


def _closed(mapping: dict[str, Any], expected: set[str], where: str) -> None:
    observed = set(mapping)
    if observed != expected:
        raise CodesignError(
            f"{where} keys differ: missing={sorted(expected-observed)}, "
            f"extra={sorted(observed-expected)}"
        )


def _number(mapping: dict[str, Any], key: str) -> float:
    try:
        value = float(mapping[key])
    except (KeyError, TypeError, ValueError) as error:
        raise CodesignError(f"{key!r} must be a finite decimal string") from error
    if not math.isfinite(value):
        raise CodesignError(f"{key!r} must be finite")
    return value


def _integer(mapping: dict[str, Any], key: str) -> int:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise CodesignError(f"{key!r} must be an integer")
    return value


def _exact(value: float, places: int = 12) -> str:
    if not math.isfinite(value):
        raise CodesignError(f"non-finite result {value!r}")
    rendered = f"{value:.{places}f}".rstrip("0").rstrip(".")
    return rendered if rendered not in {"", "-0"} else "0"


def _sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def _load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise CodesignError(f"cannot load Python module {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


@dataclass(frozen=True)
class Gate:
    minimum_reduction: float
    maximum_normal_ratio: float
    minimum_field_ratio: float
    maximum_field_ratio: float
    maximum_rms_displacement_m: float
    maximum_point_displacement_m: float
    maximum_length_change: float
    maximum_curvature_ratio: float
    maximum_compliance: float
    maximum_condition: float
    maximum_resolution_difference: float
    maximum_derivative_difference: float


@dataclass(frozen=True)
class CodesignModel:
    model_id: str
    base_model_sha256: str
    search_record_sha256: str
    maximum_fourier_mode: int
    curvature_samples: int
    repair_scale: float
    gate: Gate


def load_codesign_model(raw: bytes) -> CodesignModel:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CodesignError(f"co-design model is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise CodesignError("co-design model root must be an object")
    _closed(
        data,
        {
            "schema",
            "model_id",
            "base_model_sha256",
            "search_record_sha256",
            "parameterization",
            "selection",
            "gate",
        },
        "model",
    )
    if data["schema"] != MODEL_SCHEMA:
        raise CodesignError(f"unsupported model schema {data['schema']!r}")
    if not isinstance(data["model_id"], str) or not data["model_id"]:
        raise CodesignError("model_id must be a nonempty string")
    for key in ("base_model_sha256", "search_record_sha256"):
        value = data[key]
        if not isinstance(value, str) or not value.startswith("sha256:") or len(value) != 71:
            raise CodesignError(f"{key} must be a sha256 identity")

    parameterization = data["parameterization"]
    if not isinstance(parameterization, dict):
        raise CodesignError("parameterization must be an object")
    _closed(
        parameterization,
        {"maximum_fourier_mode", "coefficient_order", "curvature_samples"},
        "parameterization",
    )
    if parameterization["coefficient_order"] != (
        "base-coil-major,coordinate-major,constant-cosine-then-mode-sine-cosine"
    ):
        raise CodesignError("unsupported coefficient order")
    maximum_mode = _integer(parameterization, "maximum_fourier_mode")
    curvature_samples = _integer(parameterization, "curvature_samples")
    if not 1 <= maximum_mode <= 12 or curvature_samples < 128:
        raise CodesignError("parameterization is outside the supported box")

    selection = data["selection"]
    if not isinstance(selection, dict):
        raise CodesignError("selection must be an object")
    _closed(selection, {"repair_scale", "reason"}, "selection")
    if selection["reason"] != "conservative-rollback-after-fine-grid-normal-field-failure":
        raise CodesignError("selection reason does not match the declared loop")
    repair_scale = _number(selection, "repair_scale")
    if not 0 < repair_scale <= 1:
        raise CodesignError("repair_scale must lie in (0, 1]")

    gate_data = data["gate"]
    if not isinstance(gate_data, dict):
        raise CodesignError("gate must be an object")
    expected_gate = {
        "minimum_reduction",
        "maximum_normal_field_ratio",
        "minimum_field_strength_ratio",
        "maximum_field_strength_ratio",
        "maximum_rms_displacement_m",
        "maximum_point_displacement_m",
        "maximum_length_relative_change",
        "maximum_curvature_ratio",
        "maximum_compliance_ratio",
        "maximum_condition_number",
        "maximum_resolution_difference",
        "maximum_derivative_difference",
    }
    _closed(gate_data, expected_gate, "gate")
    # Set field order explicitly: set iteration is not stable across processes.
    gate = Gate(
        minimum_reduction=_number(gate_data, "minimum_reduction"),
        maximum_normal_ratio=_number(gate_data, "maximum_normal_field_ratio"),
        minimum_field_ratio=_number(gate_data, "minimum_field_strength_ratio"),
        maximum_field_ratio=_number(gate_data, "maximum_field_strength_ratio"),
        maximum_rms_displacement_m=_number(gate_data, "maximum_rms_displacement_m"),
        maximum_point_displacement_m=_number(gate_data, "maximum_point_displacement_m"),
        maximum_length_change=_number(gate_data, "maximum_length_relative_change"),
        maximum_curvature_ratio=_number(gate_data, "maximum_curvature_ratio"),
        maximum_compliance=_number(gate_data, "maximum_compliance_ratio"),
        maximum_condition=_number(gate_data, "maximum_condition_number"),
        maximum_resolution_difference=_number(gate_data, "maximum_resolution_difference"),
        maximum_derivative_difference=_number(gate_data, "maximum_derivative_difference"),
    )
    if min(gate.__dict__.values()) <= 0:
        raise CodesignError("all gate limits must be positive")
    if gate.minimum_field_ratio >= gate.maximum_field_ratio:
        raise CodesignError("field-strength limits are reversed")
    return CodesignModel(
        model_id=data["model_id"],
        base_model_sha256=data["base_model_sha256"],
        search_record_sha256=data["search_record_sha256"],
        maximum_fourier_mode=maximum_mode,
        curvature_samples=curvature_samples,
        repair_scale=repair_scale,
        gate=gate,
    )


@dataclass(frozen=True)
class SearchRecord:
    candidate: np.ndarray
    seed: int
    evaluations: int
    screen_reduction: float


def load_search_record(raw: bytes) -> SearchRecord:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CodesignError(f"search record is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise CodesignError("search record root must be an object")
    _closed(
        data,
        {"schema", "status", "registration", "method", "trace", "raw_candidate"},
        "search record",
    )
    if data["schema"] != SEARCH_SCHEMA or data["status"] != "exploratory-complete":
        raise CodesignError("unsupported or incomplete search record")
    registration = data["registration"]
    method = data["method"]
    trace = data["trace"]
    candidate = data["raw_candidate"]
    for value, expected, where in (
        (registration, {"timing", "frozen_gates"}, "registration"),
        (
            method,
            {
                "algorithm",
                "random_seed",
                "screen_resolution",
                "maximum_fourier_mode",
                "near_null_dimensions",
                "field_derivative_step_m",
                "objective_derivative_step_m",
                "iterations",
                "random_directions_per_iteration",
                "line_steps_m",
            },
            "method",
        ),
        (trace, {"baseline", "iterations", "extrapolation"}, "trace"),
        (
            candidate,
            {
                "coefficient_offsets_m",
                "screen_reduction",
                "screen_normal_field_ratio",
                "screen_field_strength_ratio",
                "screen_rms_displacement_m",
                "screen_maximum_displacement_m",
                "screen_maximum_length_relative_change",
                "screen_maximum_curvature_ratio",
                "screen_compliance_ratio",
                "screen_condition_number",
            },
            "raw_candidate",
        ),
    ):
        if not isinstance(value, dict):
            raise CodesignError(f"{where} must be an object")
        _closed(value, expected, where)
    if registration["timing"] != "gates-frozen-before-candidate-search":
        raise CodesignError("search record does not assert pre-search gate freezing")
    if method["algorithm"] != "iterated-normal-field-near-nullspace-descent":
        raise CodesignError("unsupported search algorithm")
    offsets = candidate["coefficient_offsets_m"]
    try:
        vector = np.asarray([float(value) for value in offsets], dtype=float)
    except (TypeError, ValueError) as error:
        raise CodesignError("candidate offsets must be decimal strings") from error
    if vector.ndim != 1 or not np.all(np.isfinite(vector)):
        raise CodesignError("candidate offsets must form one finite vector")
    return SearchRecord(
        candidate=vector,
        seed=_integer(method, "random_seed"),
        evaluations=_integer(trace["iterations"][-1], "cumulative_evaluations"),
        screen_reduction=_number(candidate, "screen_reduction"),
    )


def parameter_indices(base_model: Any, maximum_mode: int) -> list[tuple[int, int]]:
    result = []
    for coil_index in range(base_model.base_coil_count):
        for coordinate in range(3):
            offset = 6 * coil_index + 2 * coordinate
            result.append((0, offset + 1))
            for mode in range(1, maximum_mode + 1):
                result.extend(((mode, offset), (mode, offset + 1)))
    return result


def candidate_coefficients(
    original: np.ndarray,
    indices: list[tuple[int, int]],
    offsets: np.ndarray,
    scale: float,
) -> np.ndarray:
    if len(indices) != len(offsets):
        raise CodesignError(
            f"expected {len(indices)} candidate offsets, found {len(offsets)}"
        )
    result = original.copy()
    for value, (row, column) in zip(offsets, indices, strict=True):
        result[row, column] += scale * value
    return result


def base_coils(coefficients: np.ndarray, base_model: Any, segments: int) -> list[np.ndarray]:
    theta = 2.0 * np.pi * np.arange(segments) / segments
    curves = []
    for coil_index in range(base_model.base_coil_count):
        xyz = np.zeros((segments, 3))
        for coordinate in range(3):
            offset = 6 * coil_index + 2 * coordinate
            xyz[:, coordinate] = coefficients[0, offset + 1]
            for mode in range(1, base_model.coil_fourier_order + 1):
                xyz[:, coordinate] += (
                    coefficients[mode, offset] * np.sin(mode * theta)
                    + coefficients[mode, offset + 1] * np.cos(mode * theta)
                )
        curves.append(xyz)
    return curves


def physical_coils(
    coefficients: np.ndarray, base_model: Any, segments: int
) -> tuple[list[np.ndarray], np.ndarray]:
    bases = base_coils(coefficients, base_model, segments)
    flips = (False, True) if base_model.stellarator_symmetry else (False,)
    coils = []
    currents = []
    for period in range(base_model.nfp):
        phi = 2.0 * np.pi * period / base_model.nfp
        rotation = np.asarray(
            (
                (math.cos(phi), -math.sin(phi), 0.0),
                (math.sin(phi), math.cos(phi), 0.0),
                (0.0, 0.0, 1.0),
            )
        ).T
        for flip in flips:
            transform = rotation
            current_sign = 1.0
            if flip:
                transform = transform @ np.diag((1.0, -1.0, -1.0))
                current_sign = -1.0
            for curve, current in zip(bases, base_model.base_currents_a, strict=True):
                coils.append(curve @ transform)
                currents.append(current_sign * current)
    return coils, np.asarray(currents)


@dataclass(frozen=True)
class GridResult:
    baseline_ceiling: float
    candidate_ceiling: float
    normal_ratio: float
    field_ratio: float
    compliance: float
    condition: float
    sensitivity: np.ndarray


def analyze_grid(
    ceiling: Any,
    kernel: Any,
    boundary: np.ndarray,
    original_coefficients: np.ndarray,
    design_coefficients: np.ndarray,
    base_model: Any,
    resolution: Any,
    derivative_step_m: float,
) -> GridResult:
    points, normals, weights = ceiling.surface(boundary, base_model, resolution)
    original_coils, currents = physical_coils(
        original_coefficients, base_model, resolution.coil_segments
    )
    design_coils, design_currents = physical_coils(
        design_coefficients, base_model, resolution.coil_segments
    )
    original_field = ceiling.total_field(kernel, original_coils, points, currents)
    field_scale = math.sqrt(
        float(np.sum(weights * np.einsum("ij,ij->i", original_field, original_field)))
    )
    original_normal = math.sqrt(
        float(np.sum(weights * np.einsum("ij,ij->i", original_field, normals) ** 2))
    ) / field_scale
    design_field = ceiling.total_field(
        kernel, design_coils, points, design_currents
    )
    design_normal = math.sqrt(
        float(np.sum(weights * np.einsum("ij,ij->i", design_field, normals) ** 2))
    ) / field_scale
    field_ratio = math.sqrt(
        float(np.sum(weights * np.einsum("ij,ij->i", design_field, design_field)))
    ) / field_scale

    original_sensitivity = ceiling.magnetic_sensitivity(
        kernel,
        original_coils,
        currents,
        points,
        normals,
        weights,
        ceiling.local_frames(original_coils),
        field_scale,
        derivative_step_m,
    )
    original_hessian = original_sensitivity.T @ original_sensitivity
    _, _, control_score, _ = ceiling.uncoupled_control(
        kernel, original_hessian, len(original_coils), base_model
    )
    original_ideal, _, _ = ceiling.ideal_full_stiffness(original_hessian)
    baseline_ideal_score = ceiling.response_rms(original_hessian, original_ideal)

    sensitivity = ceiling.magnetic_sensitivity(
        kernel,
        design_coils,
        design_currents,
        points,
        normals,
        weights,
        ceiling.local_frames(design_coils),
        field_scale,
        derivative_step_m,
    )
    hessian = sensitivity.T @ sensitivity
    ideal, values, _ = ceiling.ideal_full_stiffness(hessian)
    ideal_score = ceiling.response_rms(hessian, ideal)
    return GridResult(
        baseline_ceiling=control_score / baseline_ideal_score,
        candidate_ceiling=control_score / ideal_score,
        normal_ratio=design_normal / original_normal,
        field_ratio=field_ratio,
        compliance=1.0 / float(values[0]),
        condition=float(values[-1] / values[0]),
        sensitivity=sensitivity,
    )


def curve_lengths(curves: list[np.ndarray]) -> np.ndarray:
    return np.asarray(
        [
            float(
                np.sum(np.linalg.norm(np.roll(curve, -1, axis=0) - curve, axis=1))
            )
            for curve in curves
        ]
    )


def maximum_curvatures(
    coefficients: np.ndarray, base_model: Any, samples: int
) -> np.ndarray:
    theta = 2.0 * np.pi * np.arange(samples) / samples
    maxima = []
    for coil_index in range(base_model.base_coil_count):
        first = np.zeros((samples, 3))
        second = np.zeros((samples, 3))
        for coordinate in range(3):
            offset = 6 * coil_index + 2 * coordinate
            for mode in range(1, base_model.coil_fourier_order + 1):
                sine = coefficients[mode, offset]
                cosine = coefficients[mode, offset + 1]
                first[:, coordinate] += mode * (
                    sine * np.cos(mode * theta) - cosine * np.sin(mode * theta)
                )
                second[:, coordinate] -= mode * mode * (
                    sine * np.sin(mode * theta) + cosine * np.cos(mode * theta)
                )
        speed = np.linalg.norm(first, axis=1)
        if np.any(speed <= 1.0e-12):
            raise CodesignError("candidate contains a degenerate coil tangent")
        curvature = np.linalg.norm(np.cross(first, second), axis=1) / speed**3
        maxima.append(float(np.max(curvature)))
    return np.asarray(maxima)


def geometry_metrics(
    original: np.ndarray,
    candidate: np.ndarray,
    base_model: Any,
    samples: int,
) -> dict[str, float]:
    original_curves = base_coils(original, base_model, samples)
    candidate_curves = base_coils(candidate, base_model, samples)
    displacement = np.concatenate(
        [
            new_curve - old_curve
            for new_curve, old_curve in zip(
                candidate_curves, original_curves, strict=True
            )
        ]
    )
    norms = np.linalg.norm(displacement, axis=1)
    length_change = np.max(
        np.abs(curve_lengths(candidate_curves) / curve_lengths(original_curves) - 1.0)
    )
    curvature_ratio = np.max(
        maximum_curvatures(candidate, base_model, samples)
        / maximum_curvatures(original, base_model, samples)
    )
    return {
        "rms_displacement_m": math.sqrt(float(np.mean(norms**2))),
        "maximum_displacement_m": float(np.max(norms)),
        "maximum_length_relative_change": float(length_change),
        "maximum_curvature_ratio": float(curvature_ratio),
    }


def run(
    ceiling: Any,
    kernel: Any,
    coil_path: Path,
    boundary_path: Path,
    base_model_path: Path,
    search_record_path: Path,
    codesign_model_path: Path,
) -> dict[str, Any]:
    base_model = ceiling.load_model(base_model_path.read_bytes())
    codesign_model = load_codesign_model(codesign_model_path.read_bytes())
    if _sha256(base_model_path) != codesign_model.base_model_sha256:
        raise CodesignError("base model digest does not match co-design declaration")
    if _sha256(search_record_path) != codesign_model.search_record_sha256:
        raise CodesignError("search record digest does not match co-design declaration")
    search = load_search_record(search_record_path.read_bytes())
    if search.seed != 20260904:
        raise CodesignError("unexpected exploratory search seed")

    original = np.loadtxt(coil_path, delimiter=",")
    if original.ndim != 2 or original.shape[1] != 6 * base_model.base_coil_count:
        raise CodesignError("coil coefficient table has the wrong shape")
    indices = parameter_indices(base_model, codesign_model.maximum_fourier_mode)
    raw = candidate_coefficients(original, indices, search.candidate, 1.0)
    repaired = candidate_coefficients(
        original, indices, search.candidate, codesign_model.repair_scale
    )
    boundary = ceiling.boundary_coefficients(boundary_path, base_model)
    step = base_model.derivative_step_m / 2.0
    reference = analyze_grid(
        ceiling,
        kernel,
        boundary,
        original,
        repaired,
        base_model,
        base_model.reference,
        step,
    )
    fine = analyze_grid(
        ceiling,
        kernel,
        boundary,
        original,
        repaired,
        base_model,
        base_model.fine,
        step,
    )
    raw_fine = analyze_grid(
        ceiling,
        kernel,
        boundary,
        original,
        raw,
        base_model,
        base_model.fine,
        step,
    )

    fine_points, fine_normals, fine_weights = ceiling.surface(
        boundary, base_model, base_model.fine
    )
    fine_coils, fine_currents = physical_coils(
        repaired, base_model, base_model.fine.coil_segments
    )
    original_fine_coils, _ = physical_coils(
        original, base_model, base_model.fine.coil_segments
    )
    original_fine_field = ceiling.total_field(
        kernel, original_fine_coils, fine_points, fine_currents
    )
    fine_field_scale = math.sqrt(
        float(
            np.sum(
                fine_weights
                * np.einsum("ij,ij->i", original_fine_field, original_fine_field)
            )
        )
    )
    wide_sensitivity = ceiling.magnetic_sensitivity(
        kernel,
        fine_coils,
        fine_currents,
        fine_points,
        fine_normals,
        fine_weights,
        ceiling.local_frames(fine_coils),
        fine_field_scale,
        base_model.derivative_step_m,
    )
    derivative_difference = float(
        np.linalg.norm(fine.sensitivity - wide_sensitivity)
        / np.linalg.norm(fine.sensitivity)
    )
    resolution_difference = abs(
        reference.candidate_ceiling - fine.candidate_ceiling
    ) / fine.candidate_ceiling
    geometry = geometry_metrics(
        original, repaired, base_model, codesign_model.curvature_samples
    )
    gate = codesign_model.gate
    geometry_utilization = max(
        geometry["rms_displacement_m"] / gate.maximum_rms_displacement_m,
        geometry["maximum_displacement_m"] / gate.maximum_point_displacement_m,
        geometry["maximum_length_relative_change"] / gate.maximum_length_change,
        geometry["maximum_curvature_ratio"] / gate.maximum_curvature_ratio,
    )
    checks = {
        "reduction": fine.candidate_ceiling >= gate.minimum_reduction,
        "normal_field": fine.normal_ratio <= gate.maximum_normal_ratio,
        "field_strength_lower": fine.field_ratio >= gate.minimum_field_ratio,
        "field_strength_upper": fine.field_ratio <= gate.maximum_field_ratio,
        "geometry": geometry_utilization <= 1.0,
        "compliance": fine.compliance <= gate.maximum_compliance,
        "condition": fine.condition <= gate.maximum_condition,
        "resolution": resolution_difference <= gate.maximum_resolution_difference,
        "derivative": derivative_difference <= gate.maximum_derivative_difference,
    }
    return {
        "schema": RESULT_SCHEMA,
        "model_id": codesign_model.model_id,
        "source": {
            "repository": base_model.upstream_repository,
            "commit": base_model.upstream_commit,
            "coil_sha256": _sha256(coil_path),
            "boundary_sha256": _sha256(boundary_path),
            "base_model_sha256": _sha256(base_model_path),
            "search_record_sha256": _sha256(search_record_path),
            "codesign_model_sha256": _sha256(codesign_model_path),
        },
        "method": {
            "candidate_family": "low-order-Fourier-coil-perturbations",
            "candidate_selection": "normal-field-near-nullspace-descent",
            "support_relaxation": "any-equal-trace-symmetric-positive-definite-stiffness",
            "control": "original-NCSX-best-declared-uncoupled-support",
            "normalization": "original-NCSX-surface-rms-field",
            "repair": "uniform-coefficient-rollback",
            "numpy_version": np.__version__,
        },
        "search_audit": {
            "random_seed": search.seed,
            "cumulative_screen_evaluations": search.evaluations,
            "recorded_screen_reduction": _exact(search.screen_reduction),
            "repair_scale": _exact(codesign_model.repair_scale),
            "raw_fine_reduction": _exact(raw_fine.candidate_ceiling),
            "raw_fine_normal_field_ratio": _exact(raw_fine.normal_ratio),
        },
        "metrics": {
            "fine_reduction_factor": _exact(fine.candidate_ceiling),
            "reference_reduction_factor": _exact(reference.candidate_ceiling),
            "fine_baseline_ceiling_factor": _exact(fine.baseline_ceiling),
            "fine_improvement_over_fixed_geometry": _exact(
                fine.candidate_ceiling / fine.baseline_ceiling
            ),
            "fine_normal_field_ratio": _exact(fine.normal_ratio),
            "fine_field_strength_ratio": _exact(fine.field_ratio),
            "geometry_envelope_utilization": _exact(geometry_utilization),
            "ideal_worst_compliance_ratio": _exact(fine.compliance),
            "ideal_condition_number": _exact(fine.condition),
            "resolution_relative_difference": _exact(resolution_difference),
            "derivative_relative_difference": _exact(derivative_difference),
            "target_shortfall": _exact(gate.minimum_reduction - fine.candidate_ceiling),
        },
        "geometry": {key: _exact(value) for key, value in geometry.items()},
        "gate_checks": {key: "pass" if value else "fail" for key, value in checks.items()},
        "interpretation": "candidate-justified" if all(checks.values()) else "stop",
        "limitations": [
            "The local Fourier search is not a global proof over coil geometry.",
            "The full stiffness matrix is a generous mathematical ceiling, not a realizable support.",
            "No free-boundary equilibrium, structural stress, force, clearance, winding-pack, quench, cost, safety, novelty, or patent analysis is performed.",
            "The fine grid was used as compiler feedback for the declared rollback and is not an independent holdout.",
            "Core verifies identities, execution, extraction, and requirement arithmetic; it does not validate the scientific model.",
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ceiling_library", type=Path)
    parser.add_argument("field_kernel", type=Path)
    parser.add_argument("coil_data", type=Path)
    parser.add_argument("boundary_data", type=Path)
    parser.add_argument("base_model", type=Path)
    parser.add_argument("search_record", type=Path)
    parser.add_argument("codesign_model", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args(argv)
    try:
        ceiling = _load_module("ncsx_passive_ceiling_library", args.ceiling_library)
        kernel = ceiling.load_field_kernel(args.field_kernel)
        result = run(
            ceiling,
            kernel,
            args.coil_data,
            args.boundary_data,
            args.base_model,
            args.search_record,
            args.codesign_model,
        )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
    except (CodesignError, OSError, ValueError) as error:
        print(f"co-design gate failed: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
