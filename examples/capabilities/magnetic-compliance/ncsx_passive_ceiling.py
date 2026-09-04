#!/usr/bin/env python3
"""Full-matrix passive-support ceiling on the public SIMSOPT NCSX data.

For a rigid-translation magnetic sensitivity matrix S and a symmetric
positive-definite stiffness matrix K, the direction-neutral squared response
is tr(H K^-2) / n, where H = S^T S.  At fixed tr(K), the global optimum shares
eigenvectors with H and has stiffness eigenvalues proportional to the cube
roots of H's eigenvalues.  This checker constructs that deliberately
unrealizable best case and compares it with the declared uncoupled control.
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


SCHEMA = "avila.magnetic-compliance/ncsx-ceiling-model/v1"
RESULT_SCHEMA = "avila.magnetic-compliance/ncsx-ceiling-result/v1"


class CeilingError(ValueError):
    """An input is malformed or the ceiling's mathematical box is violated."""


def _closed(mapping: dict[str, Any], expected: set[str], where: str) -> None:
    observed = set(mapping)
    if observed != expected:
        raise CeilingError(
            f"{where} keys differ: "
            f"missing={sorted(expected - observed)}, extra={sorted(observed - expected)}"
        )


def _number(mapping: dict[str, Any], key: str) -> float:
    try:
        value = float(mapping[key])
    except (KeyError, TypeError, ValueError) as error:
        raise CeilingError(f"`{key}` must be a finite decimal string") from error
    if not math.isfinite(value):
        raise CeilingError(f"`{key}` must be finite")
    return value


def _integer(mapping: dict[str, Any], key: str) -> int:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise CeilingError(f"`{key}` must be an integer")
    return value


def _exact(value: float, places: int = 12) -> str:
    if not math.isfinite(value):
        raise CeilingError(f"non-finite result {value!r}")
    rendered = f"{value:.{places}f}".rstrip("0").rstrip(".")
    return rendered if rendered not in {"", "-0"} else "0"


def _sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


@dataclass(frozen=True)
class Resolution:
    coil_segments: int
    surface_toroidal: int
    surface_poloidal: int


@dataclass(frozen=True)
class Model:
    model_id: str
    upstream_repository: str
    upstream_commit: str
    coil_path: str
    boundary_path: str
    nfp: int
    stellarator_symmetry: bool
    base_coil_count: int
    coil_fourier_order: int
    base_currents_a: tuple[float, ...]
    boundary_coefficient_count: int
    reference: Resolution
    fine: Resolution
    derivative_step_m: float
    operating_displacement_m: float
    allocation_levels: tuple[float, ...]
    maximum_compliance_ratio: float
    maximum_condition_number: float
    minimum_linear_ceiling: float
    maximum_linearization_error: float
    maximum_resolution_difference: float
    maximum_derivative_difference: float


def _resolution(data: Any, where: str) -> Resolution:
    if not isinstance(data, dict):
        raise CeilingError(f"{where} must be an object")
    _closed(data, {"coil_segments", "surface_toroidal", "surface_poloidal"}, where)
    result = Resolution(
        coil_segments=_integer(data, "coil_segments"),
        surface_toroidal=_integer(data, "surface_toroidal"),
        surface_poloidal=_integer(data, "surface_poloidal"),
    )
    if min(result.coil_segments, result.surface_toroidal, result.surface_poloidal) <= 0:
        raise CeilingError(f"{where} resolutions must be positive")
    if any(
        value % 2
        for value in (
            result.coil_segments,
            result.surface_toroidal,
            result.surface_poloidal,
        )
    ):
        raise CeilingError(f"{where} resolutions must be even")
    return result


def load_model(raw: bytes) -> Model:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CeilingError(f"model is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise CeilingError("model root must be an object")
    _closed(
        data,
        {
            "schema",
            "model_id",
            "source",
            "geometry",
            "numerics",
            "uncoupled_control",
            "gate",
        },
        "model",
    )
    if data["schema"] != SCHEMA:
        raise CeilingError(f"unsupported schema {data['schema']!r}")
    if not isinstance(data["model_id"], str) or not data["model_id"]:
        raise CeilingError("`model_id` must be a nonempty string")

    source = data["source"]
    if not isinstance(source, dict):
        raise CeilingError("source must be an object")
    _closed(source, {"repository", "commit", "coil_path", "boundary_path"}, "source")
    for key in ("repository", "commit", "coil_path", "boundary_path"):
        if not isinstance(source[key], str) or not source[key]:
            raise CeilingError(f"source `{key}` must be a nonempty string")

    geometry = data["geometry"]
    if not isinstance(geometry, dict):
        raise CeilingError("geometry must be an object")
    _closed(
        geometry,
        {
            "nfp",
            "stellarator_symmetry",
            "base_coil_count",
            "coil_fourier_order",
            "base_currents_a",
            "boundary_coefficient_count",
        },
        "geometry",
    )
    if not isinstance(geometry["stellarator_symmetry"], bool):
        raise CeilingError("`stellarator_symmetry` must be boolean")
    try:
        currents = tuple(float(value) for value in geometry["base_currents_a"])
    except (TypeError, ValueError) as error:
        raise CeilingError("`base_currents_a` must be decimal strings") from error
    if not currents or any(not math.isfinite(value) or value == 0 for value in currents):
        raise CeilingError("base currents must be finite and nonzero")

    numerics = data["numerics"]
    if not isinstance(numerics, dict):
        raise CeilingError("numerics must be an object")
    _closed(
        numerics,
        {"reference", "fine", "derivative_step_m", "operating_displacement_m"},
        "numerics",
    )
    reference = _resolution(numerics["reference"], "numerics.reference")
    fine = _resolution(numerics["fine"], "numerics.fine")
    if not (
        fine.coil_segments > reference.coil_segments
        and fine.surface_toroidal > reference.surface_toroidal
        and fine.surface_poloidal > reference.surface_poloidal
    ):
        raise CeilingError("every fine resolution must exceed its reference resolution")

    control = data["uncoupled_control"]
    if not isinstance(control, dict):
        raise CeilingError("uncoupled_control must be an object")
    _closed(
        control,
        {"allocation_levels", "maximum_compliance_ratio", "maximum_condition_number"},
        "uncoupled_control",
    )
    try:
        levels = tuple(float(value) for value in control["allocation_levels"])
    except (TypeError, ValueError) as error:
        raise CeilingError("allocation levels must be decimal strings") from error
    if not levels or any(not math.isfinite(value) or value <= 0 for value in levels):
        raise CeilingError("allocation levels must be finite and positive")

    gate = data["gate"]
    if not isinstance(gate, dict):
        raise CeilingError("gate must be an object")
    _closed(
        gate,
        {
            "minimum_linear_ceiling",
            "maximum_compliance_ratio",
            "maximum_condition_number",
            "maximum_linearization_error",
            "maximum_resolution_difference",
            "maximum_derivative_difference",
        },
        "gate",
    )

    model = Model(
        model_id=data["model_id"],
        upstream_repository=source["repository"],
        upstream_commit=source["commit"],
        coil_path=source["coil_path"],
        boundary_path=source["boundary_path"],
        nfp=_integer(geometry, "nfp"),
        stellarator_symmetry=geometry["stellarator_symmetry"],
        base_coil_count=_integer(geometry, "base_coil_count"),
        coil_fourier_order=_integer(geometry, "coil_fourier_order"),
        base_currents_a=currents,
        boundary_coefficient_count=_integer(geometry, "boundary_coefficient_count"),
        reference=reference,
        fine=fine,
        derivative_step_m=_number(numerics, "derivative_step_m"),
        operating_displacement_m=_number(numerics, "operating_displacement_m"),
        allocation_levels=levels,
        maximum_compliance_ratio=_number(control, "maximum_compliance_ratio"),
        maximum_condition_number=_number(control, "maximum_condition_number"),
        minimum_linear_ceiling=_number(gate, "minimum_linear_ceiling"),
        maximum_linearization_error=_number(gate, "maximum_linearization_error"),
        maximum_resolution_difference=_number(gate, "maximum_resolution_difference"),
        maximum_derivative_difference=_number(gate, "maximum_derivative_difference"),
    )
    if model.nfp <= 0 or model.base_coil_count <= 0 or model.coil_fourier_order <= 0:
        raise CeilingError("geometry counts and Fourier order must be positive")
    if len(model.base_currents_a) != model.base_coil_count:
        raise CeilingError("one base current is required for each base coil")
    if model.boundary_coefficient_count <= 0:
        raise CeilingError("boundary coefficient count must be positive")
    if not 0 < model.derivative_step_m <= 0.01:
        raise CeilingError("derivative step must lie in (0, 0.01]")
    if not 0 < model.operating_displacement_m <= 0.02:
        raise CeilingError("operating displacement must lie in (0, 0.02]")
    if min(
        model.maximum_compliance_ratio,
        model.maximum_condition_number,
        model.minimum_linear_ceiling,
        model.maximum_linearization_error,
        model.maximum_resolution_difference,
        model.maximum_derivative_difference,
    ) <= 0:
        raise CeilingError("control and gate limits must be positive")
    if abs(model.maximum_compliance_ratio - _number(gate, "maximum_compliance_ratio")) > 0:
        raise CeilingError("internal compliance limit mismatch")
    if abs(model.maximum_condition_number - _number(gate, "maximum_condition_number")) > 0:
        raise CeilingError("internal condition-number limit mismatch")
    return model


def load_field_kernel(path: Path) -> Any:
    spec = importlib.util.spec_from_file_location("magnetic_compliance_kernel", path)
    if spec is None or spec.loader is None:
        raise CeilingError("cannot load the declared field kernel")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def base_coils(path: Path, model: Model, segments: int) -> list[np.ndarray]:
    data = np.loadtxt(path, delimiter=",")
    if data.ndim != 2 or data.shape[1] != 6 * model.base_coil_count:
        raise CeilingError("coil coefficient table has the wrong number of columns")
    if data.shape[0] <= model.coil_fourier_order:
        raise CeilingError("coil coefficient table does not contain the declared Fourier order")
    theta = 2.0 * np.pi * np.arange(segments) / segments
    curves: list[np.ndarray] = []
    for coil_index in range(model.base_coil_count):
        xyz = np.zeros((segments, 3))
        for coordinate in range(3):
            offset = 6 * coil_index + 2 * coordinate
            xyz[:, coordinate] = data[0, offset + 1]
            for mode in range(1, model.coil_fourier_order + 1):
                xyz[:, coordinate] += (
                    data[mode, offset] * np.sin(mode * theta)
                    + data[mode, offset + 1] * np.cos(mode * theta)
                )
        curves.append(xyz)
    return curves


def physical_coils(
    path: Path, model: Model, segments: int
) -> tuple[list[np.ndarray], np.ndarray]:
    bases = base_coils(path, model, segments)
    flips = (False, True) if model.stellarator_symmetry else (False,)
    coils: list[np.ndarray] = []
    currents: list[float] = []
    for period in range(model.nfp):
        phi = 2.0 * np.pi * period / model.nfp
        rotation = np.array(
            [
                [math.cos(phi), -math.sin(phi), 0.0],
                [math.sin(phi), math.cos(phi), 0.0],
                [0.0, 0.0, 1.0],
            ]
        ).T
        for flip in flips:
            transform = rotation
            current_sign = 1.0
            if flip:
                transform = transform @ np.diag((1.0, -1.0, -1.0))
                current_sign = -1.0
            for curve, current in zip(bases, model.base_currents_a, strict=True):
                coils.append(curve @ transform)
                currents.append(current_sign * current)
    return coils, np.asarray(currents)


def boundary_coefficients(path: Path, model: Model) -> np.ndarray:
    lines = path.read_text(encoding="utf-8").splitlines()
    header_index = next(
        (index for index, line in enumerate(lines) if line.strip().startswith("# n m")),
        None,
    )
    if header_index is None:
        raise CeilingError("boundary coefficient header is absent")
    rows = []
    for line in lines[header_index + 1 :]:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        fields = stripped.split()
        if len(fields) != 6:
            raise CeilingError("boundary coefficient row must have six fields")
        rows.append([float(value) for value in fields])
    result = np.asarray(rows)
    if result.shape != (model.boundary_coefficient_count, 6):
        raise CeilingError(
            f"expected {model.boundary_coefficient_count} boundary rows, got {len(rows)}"
        )
    return result


def surface(
    coefficients: np.ndarray, model: Model, resolution: Resolution
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    phi = 2.0 * np.pi * (
        np.arange(resolution.surface_toroidal) + 0.5
    ) / resolution.surface_toroidal
    theta = 2.0 * np.pi * (
        np.arange(resolution.surface_poloidal) + 0.5
    ) / resolution.surface_poloidal
    phi_grid, theta_grid = np.meshgrid(phi, theta, indexing="ij")
    radius = np.zeros_like(phi_grid)
    height = np.zeros_like(phi_grid)
    radius_theta = np.zeros_like(phi_grid)
    height_theta = np.zeros_like(phi_grid)
    radius_phi = np.zeros_like(phi_grid)
    height_phi = np.zeros_like(phi_grid)
    for n_raw, m_raw, radius_cos, radius_sin, height_cos, height_sin in coefficients:
        n = int(n_raw)
        m = int(m_raw)
        angle = m * theta_grid - n * model.nfp * phi_grid
        cosine = np.cos(angle)
        sine = np.sin(angle)
        radius += radius_cos * cosine + radius_sin * sine
        height += height_cos * cosine + height_sin * sine
        radius_theta += m * (-radius_cos * sine + radius_sin * cosine)
        height_theta += m * (-height_cos * sine + height_sin * cosine)
        radius_phi += n * model.nfp * (radius_cos * sine - radius_sin * cosine)
        height_phi += n * model.nfp * (height_cos * sine - height_sin * cosine)
    cosine_phi = np.cos(phi_grid)
    sine_phi = np.sin(phi_grid)
    points = np.stack((radius * cosine_phi, radius * sine_phi, height), axis=-1)
    tangent_theta = np.stack(
        (radius_theta * cosine_phi, radius_theta * sine_phi, height_theta), axis=-1
    )
    tangent_phi = np.stack(
        (
            radius_phi * cosine_phi - radius * sine_phi,
            radius_phi * sine_phi + radius * cosine_phi,
            height_phi,
        ),
        axis=-1,
    )
    normal_vectors = np.cross(tangent_phi, tangent_theta)
    area = np.linalg.norm(normal_vectors, axis=-1)
    if np.any(area <= 1.0e-14):
        raise CeilingError("boundary surface contains a degenerate sample")
    normals = normal_vectors / area[:, :, None]
    weights = area / np.sum(area)
    return points.reshape((-1, 3)), normals.reshape((-1, 3)), weights.reshape(-1)


def local_frames(coils: list[np.ndarray]) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    centers = np.asarray([np.mean(coil, axis=0) for coil in coils])
    radial = centers.copy()
    radial[:, 2] = 0.0
    radial_norms = np.linalg.norm(radial, axis=1)
    if np.any(radial_norms <= 1.0e-14):
        raise CeilingError("a coil center has no cylindrical radial direction")
    radial /= radial_norms[:, None]
    toroidal = np.column_stack((-radial[:, 1], radial[:, 0], np.zeros(len(coils))))
    vertical = np.tile((0.0, 0.0, 1.0), (len(coils), 1))
    return radial, toroidal, vertical


def total_field(
    kernel: Any, coils: list[np.ndarray], points: np.ndarray, currents: np.ndarray
) -> np.ndarray:
    result = np.zeros_like(points)
    for coil, current in zip(coils, currents, strict=True):
        result += kernel.field_from_coil(coil, points, float(current))
    return result


def magnetic_sensitivity(
    kernel: Any,
    coils: list[np.ndarray],
    currents: np.ndarray,
    points: np.ndarray,
    normals: np.ndarray,
    weights: np.ndarray,
    frames: tuple[np.ndarray, ...],
    field_scale: float,
    step_m: float,
) -> np.ndarray:
    result = np.empty((len(points), 3 * len(coils)))
    root_weights = np.sqrt(weights)
    for coil_index, coil in enumerate(coils):
        for direction_index, directions in enumerate(frames):
            direction = directions[coil_index]
            plus = kernel.field_from_coil(
                coil + step_m * direction, points, float(currents[coil_index])
            )
            minus = kernel.field_from_coil(
                coil - step_m * direction, points, float(currents[coil_index])
            )
            derivative = np.einsum("ij,ij->i", plus - minus, normals) / (2.0 * step_m)
            result[:, 3 * coil_index + direction_index] = (
                root_weights * derivative / field_scale
            )
    return result


def response_rms(hessian: np.ndarray, stiffness: np.ndarray) -> float:
    inverse = np.linalg.solve(stiffness, np.eye(stiffness.shape[0]))
    return math.sqrt(float(np.trace(inverse @ hessian @ inverse) / len(stiffness)))


def uncoupled_control(
    kernel: Any, hessian: np.ndarray, count: int, model: Model
) -> tuple[np.ndarray, tuple[float, float, float], float, int]:
    triples = kernel.allocation_triples(model.allocation_levels)
    best_matrix: np.ndarray | None = None
    best_allocations: tuple[float, float, float] | None = None
    best_score = math.inf
    evaluated = 0
    for allocations in triples:
        matrix = kernel.support_matrix(count, allocations, (0.0, 0.0, 0.0))
        eigenvalues = np.linalg.eigvalsh(matrix)
        compliance = 1.0 / float(eigenvalues[0])
        condition = float(eigenvalues[-1] / eigenvalues[0])
        evaluated += 1
        if (
            compliance > model.maximum_compliance_ratio + 1.0e-12
            or condition > model.maximum_condition_number + 1.0e-12
        ):
            continue
        score = response_rms(hessian, matrix)
        key = (score, allocations)
        if best_matrix is None or key < (best_score, best_allocations):
            best_matrix = matrix
            best_allocations = allocations
            best_score = score
    if best_matrix is None or best_allocations is None:
        raise CeilingError("no admissible uncoupled control exists")
    return best_matrix, best_allocations, best_score, evaluated


def ideal_full_stiffness(hessian: np.ndarray) -> tuple[np.ndarray, np.ndarray, float]:
    values, vectors = np.linalg.eigh(hessian)
    largest = float(values[-1])
    if largest <= 0 or float(values[0]) <= largest * 1.0e-12:
        raise CeilingError("sensitivity Hessian is not numerically positive definite")
    stiffness_values = np.cbrt(values)
    stiffness_values *= len(values) / np.sum(stiffness_values)
    matrix = (vectors * stiffness_values) @ vectors.T
    matrix = 0.5 * (matrix + matrix.T)
    scale = float(np.mean(stiffness_values**3 / values))
    stationarity_residual = float(
        np.linalg.norm(stiffness_values**3 - scale * values)
        / np.linalg.norm(stiffness_values**3)
    )
    return matrix, stiffness_values, stationarity_residual


@dataclass(frozen=True)
class Analysis:
    coils: list[np.ndarray]
    currents: np.ndarray
    points: np.ndarray
    normals: np.ndarray
    weights: np.ndarray
    frames: tuple[np.ndarray, ...]
    base_field: np.ndarray
    field_scale: float
    sensitivity: np.ndarray
    hessian: np.ndarray
    control: np.ndarray
    control_allocations: tuple[float, float, float]
    control_score: float
    ideal: np.ndarray
    ideal_values: np.ndarray
    ideal_score: float
    stationarity_residual: float
    controls_evaluated: int
    base_normal_fraction: float


def analyze(
    kernel: Any,
    coil_path: Path,
    boundary: np.ndarray,
    model: Model,
    resolution: Resolution,
    derivative_step_m: float,
) -> Analysis:
    coils, currents = physical_coils(coil_path, model, resolution.coil_segments)
    points, normals, weights = surface(boundary, model, resolution)
    frames = local_frames(coils)
    base_field = total_field(kernel, coils, points, currents)
    field_scale = math.sqrt(
        float(np.sum(weights * np.einsum("ij,ij->i", base_field, base_field)))
    )
    if field_scale <= 0:
        raise CeilingError("coil set produces no reference magnetic field")
    sensitivity = magnetic_sensitivity(
        kernel,
        coils,
        currents,
        points,
        normals,
        weights,
        frames,
        field_scale,
        derivative_step_m,
    )
    hessian = sensitivity.T @ sensitivity
    control, allocations, control_score, evaluated = uncoupled_control(
        kernel, hessian, len(coils), model
    )
    ideal, ideal_values, residual = ideal_full_stiffness(hessian)
    ideal_score = response_rms(hessian, ideal)
    normal_field = np.einsum("ij,ij->i", base_field, normals)
    base_normal_fraction = math.sqrt(float(np.sum(weights * normal_field**2))) / field_scale
    return Analysis(
        coils=coils,
        currents=currents,
        points=points,
        normals=normals,
        weights=weights,
        frames=frames,
        base_field=base_field,
        field_scale=field_scale,
        sensitivity=sensitivity,
        hessian=hessian,
        control=control,
        control_allocations=allocations,
        control_score=control_score,
        ideal=ideal,
        ideal_values=ideal_values,
        ideal_score=ideal_score,
        stationarity_residual=residual,
        controls_evaluated=evaluated,
        base_normal_fraction=base_normal_fraction,
    )


def exact_incremental_error(
    kernel: Any, analysis: Analysis, translations: np.ndarray
) -> float:
    moved: list[np.ndarray] = []
    for index, coil in enumerate(analysis.coils):
        shift = sum(
            translations[index, direction] * analysis.frames[direction][index]
            for direction in range(3)
        )
        moved.append(coil + shift)
    change = (
        total_field(kernel, moved, analysis.points, analysis.currents)
        - analysis.base_field
    )
    normal_change = np.einsum("ij,ij->i", change, analysis.normals)
    return (
        math.sqrt(float(np.sum(analysis.weights * normal_change**2)))
        / analysis.field_scale
    )


def nonlinear_check(
    kernel: Any, analysis: Analysis, displacement_m: float
) -> tuple[float, float]:
    loads = displacement_m * np.eye(analysis.ideal.shape[0])
    exact_rms: list[float] = []
    linear_rms: list[float] = []
    discrepancies: list[float] = []
    for stiffness in (analysis.control, analysis.ideal):
        responses = np.linalg.solve(stiffness, loads)
        exact_errors = []
        linear_errors = []
        for column in range(responses.shape[1]):
            response = responses[:, column]
            linear_error = float(np.linalg.norm(analysis.sensitivity @ response))
            exact_error = exact_incremental_error(
                kernel, analysis, response.reshape((len(analysis.coils), 3))
            )
            linear_errors.append(linear_error)
            exact_errors.append(exact_error)
            if exact_error > 1.0e-12:
                discrepancies.append(abs(exact_error - linear_error) / exact_error)
        exact_rms.append(math.sqrt(float(np.mean(np.square(exact_errors)))))
        linear_rms.append(math.sqrt(float(np.mean(np.square(linear_errors)))))
    exact_reduction = exact_rms[0] / exact_rms[1]
    return exact_reduction, max(discrepancies, default=0.0)


def run(
    kernel: Any,
    coil_path: Path,
    boundary_path: Path,
    model: Model,
    source_hashes: dict[str, str],
) -> dict[str, Any]:
    boundary = boundary_coefficients(boundary_path, model)
    primary_step = model.derivative_step_m / 2.0
    reference = analyze(
        kernel, coil_path, boundary, model, model.reference, primary_step
    )
    fine = analyze(kernel, coil_path, boundary, model, model.fine, primary_step)
    fine_wide_step = magnetic_sensitivity(
        kernel,
        fine.coils,
        fine.currents,
        fine.points,
        fine.normals,
        fine.weights,
        fine.frames,
        fine.field_scale,
        model.derivative_step_m,
    )
    derivative_difference = float(
        np.linalg.norm(fine.sensitivity - fine_wide_step)
        / np.linalg.norm(fine.sensitivity)
    )

    reference_ceiling = reference.control_score / reference.ideal_score
    fine_ceiling = fine.control_score / fine.ideal_score
    resolution_difference = abs(reference_ceiling - fine_ceiling) / fine_ceiling
    exact_reduction, linearization_error = nonlinear_check(
        kernel, reference, model.operating_displacement_m
    )
    ideal_minimum = float(fine.ideal_values[0])
    ideal_maximum = float(fine.ideal_values[-1])
    compliance_ratio = 1.0 / ideal_minimum
    condition_number = ideal_maximum / ideal_minimum
    singular_values = np.linalg.svd(fine.sensitivity, compute_uv=False)
    sensitivity_contrast = float(singular_values[0] / singular_values[-1])
    low_sensitivity_fraction = float(
        np.mean(singular_values <= singular_values[0] / 10.0)
    )

    checks = {
        "linear_ceiling": fine_ceiling >= model.minimum_linear_ceiling,
        "compliance_ratio": compliance_ratio <= model.maximum_compliance_ratio,
        "condition_number": condition_number <= model.maximum_condition_number,
        "linearization_error": linearization_error <= model.maximum_linearization_error,
        "resolution_difference": resolution_difference
        <= model.maximum_resolution_difference,
        "derivative_difference": derivative_difference
        <= model.maximum_derivative_difference,
    }
    interpretation = "search-justified" if all(checks.values()) else "stop"
    return {
        "schema": RESULT_SCHEMA,
        "model_id": model.model_id,
        "source": {
            "repository": model.upstream_repository,
            "commit": model.upstream_commit,
            "declared_coil_path": model.coil_path,
            "declared_boundary_path": model.boundary_path,
            **source_hashes,
        },
        "method": {
            "field_model": "midpoint-filament-biot-savart",
            "deformation_model": "rigid-coil-local-translations",
            "control": "best-closed-grid-uncoupled-directional-support",
            "relaxation": "any-symmetric-positive-definite-stiffness-at-equal-trace",
            "closed_form": "shared-hessian-eigenvectors-with-cube-root-eigenvalue-allocation",
            "numpy_version": np.__version__,
        },
        "metrics": {
            "fine_linear_ceiling_factor": _exact(fine_ceiling),
            "reference_linear_ceiling_factor": _exact(reference_ceiling),
            "reference_exact_realization_factor": _exact(exact_reduction),
            "ideal_worst_compliance_ratio": _exact(compliance_ratio),
            "ideal_condition_number": _exact(condition_number),
            "linearization_relative_error": _exact(linearization_error),
            "resolution_relative_difference": _exact(resolution_difference),
            "derivative_relative_difference": _exact(derivative_difference),
            "sensitivity_singular_value_contrast": _exact(sensitivity_contrast),
            "low_sensitivity_dimension_fraction": _exact(low_sensitivity_fraction),
            "base_normal_field_fraction": _exact(fine.base_normal_fraction),
            "stiffness_trace_ratio": _exact(np.trace(fine.ideal) / len(fine.ideal)),
            "stationarity_relative_residual": _exact(fine.stationarity_residual),
        },
        "geometry": {
            "base_coils": model.base_coil_count,
            "physical_coils": len(fine.coils),
            "rigid_translation_dofs": fine.ideal.shape[0],
            "coil_fourier_order": model.coil_fourier_order,
        },
        "selected": {
            "fine_control_allocations": [
                _exact(value) for value in fine.control_allocations
            ],
            "fine_ideal_minimum_stiffness": _exact(ideal_minimum),
            "fine_ideal_maximum_stiffness": _exact(ideal_maximum),
        },
        "search": {
            "reference_controls_evaluated": reference.controls_evaluated,
            "fine_controls_evaluated": fine.controls_evaluated,
        },
        "gate_checks": {key: "pass" if value else "fail" for key, value in checks.items()},
        "interpretation": interpretation,
        "limitations": [
            "The NCSX files are public SIMSOPT reference data; this calculation is not endorsed or validated by the SIMSOPT authors or NCSX institutions.",
            "Only the modular coils represented by the SIMSOPT configuration are included; circular coils are absent.",
            "The result is a ceiling for the discretized rigid-translation, passive linear stiffness model, not for nonlinear, active, coil-geometry, or plasma-equilibrium co-design.",
            "The full stiffness matrix is a mathematical relaxation and is not claimed to be structurally realizable.",
            "Equal stiffness trace is not equal mass, stress, cost, access, fatigue life, or manufacturability.",
            "Passing would justify a topology campaign; it would not establish a practical support, novelty, patentability, safety, or reactor performance.",
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("field_kernel", type=Path)
    parser.add_argument("coils", type=Path)
    parser.add_argument("boundary", type=Path)
    parser.add_argument("model", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args(argv)
    try:
        model_raw = args.model.read_bytes()
        model = load_model(model_raw)
        kernel = load_field_kernel(args.field_kernel)
        source_hashes = {
            "field_kernel_sha256": _sha256(args.field_kernel),
            "coil_data_sha256": _sha256(args.coils),
            "boundary_data_sha256": _sha256(args.boundary),
            "model_sha256": "sha256:" + hashlib.sha256(model_raw).hexdigest(),
        }
        result = run(kernel, args.coils, args.boundary, model, source_hashes)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
    except (OSError, CeilingError, ValueError, np.linalg.LinAlgError) as error:
        print(f"NCSX passive-ceiling checker: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
