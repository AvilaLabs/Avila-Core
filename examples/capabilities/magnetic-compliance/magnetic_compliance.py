#!/usr/bin/env python3
"""Reduced-order feasibility checker for field-orthogonal coil supports.

This is deliberately not a stellarator design code.  It uses filamentary
Biot--Savart fields, rigid translations of an analytic set of non-planar
modular coils, and a cyclic spring network.  Its only scientific question is
whether a local, positive-definite support can redirect compliance toward
magnetically less-sensitive collective motions at fixed stiffness trace.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np


MU0_OVER_4PI = 1.0e-7
SCHEMA = "avila.magnetic-compliance/feasibility-model/v1"
RESULT_SCHEMA = "avila.magnetic-compliance/feasibility-result/v1"


class ModelError(ValueError):
    """The declared model is malformed or outside this checker's box."""


def _exact(value: float, places: int = 12) -> str:
    if not math.isfinite(value):
        raise ModelError(f"non-finite result {value!r}")
    rendered = f"{value:.{places}f}".rstrip("0").rstrip(".")
    return rendered if rendered not in {"", "-0"} else "0"


def _number(mapping: dict[str, Any], key: str) -> float:
    try:
        value = float(mapping[key])
    except (KeyError, TypeError, ValueError) as error:
        raise ModelError(f"`{key}` must be a finite decimal string") from error
    if not math.isfinite(value):
        raise ModelError(f"`{key}` must be finite")
    return value


def _integer(mapping: dict[str, Any], key: str) -> int:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise ModelError(f"`{key}` must be an integer")
    return value


def _closed(mapping: dict[str, Any], expected: set[str], where: str) -> None:
    observed = set(mapping)
    if observed != expected:
        missing = sorted(expected - observed)
        extra = sorted(observed - expected)
        raise ModelError(f"{where} keys differ: missing={missing}, extra={extra}")


@dataclass(frozen=True)
class Geometry:
    coil_count: int
    major_radius_m: float
    coil_minor_radius_m: float
    plasma_minor_radius_m: float
    nonplanar_amplitude_m: float
    nonplanar_harmonic: int
    current_a: float
    fine_coil_segments: int
    fine_surface_toroidal: int
    fine_surface_poloidal: int


@dataclass(frozen=True)
class Search:
    allocation_levels: tuple[float, ...]
    coupling_levels: tuple[float, ...]
    maximum_compliance_ratio: float
    maximum_condition_number: float


@dataclass(frozen=True)
class Gate:
    minimum_exact_reduction: float
    maximum_nominal_error_ratio: float
    maximum_compliance_ratio: float
    maximum_linearization_error: float
    maximum_resolution_difference: float
    maximum_derivative_difference: float
    minimum_coupling_share: float


@dataclass(frozen=True)
class Model:
    model_id: str
    geometry: Geometry
    derivative_step_m: float
    operating_displacement_m: float
    search: Search
    gate: Gate


def load_model(raw: bytes) -> Model:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise ModelError(f"model is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise ModelError("model root must be an object")
    _closed(
        data,
        {
            "schema",
            "model_id",
            "geometry",
            "derivative_step_m",
            "operating_displacement_m",
            "support_search",
            "feasibility_gate",
        },
        "model",
    )
    if data["schema"] != SCHEMA:
        raise ModelError(f"unsupported schema {data['schema']!r}")
    if not isinstance(data["model_id"], str) or not data["model_id"]:
        raise ModelError("`model_id` must be a nonempty string")

    geometry = data["geometry"]
    if not isinstance(geometry, dict):
        raise ModelError("`geometry` must be an object")
    _closed(
        geometry,
        {
            "coil_count",
            "major_radius_m",
            "coil_minor_radius_m",
            "plasma_minor_radius_m",
            "nonplanar_amplitude_m",
            "nonplanar_harmonic",
            "current_a",
            "fine_coil_segments",
            "fine_surface_toroidal",
            "fine_surface_poloidal",
        },
        "geometry",
    )
    geom = Geometry(
        coil_count=_integer(geometry, "coil_count"),
        major_radius_m=_number(geometry, "major_radius_m"),
        coil_minor_radius_m=_number(geometry, "coil_minor_radius_m"),
        plasma_minor_radius_m=_number(geometry, "plasma_minor_radius_m"),
        nonplanar_amplitude_m=_number(geometry, "nonplanar_amplitude_m"),
        nonplanar_harmonic=_integer(geometry, "nonplanar_harmonic"),
        current_a=_number(geometry, "current_a"),
        fine_coil_segments=_integer(geometry, "fine_coil_segments"),
        fine_surface_toroidal=_integer(geometry, "fine_surface_toroidal"),
        fine_surface_poloidal=_integer(geometry, "fine_surface_poloidal"),
    )
    if geom.coil_count < 6 or geom.coil_count > 16 or geom.coil_count % 2:
        raise ModelError("`coil_count` must be even and lie in [6, 16]")
    if geom.major_radius_m <= 0:
        raise ModelError("`major_radius_m` must be positive")
    if not (0 < geom.plasma_minor_radius_m < geom.coil_minor_radius_m):
        raise ModelError("minor radii must satisfy 0 < plasma < coil")
    if geom.major_radius_m <= geom.coil_minor_radius_m + abs(geom.nonplanar_amplitude_m):
        raise ModelError("coil geometry would intersect the cylindrical axis")
    if geom.nonplanar_amplitude_m <= 0 or geom.nonplanar_harmonic < 1:
        raise ModelError("the surrogate must contain a non-planar harmonic")
    if geom.current_a <= 0:
        raise ModelError("`current_a` must be positive")
    for name, value, low, high in (
        ("fine_coil_segments", geom.fine_coil_segments, 48, 256),
        ("fine_surface_toroidal", geom.fine_surface_toroidal, 12, 48),
        ("fine_surface_poloidal", geom.fine_surface_poloidal, 8, 32),
    ):
        if value < low or value > high or value % 2:
            raise ModelError(f"`{name}` must be even and lie in [{low}, {high}]")

    search_data = data["support_search"]
    if not isinstance(search_data, dict):
        raise ModelError("`support_search` must be an object")
    _closed(
        search_data,
        {
            "allocation_levels",
            "coupling_levels",
            "maximum_compliance_ratio",
            "maximum_condition_number",
        },
        "support_search",
    )
    try:
        allocations = tuple(float(value) for value in search_data["allocation_levels"])
        couplings = tuple(float(value) for value in search_data["coupling_levels"])
    except (KeyError, TypeError, ValueError) as error:
        raise ModelError("search levels must be arrays of decimal strings") from error
    if not allocations or any(value <= 0 for value in allocations):
        raise ModelError("allocation levels must be positive")
    if not couplings or 0.0 not in couplings or any(not 0 <= value < 1 for value in couplings):
        raise ModelError("coupling levels must contain zero and lie in [0, 1)")
    search = Search(
        allocation_levels=allocations,
        coupling_levels=couplings,
        maximum_compliance_ratio=_number(search_data, "maximum_compliance_ratio"),
        maximum_condition_number=_number(search_data, "maximum_condition_number"),
    )

    gate_data = data["feasibility_gate"]
    if not isinstance(gate_data, dict):
        raise ModelError("`feasibility_gate` must be an object")
    _closed(
        gate_data,
        {
            "minimum_exact_reduction",
            "maximum_nominal_error_ratio",
            "maximum_compliance_ratio",
            "maximum_linearization_error",
            "maximum_resolution_difference",
            "maximum_derivative_difference",
            "minimum_coupling_share",
        },
        "feasibility_gate",
    )
    gate = Gate(
        minimum_exact_reduction=_number(gate_data, "minimum_exact_reduction"),
        maximum_nominal_error_ratio=_number(gate_data, "maximum_nominal_error_ratio"),
        maximum_compliance_ratio=_number(gate_data, "maximum_compliance_ratio"),
        maximum_linearization_error=_number(gate_data, "maximum_linearization_error"),
        maximum_resolution_difference=_number(gate_data, "maximum_resolution_difference"),
        maximum_derivative_difference=_number(gate_data, "maximum_derivative_difference"),
        minimum_coupling_share=_number(gate_data, "minimum_coupling_share"),
    )
    derivative_step = _number(data, "derivative_step_m")
    operating_displacement = _number(data, "operating_displacement_m")
    if not 0 < derivative_step <= 1.0e-2:
        raise ModelError("`derivative_step_m` must lie in (0, 0.01]")
    if not 0 < operating_displacement <= 2.0e-2:
        raise ModelError("`operating_displacement_m` must lie in (0, 0.02]")
    if search.maximum_compliance_ratio <= 1 or search.maximum_condition_number <= 1:
        raise ModelError("support search bounds must exceed one")
    if min(
        gate.minimum_exact_reduction,
        gate.maximum_nominal_error_ratio,
        gate.maximum_compliance_ratio,
        gate.maximum_linearization_error,
        gate.maximum_resolution_difference,
        gate.maximum_derivative_difference,
        gate.minimum_coupling_share,
    ) < 0:
        raise ModelError("gate thresholds cannot be negative")

    return Model(
        model_id=data["model_id"],
        geometry=geom,
        derivative_step_m=derivative_step,
        operating_displacement_m=operating_displacement,
        search=search,
        gate=gate,
    )


def local_frames(count: int) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    phi = 2.0 * np.pi * np.arange(count) / count
    radial = np.column_stack((np.cos(phi), np.sin(phi), np.zeros(count)))
    toroidal = np.column_stack((-np.sin(phi), np.cos(phi), np.zeros(count)))
    vertical = np.tile(np.array((0.0, 0.0, 1.0)), (count, 1))
    return radial, toroidal, vertical


def make_coils(geometry: Geometry, segments: int) -> list[np.ndarray]:
    theta = 2.0 * np.pi * np.arange(segments) / segments
    radial, toroidal, vertical = local_frames(geometry.coil_count)
    coils: list[np.ndarray] = []
    for index in range(geometry.coil_count):
        # A rotated family of identical, closed, non-planar modular coils.
        # The second harmonic displaces the winding toroidally without making
        # the centerline discontinuous.
        local_radius = geometry.major_radius_m + geometry.coil_minor_radius_m * np.cos(theta)
        toroidal_offset = geometry.nonplanar_amplitude_m * np.sin(
            geometry.nonplanar_harmonic * theta
        )
        height = geometry.coil_minor_radius_m * np.sin(theta)
        points = (
            local_radius[:, None] * radial[index]
            + toroidal_offset[:, None] * toroidal[index]
            + height[:, None] * vertical[index]
        )
        coils.append(points)
    return coils


def make_surface(
    geometry: Geometry, toroidal_count: int, poloidal_count: int
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    phi = 2.0 * np.pi * np.arange(toroidal_count) / toroidal_count
    theta = 2.0 * np.pi * (np.arange(poloidal_count) + 0.5) / poloidal_count
    phi_grid, theta_grid = np.meshgrid(phi, theta, indexing="ij")
    radius = geometry.major_radius_m + geometry.plasma_minor_radius_m * np.cos(theta_grid)
    points = np.stack(
        (
            radius * np.cos(phi_grid),
            radius * np.sin(phi_grid),
            geometry.plasma_minor_radius_m * np.sin(theta_grid),
        ),
        axis=-1,
    ).reshape((-1, 3))
    normals = np.stack(
        (
            np.cos(theta_grid) * np.cos(phi_grid),
            np.cos(theta_grid) * np.sin(phi_grid),
            np.sin(theta_grid),
        ),
        axis=-1,
    ).reshape((-1, 3))
    weights = radius.reshape(-1)
    weights = weights / np.sum(weights)
    return points, normals, weights


def segment_geometry(coil: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    following = np.roll(coil, -1, axis=0)
    return 0.5 * (coil + following), following - coil


def field_from_coil(coil: np.ndarray, points: np.ndarray, current_a: float) -> np.ndarray:
    midpoints, line_elements = segment_geometry(coil)
    displacement = points[:, None, :] - midpoints[None, :, :]
    distance_squared = np.einsum("...i,...i->...", displacement, displacement)
    if np.any(distance_squared <= 1.0e-16):
        raise ModelError("a field point lies on a filament")
    kernel = np.cross(line_elements[None, :, :], displacement)
    kernel /= distance_squared[:, :, None] ** 1.5
    return MU0_OVER_4PI * current_a * np.sum(kernel, axis=1)


def total_field(coils: list[np.ndarray], points: np.ndarray, current_a: float) -> np.ndarray:
    field = np.zeros_like(points)
    for coil in coils:
        field += field_from_coil(coil, points, current_a)
    return field


def translated_coils(
    coils: list[np.ndarray], translations: np.ndarray, frames: tuple[np.ndarray, ...]
) -> list[np.ndarray]:
    radial, toroidal, vertical = frames
    translated: list[np.ndarray] = []
    for index, coil in enumerate(coils):
        shift = (
            translations[index, 0] * radial[index]
            + translations[index, 1] * toroidal[index]
            + translations[index, 2] * vertical[index]
        )
        translated.append(coil + shift)
    return translated


def sensitivity_matrix(
    coils: list[np.ndarray],
    points: np.ndarray,
    normals: np.ndarray,
    weights: np.ndarray,
    frames: tuple[np.ndarray, ...],
    current_a: float,
    step_m: float,
    field_scale_t: float,
) -> np.ndarray:
    count = len(coils)
    root_weights = np.sqrt(weights)
    sensitivity = np.empty((len(points), 3 * count))
    for coil_index in range(count):
        for direction_index, directions in enumerate(frames):
            direction = directions[coil_index]
            plus = field_from_coil(coils[coil_index] + step_m * direction, points, current_a)
            minus = field_from_coil(coils[coil_index] - step_m * direction, points, current_a)
            normal_derivative = np.einsum("ij,ij->i", plus - minus, normals) / (2.0 * step_m)
            sensitivity[:, 3 * coil_index + direction_index] = (
                root_weights * normal_derivative / field_scale_t
            )
    return sensitivity


def generalized_forces(
    coils: list[np.ndarray], frames: tuple[np.ndarray, ...], current_a: float
) -> np.ndarray:
    radial, toroidal, vertical = frames
    forces = np.empty((len(coils), 3))
    for index, coil in enumerate(coils):
        midpoints, line_elements = segment_geometry(coil)
        external_field = np.zeros_like(midpoints)
        for other_index, other in enumerate(coils):
            if other_index != index:
                external_field += field_from_coil(other, midpoints, current_a)
        net_force = current_a * np.sum(np.cross(line_elements, external_field), axis=0)
        forces[index] = (
            np.dot(net_force, radial[index]),
            np.dot(net_force, toroidal[index]),
            np.dot(net_force, vertical[index]),
        )
    return forces.reshape(-1)


def ring_laplacian(count: int) -> np.ndarray:
    matrix = 2.0 * np.eye(count)
    for index in range(count):
        matrix[index, (index - 1) % count] = -1.0
        matrix[index, (index + 1) % count] = -1.0
    return matrix


def support_matrix(
    count: int, allocations: tuple[float, float, float], couplings: tuple[float, float, float]
) -> np.ndarray:
    matrix = np.zeros((3 * count, 3 * count))
    laplacian = ring_laplacian(count)
    for direction in range(3):
        allocation = allocations[direction]
        coupling = couplings[direction]
        ground = allocation * (1.0 - coupling)
        neighbor = allocation * coupling / 2.0
        block = ground * np.eye(count) + neighbor * laplacian
        indices = np.arange(direction, 3 * count, 3)
        matrix[np.ix_(indices, indices)] = block
    return matrix


@dataclass(frozen=True)
class SupportResult:
    allocations: tuple[float, float, float]
    couplings: tuple[float, float, float]
    matrix: np.ndarray
    transfer_rms: float
    transfer_worst: float
    nominal_error: float
    compliance_ratio: float
    condition_number: float
    coupling_share: float


def score_support(
    matrix: np.ndarray,
    hessian: np.ndarray,
    nominal_force: np.ndarray,
    allocations: tuple[float, float, float],
    couplings: tuple[float, float, float],
) -> SupportResult:
    inverse = np.linalg.solve(matrix, np.eye(matrix.shape[0]))
    response_hessian = inverse.T @ hessian @ inverse
    mode_errors_squared = np.maximum(np.diag(response_hessian), 0.0)
    eigenvalues = np.linalg.eigvalsh(matrix)
    nominal_response = inverse @ nominal_force
    nominal_error = math.sqrt(max(float(nominal_response @ hessian @ nominal_response), 0.0))
    return SupportResult(
        allocations=allocations,
        couplings=couplings,
        matrix=matrix,
        transfer_rms=math.sqrt(float(np.mean(mode_errors_squared))),
        transfer_worst=math.sqrt(float(np.max(mode_errors_squared))),
        nominal_error=nominal_error,
        compliance_ratio=1.0 / float(eigenvalues[0]),
        condition_number=float(eigenvalues[-1] / eigenvalues[0]),
        coupling_share=sum(a * c for a, c in zip(allocations, couplings, strict=True)) / 3.0,
    )


def allocation_triples(levels: tuple[float, ...]) -> list[tuple[float, float, float]]:
    triples = []
    for first in levels:
        for second in levels:
            third = 3.0 - first - second
            if any(abs(third - level) <= 1.0e-12 for level in levels):
                triples.append((first, second, third))
    return sorted(set(triples))


def search_supports(
    hessian: np.ndarray, nominal_force: np.ndarray, count: int, search: Search
) -> tuple[SupportResult, SupportResult, int]:
    allocations = allocation_triples(search.allocation_levels)
    if not allocations:
        raise ModelError("allocation grid contains no triple summing to three")

    best_uncoupled: SupportResult | None = None
    best_coupled: SupportResult | None = None
    evaluated = 0
    for allocation in allocations:
        coupling_sets = [(0.0, 0.0, 0.0)]
        coupling_sets.extend(
            (first, second, third)
            for first in search.coupling_levels
            for second in search.coupling_levels
            for third in search.coupling_levels
            if max(first, second, third) > 0
        )
        for coupling in coupling_sets:
            matrix = support_matrix(count, allocation, coupling)
            result = score_support(matrix, hessian, nominal_force, allocation, coupling)
            evaluated += 1
            if (
                result.compliance_ratio > search.maximum_compliance_ratio + 1.0e-12
                or result.condition_number > search.maximum_condition_number + 1.0e-12
            ):
                continue
            key = (result.transfer_rms, result.transfer_worst, result.nominal_error, allocation, coupling)
            if max(coupling) == 0:
                if best_uncoupled is None or key < (
                    best_uncoupled.transfer_rms,
                    best_uncoupled.transfer_worst,
                    best_uncoupled.nominal_error,
                    best_uncoupled.allocations,
                    best_uncoupled.couplings,
                ):
                    best_uncoupled = result
            elif best_coupled is None or key < (
                best_coupled.transfer_rms,
                best_coupled.transfer_worst,
                best_coupled.nominal_error,
                best_coupled.allocations,
                best_coupled.couplings,
            ):
                best_coupled = result
    if best_uncoupled is None or best_coupled is None:
        raise ModelError("support search found no feasible baseline or coupled candidate")
    return best_uncoupled, best_coupled, evaluated


@dataclass(frozen=True)
class ResolutionResult:
    coils: list[np.ndarray]
    points: np.ndarray
    normals: np.ndarray
    weights: np.ndarray
    frames: tuple[np.ndarray, ...]
    base_field: np.ndarray
    field_scale_t: float
    sensitivity: np.ndarray
    derivative_difference: float
    nominal_force: np.ndarray
    force_symmetry_residual: float
    baseline: SupportResult
    candidate: SupportResult
    evaluated: int


def analyze_resolution(
    model: Model, coil_segments: int, surface_toroidal: int, surface_poloidal: int
) -> ResolutionResult:
    geometry = model.geometry
    coils = make_coils(geometry, coil_segments)
    points, normals, weights = make_surface(geometry, surface_toroidal, surface_poloidal)
    frames = local_frames(geometry.coil_count)
    base_field = total_field(coils, points, geometry.current_a)
    field_scale_t = math.sqrt(float(np.sum(weights * np.einsum("ij,ij->i", base_field, base_field))))
    if field_scale_t <= 0:
        raise ModelError("surrogate produces no reference magnetic field")
    coarse_derivative = sensitivity_matrix(
        coils,
        points,
        normals,
        weights,
        frames,
        geometry.current_a,
        model.derivative_step_m,
        field_scale_t,
    )
    sensitivity = sensitivity_matrix(
        coils,
        points,
        normals,
        weights,
        frames,
        geometry.current_a,
        model.derivative_step_m / 2.0,
        field_scale_t,
    )
    derivative_difference = float(
        np.linalg.norm(sensitivity - coarse_derivative) / max(np.linalg.norm(sensitivity), 1.0e-30)
    )
    nominal_force = generalized_forces(coils, frames, geometry.current_a)
    force_norm = float(np.linalg.norm(nominal_force))
    if force_norm <= 0:
        raise ModelError("surrogate produces no inter-coil force")
    nominal_force = nominal_force / force_norm
    force_by_coil = nominal_force.reshape((-1, 3))
    force_symmetry_residual = float(
        np.linalg.norm(force_by_coil - np.mean(force_by_coil, axis=0))
        / max(np.linalg.norm(force_by_coil), 1.0e-30)
    )
    hessian = sensitivity.T @ sensitivity
    baseline, candidate, evaluated = search_supports(
        hessian, nominal_force, geometry.coil_count, model.search
    )
    return ResolutionResult(
        coils=coils,
        points=points,
        normals=normals,
        weights=weights,
        frames=frames,
        base_field=base_field,
        field_scale_t=field_scale_t,
        sensitivity=sensitivity,
        derivative_difference=derivative_difference,
        nominal_force=nominal_force,
        force_symmetry_residual=force_symmetry_residual,
        baseline=baseline,
        candidate=candidate,
        evaluated=evaluated,
    )


def exact_incremental_error(
    result: ResolutionResult, geometry: Geometry, translations: np.ndarray
) -> float:
    moved = translated_coils(result.coils, translations, result.frames)
    changed_field = total_field(moved, result.points, geometry.current_a) - result.base_field
    normal_change = np.einsum("ij,ij->i", changed_field, result.normals)
    return math.sqrt(float(np.sum(result.weights * normal_change**2))) / result.field_scale_t


def nonlinear_metrics(
    result: ResolutionResult, model: Model
) -> tuple[float, float, float]:
    dimension = 3 * model.geometry.coil_count
    loads = model.operating_displacement_m * np.eye(dimension)
    linear_aggregate: list[float] = []
    exact_aggregate: list[float] = []
    exact_by_design: list[float] = []
    linear_by_design: list[float] = []
    for support in (result.baseline, result.candidate):
        responses = np.linalg.solve(support.matrix, loads)
        exact_errors = []
        linear_errors = []
        for column in range(dimension):
            translation = responses[:, column]
            linear_errors.append(float(np.linalg.norm(result.sensitivity @ translation)))
            exact_errors.append(
                exact_incremental_error(
                    result, model.geometry, translation.reshape((model.geometry.coil_count, 3))
                )
            )
        linear_rms = math.sqrt(float(np.mean(np.square(linear_errors))))
        exact_rms = math.sqrt(float(np.mean(np.square(exact_errors))))
        linear_aggregate.append(linear_rms)
        exact_aggregate.append(exact_rms)
        linear_by_design.extend(linear_errors)
        exact_by_design.extend(exact_errors)

    exact_reduction = exact_aggregate[0] / exact_aggregate[1]
    discrepancies = [
        abs(exact - linear) / max(exact, 1.0e-30)
        for exact, linear in zip(exact_by_design, linear_by_design, strict=True)
        if exact > 1.0e-12
    ]
    linearization_error = max(discrepancies, default=0.0)

    # Check the derived, symmetric inter-coil load separately from the
    # direction-neutral modal ensemble used to search the supports.
    isotropic_response = result.nominal_force
    isotropic_max = float(
        np.max(np.linalg.norm(isotropic_response.reshape((-1, 3)), axis=1))
    )
    load_scale = model.operating_displacement_m / isotropic_max
    nominal_errors = []
    for support in (result.baseline, result.candidate):
        response = np.linalg.solve(support.matrix, result.nominal_force * load_scale)
        nominal_errors.append(
            exact_incremental_error(
                result, model.geometry, response.reshape((model.geometry.coil_count, 3))
            )
        )
    nominal_error_ratio = nominal_errors[1] / max(nominal_errors[0], 1.0e-30)
    return exact_reduction, nominal_error_ratio, linearization_error


def run(model: Model, input_sha256: str) -> dict[str, Any]:
    geometry = model.geometry
    coarse = analyze_resolution(
        model,
        geometry.fine_coil_segments // 2,
        geometry.fine_surface_toroidal // 2,
        geometry.fine_surface_poloidal // 2,
    )
    fine = analyze_resolution(
        model,
        geometry.fine_coil_segments,
        geometry.fine_surface_toroidal,
        geometry.fine_surface_poloidal,
    )
    exact_reduction, nominal_error_ratio, linearization_error = nonlinear_metrics(fine, model)
    coarse_reduction = coarse.baseline.transfer_rms / coarse.candidate.transfer_rms
    fine_linear_reduction = fine.baseline.transfer_rms / fine.candidate.transfer_rms
    resolution_difference = abs(coarse_reduction - fine_linear_reduction) / fine_linear_reduction

    singular_values = np.linalg.svd(fine.sensitivity, compute_uv=False)
    positive = singular_values[singular_values > singular_values[0] * 1.0e-12]
    sensitivity_contrast = float(positive[0] / positive[-1]) if len(positive) else 1.0
    low_sensitivity_fraction = float(np.mean(singular_values <= singular_values[0] / 10.0))

    gate_checks = {
        "exact_reduction": exact_reduction >= model.gate.minimum_exact_reduction,
        "nominal_error_ratio": nominal_error_ratio <= model.gate.maximum_nominal_error_ratio,
        "compliance_ratio": fine.candidate.compliance_ratio <= model.gate.maximum_compliance_ratio,
        "linearization_error": linearization_error <= model.gate.maximum_linearization_error,
        "resolution_difference": resolution_difference <= model.gate.maximum_resolution_difference,
        "derivative_difference": fine.derivative_difference
        <= model.gate.maximum_derivative_difference,
        "coupling_share": fine.candidate.coupling_share >= model.gate.minimum_coupling_share,
    }
    interpretation = "proceed" if all(gate_checks.values()) else "stop"

    return {
        "schema": RESULT_SCHEMA,
        "model_id": model.model_id,
        "input_sha256": f"sha256:{input_sha256}",
        "method": {
            "field_model": "midpoint-filament-biot-savart",
            "deformation_model": "rigid-coil-local-translations",
            "support_model": "positive-definite-ground-plus-neighbor-ring-springs",
            "search_model": "closed-grid-equal-stiffness-trace",
            "numpy_version": np.__version__,
        },
        "metrics": {
            "exact_error_reduction_factor": _exact(exact_reduction),
            "linear_error_reduction_factor": _exact(fine_linear_reduction),
            "nominal_load_error_ratio": _exact(nominal_error_ratio),
            "worst_compliance_ratio": _exact(fine.candidate.compliance_ratio),
            "support_condition_number": _exact(fine.candidate.condition_number),
            "linearization_relative_error": _exact(linearization_error),
            "resolution_relative_difference": _exact(resolution_difference),
            "derivative_relative_difference": _exact(fine.derivative_difference),
            "sensitivity_singular_value_contrast": _exact(sensitivity_contrast),
            "low_sensitivity_dimension_fraction": _exact(low_sensitivity_fraction),
            "neighbor_coupling_stiffness_share": _exact(fine.candidate.coupling_share),
            "stiffness_trace_ratio": _exact(
                np.trace(fine.candidate.matrix) / np.trace(fine.baseline.matrix)
            ),
            "nominal_force_symmetry_residual": _exact(fine.force_symmetry_residual),
        },
        "selected_support": {
            "uncoupled_allocations": [_exact(value) for value in fine.baseline.allocations],
            "coupled_allocations": [_exact(value) for value in fine.candidate.allocations],
            "coupling_fractions": [_exact(value) for value in fine.candidate.couplings],
        },
        "search": {
            "coarse_candidates_evaluated": coarse.evaluated,
            "fine_candidates_evaluated": fine.evaluated,
        },
        "gate_checks": {key: "pass" if value else "fail" for key, value in gate_checks.items()},
        "interpretation": interpretation,
        "limitations": [
            "The analytic coil family is not an optimized or experimentally validated stellarator.",
            "Only rigid translations are represented; coil bending, casing strain, joints, preload, buckling, fatigue, and supports attached to a surrounding structure are absent.",
            "The equal-amplitude translation-load basis is direction-neutral and is not a distribution of reactor loads.",
            "Equal stiffness trace is only a resource proxy; it is not equal mass, cost, stress, or manufacturability.",
            "A positive result establishes a reduced-order mechanism worth testing on published geometry; it does not establish a practical support or patentable embodiment.",
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("model", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args(argv)
    try:
        raw = args.model.read_bytes()
        model = load_model(raw)
        result = run(model, hashlib.sha256(raw).hexdigest())
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
    except (OSError, ModelError, np.linalg.LinAlgError) as error:
        print(f"magnetic-compliance checker: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
