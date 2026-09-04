#!/usr/bin/env python3
"""Falsify a passive differential dump on a dimensional NCSX copper model.

This checker deliberately does not model a superconducting quench. Historical
NCSX modular coils were LN2-precooled copper windings. The electrical and
thermal scales here come from the coherent m45r00 conceptual-design table; the
material curve is an explicit NIST-OFHC modeling assumption because that table
does not establish the copper grade. Separately identified SIMSOPT NCSX coils
and a c09r00-named boundary are used only for field, force, and cross-circuit-
mutual surrogates; their revision mismatch is computed.

One converter loses voltage at fault onset. Healthy converters hold their
pre-fault loss-compensation voltages until a declared detection delay, when all
converters are bypassed and a passive resistor mesh is inserted. The checker
compares one supplied differential/common resistance ratio with the same
private-resistor network without shared branches. It searches no candidate.
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


MODEL_SCHEMA = "avila.magnetic-compliance/ncsx-copper-fault-model/v1"
CANDIDATE_SCHEMA = "avila.magnetic-compliance/ncsx-copper-fault-candidate/v1"
RESULT_SCHEMA = "avila.magnetic-compliance/ncsx-copper-fault-result/v1"


class CopperFaultError(ValueError):
    """An input or calculation is outside the declared falsification model."""


def _closed(mapping: dict[str, Any], expected: set[str], where: str) -> None:
    observed = set(mapping)
    if observed != expected:
        raise CopperFaultError(
            f"{where} keys differ: missing={sorted(expected - observed)}, "
            f"extra={sorted(observed - expected)}"
        )


def _number(mapping: dict[str, Any], key: str) -> float:
    try:
        value = float(mapping[key])
    except (KeyError, TypeError, ValueError) as error:
        raise CopperFaultError(f"{key!r} must be a finite decimal string") from error
    if not math.isfinite(value):
        raise CopperFaultError(f"{key!r} must be finite")
    return value


def _numbers(value: Any, count: int, where: str) -> np.ndarray:
    if not isinstance(value, list) or len(value) != count:
        raise CopperFaultError(f"{where} must contain {count} decimal strings")
    try:
        result = np.asarray([float(item) for item in value], dtype=float)
    except (TypeError, ValueError) as error:
        raise CopperFaultError(f"{where} must contain decimal strings") from error
    if not np.all(np.isfinite(result)):
        raise CopperFaultError(f"{where} contains a non-finite value")
    return result


def _integer(mapping: dict[str, Any], key: str) -> int:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise CopperFaultError(f"{key!r} must be an integer")
    return value


def _exact(value: float, places: int = 12) -> str:
    if not math.isfinite(value):
        raise CopperFaultError(f"non-finite result {value!r}")
    rendered = f"{value:.{places}f}".rstrip("0").rstrip(".")
    return rendered if rendered not in {"", "-0"} else "0"


def _sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def _load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise CopperFaultError(f"cannot load Python module {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


@dataclass(frozen=True)
class Gate:
    minimum_magnetic_reduction: float
    minimum_force_reduction: float
    maximum_total_i2t_a2s: float
    maximum_temperature_k: float
    maximum_absolute_current_a: float
    minimum_current_a: float
    maximum_modeled_component_voltage_v: float
    maximum_final_energy_fraction: float
    maximum_energy_balance_relative_error: float
    maximum_time_step_relative_difference: float
    maximum_shape_resolution_relative_difference: float
    maximum_inductance_resolution_relative_difference: float
    maximum_inductance_condition_number: float
    maximum_geometry_length_mismatch: float
    maximum_thermal_validation_energy_error: float


@dataclass(frozen=True)
class Material:
    heat_capacity_coefficients: np.ndarray
    heat_capacity_fit_error: float
    resistivity_coefficients: np.ndarray
    resistivity_sensitivity_scales: tuple[float, ...]
    resistivity_sensitivity_basis: str

    @property
    def heat_capacity_sensitivity_scales(self) -> tuple[float, float, float]:
        return (1.0 - self.heat_capacity_fit_error, 1.0, 1.0 + self.heat_capacity_fit_error)


@dataclass(frozen=True)
class Model:
    model_id: str
    base_model_sha256: str
    source_scope: dict[str, str]
    names: tuple[str, ...]
    coils_per_circuit: int
    turns_per_coil: int
    centerline_lengths: np.ndarray
    helical_length_factor: float
    copper_area_m2: float
    copper_density: float
    resistivity_85k: float
    resistance_85k: np.ndarray
    line_resistance: float
    self_inductance: np.ndarray
    line_inductance: float
    initial_current: np.ndarray
    initial_temperature: np.ndarray
    prehistory_times: np.ndarray
    prehistory_currents: np.ndarray
    validation_temperature: np.ndarray
    validation_energy: np.ndarray
    material: Material
    detection_delays: tuple[float, ...]
    nominal_detection_delay: float
    end_time: float
    reference_time_step: float
    refined_time_step: float
    geometry_confirmation_multiplier: int
    private_dump: float
    control_ratio: float
    gate: Gate

    @property
    def copper_mass(self) -> np.ndarray:
        return (
            self.coils_per_circuit
            * self.turns_per_coil
            * self.centerline_lengths
            * self.helical_length_factor
            * self.copper_area_m2
            * self.copper_density
        )


@dataclass(frozen=True)
class Candidate:
    candidate_id: str
    differential_ratio: float


def load_model(raw: bytes) -> Model:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CopperFaultError(f"copper-fault model is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise CopperFaultError("copper-fault model root must be an object")
    _closed(
        data,
        {
            "schema",
            "model_id",
            "base_model_sha256",
            "source_scope",
            "circuits",
            "material",
            "transient",
            "control",
            "gate",
        },
        "model",
    )
    if data["schema"] != MODEL_SCHEMA:
        raise CopperFaultError(f"unsupported model schema {data['schema']!r}")
    if not isinstance(data["model_id"], str) or not data["model_id"]:
        raise CopperFaultError("model_id must be a nonempty string")
    digest = data["base_model_sha256"]
    if not isinstance(digest, str) or not digest.startswith("sha256:") or len(digest) != 71:
        raise CopperFaultError("base_model_sha256 must be a sha256 identity")

    sources = data["source_scope"]
    if not isinstance(sources, dict):
        raise CopperFaultError("source_scope must be an object")
    _closed(
        sources,
        {
            "dimensional_revision",
            "field_surrogate_revision",
            "engineering_design_url",
            "technical_data_url",
            "nist_heat_capacity_url",
            "nist_resistivity_url",
            "production_voltage_url",
            "protection_closeout_url",
            "copper_density_basis",
            "material_grade_basis",
        },
        "source_scope",
    )
    if any(not isinstance(value, str) or not value for value in sources.values()):
        raise CopperFaultError("source_scope values must be nonempty strings")
    if sources["dimensional_revision"] != "NCSX conceptual-design m45r00":
        raise CopperFaultError("unsupported dimensional revision")
    if sources["field_surrogate_revision"] != (
        "SIMSOPT NCSX coils with c09r00-named boundary"
    ):
        raise CopperFaultError("unsupported field-surrogate revision")

    circuits = data["circuits"]
    if not isinstance(circuits, dict):
        raise CopperFaultError("circuits must be an object")
    _closed(
        circuits,
        {
            "names",
            "coils_per_circuit",
            "turns_per_coil",
            "centerline_length_m",
            "helical_length_factor",
            "copper_area_mm2",
            "copper_density_kg_m3",
            "resistivity_at_85k_ohm_m",
            "coil_resistance_at_85k_ohm",
            "line_resistance_ohm",
            "coil_self_inductance_h",
            "line_inductance_h",
            "fault_onset_current_a",
            "fault_onset_temperature_k",
            "prehistory_time_s",
            "prehistory_current_a",
            "thermal_validation_final_temperature_k",
            "thermal_validation_deposited_energy_j",
        },
        "circuits",
    )
    if circuits["names"] != ["M1", "M2", "M3"]:
        raise CopperFaultError("the supported circuit order is M1, M2, M3")
    coils_per_circuit = _integer(circuits, "coils_per_circuit")
    turns_per_coil = _integer(circuits, "turns_per_coil")
    if (coils_per_circuit, turns_per_coil) != (6, 36):
        raise CopperFaultError("the m45r00 model requires six 36-turn coils per circuit")
    prehistory_times = _numbers(
        circuits["prehistory_time_s"], len(circuits["prehistory_time_s"]), "prehistory_time_s"
    )
    raw_history = circuits["prehistory_current_a"]
    if not isinstance(raw_history, list) or len(raw_history) != len(prehistory_times):
        raise CopperFaultError("prehistory_current_a must match prehistory_time_s")
    prehistory_currents = np.stack(
        [_numbers(row, 3, "prehistory_current_a row") for row in raw_history]
    )
    if np.any(np.diff(prehistory_times) <= 0) or abs(prehistory_times[-1] - 0.213) > 1.0e-12:
        raise CopperFaultError("prehistory times must increase through the 0.213 s fault state")

    material_data = data["material"]
    if not isinstance(material_data, dict):
        raise CopperFaultError("material must be an object")
    _closed(
        material_data,
        {
            "specific_heat_log10_coefficients",
            "specific_heat_fit_error",
            "resistivity_coefficients",
            "resistivity_model",
            "resistivity_sensitivity_scales",
            "resistivity_sensitivity_basis",
        },
        "material",
    )
    if material_data["resistivity_model"] != (
        "nist-intrinsic-shape-scaled-to-published-85k-value"
    ):
        raise CopperFaultError("unsupported copper resistivity model")
    if material_data["resistivity_sensitivity_basis"] != (
        "chosen-model-envelope-not-nist-uncertainty"
    ):
        raise CopperFaultError("unsupported resistivity sensitivity basis")
    heat_coefficients = _numbers(
        material_data["specific_heat_log10_coefficients"], 9, "heat-capacity coefficients"
    )
    resistivity_coefficients = _numbers(
        material_data["resistivity_coefficients"], 7, "resistivity coefficients"
    )
    sensitivity = tuple(
        _numbers(
            material_data["resistivity_sensitivity_scales"],
            len(material_data["resistivity_sensitivity_scales"]),
            "resistivity sensitivity scales",
        )
    )
    if sensitivity != (0.85, 1.0, 1.15):
        raise CopperFaultError("the supported resistivity sensitivity is 0.85, 1, 1.15")

    transient = data["transient"]
    if not isinstance(transient, dict):
        raise CopperFaultError("transient must be an object")
    _closed(
        transient,
        {
            "fault",
            "detection_delays_s",
            "nominal_detection_delay_s",
            "end_time_s",
            "reference_time_step_s",
            "refined_time_step_s",
            "geometry_confirmation_multiplier",
        },
        "transient",
    )
    expected_fault = (
        "one-converter-voltage-collapses-while-healthy-converters-hold-steady-loss-"
        "compensation-until-global-bypass"
    )
    if transient["fault"] != expected_fault:
        raise CopperFaultError("unsupported converter-fault sequence")
    delays = tuple(
        _numbers(
            transient["detection_delays_s"],
            len(transient["detection_delays_s"]),
            "detection delays",
        )
    )
    if delays != (0.0, 0.025, 0.05, 0.1, 0.12):
        raise CopperFaultError("the declared delay sensitivity changed")

    control = data["control"]
    if not isinstance(control, dict):
        raise CopperFaultError("control must be an object")
    _closed(
        control,
        {"private_dump_resistance_ohm", "differential_to_common_mode_ratio"},
        "control",
    )
    gate_data = data["gate"]
    if not isinstance(gate_data, dict):
        raise CopperFaultError("gate must be an object")
    gate_keys = set(Gate.__annotations__)
    _closed(gate_data, gate_keys, "gate")
    gate = Gate(**{key: _number(gate_data, key) for key in gate_keys})

    model = Model(
        model_id=data["model_id"],
        base_model_sha256=digest,
        source_scope=dict(sources),
        names=tuple(circuits["names"]),
        coils_per_circuit=coils_per_circuit,
        turns_per_coil=turns_per_coil,
        centerline_lengths=_numbers(circuits["centerline_length_m"], 3, "centerline lengths"),
        helical_length_factor=_number(circuits, "helical_length_factor"),
        copper_area_m2=_number(circuits, "copper_area_mm2") * 1.0e-6,
        copper_density=_number(circuits, "copper_density_kg_m3"),
        resistivity_85k=_number(circuits, "resistivity_at_85k_ohm_m"),
        resistance_85k=_numbers(
            circuits["coil_resistance_at_85k_ohm"], 3, "85 K resistances"
        ),
        line_resistance=_number(circuits, "line_resistance_ohm"),
        self_inductance=_numbers(
            circuits["coil_self_inductance_h"], 3, "self inductances"
        ),
        line_inductance=_number(circuits, "line_inductance_h"),
        initial_current=_numbers(circuits["fault_onset_current_a"], 3, "initial currents"),
        initial_temperature=_numbers(
            circuits["fault_onset_temperature_k"], 3, "initial temperatures"
        ),
        prehistory_times=prehistory_times,
        prehistory_currents=prehistory_currents,
        validation_temperature=_numbers(
            circuits["thermal_validation_final_temperature_k"],
            3,
            "validation temperatures",
        ),
        validation_energy=_numbers(
            circuits["thermal_validation_deposited_energy_j"],
            3,
            "validation energies",
        ),
        material=Material(
            heat_coefficients,
            _number(material_data, "specific_heat_fit_error"),
            resistivity_coefficients,
            sensitivity,
            material_data["resistivity_sensitivity_basis"],
        ),
        detection_delays=delays,
        nominal_detection_delay=_number(transient, "nominal_detection_delay_s"),
        end_time=_number(transient, "end_time_s"),
        reference_time_step=_number(transient, "reference_time_step_s"),
        refined_time_step=_number(transient, "refined_time_step_s"),
        geometry_confirmation_multiplier=_integer(
            transient, "geometry_confirmation_multiplier"
        ),
        private_dump=_number(control, "private_dump_resistance_ohm"),
        control_ratio=_number(control, "differential_to_common_mode_ratio"),
        gate=gate,
    )
    positive = (
        *model.centerline_lengths,
        model.helical_length_factor,
        model.copper_area_m2,
        model.copper_density,
        model.resistivity_85k,
        *model.resistance_85k,
        model.line_resistance,
        *model.self_inductance,
        model.line_inductance,
        *model.initial_current,
        *model.initial_temperature,
        model.end_time,
        model.reference_time_step,
        model.refined_time_step,
        model.private_dump,
    )
    if min(positive) <= 0:
        raise CopperFaultError("all physical scales must be positive")
    if not model.refined_time_step < model.reference_time_step:
        raise CopperFaultError("refined time step must be smaller")
    if model.geometry_confirmation_multiplier != 2:
        raise CopperFaultError("the frozen geometry confirmation multiplier is two")
    if model.nominal_detection_delay not in model.detection_delays:
        raise CopperFaultError("nominal delay must be one of the sensitivity delays")
    for delay in model.detection_delays:
        for step in (model.reference_time_step, model.refined_time_step):
            if abs(delay / step - round(delay / step)) > 1.0e-10:
                raise CopperFaultError("detection delays must align with both time grids")
    if model.control_ratio != 1.0:
        raise CopperFaultError("the independent control must have ratio one")
    if model.material.heat_capacity_fit_error != 0.05:
        raise CopperFaultError("the bound NIST heat-capacity fit error must be five percent")
    if model.gate.minimum_magnetic_reduction <= 1 or model.gate.minimum_force_reduction <= 1:
        raise CopperFaultError("shape gates must require improvement")
    if model.gate.minimum_current_a != 0:
        raise CopperFaultError("the current-reversal gate must be zero amperes")
    return model


def load_candidate(raw: bytes) -> Candidate:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CopperFaultError(f"candidate is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise CopperFaultError("candidate root must be an object")
    _closed(data, {"schema", "candidate_id", "network"}, "candidate")
    if data["schema"] != CANDIDATE_SCHEMA:
        raise CopperFaultError(f"unsupported candidate schema {data['schema']!r}")
    if not isinstance(data["candidate_id"], str) or not data["candidate_id"]:
        raise CopperFaultError("candidate_id must be a nonempty string")
    network = data["network"]
    if not isinstance(network, dict):
        raise CopperFaultError("candidate.network must be an object")
    _closed(network, {"differential_to_common_mode_ratio"}, "candidate.network")
    ratio = _number(network, "differential_to_common_mode_ratio")
    if not 1.0 <= ratio <= 4.0:
        raise CopperFaultError("differential/common ratio must lie in [1, 4]")
    return Candidate(data["candidate_id"], ratio)


def heat_capacity(temperature_k: float | np.ndarray, material: Material) -> Any:
    temperature = np.asarray(temperature_k)
    if np.any((temperature < 4) | (temperature > 300)):
        raise CopperFaultError("NIST heat-capacity fit is used only from 4 to 300 K")
    x = np.log10(temperature)
    polynomial = sum(
        coefficient * x**power
        for power, coefficient in enumerate(material.heat_capacity_coefficients)
    )
    result = 10.0**polynomial
    return float(result) if result.ndim == 0 else result


def intrinsic_resistivity(temperature_k: float | np.ndarray, material: Material) -> Any:
    temperature = np.asarray(temperature_k, dtype=float)
    if np.any((temperature < 2) | (temperature > 300)):
        raise CopperFaultError("NIST resistivity fit is used only from 2 to 300 K")
    p1, p2, p3, p4, p5, p6, _p7 = material.resistivity_coefficients
    result = p1 * temperature**p2 / (
        1.0
        + p1
        * p3
        * temperature ** (p2 - p4)
        * np.exp(-((p5 / temperature) ** p6))
    )
    return float(result) if result.ndim == 0 else result


def copper_resistivity(
    temperature_k: float | np.ndarray, model: Model, sensitivity_scale: float = 1.0
) -> Any:
    """NIST intrinsic temperature shape, anchored to m45r00's rho(85 K).

    The m45r00 value is 0.83% below the NIST zero-residual curve at 85 K and
    cannot identify RRR. Scaling the curve makes that mismatch explicit while
    retaining the sourced 85--125 K temperature dependence.
    """

    intrinsic_85 = intrinsic_resistivity(85.0, model.material)
    result = (
        sensitivity_scale
        * model.resistivity_85k
        * intrinsic_resistivity(temperature_k, model.material)
        / intrinsic_85
    )
    return result


def _integral(function: Any, start: float, end: float, samples: int = 20001) -> float:
    grid = np.linspace(start, end, samples)
    return float(np.trapezoid(function(grid), grid))


@dataclass(frozen=True)
class ThermalValidation:
    inferred_energy: np.ndarray
    inferred_temperature: np.ndarray
    maximum_energy_relative_error: float
    maximum_temperature_absolute_error: float


def validate_thermal_scale(model: Model) -> ThermalValidation:
    inferred_energy = np.asarray(
        [
            mass
            * _integral(
                lambda temperature: heat_capacity(temperature, model.material),
                85.0,
                final_temperature,
            )
            for mass, final_temperature in zip(
                model.copper_mass, model.validation_temperature, strict=True
            )
        ]
    )
    inferred_temperature = []
    for mass, target_energy in zip(model.copper_mass, model.validation_energy, strict=True):
        low, high = 85.0, 150.0
        for _ in range(60):
            middle = (low + high) / 2.0
            energy = mass * _integral(
                lambda temperature: heat_capacity(temperature, model.material),
                85.0,
                middle,
                2001,
            )
            if energy < target_energy:
                low = middle
            else:
                high = middle
        inferred_temperature.append((low + high) / 2.0)
    inferred_temperature_array = np.asarray(inferred_temperature)
    return ThermalValidation(
        inferred_energy=inferred_energy,
        inferred_temperature=inferred_temperature_array,
        maximum_energy_relative_error=float(
            np.max(np.abs(inferred_energy - model.validation_energy) / model.validation_energy)
        ),
        maximum_temperature_absolute_error=float(
            np.max(np.abs(inferred_temperature_array - model.validation_temperature))
        ),
    )


def resistor_network(private: float, ratio: float) -> tuple[np.ndarray, np.ndarray, float]:
    """Return branch incidence, mesh resistance, and pair-shared resistance."""

    if private <= 0 or ratio < 1:
        raise CopperFaultError("the passive network needs p > 0 and ratio >= 1")
    incidence = np.asarray(((1.0, -1.0, 0.0), (0.0, 1.0, -1.0), (-1.0, 0.0, 1.0)))
    shared = private * (ratio - 1.0) / 3.0
    matrix = private * np.eye(3) + shared * incidence.T @ incidence
    return incidence, matrix, shared


def verify_network(private: float, ratio: float) -> bool:
    incidence, matrix, shared = resistor_network(private, ratio)
    if shared < 0 or not np.allclose(matrix, matrix.T):
        return False
    if np.min(np.linalg.eigvalsh(matrix)) <= 0:
        return False
    probes = (
        np.asarray((1.0, 1.0, 1.0)),
        np.asarray((1.0, -2.0, 0.5)),
        np.asarray((3.0, 0.0, -1.0)),
    )
    return all(
        math.isclose(
            float(probe @ matrix @ probe),
            private * float(probe @ probe)
            + shared * float((incidence @ probe) @ (incidence @ probe)),
            rel_tol=1.0e-13,
            abs_tol=1.0e-13,
        )
        for probe in probes
    )


def _type_groups() -> tuple[tuple[int, ...], ...]:
    return tuple(tuple(range(kind, 18, 3)) for kind in range(3))


def _coil_lengths(coils: list[np.ndarray]) -> np.ndarray:
    lengths = []
    for group in _type_groups():
        per_coil = [
            float(np.sum(np.linalg.norm(np.roll(coils[index], -1, axis=0) - coils[index], axis=1)))
            for index in group
        ]
        lengths.append(float(np.mean(per_coil)))
    return np.asarray(lengths)


def dimensional_inductance(
    coils: list[np.ndarray], signs: np.ndarray, model: Model, kernel: Any
) -> np.ndarray:
    """Use sourced self terms and only c09r00 cross-circuit mutual terms."""

    if len(coils) != 18 or signs.shape != (18,):
        raise CopperFaultError("inductance construction requires 18 signed coils")
    midpoints = []
    elements = []
    for coil in coils:
        midpoint, element = kernel.segment_geometry(coil)
        midpoints.append(midpoint)
        elements.append(element)
    single_turn = np.zeros((18, 18))
    for first in range(18):
        for second in range(first + 1, 18):
            delta = midpoints[first][:, None, :] - midpoints[second][None, :, :]
            distance = np.linalg.norm(delta, axis=2)
            if np.any(distance <= 0):
                raise CopperFaultError("distinct filament segments intersect numerically")
            value = 1.0e-7 * np.sum(
                (elements[first] @ elements[second].T) / distance
            )
            single_turn[first, second] = value
            single_turn[second, first] = value
    winding = np.zeros((18, 3))
    for circuit, group in enumerate(_type_groups()):
        winding[list(group), circuit] = signs[list(group)] * model.turns_per_coil
    result = winding.T @ single_turn @ winding
    np.fill_diagonal(result, model.self_inductance)
    result = 0.5 * (result + result.T)
    eigenvalues = np.linalg.eigvalsh(result)
    if eigenvalues[0] <= 0:
        raise CopperFaultError("the dimensional inductance matrix is not positive definite")
    return result


@dataclass(frozen=True)
class ShapeModel:
    magnetic_gram: np.ndarray
    force_gram: np.ndarray
    current_weights: np.ndarray


def type_current_scaling(
    analysis: Any, model: Model
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    groups = _type_groups()
    geometry_ampere_turns = np.asarray(
        [np.mean(np.abs(analysis.currents[list(group)])) for group in groups]
    )
    target_ampere_turns = model.turns_per_coil * model.initial_current
    if np.min(geometry_ampere_turns) <= 0 or np.min(target_ampere_turns) <= 0:
        raise CopperFaultError("field scaling requires positive type-circuit currents")
    return (
        geometry_ampere_turns,
        target_ampere_turns,
        target_ampere_turns / geometry_ampere_turns,
    )


def shape_model(analysis: Any, model: Model) -> ShapeModel:
    groups = _type_groups()
    _, _, type_scales = type_current_scaling(analysis, model)
    circuit_fields = np.stack(
        [
            type_scales[kind]
            * np.sum(analysis.weighted_normal_fields[list(group)], axis=0)
            for kind, group in enumerate(groups)
        ]
    )
    magnetic_gram = circuit_fields @ circuit_fields.T
    force_terms: list[np.ndarray] = []
    raw_force_terms: list[np.ndarray] = []
    for receiver_kind, receiver_group in enumerate(groups):
        receivers = np.zeros(18, dtype=bool)
        receivers[list(receiver_group)] = True
        for source_kind, source_group in enumerate(groups):
            contribution = np.zeros_like(analysis.nominal_force_density)
            contribution[receivers] = np.sum(
                analysis.pair_force_density[receivers][:, list(source_group)], axis=1
            )
            contribution *= type_scales[receiver_kind] * type_scales[source_kind]
            raw_force_terms.append(contribution)
    actual_nominal_force = np.sum(raw_force_terms, axis=0)
    actual_nominal_force_rms = math.sqrt(
        float(
            np.sum(
                analysis.force_weights[:, :, None] * actual_nominal_force**2
            )
        )
    )
    if actual_nominal_force_rms <= 0:
        raise CopperFaultError("the scaled nominal force pattern has zero norm")
    for contribution in raw_force_terms:
        weighted = (
            contribution
            * np.sqrt(analysis.force_weights)[:, :, None]
            / actual_nominal_force_rms
        )
        force_terms.append(weighted.reshape(-1))
    stacked = np.stack(force_terms)
    ones = np.ones(3)
    common_norm = float(ones @ magnetic_gram @ ones)
    if common_norm <= 0:
        raise CopperFaultError("the nominal common field pattern has zero norm")
    # This is the exact least-squares projection onto the nominal field
    # pattern: s=(1^T G f)/(1^T G 1), stored as s=w^T f.
    projection_weights = magnetic_gram @ ones / common_norm
    return ShapeModel(magnetic_gram, stacked @ stacked.T, projection_weights)


def shape_departure(
    currents: np.ndarray, initial: np.ndarray, shape: ShapeModel
) -> tuple[float, float]:
    fractions = currents / initial
    common_scale = float(shape.current_weights @ fractions)
    differential = fractions - common_scale
    magnetic_sq = float(differential @ shape.magnetic_gram @ differential)
    products = np.outer(fractions, fractions).reshape(9)
    force_coefficients = products - common_scale**2
    force_sq = float(force_coefficients @ shape.force_gram @ force_coefficients)
    return math.sqrt(max(magnetic_sq, 0.0)), math.sqrt(max(force_sq, 0.0))


@dataclass(frozen=True)
class TransientMetric:
    fault: int
    delay: float
    resistance_scale: float
    heat_capacity_scale: float
    peak_magnetic_departure: float
    peak_magnetic_time: float
    peak_force_departure: float
    peak_force_time: float
    tail_i2t: np.ndarray
    total_i2t: np.ndarray
    maximum_temperature: float
    maximum_absolute_current: float
    minimum_current: float
    maximum_component_voltage: float
    final_energy_fraction: float
    energy_balance_relative_error: float
    coil_energy: np.ndarray
    line_energy: float
    private_energy: float
    shared_energy: float
    source_work: float


def _prehistory_i2t(model: Model) -> np.ndarray:
    # The sparse source table is declared piecewise-linear in current. The
    # square of a linear segment integrates exactly to
    # dt*(i0^2 + i0*i1 + i1^2)/3; trapezoiding the squared samples would
    # materially overestimate this ramped prehistory.
    left = model.prehistory_currents[:-1]
    right = model.prehistory_currents[1:]
    duration = np.diff(model.prehistory_times)[:, None]
    return np.sum(duration * (left**2 + left * right + right**2) / 3.0, axis=0)


def simulate(
    inductance_coil: np.ndarray,
    shape: ShapeModel,
    model: Model,
    ratio: float,
    fault: int,
    delay: float,
    resistance_scale: float,
    step: float,
    heat_capacity_scale: float = 1.0,
) -> TransientMetric:
    return _simulate_batch(
        inductance_coil,
        shape,
        model,
        np.asarray((ratio,)),
        np.asarray((fault,), dtype=int),
        np.asarray((delay,)),
        np.asarray((resistance_scale,)),
        np.asarray((heat_capacity_scale,)),
        step,
    )[0]


def _simulate_batch(
    inductance_coil: np.ndarray,
    shape: ShapeModel,
    model: Model,
    ratios: np.ndarray,
    faults: np.ndarray,
    delays: np.ndarray,
    resistance_scales: np.ndarray,
    heat_capacity_scales: np.ndarray,
    step: float,
) -> tuple[TransientMetric, ...]:
    """Integrate an ordered scenario batch on one event-aligned time grid."""

    count = len(ratios)
    if count == 0 or any(
        len(values) != count
        for values in (faults, delays, resistance_scales, heat_capacity_scales)
    ):
        raise CopperFaultError("transient scenario arrays must have one nonempty common length")
    if np.any((faults < 0) | (faults > 2)):
        raise CopperFaultError("fault indices must lie in 0..2")
    if any(float(value) not in model.detection_delays for value in delays):
        raise CopperFaultError("a delay is outside the frozen sensitivity set")
    if any(
        float(value) not in model.material.resistivity_sensitivity_scales
        for value in resistance_scales
    ):
        raise CopperFaultError("a resistance scale is outside the frozen sensitivity set")
    if any(
        float(value) not in model.material.heat_capacity_sensitivity_scales
        for value in heat_capacity_scales
    ):
        raise CopperFaultError(
            "a heat-capacity scale is outside the frozen sensitivity set"
        )
    if np.any((ratios < 1.0) | (ratios > 4.0)):
        raise CopperFaultError("differential/common ratios must lie in [1, 4]")
    if abs(model.end_time / step - round(model.end_time / step)) > 1.0e-9:
        raise CopperFaultError("end time must align with the time step")

    incidence, _, _ = resistor_network(model.private_dump, 1.0)
    laplacian = incidence.T @ incidence
    shared = model.private_dump * (ratios - 1.0) / 3.0
    protection = (
        model.private_dump * np.eye(3)[None, :, :]
        + shared[:, None, None] * laplacian[None, :, :]
    )
    inductance = inductance_coil + model.line_inductance * np.eye(3)
    inverse_inductance = np.linalg.inv(inductance)
    resistance_geometry = model.resistance_85k / model.resistivity_85k
    initial_coil_resistance = resistance_geometry * copper_resistivity(
        model.initial_temperature[None, :],
        model,
        resistance_scales[:, None],
    )
    holding_voltage = (initial_coil_resistance + model.line_resistance) * model.initial_current
    holding_voltage[np.arange(count), faults] = 0.0
    initial_energy = 0.5 * float(model.initial_current @ inductance @ model.initial_current)
    if initial_energy <= 0:
        raise CopperFaultError("initial magnetic energy is not positive")

    # i[3], T[3], tail I2t[3], winding heat[3], then line/private/shared/work.
    initial_state = np.concatenate(
        (model.initial_current, model.initial_temperature, np.zeros(10))
    )
    state = np.tile(initial_state, (count, 1))

    def physics(
        value: np.ndarray, active: np.ndarray
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
        current = value[:, :3]
        temperature = value[:, 3:6]
        coil_resistance = resistance_geometry * copper_resistivity(
            temperature, model, resistance_scales[:, None]
        )
        source = np.where(active[:, None], 0.0, holding_voltage)
        resistive_drop = (coil_resistance + model.line_resistance) * current
        protection_drop = np.einsum("nij,nj->ni", protection, current)
        resistive_drop += np.where(active[:, None], protection_drop, 0.0)
        d_current = (source - resistive_drop) @ inverse_inductance.T
        winding_power = current**2 * coil_resistance
        d_temperature = winding_power / (
            model.copper_mass[None, :]
            * heat_capacity_scales[:, None]
            * heat_capacity(temperature, model.material)
        )
        current_norm = np.sum(current**2, axis=1)
        line_power = model.line_resistance * current_norm
        private_power = np.where(active, model.private_dump * current_norm, 0.0)
        difference = current @ incidence.T
        shared_power = np.where(
            active, shared * np.sum(difference**2, axis=1), 0.0
        )
        source_power = np.sum(source * current, axis=1)
        derivative = np.concatenate(
            (
                d_current,
                d_temperature,
                current**2,
                winding_power,
                np.stack((line_power, private_power, shared_power, source_power), axis=1),
            ),
            axis=1,
        )
        return derivative, source, coil_resistance

    peak_magnetic = np.zeros(count)
    peak_magnetic_time = np.zeros(count)
    peak_force = np.zeros(count)
    peak_force_time = np.zeros(count)
    maximum_temperature = np.full(count, float(np.max(model.initial_temperature)))
    maximum_current = np.full(count, float(np.max(np.abs(model.initial_current))))
    minimum_current = np.full(count, float(np.min(model.initial_current)))
    maximum_voltage = np.zeros(count)

    steps = int(round(model.end_time / step))
    for index in range(steps + 1):
        time = index * step
        current = state[:, :3]
        temperature = state[:, 3:6]
        fractions = current / model.initial_current[None, :]
        common_scale = fractions @ shape.current_weights
        differential = fractions - common_scale[:, None]
        magnetic = np.sqrt(
            np.maximum(
                np.einsum(
                    "ni,ij,nj->n", differential, shape.magnetic_gram, differential
                ),
                0.0,
            )
        )
        products = (fractions[:, :, None] * fractions[:, None, :]).reshape(count, 9)
        force_coefficients = products - common_scale[:, None] ** 2
        force = np.sqrt(
            np.maximum(
                np.einsum(
                    "ni,ij,nj->n",
                    force_coefficients,
                    shape.force_gram,
                    force_coefficients,
                ),
                0.0,
            )
        )
        new_magnetic = magnetic > peak_magnetic
        peak_magnetic[new_magnetic] = magnetic[new_magnetic]
        peak_magnetic_time[new_magnetic] = time
        new_force = force > peak_force
        peak_force[new_force] = force[new_force]
        peak_force_time[new_force] = time
        maximum_temperature = np.maximum(maximum_temperature, np.max(temperature, axis=1))
        maximum_current = np.maximum(maximum_current, np.max(np.abs(current), axis=1))
        minimum_current = np.minimum(minimum_current, np.min(current, axis=1))

        # Delays are exact grid points. Freezing this mask for all four RK
        # stages integrates up to an event with the old topology and begins the
        # new topology on the following interval, avoiding half-step switching.
        active = time + 1.0e-14 >= delays
        slope, source, coil_resistance = physics(state, active)
        inductive_coil = slope[:, :3] @ inductance_coil.T
        line_voltage = (
            model.line_resistance * current + model.line_inductance * slope[:, :3]
        )
        shared_voltage = shared[:, None] * np.abs(current @ incidence.T)
        component_voltages = np.concatenate(
            (
                np.abs(coil_resistance * current + inductive_coil),
                np.abs(line_voltage),
                np.abs(source),
                np.where(active[:, None], model.private_dump * np.abs(current), 0.0),
                np.where(active[:, None], shared_voltage, 0.0),
            ),
            axis=1,
        )
        maximum_voltage = np.maximum(maximum_voltage, np.max(component_voltages, axis=1))
        if index == steps:
            break
        k1 = slope
        k2 = physics(state + step * k1 / 2.0, active)[0]
        k3 = physics(state + step * k2 / 2.0, active)[0]
        k4 = physics(state + step * k3, active)[0]
        state = state + step * (k1 + 2 * k2 + 2 * k3 + k4) / 6.0
        if not np.all(np.isfinite(state)):
            raise CopperFaultError("transient integration produced a non-finite state")

    final_current = state[:, :3]
    final_energy = 0.5 * np.einsum(
        "ni,ij,nj->n", final_current, inductance, final_current
    )
    coil_energy = state[:, 9:12]
    line_energy = state[:, 12]
    private_energy = state[:, 13]
    shared_energy = state[:, 14]
    source_work = state[:, 15]
    dissipated = (
        np.sum(coil_energy, axis=1) + line_energy + private_energy + shared_energy
    )
    balance = np.abs(initial_energy + source_work - final_energy - dissipated) / np.maximum(
        initial_energy + np.abs(source_work), 1.0
    )
    prehistory_i2t = _prehistory_i2t(model)
    metrics = []
    for index in range(count):
        tail_i2t = state[index, 6:9]
        metrics.append(
            TransientMetric(
                fault=int(faults[index]),
                delay=float(delays[index]),
                resistance_scale=float(resistance_scales[index]),
                heat_capacity_scale=float(heat_capacity_scales[index]),
                peak_magnetic_departure=float(peak_magnetic[index]),
                peak_magnetic_time=float(peak_magnetic_time[index]),
                peak_force_departure=float(peak_force[index]),
                peak_force_time=float(peak_force_time[index]),
                tail_i2t=tail_i2t.copy(),
                total_i2t=prehistory_i2t + tail_i2t,
                maximum_temperature=float(maximum_temperature[index]),
                maximum_absolute_current=float(maximum_current[index]),
                minimum_current=float(minimum_current[index]),
                maximum_component_voltage=float(maximum_voltage[index]),
                final_energy_fraction=float(final_energy[index] / initial_energy),
                energy_balance_relative_error=float(balance[index]),
                coil_energy=coil_energy[index].copy(),
                line_energy=float(line_energy[index]),
                private_energy=float(private_energy[index]),
                shared_energy=float(shared_energy[index]),
                source_work=float(source_work[index]),
            )
        )
    return tuple(metrics)


@dataclass(frozen=True)
class Evaluation:
    minimum_magnetic_reduction: float
    minimum_force_reduction: float
    maximum_total_i2t: float
    maximum_temperature: float
    maximum_absolute_current: float
    minimum_current: float
    maximum_component_voltage: float
    maximum_final_energy_fraction: float
    maximum_energy_balance_error: float
    at_or_before_activation_peak_cases: int
    comparisons: int
    control: tuple[TransientMetric, ...]
    candidate: tuple[TransientMetric, ...]


def _at_or_before_activation(peak_time: float, delay: float) -> bool:
    # The state sampled exactly at `delay` is still the pre-switch state; the
    # active topology first changes the following integration interval.
    return peak_time <= delay + 1.0e-12


def evaluate_sweep(
    inductance: np.ndarray,
    shape: ShapeModel,
    model: Model,
    ratio: float,
    step: float,
    resistance_scales: Iterable[float] | None = None,
    heat_capacity_scales: Iterable[float] | None = None,
    delays: Iterable[float] | None = None,
) -> Evaluation:
    scales = tuple(
        model.material.resistivity_sensitivity_scales
        if resistance_scales is None
        else resistance_scales
    )
    capacity_scales = tuple(
        model.material.heat_capacity_sensitivity_scales
        if heat_capacity_scales is None
        else heat_capacity_scales
    )
    delay_values = tuple(model.detection_delays if delays is None else delays)
    ratios = []
    faults = []
    scenario_delays = []
    scenario_resistance_scales = []
    scenario_capacity_scales = []
    for resistance_scale in scales:
        for capacity_scale in capacity_scales:
            for delay in delay_values:
                for fault in range(3):
                    for network_ratio in (model.control_ratio, ratio):
                        ratios.append(network_ratio)
                        faults.append(fault)
                        scenario_delays.append(delay)
                        scenario_resistance_scales.append(resistance_scale)
                        scenario_capacity_scales.append(capacity_scale)
    metrics = _simulate_batch(
        inductance,
        shape,
        model,
        np.asarray(ratios),
        np.asarray(faults, dtype=int),
        np.asarray(scenario_delays),
        np.asarray(scenario_resistance_scales),
        np.asarray(scenario_capacity_scales),
        step,
    )
    control = list(metrics[0::2])
    candidate = list(metrics[1::2])
    reductions_magnetic = []
    reductions_force = []
    at_or_before_activation = 0
    for baseline, proposed in zip(control, candidate, strict=True):
        if min(
            baseline.peak_magnetic_departure,
            baseline.peak_force_departure,
            proposed.peak_magnetic_departure,
            proposed.peak_force_departure,
        ) <= 0:
            raise CopperFaultError("a transient shape metric is degenerate")
        reductions_magnetic.append(
            baseline.peak_magnetic_departure / proposed.peak_magnetic_departure
        )
        reductions_force.append(
            baseline.peak_force_departure / proposed.peak_force_departure
        )
        if (
            _at_or_before_activation(proposed.peak_magnetic_time, proposed.delay)
            or _at_or_before_activation(proposed.peak_force_time, proposed.delay)
        ):
            at_or_before_activation += 1
    return Evaluation(
        minimum_magnetic_reduction=min(reductions_magnetic),
        minimum_force_reduction=min(reductions_force),
        maximum_total_i2t=max(float(np.max(item.total_i2t)) for item in candidate),
        maximum_temperature=max(item.maximum_temperature for item in candidate),
        maximum_absolute_current=max(item.maximum_absolute_current for item in candidate),
        minimum_current=min(item.minimum_current for item in candidate),
        maximum_component_voltage=max(item.maximum_component_voltage for item in candidate),
        maximum_final_energy_fraction=max(item.final_energy_fraction for item in candidate),
        maximum_energy_balance_error=max(
            item.energy_balance_relative_error for item in candidate
        ),
        at_or_before_activation_peak_cases=at_or_before_activation,
        comparisons=len(candidate),
        control=tuple(control),
        candidate=tuple(candidate),
    )


def _relative_difference(first: float, second: float) -> float:
    scale = max(abs(first), abs(second), 1.0e-30)
    return abs(first - second) / scale


def _evaluation_difference(first: Evaluation, second: Evaluation) -> float:
    """Return the largest convergence change in a declared physical metric.

    The energy-balance residual is deliberately excluded: it has its own
    absolute numerical gate, and a relative change between two roundoff-sized
    residuals is not a useful convergence measure.  In addition to the gated
    aggregates, compare each raw control/candidate shape peak so correlated
    motion cannot be hidden by an unchanged reduction ratio.
    """
    fields = (
        "minimum_magnetic_reduction",
        "minimum_force_reduction",
        "maximum_total_i2t",
        "maximum_temperature",
        "maximum_absolute_current",
        "minimum_current",
        "maximum_component_voltage",
        "maximum_final_energy_fraction",
    )
    differences = [
        _relative_difference(getattr(first, field), getattr(second, field))
        for field in fields
    ]
    if len(first.control) != len(second.control) or len(first.candidate) != len(
        second.candidate
    ):
        raise CopperFaultError("convergence evaluations cover different scenarios")
    for first_group, second_group in (
        (first.control, second.control),
        (first.candidate, second.candidate),
    ):
        for first_metric, second_metric in zip(
            first_group, second_group, strict=True
        ):
            differences.extend(
                (
                    _relative_difference(
                        first_metric.peak_magnetic_departure,
                        second_metric.peak_magnetic_departure,
                    ),
                    _relative_difference(
                        first_metric.peak_force_departure,
                        second_metric.peak_force_departure,
                    ),
                )
            )
    return max(differences)


def _mutual_inductance_difference(first: np.ndarray, second: np.ndarray) -> float:
    """Compare only the constructed off-diagonal mutual-inductance terms."""
    if first.shape != second.shape or first.ndim != 2 or first.shape[0] != first.shape[1]:
        raise CopperFaultError("inductance matrices must be square and shape-matched")
    off_diagonal = ~np.eye(first.shape[0], dtype=bool)
    denominator = float(np.linalg.norm(second[off_diagonal]))
    if denominator <= 0:
        raise CopperFaultError("mutual-inductance comparison has zero norm")
    return float(np.linalg.norm((second - first)[off_diagonal]) / denominator)


def _metric_json(metric: TransientMetric, names: tuple[str, ...]) -> dict[str, Any]:
    return {
        "fault_circuit": names[metric.fault],
        "detection_delay_s": _exact(metric.delay),
        "resistivity_scale": _exact(metric.resistance_scale),
        "heat_capacity_scale": _exact(metric.heat_capacity_scale),
        "peak_magnetic_shape_departure": _exact(metric.peak_magnetic_departure),
        "peak_magnetic_time_s": _exact(metric.peak_magnetic_time),
        "peak_force_shape_departure": _exact(metric.peak_force_departure),
        "peak_force_time_s": _exact(metric.peak_force_time),
        "tail_i2t_a2s": [_exact(value) for value in metric.tail_i2t],
        "total_i2t_a2s": [_exact(value) for value in metric.total_i2t],
        "maximum_temperature_k": _exact(metric.maximum_temperature),
        "maximum_absolute_current_a": _exact(metric.maximum_absolute_current),
        "minimum_current_a": _exact(metric.minimum_current),
        "maximum_modeled_component_voltage_v": _exact(
            metric.maximum_component_voltage
        ),
        "final_energy_fraction": _exact(metric.final_energy_fraction),
        "energy_balance_relative_error": _exact(metric.energy_balance_relative_error),
    }


def evaluate(
    kernel_path: Path,
    ceiling_path: Path,
    fault_library_path: Path,
    coil_path: Path,
    boundary_path: Path,
    base_model_path: Path,
    copper_model_path: Path,
    candidate_path: Path,
) -> dict[str, Any]:
    model = load_model(copper_model_path.read_bytes())
    candidate = load_candidate(candidate_path.read_bytes())
    if _sha256(base_model_path) != model.base_model_sha256:
        raise CopperFaultError("base model digest does not match copper-fault model")
    kernel = _load_module("ncsx_copper_field_kernel", kernel_path)
    ceiling = _load_module("ncsx_copper_ceiling", ceiling_path)
    fault_library = _load_module("ncsx_copper_fault_library", fault_library_path)
    base_model = ceiling.load_model(base_model_path.read_bytes())
    boundary = ceiling.boundary_coefficients(boundary_path, base_model)

    analyses = []
    inductances = []
    shapes = []
    surrogate_lengths = []
    confirmation_resolution = ceiling.Resolution(
        coil_segments=(
            base_model.fine.coil_segments * model.geometry_confirmation_multiplier
        ),
        surface_toroidal=(
            base_model.fine.surface_toroidal * model.geometry_confirmation_multiplier
        ),
        surface_poloidal=(
            base_model.fine.surface_poloidal * model.geometry_confirmation_multiplier
        ),
    )
    for resolution in (base_model.fine, confirmation_resolution):
        analysis = fault_library.analyze(
            ceiling, kernel, coil_path, boundary, base_model, resolution
        )
        coils, currents = ceiling.physical_coils(
            coil_path, base_model, resolution.coil_segments
        )
        analyses.append(analysis)
        inductances.append(
            dimensional_inductance(coils, np.sign(currents), model, kernel)
        )
        shapes.append(shape_model(analysis, model))
        surrogate_lengths.append(_coil_lengths(coils))

    thermal_validation = validate_thermal_scale(model)
    authoritative = evaluate_sweep(
        inductances[1],
        shapes[1],
        model,
        candidate.differential_ratio,
        model.reference_time_step,
    )
    # Geometry and time-step checks use nominal material properties and the
    # full delay/fault set. The broader sensitivity sweep is in `authoritative`.
    base_fine = evaluate_sweep(
        inductances[0],
        shapes[0],
        model,
        candidate.differential_ratio,
        model.reference_time_step,
        resistance_scales=(1.0,),
        heat_capacity_scales=(1.0,),
    )
    confirmation_nominal = evaluate_sweep(
        inductances[1],
        shapes[1],
        model,
        candidate.differential_ratio,
        model.reference_time_step,
        resistance_scales=(1.0,),
        heat_capacity_scales=(1.0,),
    )
    refined = evaluate_sweep(
        inductances[1],
        shapes[1],
        model,
        candidate.differential_ratio,
        model.refined_time_step,
        resistance_scales=(1.0,),
        heat_capacity_scales=(1.0,),
    )
    shape_resolution_difference = _evaluation_difference(
        base_fine, confirmation_nominal
    )
    inductance_resolution_difference = _mutual_inductance_difference(
        inductances[0], inductances[1]
    )
    time_step_difference = _evaluation_difference(confirmation_nominal, refined)
    inductance = inductances[1] + model.line_inductance * np.eye(3)
    condition_number = float(np.linalg.cond(inductance))
    length_mismatch = float(
        np.max(np.abs(surrogate_lengths[1] - model.centerline_lengths) / model.centerline_lengths)
    )
    incidence, network, shared = resistor_network(
        model.private_dump, candidate.differential_ratio
    )
    topology_valid = verify_network(model.private_dump, candidate.differential_ratio)
    coverage_complete = (
        authoritative.comparisons
        == len(model.detection_delays)
        * len(model.material.resistivity_sensitivity_scales)
        * len(model.material.heat_capacity_sensitivity_scales)
        * 3
    )

    gate = model.gate
    gate_verdicts = {
        "minimum_magnetic_reduction": authoritative.minimum_magnetic_reduction
        + 1.0e-12
        >= gate.minimum_magnetic_reduction,
        "minimum_force_reduction": authoritative.minimum_force_reduction + 1.0e-12
        >= gate.minimum_force_reduction,
        "maximum_total_i2t": authoritative.maximum_total_i2t
        <= gate.maximum_total_i2t_a2s + 1.0e-6,
        "maximum_temperature": authoritative.maximum_temperature
        <= gate.maximum_temperature_k + 1.0e-9,
        "maximum_absolute_current": authoritative.maximum_absolute_current
        <= gate.maximum_absolute_current_a + 1.0e-9,
        "no_current_reversal": authoritative.minimum_current + 1.0e-9
        >= gate.minimum_current_a,
        "maximum_modeled_component_voltage": authoritative.maximum_component_voltage
        <= gate.maximum_modeled_component_voltage_v + 1.0e-9,
        "maximum_final_energy_fraction": authoritative.maximum_final_energy_fraction
        <= gate.maximum_final_energy_fraction + 1.0e-12,
        "maximum_energy_balance_error": authoritative.maximum_energy_balance_error
        <= gate.maximum_energy_balance_relative_error + 1.0e-12,
        "time_step_convergence": time_step_difference
        <= gate.maximum_time_step_relative_difference + 1.0e-12,
        "shape_resolution_convergence": shape_resolution_difference
        <= gate.maximum_shape_resolution_relative_difference + 1.0e-12,
        "inductance_resolution_convergence": inductance_resolution_difference
        <= gate.maximum_inductance_resolution_relative_difference + 1.0e-12,
        "inductance_conditioning": condition_number
        <= gate.maximum_inductance_condition_number + 1.0e-12,
        "geometry_length_cross_check": length_mismatch
        <= gate.maximum_geometry_length_mismatch + 1.0e-12,
        "thermal_scale_validation": thermal_validation.maximum_energy_relative_error
        <= gate.maximum_thermal_validation_energy_error + 1.0e-12,
        "passive_network_identity": topology_valid,
        "sensitivity_coverage": coverage_complete,
    }
    passed = all(gate_verdicts.values())
    failed_gates = [name for name, verdict in gate_verdicts.items() if not verdict]
    magnetic_reductions = np.asarray(
        [
            control.peak_magnetic_departure / proposed.peak_magnetic_departure
            for control, proposed in zip(
                authoritative.control, authoritative.candidate, strict=True
            )
        ]
    )
    force_reductions = np.asarray(
        [
            control.peak_force_departure / proposed.peak_force_departure
            for control, proposed in zip(
                authoritative.control, authoritative.candidate, strict=True
            )
        ]
    )
    magnetic_limit_index = int(np.argmin(magnetic_reductions))
    force_limit_index = int(np.argmin(force_reductions))
    magnetic_control_limit = authoritative.control[magnetic_limit_index]
    force_control_limit = authoritative.control[force_limit_index]
    magnetic_limit = authoritative.candidate[magnetic_limit_index]
    force_limit = authoritative.candidate[force_limit_index]
    shape_failed = not (
        gate_verdicts["minimum_magnetic_reduction"]
        and gate_verdicts["minimum_force_reduction"]
    )
    failed_shape_limits = []
    if not gate_verdicts["minimum_magnetic_reduction"]:
        failed_shape_limits.append(
            _at_or_before_activation(
                magnetic_limit.peak_magnetic_time, magnetic_limit.delay
            )
        )
    if not gate_verdicts["minimum_force_reduction"]:
        failed_shape_limits.append(
            _at_or_before_activation(force_limit.peak_force_time, force_limit.delay)
        )
    if shape_failed and failed_shape_limits and all(failed_shape_limits):
        causal_failure = "limiting-shape-peak-at-or-before-network-activation"
    elif shape_failed and any(failed_shape_limits):
        causal_failure = "mixed-pre-and-post-activation-shape-failure"
    elif shape_failed:
        causal_failure = "post-activation-shape-redistribution"
    else:
        causal_failure = "non-shape-gate-failure" if failed_gates else "none"

    nominal_records = [
        metric
        for metric in confirmation_nominal.candidate
        if metric.delay == model.nominal_detection_delay
    ]
    nominal_controls = [
        metric
        for metric in confirmation_nominal.control
        if metric.delay == model.nominal_detection_delay
    ]
    geometry_ampere_turns, target_ampere_turns, field_current_scales = (
        type_current_scaling(analyses[1], model)
    )
    return {
        "schema": RESULT_SCHEMA,
        "model_id": model.model_id,
        "candidate_id": candidate.candidate_id,
        "source_identity": {
            "coil_sha256": _sha256(coil_path),
            "boundary_sha256": _sha256(boundary_path),
            "base_model_sha256": _sha256(base_model_path),
            "copper_model_sha256": _sha256(copper_model_path),
            "candidate_sha256": _sha256(candidate_path),
            "field_kernel_sha256": _sha256(kernel_path),
            "ceiling_library_sha256": _sha256(ceiling_path),
            "fault_library_sha256": _sha256(fault_library_path),
        },
        "source_scope": model.source_scope,
        "method": {
            "technology": "cryoresistive-copper-not-superconducting",
            "circuit_equation": "L-di-dt-equals-source-minus-temperature-dependent-copper-line-and-switched-protection-drops",
            "thermal_equation": "adiabatic-lumped-copper-mass-times-cp-dT-dt-equals-i-squared-Rcoil",
            "fault": "one-converter-voltage-collapse-followed-by-global-bypass-and-dump",
            "field_force_basis": "scaled-SIMSOPT-NCSX-coils-with-c09r00-named-boundary-field-and-inter-coil-filament-force-shape-departure-from-best-common-scale",
            "geometry_base_fine_resolution": {
                "coil_segments": base_model.fine.coil_segments,
                "surface_toroidal": base_model.fine.surface_toroidal,
                "surface_poloidal": base_model.fine.surface_poloidal,
            },
            "geometry_confirmation_resolution": {
                "coil_segments": confirmation_resolution.coil_segments,
                "surface_toroidal": confirmation_resolution.surface_toroidal,
                "surface_poloidal": confirmation_resolution.surface_poloidal,
            },
            "integration": "event-aligned-fixed-step-rk4",
            "convergence_scope": "all-faults-and-delays-at-nominal-material-properties",
            "search_inside_checker": False,
            "numpy_version": np.__version__,
        },
        "material": {
            "copper_mass_kg": [_exact(value) for value in model.copper_mass],
            "resistivity_at_85k_ohm_m": _exact(model.resistivity_85k, 15),
            "nist_intrinsic_resistivity_at_85k_ohm_m": _exact(
                intrinsic_resistivity(85.0, model.material), 15
            ),
            "resistivity_anchor_scale": _exact(
                model.resistivity_85k
                / intrinsic_resistivity(85.0, model.material),
                12,
            ),
            "resistivity_sensitivity_scales": [
                _exact(value) for value in model.material.resistivity_sensitivity_scales
            ],
            "resistivity_sensitivity_basis": (
                model.material.resistivity_sensitivity_basis
            ),
            "specific_heat_curve_fit_error": _exact(
                model.material.heat_capacity_fit_error
            ),
            "specific_heat_sensitivity_scales": [
                _exact(value)
                for value in model.material.heat_capacity_sensitivity_scales
            ],
            "specific_heat_sensitivity_basis": (
                "nist-five-percent-fit-error-endpoints-and-nominal"
            ),
            "specific_heat_grade_assumption": model.source_scope[
                "material_grade_basis"
            ],
            "copper_density_basis": model.source_scope["copper_density_basis"],
        },
        "prehistory": {
            "interpolation": "piecewise-linear-current-with-exact-square-integral",
            "i2t_a2s": [_exact(value) for value in _prehistory_i2t(model)],
        },
        "thermal_validation": {
            "published_energy_j": [_exact(value) for value in model.validation_energy],
            "energy_from_published_temperature_j": [
                _exact(value) for value in thermal_validation.inferred_energy
            ],
            "published_final_temperature_k": [
                _exact(value) for value in model.validation_temperature
            ],
            "temperature_from_published_energy_k": [
                _exact(value) for value in thermal_validation.inferred_temperature
            ],
            "maximum_energy_relative_error": _exact(
                thermal_validation.maximum_energy_relative_error
            ),
            "maximum_temperature_absolute_error_k": _exact(
                thermal_validation.maximum_temperature_absolute_error
            ),
            "interpretation": "internal-curve-reproduction-not-physical-temperature-accuracy",
        },
        "geometry_surrogate": {
            "dimensional_revision": model.source_scope["dimensional_revision"],
            "field_revision": model.source_scope["field_surrogate_revision"],
            "published_centerline_length_m": [
                _exact(value) for value in model.centerline_lengths
            ],
            "base_fine_centerline_length_m": [
                _exact(value) for value in surrogate_lengths[0]
            ],
            "confirmation_centerline_length_m": [
                _exact(value) for value in surrogate_lengths[1]
            ],
            "maximum_relative_length_mismatch": _exact(length_mismatch),
            "revision_equivalence_established": False,
            "simsopt_ncsx_geometry_ampere_turns_a": [
                _exact(value) for value in geometry_ampere_turns
            ],
            "m45r00_fault_state_ampere_turns_a": [
                _exact(value) for value in target_ampere_turns
            ],
            "field_current_scaling": [
                _exact(value) for value in field_current_scales
            ],
        },
        "inductance": {
            "resolution_metric": "off-diagonal-mutual-frobenius-relative-change",
            "base_fine_coil_matrix_h": [
                [_exact(value) for value in row] for row in inductances[0]
            ],
            "confirmation_coil_matrix_h": [
                [_exact(value) for value in row] for row in inductances[1]
            ],
            "line_inductance_h": _exact(model.line_inductance),
            "confirmation_total_eigenvalues_h": [
                _exact(value) for value in np.linalg.eigvalsh(inductance)
            ],
            "confirmation_total_condition_number": _exact(condition_number),
        },
        "network": {
            "private_dump_resistance_ohm": _exact(model.private_dump),
            "shared_pair_resistance_ohm": _exact(shared),
            "differential_to_common_mode_ratio": _exact(
                candidate.differential_ratio
            ),
            "shared_branch_incidence": incidence.astype(int).tolist(),
            "switched_mesh_resistance_ohm": [
                [_exact(value) for value in row] for row in network
            ],
            "activation": "private-and-shared-branches-together-at-detection",
        },
        "metrics": {
            "minimum_magnetic_reduction_factor": _exact(
                authoritative.minimum_magnetic_reduction
            ),
            "minimum_force_reduction_factor": _exact(
                authoritative.minimum_force_reduction
            ),
            "maximum_total_i2t_a2s": _exact(authoritative.maximum_total_i2t),
            "maximum_temperature_k": _exact(authoritative.maximum_temperature),
            "maximum_absolute_current_a": _exact(
                authoritative.maximum_absolute_current
            ),
            "minimum_current_a": _exact(authoritative.minimum_current),
            "maximum_modeled_component_voltage_v": _exact(
                authoritative.maximum_component_voltage
            ),
            "maximum_final_energy_fraction": _exact(
                authoritative.maximum_final_energy_fraction
            ),
            "maximum_energy_balance_relative_error": _exact(
                authoritative.maximum_energy_balance_error
            ),
            "time_step_relative_difference": _exact(time_step_difference),
            "shape_resolution_relative_difference": _exact(
                shape_resolution_difference
            ),
            "inductance_resolution_relative_difference": _exact(
                inductance_resolution_difference
            ),
            "inductance_condition_number": _exact(condition_number),
            "geometry_length_mismatch": _exact(length_mismatch),
            "thermal_validation_energy_error": _exact(
                thermal_validation.maximum_energy_relative_error
            ),
        },
        "gate_verdicts": gate_verdicts,
        "failed_gates": failed_gates,
        "compiler_feedback": {
            "primary_cause": causal_failure,
            "observation": (
                "Each limiting failed shape metric peaks no later than the network "
                "activation event. Post-detection differential resistance cannot "
                "retroactively suppress those peaks."
                if causal_failure
                == "limiting-shape-peak-at-or-before-network-activation"
                else (
                    "The failed shape metrics have both activation-limited and "
                    "post-activation limiting cases."
                    if causal_failure == "mixed-pre-and-post-activation-shape-failure"
                    else (
                        "The limiting failed shape metrics arise after activation, "
                        "so the mesh's current redistribution does not achieve the "
                        "required field or force reduction."
                        if causal_failure == "post-activation-shape-redistribution"
                        else "No shape-mechanism failure was established."
                    )
                )
            ),
            "repair_boundary": (
                "A next candidate must change pre-detection physics or use an explicit "
                "self-acting transient-only branch; increasing the same delayed mesh "
                "ratio is not a responsive repair."
                if causal_failure
                == "limiting-shape-peak-at-or-before-network-activation"
                else (
                    "Do not increase the same mesh ratio blindly; a child must alter "
                    "activation timing or network dynamics and predict which limiting "
                    "case it removes."
                    if shape_failed
                    else "Inspect the failed gate list before defining a child attempt."
                )
            ),
        },
        "sensitivity": {
            "fault_circuits": list(model.names),
            "detection_delays_s": [_exact(value) for value in model.detection_delays],
            "resistivity_scales": [
                _exact(value) for value in model.material.resistivity_sensitivity_scales
            ],
            "heat_capacity_scales": [
                _exact(value)
                for value in model.material.heat_capacity_sensitivity_scales
            ],
            "comparisons": authoritative.comparisons,
            "at_or_before_activation_peak_cases": (
                authoritative.at_or_before_activation_peak_cases
            ),
            "coverage": "complete" if coverage_complete else "incomplete",
        },
        "limiting_shape_cases": {
            "magnetic": {
                "reduction_factor": _exact(
                    magnetic_reductions[magnetic_limit_index]
                ),
                "fault_circuit": model.names[magnetic_limit.fault],
                "detection_delay_s": _exact(magnetic_limit.delay),
                "resistivity_scale": _exact(magnetic_limit.resistance_scale),
                "heat_capacity_scale": _exact(
                    magnetic_limit.heat_capacity_scale
                ),
                "candidate_peak_time_s": _exact(
                    magnetic_limit.peak_magnetic_time
                ),
                "control_peak_time_s": _exact(
                    magnetic_control_limit.peak_magnetic_time
                ),
                "candidate_peak_departure": _exact(
                    magnetic_limit.peak_magnetic_departure
                ),
                "control_peak_departure": _exact(
                    magnetic_control_limit.peak_magnetic_departure
                ),
                "at_or_before_activation": (
                    _at_or_before_activation(
                        magnetic_limit.peak_magnetic_time, magnetic_limit.delay
                    )
                ),
            },
            "force": {
                "reduction_factor": _exact(force_reductions[force_limit_index]),
                "fault_circuit": model.names[force_limit.fault],
                "detection_delay_s": _exact(force_limit.delay),
                "resistivity_scale": _exact(force_limit.resistance_scale),
                "heat_capacity_scale": _exact(force_limit.heat_capacity_scale),
                "candidate_peak_time_s": _exact(force_limit.peak_force_time),
                "control_peak_time_s": _exact(
                    force_control_limit.peak_force_time
                ),
                "candidate_peak_departure": _exact(
                    force_limit.peak_force_departure
                ),
                "control_peak_departure": _exact(
                    force_control_limit.peak_force_departure
                ),
                "at_or_before_activation": (
                    _at_or_before_activation(
                        force_limit.peak_force_time, force_limit.delay
                    )
                ),
            },
        },
        "nominal_delay": {
            "control": [_metric_json(item, model.names) for item in nominal_controls],
            "candidate": [_metric_json(item, model.names) for item in nominal_records],
        },
        "topology_validity": "valid" if topology_valid else "invalid",
        "sweep_coverage": "complete" if coverage_complete else "incomplete",
        "interpretation": "advance" if passed else "stop",
        "limitations": [
            "The m45r00 dimensional model and SIMSOPT NCSX coils paired with a c09r00-named boundary are not proven to be the same revision; a sub-one-percent centerline-length check does not establish geometry equivalence.",
            "The fault is converter-voltage collapse, not the later NCSX strawman cases of one coil terminal short or a string held at zero current.",
            "Each winding is one adiabatic lump; local strand, joint, turn, insulation, cooling, and hotspot behavior are absent.",
            "The 2 kV screen is borrowed from a later production-design operating-voltage datum whose terminal or ground reference was not specified.",
            "The switched resistor incidence proves a passive circuit-theory mesh, not switch commutation, arcing, grounding, packaging, or failure tolerance.",
            "The m45r00 source does not establish a copper grade; NIST OFHC heat capacity, 8960 kg/m3 density, and the plus-or-minus-15-percent resistivity sensitivity are explicit modeling assumptions.",
            "The NIST heat-capacity fit's stated plus-or-minus-five-percent error is propagated as a sensitivity; the tighter reproduction gate checks internal consistency with published aggregate energy and temperature, not physical temperature accuracy.",
            "The published 5.3e8 A2s conceptual circuit-rating datum is used provisionally as a screen, not as a demonstrated fault-survival or damage threshold.",
            "Every fault starts from the selected published 0.213 s pre-high-beta state; later and worst-time-in-pulse fault onsets are not covered, so the thermal and I2t passes are conditional on that onset.",
            "The NIST zero-field resistivity curve is anchored at the published 85 K value; magnetoresistance is not resolved.",
            "The inter-coil filament-force shape surrogate omits singular self/hoop force and does not establish structural stress.",
            "Field and inter-coil filament-force shape metrics do not establish plasma survival, safety, manufacturability, novelty, patentability, or commercial value.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("field_kernel", type=Path)
    parser.add_argument("ceiling_library", type=Path)
    parser.add_argument("fault_library", type=Path)
    parser.add_argument("coil_data", type=Path)
    parser.add_argument("boundary_data", type=Path)
    parser.add_argument("base_model", type=Path)
    parser.add_argument("copper_model", type=Path)
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
            arguments.copper_model,
            arguments.candidate,
        )
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    except (OSError, CopperFaultError, ValueError, np.linalg.LinAlgError) as error:
        print(f"copper-fault gate failed: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
