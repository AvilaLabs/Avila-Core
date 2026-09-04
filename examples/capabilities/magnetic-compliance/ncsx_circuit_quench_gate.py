#!/usr/bin/env python3
"""Evaluate one passive mode-selective dump-network candidate on NCSX geometry.

The checker is a deliberately reduced-order feasibility model.  It constructs
a three-loop regularized-Neumann inductance matrix, applies a prescribed
single-loop quench resistance, and integrates

    L dx/dt + (R_protection + R_quench(t)) x = 0.

The candidate does not select a winding partition or search internally.  It
only specifies the private dump resistance and the ratio between differential-
and common-mode resistance.  Core supplies that JSON document as a free input
and records candidate lineage outside this checker.

This is not a conductor-temperature, insulation-voltage, switching, plasma,
or qualified quench-protection model.
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
from typing import Any, Iterable

import numpy as np


MODEL_SCHEMA = "avila.magnetic-compliance/ncsx-circuit-quench-model/v1"
CANDIDATE_SCHEMA = "avila.magnetic-compliance/ncsx-circuit-quench-candidate/v1"
RESULT_SCHEMA = "avila.magnetic-compliance/ncsx-circuit-quench-result/v1"


class CircuitQuenchError(ValueError):
    """An input or calculation lies outside the declared feasibility model."""


def _closed(mapping: dict[str, Any], expected: set[str], where: str) -> None:
    observed = set(mapping)
    if observed != expected:
        raise CircuitQuenchError(
            f"{where} keys differ: missing={sorted(expected - observed)}, "
            f"extra={sorted(observed - expected)}"
        )


def _number(mapping: dict[str, Any], key: str) -> float:
    try:
        value = float(mapping[key])
    except (KeyError, TypeError, ValueError) as error:
        raise CircuitQuenchError(f"{key!r} must be a finite decimal string") from error
    if not math.isfinite(value):
        raise CircuitQuenchError(f"{key!r} must be finite")
    return value


def _integer(mapping: dict[str, Any], key: str) -> int:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise CircuitQuenchError(f"{key!r} must be an integer")
    return value


def _exact(value: float, places: int = 12) -> str:
    if not math.isfinite(value):
        raise CircuitQuenchError(f"non-finite result {value!r}")
    rendered = f"{value:.{places}f}".rstrip("0").rstrip(".")
    return rendered if rendered not in {"", "-0"} else "0"


def _sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def _load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise CircuitQuenchError(f"cannot load Python module {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


@dataclass(frozen=True)
class Gate:
    minimum_magnetic_reduction: float
    minimum_force_reduction: float
    maximum_i2t_ratio: float
    maximum_element_voltage_ratio: float
    maximum_final_energy_fraction: float
    maximum_resolution_relative_difference: float
    maximum_time_step_relative_difference: float
    maximum_inductance_condition_number: float
    maximum_fault_relative_spread: float


@dataclass(frozen=True)
class Model:
    model_id: str
    base_model_sha256: str
    groups: tuple[tuple[int, ...], ...]
    regularization_radius_m: float
    detection_time: float
    quench_rise_time: float
    quench_plateau: float
    end_time: float
    reference_time_step: float
    refined_time_step: float
    control_private_resistance: float
    control_differential_ratio: float
    gate: Gate


@dataclass(frozen=True)
class Candidate:
    candidate_id: str
    private_resistance: float
    differential_ratio: float


def _groups(value: Any) -> tuple[tuple[int, ...], ...]:
    if not isinstance(value, list) or len(value) != 3:
        raise CircuitQuenchError("architecture.groups must contain three circuits")
    groups: list[tuple[int, ...]] = []
    for raw in value:
        if not isinstance(raw, list) or any(
            isinstance(index, bool) or not isinstance(index, int) for index in raw
        ):
            raise CircuitQuenchError("every circuit group must be an integer array")
        groups.append(tuple(raw))
    normalized = tuple(groups)
    if any(len(group) != 6 for group in normalized):
        raise CircuitQuenchError("every circuit must contain six physical coils")
    if sorted(index for group in normalized for index in group) != list(range(18)):
        raise CircuitQuenchError("circuit groups must partition physical coils 0..17")
    for group in normalized:
        counts = [sum(index % 3 == kind for index in group) for kind in range(3)]
        if counts != [2, 2, 2]:
            raise CircuitQuenchError("every circuit must contain two coils of each type")
    return normalized


def load_model(raw: bytes) -> Model:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CircuitQuenchError(f"circuit-quench model is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise CircuitQuenchError("circuit-quench model root must be an object")
    _closed(
        data,
        {
            "schema",
            "model_id",
            "base_model_sha256",
            "architecture",
            "transient",
            "control",
            "gate",
        },
        "model",
    )
    if data["schema"] != MODEL_SCHEMA:
        raise CircuitQuenchError(f"unsupported model schema {data['schema']!r}")
    if not isinstance(data["model_id"], str) or not data["model_id"]:
        raise CircuitQuenchError("model_id must be a nonempty string")
    base_digest = data["base_model_sha256"]
    if (
        not isinstance(base_digest, str)
        or not base_digest.startswith("sha256:")
        or len(base_digest) != 71
    ):
        raise CircuitQuenchError("base_model_sha256 must be a sha256 identity")

    architecture = data["architecture"]
    if not isinstance(architecture, dict):
        raise CircuitQuenchError("architecture must be an object")
    _closed(
        architecture,
        {
            "groups",
            "inductance_method",
            "regularization_radius_m",
            "inductance_normalization",
            "resistor_network",
        },
        "architecture",
    )
    if architecture["inductance_method"] != "regularized-neumann-midpoint":
        raise CircuitQuenchError("unsupported inductance method")
    if architecture["inductance_normalization"] != "mean-circuit-self-inductance":
        raise CircuitQuenchError("unsupported inductance normalization")
    if architecture["resistor_network"] != (
        "three-private-loop-resistors-plus-equal-pairwise-shared-resistors"
    ):
        raise CircuitQuenchError("unsupported resistor network")

    transient = data["transient"]
    if not isinstance(transient, dict):
        raise CircuitQuenchError("transient must be an object")
    _closed(
        transient,
        {
            "initial_current_fraction",
            "detection_time",
            "quench_resistance_rise_time",
            "quench_resistance_plateau",
            "end_time",
            "reference_time_step",
            "refined_time_step",
        },
        "transient",
    )
    if _number(transient, "initial_current_fraction") != 1.0:
        raise CircuitQuenchError("initial_current_fraction must equal one")

    control = data["control"]
    if not isinstance(control, dict):
        raise CircuitQuenchError("control must be an object")
    _closed(
        control,
        {"private_resistance_scale", "differential_to_common_mode_ratio"},
        "control",
    )

    gate_data = data["gate"]
    if not isinstance(gate_data, dict):
        raise CircuitQuenchError("gate must be an object")
    gate_keys = {
        "minimum_magnetic_reduction",
        "minimum_force_reduction",
        "maximum_i2t_ratio",
        "maximum_element_voltage_ratio",
        "maximum_final_energy_fraction",
        "maximum_resolution_relative_difference",
        "maximum_time_step_relative_difference",
        "maximum_inductance_condition_number",
        "maximum_fault_relative_spread",
    }
    _closed(gate_data, gate_keys, "gate")
    gate = Gate(**{key: _number(gate_data, key) for key in gate_keys})

    model = Model(
        model_id=data["model_id"],
        base_model_sha256=base_digest,
        groups=_groups(architecture["groups"]),
        regularization_radius_m=_number(architecture, "regularization_radius_m"),
        detection_time=_number(transient, "detection_time"),
        quench_rise_time=_number(transient, "quench_resistance_rise_time"),
        quench_plateau=_number(transient, "quench_resistance_plateau"),
        end_time=_number(transient, "end_time"),
        reference_time_step=_number(transient, "reference_time_step"),
        refined_time_step=_number(transient, "refined_time_step"),
        control_private_resistance=_number(control, "private_resistance_scale"),
        control_differential_ratio=_number(
            control, "differential_to_common_mode_ratio"
        ),
        gate=gate,
    )
    positive = (
        model.regularization_radius_m,
        model.detection_time,
        model.quench_rise_time,
        model.quench_plateau,
        model.end_time,
        model.reference_time_step,
        model.refined_time_step,
        model.control_private_resistance,
        model.control_differential_ratio,
        *gate.__dict__.values(),
    )
    if min(positive) <= 0:
        raise CircuitQuenchError("all model scales and gates must be positive")
    if model.control_differential_ratio != 1.0:
        raise CircuitQuenchError("the independent-dump control must have ratio one")
    if not model.detection_time < model.quench_rise_time < model.end_time:
        raise CircuitQuenchError("transient event times are not ordered")
    if not model.refined_time_step < model.reference_time_step:
        raise CircuitQuenchError("refined_time_step must be smaller")
    for interval in (
        model.detection_time,
        model.quench_rise_time,
        model.end_time,
    ):
        for step in (model.reference_time_step, model.refined_time_step):
            if abs(interval / step - round(interval / step)) > 1.0e-9:
                raise CircuitQuenchError("event times must align with both time grids")
    if gate.minimum_magnetic_reduction <= 1 or gate.minimum_force_reduction <= 1:
        raise CircuitQuenchError("shape gates must require improvement")
    if gate.maximum_i2t_ratio < 1 or gate.maximum_element_voltage_ratio < 1:
        raise CircuitQuenchError("burden gates must permit the fixed control")
    return model


def load_candidate(raw: bytes) -> Candidate:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CircuitQuenchError(f"candidate is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise CircuitQuenchError("candidate root must be an object")
    _closed(data, {"schema", "candidate_id", "network"}, "candidate")
    if data["schema"] != CANDIDATE_SCHEMA:
        raise CircuitQuenchError(f"unsupported candidate schema {data['schema']!r}")
    if not isinstance(data["candidate_id"], str) or not data["candidate_id"]:
        raise CircuitQuenchError("candidate_id must be a nonempty string")
    network = data["network"]
    if not isinstance(network, dict):
        raise CircuitQuenchError("candidate.network must be an object")
    _closed(
        network,
        {"private_resistance_scale", "differential_to_common_mode_ratio"},
        "candidate.network",
    )
    candidate = Candidate(
        candidate_id=data["candidate_id"],
        private_resistance=_number(network, "private_resistance_scale"),
        differential_ratio=_number(
            network, "differential_to_common_mode_ratio"
        ),
    )
    if not 0.25 <= candidate.private_resistance <= 2.0:
        raise CircuitQuenchError("private resistance must lie in [0.25, 2]")
    if not 1.0 <= candidate.differential_ratio <= 4.0:
        raise CircuitQuenchError("differential/common ratio must lie in [1, 4]")
    return candidate


def resistor_network(private: float, differential_ratio: float) -> tuple[np.ndarray, float]:
    """Return the passive three-loop mesh matrix and each shared resistance.

    Three private resistors of value p and three identical branches shared by
    loop pairs produce R = p I + b Laplacian(K3).  Its common eigenvalue is p,
    its two differential eigenvalues are p + 3 b, so b=p(rho-1)/3.
    """

    if private <= 0 or differential_ratio < 1:
        raise CircuitQuenchError("network requires positive private and nonnegative shared resistance")
    shared = private * (differential_ratio - 1.0) / 3.0
    laplacian = np.full((3, 3), -1.0)
    np.fill_diagonal(laplacian, 2.0)
    matrix = private * np.eye(3) + shared * laplacian
    return matrix, shared


def normalized_inductance(
    coils: list[np.ndarray],
    currents: np.ndarray,
    groups: tuple[tuple[int, ...], ...],
    kernel: Any,
    radius_m: float,
) -> np.ndarray:
    """Build a softened-filament Neumann matrix and normalize its trace."""

    if len(coils) != 18 or currents.shape != (18,):
        raise CircuitQuenchError("inductance model requires 18 physical coils")
    midpoints: list[np.ndarray] = []
    line_elements: list[np.ndarray] = []
    for coil in coils:
        midpoint, element = kernel.segment_geometry(coil)
        midpoints.append(midpoint)
        line_elements.append(element)
    single_turn = np.zeros((18, 18))
    for first in range(18):
        for second in range(first, 18):
            delta = midpoints[first][:, None, :] - midpoints[second][None, :, :]
            distance = np.sqrt(
                np.einsum("ijk,ijk->ij", delta, delta) + radius_m**2
            )
            value = 1.0e-7 * np.sum(
                (line_elements[first] @ line_elements[second].T) / distance
            )
            single_turn[first, second] = value
            single_turn[second, first] = value
    ampere_turn_scale = float(np.mean(np.abs(currents)))
    if ampere_turn_scale <= 0:
        raise CircuitQuenchError("coil currents have no positive scale")
    winding = np.zeros((18, 3))
    for circuit, group in enumerate(groups):
        winding[list(group), circuit] = currents[list(group)] / ampere_turn_scale
    circuit_matrix = winding.T @ single_turn @ winding
    circuit_matrix = 0.5 * (circuit_matrix + circuit_matrix.T)
    mean_self = float(np.trace(circuit_matrix) / 3.0)
    if mean_self <= 0:
        raise CircuitQuenchError("regularized circuit inductance has no positive scale")
    circuit_matrix /= mean_self
    eigenvalues = np.linalg.eigvalsh(circuit_matrix)
    if eigenvalues[0] <= max(eigenvalues[-1] * 1.0e-10, 0.0):
        raise CircuitQuenchError("regularized circuit inductance is not positive definite")
    return circuit_matrix


def metric_grams(
    analysis: Any, groups: tuple[tuple[int, ...], ...]
) -> tuple[np.ndarray, np.ndarray]:
    """Compress field and force arrays into exact quadratic metric forms."""

    circuit_fields = np.stack(
        [
            np.sum(analysis.weighted_normal_fields[list(group)], axis=0)
            for group in groups
        ]
    )
    magnetic = circuit_fields @ circuit_fields.T
    force_terms: list[np.ndarray] = []
    for receiver_group in groups:
        receivers = np.zeros(18, dtype=bool)
        receivers[list(receiver_group)] = True
        for source_group in groups:
            contribution = np.zeros_like(analysis.nominal_force_density)
            contribution[receivers] = np.sum(
                analysis.pair_force_density[receivers][:, list(source_group)], axis=1
            )
            weighted = (
                contribution
                * np.sqrt(analysis.force_weights)[:, :, None]
                / analysis.nominal_force_rms
            )
            force_terms.append(weighted.reshape(-1))
    stacked = np.stack(force_terms)
    return magnetic, stacked @ stacked.T


@dataclass(frozen=True)
class Transient:
    times: np.ndarray
    currents: np.ndarray
    i2t: float
    peak_element_voltage: float
    final_energy_fraction: float
    minimum_current: float


def _rk4_segment(
    inverse_inductance: np.ndarray,
    start: float,
    end: float,
    step: float,
    initial: np.ndarray,
    fault: int,
    protection: np.ndarray,
    protection_active: bool,
    model: Model,
) -> tuple[np.ndarray, np.ndarray]:
    count = int(round((end - start) / step))
    times = start + step * np.arange(count + 1)
    values = np.empty((count + 1, 3))
    values[0] = initial
    fault_projector = np.zeros((3, 3))
    fault_projector[fault, fault] = 1.0

    def right_hand_side(time: float, state: np.ndarray) -> np.ndarray:
        quench = model.quench_plateau * min(
            max(time / model.quench_rise_time, 0.0), 1.0
        )
        resistance = quench * fault_projector
        if protection_active:
            resistance = resistance + protection
        return -inverse_inductance @ (resistance @ state)

    for index, time in enumerate(times[:-1]):
        state = values[index]
        k1 = right_hand_side(float(time), state)
        k2 = right_hand_side(float(time + step / 2.0), state + step * k1 / 2.0)
        k3 = right_hand_side(float(time + step / 2.0), state + step * k2 / 2.0)
        k4 = right_hand_side(float(time + step), state + step * k3)
        values[index + 1] = state + step * (k1 + 2 * k2 + 2 * k3 + k4) / 6.0
    return times, values


def simulate(
    inductance: np.ndarray,
    private: float,
    differential_ratio: float,
    fault: int,
    step: float,
    model: Model,
) -> Transient:
    if fault not in range(3):
        raise CircuitQuenchError("fault index must lie in 0..2")
    protection, shared = resistor_network(private, differential_ratio)
    inverse = np.linalg.inv(inductance)
    before_times, before = _rk4_segment(
        inverse,
        0.0,
        model.detection_time,
        step,
        np.ones(3),
        fault,
        protection,
        False,
        model,
    )
    after_times, after = _rk4_segment(
        inverse,
        model.detection_time,
        model.end_time,
        step,
        before[-1],
        fault,
        protection,
        True,
        model,
    )
    times = np.concatenate((before_times, after_times[1:]))
    currents = np.concatenate((before, after[1:]))
    i2t = float(np.trapezoid(currents[:, fault] ** 2, times))
    peak_voltage = 0.0
    for time, state in zip(times, currents, strict=True):
        quench = model.quench_plateau * min(
            max(float(time) / model.quench_rise_time, 0.0), 1.0
        )
        element_voltages = [quench * abs(float(state[fault]))]
        if time + 1.0e-14 >= model.detection_time:
            element_voltages.extend(private * np.abs(state))
            element_voltages.extend(
                shared * abs(float(state[first] - state[second]))
                for first in range(3)
                for second in range(first + 1, 3)
            )
        peak_voltage = max(peak_voltage, *element_voltages)
    initial_energy = float(np.ones(3) @ inductance @ np.ones(3))
    final_energy = float(currents[-1] @ inductance @ currents[-1])
    if initial_energy <= 0:
        raise CircuitQuenchError("initial magnetic energy is not positive")
    return Transient(
        times=times,
        currents=currents,
        i2t=i2t,
        peak_element_voltage=peak_voltage,
        final_energy_fraction=final_energy / initial_energy,
        minimum_current=float(np.min(currents)),
    )


@dataclass(frozen=True)
class FaultMetric:
    fault: int
    peak_magnetic_departure: float
    peak_force_departure: float
    i2t: float
    peak_element_voltage: float
    final_energy_fraction: float
    minimum_current: float


def fault_metric(
    transient: Transient,
    magnetic_gram: np.ndarray,
    force_gram: np.ndarray,
    fault: int,
) -> FaultMetric:
    currents = transient.currents
    common = np.mean(currents, axis=1)
    differential = currents - common[:, None]
    magnetic_sq = np.einsum(
        "ti,ij,tj->t", differential, magnetic_gram, differential
    )
    products = np.einsum("ti,tj->tij", currents, currents).reshape((-1, 9))
    force_coefficients = products - common[:, None] ** 2
    force_sq = np.einsum(
        "ti,ij,tj->t", force_coefficients, force_gram, force_coefficients
    )
    return FaultMetric(
        fault=fault,
        peak_magnetic_departure=math.sqrt(float(np.max(np.maximum(magnetic_sq, 0.0)))),
        peak_force_departure=math.sqrt(float(np.max(np.maximum(force_sq, 0.0)))),
        i2t=transient.i2t,
        peak_element_voltage=transient.peak_element_voltage,
        final_energy_fraction=transient.final_energy_fraction,
        minimum_current=transient.minimum_current,
    )


@dataclass(frozen=True)
class ResolutionMetric:
    magnetic_reduction: float
    force_reduction: float
    i2t_ratio: float
    element_voltage_ratio: float
    final_energy_fraction: float
    fault_relative_spread: float
    control: tuple[FaultMetric, ...]
    candidate: tuple[FaultMetric, ...]


def _relative_spread(values: Iterable[float]) -> float:
    sequence = tuple(values)
    mean = float(np.mean(sequence))
    if mean <= 0:
        raise CircuitQuenchError("relative spread requires positive values")
    return (max(sequence) - min(sequence)) / mean


def evaluate_resolution(
    inductance: np.ndarray,
    magnetic_gram: np.ndarray,
    force_gram: np.ndarray,
    candidate: Candidate,
    step: float,
    model: Model,
) -> ResolutionMetric:
    def run(private: float, ratio: float) -> tuple[FaultMetric, ...]:
        return tuple(
            fault_metric(
                simulate(inductance, private, ratio, fault, step, model),
                magnetic_gram,
                force_gram,
                fault,
            )
            for fault in range(3)
        )

    control = run(
        model.control_private_resistance, model.control_differential_ratio
    )
    proposed = run(candidate.private_resistance, candidate.differential_ratio)
    control_magnetic = max(item.peak_magnetic_departure for item in control)
    control_force = max(item.peak_force_departure for item in control)
    proposed_magnetic = max(item.peak_magnetic_departure for item in proposed)
    proposed_force = max(item.peak_force_departure for item in proposed)
    if min(control_magnetic, control_force, proposed_magnetic, proposed_force) <= 0:
        raise CircuitQuenchError("a transient shape metric is degenerate")
    spreads = []
    for records in (control, proposed):
        for field in (
            "peak_magnetic_departure",
            "peak_force_departure",
            "i2t",
            "peak_element_voltage",
        ):
            spreads.append(_relative_spread(getattr(item, field) for item in records))
    return ResolutionMetric(
        magnetic_reduction=control_magnetic / proposed_magnetic,
        force_reduction=control_force / proposed_force,
        i2t_ratio=max(item.i2t for item in proposed)
        / max(item.i2t for item in control),
        element_voltage_ratio=max(item.peak_element_voltage for item in proposed)
        / max(item.peak_element_voltage for item in control),
        final_energy_fraction=max(item.final_energy_fraction for item in proposed),
        fault_relative_spread=max(spreads),
        control=control,
        candidate=proposed,
    )


def _relative_difference(first: float, second: float) -> float:
    scale = max(abs(first), abs(second))
    if scale <= 0:
        return 0.0
    return abs(first - second) / scale


def _metric_difference(first: ResolutionMetric, second: ResolutionMetric) -> float:
    return max(
        _relative_difference(getattr(first, field), getattr(second, field))
        for field in (
            "magnetic_reduction",
            "force_reduction",
            "i2t_ratio",
            "element_voltage_ratio",
        )
    )


def _fault_json(metric: FaultMetric) -> dict[str, Any]:
    return {
        "fault_circuit": metric.fault,
        "peak_magnetic_shape_departure": _exact(metric.peak_magnetic_departure),
        "peak_force_shape_departure": _exact(metric.peak_force_departure),
        "normalized_i2t": _exact(metric.i2t),
        "peak_normalized_resistive_element_voltage": _exact(
            metric.peak_element_voltage
        ),
        "final_energy_fraction": _exact(metric.final_energy_fraction),
        "minimum_current_fraction": _exact(metric.minimum_current),
    }


def evaluate(
    kernel_path: Path,
    ceiling_path: Path,
    fault_library_path: Path,
    coil_path: Path,
    boundary_path: Path,
    base_model_path: Path,
    circuit_model_path: Path,
    candidate_path: Path,
) -> dict[str, Any]:
    model = load_model(circuit_model_path.read_bytes())
    candidate = load_candidate(candidate_path.read_bytes())
    if _sha256(base_model_path) != model.base_model_sha256:
        raise CircuitQuenchError("base model digest does not match circuit model")

    kernel = _load_module("ncsx_circuit_field_kernel", kernel_path)
    ceiling = _load_module("ncsx_circuit_ceiling", ceiling_path)
    fault_library = _load_module("ncsx_circuit_fault_library", fault_library_path)
    base_model = ceiling.load_model(base_model_path.read_bytes())
    boundary = ceiling.boundary_coefficients(boundary_path, base_model)

    analyses: list[Any] = []
    inductances: list[np.ndarray] = []
    grams: list[tuple[np.ndarray, np.ndarray]] = []
    for resolution in (base_model.reference, base_model.fine):
        analysis = fault_library.analyze(
            ceiling, kernel, coil_path, boundary, base_model, resolution
        )
        analyses.append(analysis)
        inductances.append(
            normalized_inductance(
                fault_library_ceiling_coils(
                    ceiling, coil_path, base_model, resolution.coil_segments
                ),
                analysis.currents,
                model.groups,
                kernel,
                model.regularization_radius_m,
            )
        )
        grams.append(metric_grams(analysis, model.groups))

    reference = evaluate_resolution(
        inductances[0],
        *grams[0],
        candidate,
        model.reference_time_step,
        model,
    )
    fine = evaluate_resolution(
        inductances[1],
        *grams[1],
        candidate,
        model.reference_time_step,
        model,
    )
    refined = evaluate_resolution(
        inductances[1],
        *grams[1],
        candidate,
        model.refined_time_step,
        model,
    )
    resolution_difference = _metric_difference(reference, fine)
    time_step_difference = _metric_difference(fine, refined)
    inductance_condition = float(np.linalg.cond(inductances[1]))
    inductance_difference = float(
        np.linalg.norm(inductances[1] - inductances[0])
        / np.linalg.norm(inductances[1])
    )
    _, shared = resistor_network(
        candidate.private_resistance, candidate.differential_ratio
    )
    gate = model.gate
    passed = (
        fine.magnetic_reduction + 1.0e-12 >= gate.minimum_magnetic_reduction
        and fine.force_reduction + 1.0e-12 >= gate.minimum_force_reduction
        and fine.i2t_ratio <= gate.maximum_i2t_ratio + 1.0e-12
        and fine.element_voltage_ratio
        <= gate.maximum_element_voltage_ratio + 1.0e-12
        and fine.final_energy_fraction
        <= gate.maximum_final_energy_fraction + 1.0e-12
        and resolution_difference
        <= gate.maximum_resolution_relative_difference + 1.0e-12
        and time_step_difference
        <= gate.maximum_time_step_relative_difference + 1.0e-12
        and inductance_condition
        <= gate.maximum_inductance_condition_number + 1.0e-12
        and fine.fault_relative_spread
        <= gate.maximum_fault_relative_spread + 1.0e-12
    )

    return {
        "schema": RESULT_SCHEMA,
        "model_id": model.model_id,
        "candidate_id": candidate.candidate_id,
        "source_identity": {
            "coil_sha256": _sha256(coil_path),
            "boundary_sha256": _sha256(boundary_path),
            "base_model_sha256": _sha256(base_model_path),
            "circuit_model_sha256": _sha256(circuit_model_path),
            "candidate_sha256": _sha256(candidate_path),
            "field_kernel_sha256": _sha256(kernel_path),
            "ceiling_library_sha256": _sha256(ceiling_path),
            "fault_library_sha256": _sha256(fault_library_path),
        },
        "method": {
            "circuit_equation": "L-dx-dt-plus-R-protection-plus-R-quench-times-x-equals-zero",
            "inductance": "regularized-neumann-midpoint-normalized-by-mean-self",
            "field_and_force": "case-007-common-mode-removed-shape-metrics",
            "integration": "fixed-step-rk4-with-detection-event-split",
            "numpy_version": np.__version__,
            "search_inside_checker": False,
        },
        "network": {
            "private_resistance_scale": _exact(candidate.private_resistance),
            "shared_pair_resistance_scale": _exact(shared),
            "common_mode_resistance_scale": _exact(candidate.private_resistance),
            "differential_mode_resistance_scale": _exact(
                candidate.private_resistance * candidate.differential_ratio
            ),
            "differential_to_common_mode_ratio": _exact(
                candidate.differential_ratio
            ),
        },
        "inductance": {
            "reference_normalized_matrix": [
                [_exact(value) for value in row] for row in inductances[0]
            ],
            "fine_normalized_matrix": [
                [_exact(value) for value in row] for row in inductances[1]
            ],
            "fine_eigenvalues": [
                _exact(value) for value in np.linalg.eigvalsh(inductances[1])
            ],
            "fine_condition_number": _exact(inductance_condition),
            "matrix_resolution_relative_difference": _exact(inductance_difference),
        },
        "metrics": {
            "fine_magnetic_reduction_factor": _exact(fine.magnetic_reduction),
            "fine_force_reduction_factor": _exact(fine.force_reduction),
            "fine_i2t_ratio": _exact(fine.i2t_ratio),
            "fine_peak_element_voltage_ratio": _exact(
                fine.element_voltage_ratio
            ),
            "fine_final_energy_fraction": _exact(fine.final_energy_fraction),
            "resolution_relative_difference": _exact(resolution_difference),
            "time_step_relative_difference": _exact(time_step_difference),
            "inductance_condition_number": _exact(inductance_condition),
            "fault_relative_spread": _exact(fine.fault_relative_spread),
        },
        "reference": {
            "control": [_fault_json(item) for item in reference.control],
            "candidate": [_fault_json(item) for item in reference.candidate],
        },
        "fine": {
            "control": [_fault_json(item) for item in fine.control],
            "candidate": [_fault_json(item) for item in fine.candidate],
        },
        "network_validity": "valid",
        "interpretation": "candidate-justified" if passed else "revise",
        "limitations": [
            "The regularized filament inductance is normalized and does not establish winding-pack self-inductance, stored energy, or dimensional resistance.",
            "The prescribed quench-resistance ramp is not a conductor thermal or propagation calculation; normalized I-squared-t is only a comparative burden proxy.",
            "Reported voltage is the largest normalized drop across modeled resistive elements, not terminal-to-ground, turn-to-turn, insulation, switch, or power-supply voltage.",
            "The passive mesh matrix is a circuit-theory realization of private and pair-shared resistors; no switch, diode, grounding, failure, or installation topology is designed.",
            "Magnetic and filament force-shape metrics remove the common current mode and do not establish plasma survival or structural stress.",
            "Thresholds and candidate settings were selected during interactive feasibility exploration; this is not preregistered confirmation, safety evidence, or a patentability conclusion.",
        ],
    }


def fault_library_ceiling_coils(
    ceiling: Any, coil_path: Path, base_model: Any, segments: int
) -> list[np.ndarray]:
    coils, _ = ceiling.physical_coils(coil_path, base_model, segments)
    return coils


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("field_kernel", type=Path)
    parser.add_argument("ceiling_library", type=Path)
    parser.add_argument("fault_library", type=Path)
    parser.add_argument("coil_data", type=Path)
    parser.add_argument("boundary_data", type=Path)
    parser.add_argument("base_model", type=Path)
    parser.add_argument("circuit_model", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    try:
        result = evaluate(
            arguments.field_kernel,
            arguments.ceiling_library,
            arguments.fault_library,
            arguments.coil_data,
            arguments.boundary_data,
            arguments.base_model,
            arguments.circuit_model,
            arguments.candidate,
        )
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    except (OSError, CircuitQuenchError, ValueError, np.linalg.LinAlgError) as error:
        print(f"circuit-quench gate failed: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
