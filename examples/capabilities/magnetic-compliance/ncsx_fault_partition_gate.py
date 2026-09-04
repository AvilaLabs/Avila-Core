#!/usr/bin/env python3
"""Screen fault-balanced circuit partitions on the public NCSX coil set.

This is a deliberately bounded magnetostatic experiment, not a quench model.
It partitions the 18 physical NCSX coils into three equal circuits and asks
whether losing any one circuit can disturb the magnetic and distributed-force
patterns less than a type-segregated three-circuit control.

The first search minimizes magnetic departure alone.  If that proposal misses
the fixed force-reduction gate, a second exhaustive search minimizes the worst
of the normalized magnetic and force departures.  The candidate is selected
on the reference mesh and audited, without reselection, on the fine mesh.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import itertools
import json
import math
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

import numpy as np


MODEL_SCHEMA = "avila.magnetic-compliance/ncsx-fault-partition-model/v1"
RESULT_SCHEMA = "avila.magnetic-compliance/ncsx-fault-partition-result/v1"


class PartitionError(ValueError):
    """An input or computed architecture is outside the declared screen."""


def _closed(mapping: dict[str, Any], expected: set[str], where: str) -> None:
    observed = set(mapping)
    if observed != expected:
        raise PartitionError(
            f"{where} keys differ: missing={sorted(expected-observed)}, "
            f"extra={sorted(observed-expected)}"
        )


def _number(mapping: dict[str, Any], key: str) -> float:
    try:
        value = float(mapping[key])
    except (KeyError, TypeError, ValueError) as error:
        raise PartitionError(f"{key!r} must be a finite decimal string") from error
    if not math.isfinite(value):
        raise PartitionError(f"{key!r} must be finite")
    return value


def _integer(mapping: dict[str, Any], key: str) -> int:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise PartitionError(f"{key!r} must be an integer")
    return value


def _exact(value: float, places: int = 12) -> str:
    if not math.isfinite(value):
        raise PartitionError(f"non-finite result {value!r}")
    rendered = f"{value:.{places}f}".rstrip("0").rstrip(".")
    return rendered if rendered not in {"", "-0"} else "0"


def _sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def _load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise PartitionError(f"cannot load Python module {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


@dataclass(frozen=True)
class Gate:
    minimum_magnetic_reduction: float
    minimum_force_reduction: float
    maximum_peak_force_ratio: float
    maximum_trajectory_overshoot_ratio: float
    maximum_resolution_relative_difference: float
    maximum_fault_group_relative_spread: float


@dataclass(frozen=True)
class Model:
    model_id: str
    base_model_sha256: str
    objective_rounding_places: int
    trajectory_samples: int
    gate: Gate


def load_model(raw: bytes) -> Model:
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise PartitionError(f"fault-partition model is not JSON: {error}") from error
    if not isinstance(data, dict):
        raise PartitionError("fault-partition model root must be an object")
    _closed(
        data,
        {"schema", "model_id", "base_model_sha256", "architecture", "search", "gate"},
        "model",
    )
    if data["schema"] != MODEL_SCHEMA:
        raise PartitionError(f"unsupported model schema {data['schema']!r}")
    if not isinstance(data["model_id"], str) or not data["model_id"]:
        raise PartitionError("model_id must be a nonempty string")
    digest = data["base_model_sha256"]
    if not isinstance(digest, str) or not digest.startswith("sha256:") or len(digest) != 71:
        raise PartitionError("base_model_sha256 must be a sha256 identity")

    architecture = data["architecture"]
    if not isinstance(architecture, dict):
        raise PartitionError("architecture must be an object")
    _closed(
        architecture,
        {
            "circuit_count",
            "coils_per_circuit",
            "copies_per_type_per_circuit",
            "comparator",
            "candidate_space",
            "fault_state",
            "nominal_current_realization",
        },
        "architecture",
    )
    fixed_architecture = {
        "circuit_count": 3,
        "coils_per_circuit": 6,
        "copies_per_type_per_circuit": 2,
        "comparator": "one-coil-type-per-circuit",
        "candidate_space": "all-balanced-pair-partitions-up-to-circuit-label",
        "fault_state": (
            "complete-current-loss-in-one-circuit-with-two-circuits-at-nominal-current"
        ),
        "nominal_current_realization": "turn-count-scaled-common-circuit-current",
    }
    if architecture != fixed_architecture:
        raise PartitionError("architecture differs from the supported experiment")

    search = data["search"]
    if not isinstance(search, dict):
        raise PartitionError("search must be an object")
    _closed(
        search,
        {
            "first_objective",
            "repair_objective",
            "tie_break",
            "objective_rounding_places",
            "trajectory_samples",
        },
        "search",
    )
    if search["first_objective"] != "minimum-worst-magnetic-shape-departure":
        raise PartitionError("unsupported first objective")
    if search["repair_objective"] != (
        "minimum-worst-normalized-magnetic-or-force-departure"
    ):
        raise PartitionError("unsupported repair objective")
    if search["tie_break"] != "rounded-objectives-then-lexicographic-coil-indices":
        raise PartitionError("unsupported tie break")
    rounding = _integer(search, "objective_rounding_places")
    samples = _integer(search, "trajectory_samples")
    if not 9 <= rounding <= 14 or samples < 3 or samples % 2 == 0:
        raise PartitionError("search precision or trajectory grid is outside the supported box")

    gate_data = data["gate"]
    if not isinstance(gate_data, dict):
        raise PartitionError("gate must be an object")
    _closed(
        gate_data,
        {
            "minimum_magnetic_reduction",
            "minimum_force_reduction",
            "maximum_peak_force_ratio",
            "maximum_trajectory_overshoot_ratio",
            "maximum_resolution_relative_difference",
            "maximum_fault_group_relative_spread",
        },
        "gate",
    )
    gate = Gate(**{key: _number(gate_data, key) for key in gate_data})
    if min(gate.__dict__.values()) <= 0:
        raise PartitionError("gate values must be positive")
    if gate.minimum_magnetic_reduction <= 1 or gate.minimum_force_reduction <= 1:
        raise PartitionError("reduction gates must require improvement")
    if gate.maximum_peak_force_ratio > 1 or gate.maximum_trajectory_overshoot_ratio < 1:
        raise PartitionError("force and trajectory bounds are internally inconsistent")
    return Model(data["model_id"], digest, rounding, samples, gate)


@dataclass(frozen=True)
class GroupMetric:
    magnetic_departure: float
    force_departure: float
    post_fault_normal_fraction: float
    endpoint_peak_force_ratio: float
    current_share: float


@dataclass(frozen=True)
class Analysis:
    currents: np.ndarray
    weighted_normal_fields: np.ndarray
    total_weighted_normal: np.ndarray
    pair_force_density: np.ndarray
    nominal_force_density: np.ndarray
    force_weights: np.ndarray
    nominal_force_rms: float
    nominal_peak_force: float
    base_normal_fraction: float


def pair_partitions(values: tuple[int, ...] = tuple(range(6))) -> list[tuple[tuple[int, int], ...]]:
    """Return the 15 unlabeled perfect matchings of six copy positions."""

    result: list[tuple[tuple[int, int], ...]] = []

    def visit(left: tuple[int, ...], chosen: tuple[tuple[int, int], ...]) -> None:
        if not left:
            result.append(tuple(sorted(chosen)))
            return
        first = left[0]
        for index in range(1, len(left)):
            visit(
                left[1:index] + left[index + 1 :],
                chosen + ((first, left[index]),),
            )

    visit(values, ())
    return sorted(set(result))


def physical_index(copy_index: int, type_index: int) -> int:
    return 3 * copy_index + type_index


def group_from_pairs(pairs: tuple[tuple[int, int], ...]) -> tuple[int, ...]:
    if len(pairs) != 3:
        raise PartitionError("a group needs one copy-pair for each of three coil types")
    return tuple(
        sorted(
            physical_index(copy_index, type_index)
            for type_index, pair in enumerate(pairs)
            for copy_index in pair
        )
    )


def validate_architecture(groups: Iterable[Iterable[int]]) -> tuple[tuple[int, ...], ...]:
    normalized = tuple(sorted(tuple(sorted(group)) for group in groups))
    if len(normalized) != 3 or any(len(group) != 6 for group in normalized):
        raise PartitionError("an architecture must contain three groups of six coils")
    if sorted(index for group in normalized for index in group) != list(range(18)):
        raise PartitionError("architecture groups must partition physical coil indices 0..17")
    for group in normalized:
        counts = [sum(index % 3 == type_index for index in group) for type_index in range(3)]
        if counts != [2, 2, 2]:
            raise PartitionError("each candidate circuit must contain two coils of every type")
    return normalized


def analyze(
    ceiling: Any,
    kernel: Any,
    coil_path: Path,
    boundary: np.ndarray,
    base_model: Any,
    resolution: Any,
) -> Analysis:
    coils, currents = ceiling.physical_coils(coil_path, base_model, resolution.coil_segments)
    if len(coils) != 18 or base_model.base_coil_count != 3:
        raise PartitionError("this screen requires 18 physical coils from three base types")
    points, normals, weights = ceiling.surface(boundary, base_model, resolution)
    fields = np.stack(
        [
            kernel.field_from_coil(coil, points, float(current))
            for coil, current in zip(coils, currents, strict=True)
        ]
    )
    total_field = np.sum(fields, axis=0)
    field_scale = math.sqrt(
        float(np.sum(weights * np.einsum("ij,ij->i", total_field, total_field)))
    )
    if field_scale <= 0:
        raise PartitionError("coil set produces no reference magnetic field")
    normal_fields = np.einsum("ipk,pk->ip", fields, normals)
    weighted_normal_fields = normal_fields * np.sqrt(weights)[None, :] / field_scale
    total_weighted_normal = np.sum(weighted_normal_fields, axis=0)

    count = len(coils)
    segments = resolution.coil_segments
    pair_force = np.zeros((count, count, segments, 3))
    segment_lengths = np.zeros((count, segments))
    for coil_index, coil in enumerate(coils):
        midpoints, line_elements = kernel.segment_geometry(coil)
        lengths = np.linalg.norm(line_elements, axis=1)
        if np.any(lengths <= 0):
            raise PartitionError("coil discretization contains a zero-length segment")
        segment_lengths[coil_index] = lengths
        tangents = line_elements / lengths[:, None]
        for other_index, other in enumerate(coils):
            if other_index == coil_index:
                continue
            other_field = kernel.field_from_coil(
                other, midpoints, float(currents[other_index])
            )
            pair_force[coil_index, other_index] = float(currents[coil_index]) * np.cross(
                tangents, other_field
            )
    nominal_force = np.sum(pair_force, axis=1)
    force_weights = segment_lengths / np.sum(segment_lengths)
    nominal_force_rms = math.sqrt(
        float(np.sum(force_weights[:, :, None] * nominal_force**2))
    )
    nominal_peak_force = float(np.max(np.linalg.norm(nominal_force, axis=2)))
    if nominal_force_rms <= 0 or nominal_peak_force <= 0:
        raise PartitionError("coil set produces no inter-coil force reference")
    return Analysis(
        currents=currents,
        weighted_normal_fields=weighted_normal_fields,
        total_weighted_normal=total_weighted_normal,
        pair_force_density=pair_force,
        nominal_force_density=nominal_force,
        force_weights=force_weights,
        nominal_force_rms=nominal_force_rms,
        nominal_peak_force=nominal_peak_force,
        base_normal_fraction=float(np.linalg.norm(total_weighted_normal)),
    )


def state_metric(analysis: Analysis, group: tuple[int, ...], loss_fraction: float) -> GroupMetric:
    if not 0 <= loss_fraction <= 1:
        raise PartitionError("loss fraction must lie in [0, 1]")
    group_indices = np.asarray(group, dtype=int)
    mask = np.zeros(18)
    mask[group_indices] = 1.0
    current_share = float(
        np.sum(np.abs(analysis.currents[group_indices]))
        / np.sum(np.abs(analysis.currents))
    )
    mean_fraction = 1.0 - loss_fraction * current_share
    lost_normal = loss_fraction * np.sum(
        analysis.weighted_normal_fields[group_indices], axis=0
    )
    magnetic_departure = float(
        np.linalg.norm(lost_normal - loss_fraction * current_share * analysis.total_weighted_normal)
    )
    state = 1.0 - loss_fraction * mask
    force = state[:, None, None] * np.einsum(
        "ijsl,j->isl", analysis.pair_force_density, state
    )
    expected_force = mean_fraction**2 * analysis.nominal_force_density
    force_departure = math.sqrt(
        float(np.sum(analysis.force_weights[:, :, None] * (force - expected_force) ** 2))
    ) / analysis.nominal_force_rms
    peak_force_ratio = (
        float(np.max(np.linalg.norm(force, axis=2))) / analysis.nominal_peak_force
    )
    remaining_normal = analysis.total_weighted_normal - lost_normal
    post_fault_normal_fraction = float(np.linalg.norm(remaining_normal)) / mean_fraction
    return GroupMetric(
        magnetic_departure,
        force_departure,
        post_fault_normal_fraction,
        peak_force_ratio,
        current_share,
    )


def architecture_metrics(
    analysis: Analysis, groups: tuple[tuple[int, ...], ...]
) -> tuple[list[GroupMetric], float, float]:
    metrics = [state_metric(analysis, group, 1.0) for group in groups]
    return (
        metrics,
        max(metric.magnetic_departure for metric in metrics),
        max(metric.force_departure for metric in metrics),
    )


@dataclass(frozen=True)
class Selection:
    first_groups: tuple[tuple[int, ...], ...]
    final_groups: tuple[tuple[int, ...], ...]
    architectures_evaluated: int


def select_architectures(analysis: Analysis, places: int) -> Selection:
    comparator = tuple(
        tuple(physical_index(copy_index, type_index) for copy_index in range(6))
        for type_index in range(3)
    )
    _, comparator_magnetic, comparator_force = architecture_metrics(analysis, comparator)

    partitions = pair_partitions()
    labeled = [ordering for partition in partitions for ordering in itertools.permutations(partition)]
    cache: dict[
        tuple[tuple[int, int], ...], tuple[GroupMetric, tuple[int, ...]]
    ] = {}
    for type_0_pair in itertools.combinations(range(6), 2):
        for type_1_pair in itertools.combinations(range(6), 2):
            for type_2_pair in itertools.combinations(range(6), 2):
                pairs = (type_0_pair, type_1_pair, type_2_pair)
                group = group_from_pairs(pairs)
                cache[pairs] = (state_metric(analysis, group, 1.0), group)

    best_first: tuple[Any, ...] | None = None
    best_final: tuple[Any, ...] | None = None
    evaluated = 0
    for type_0 in partitions:
        for type_1 in labeled:
            for type_2 in labeled:
                metrics: list[GroupMetric] = []
                groups: list[tuple[int, ...]] = []
                for circuit_index in range(3):
                    key = tuple(
                        tuple(sorted(partition[circuit_index]))
                        for partition in (type_0, type_1, type_2)
                    )
                    metric, group = cache[key]
                    metrics.append(metric)
                    groups.append(group)
                normalized_groups = tuple(sorted(groups))
                worst_magnetic = max(metric.magnetic_departure for metric in metrics)
                worst_force = max(metric.force_departure for metric in metrics)
                magnetic_ratio = worst_magnetic / comparator_magnetic
                force_ratio = worst_force / comparator_force
                minimax = max(magnetic_ratio, force_ratio)
                first_key = (
                    round(worst_magnetic, places),
                    round(worst_force, places),
                    normalized_groups,
                )
                final_key = (
                    round(minimax, places),
                    round(magnetic_ratio, places),
                    round(force_ratio, places),
                    normalized_groups,
                )
                if best_first is None or first_key < best_first:
                    best_first = first_key
                if best_final is None or final_key < best_final:
                    best_final = final_key
                evaluated += 1
    if best_first is None or best_final is None:
        raise PartitionError("candidate search produced no architecture")
    return Selection(
        validate_architecture(best_first[-1]),
        validate_architecture(best_final[-1]),
        evaluated,
    )


def relative_spread(values: Iterable[float]) -> float:
    sequence = tuple(values)
    mean = float(np.mean(sequence))
    if mean <= 0:
        raise PartitionError("relative spread requires positive values")
    return (max(sequence) - min(sequence)) / mean


def trajectory_audit(
    analysis: Analysis,
    groups: tuple[tuple[int, ...], ...],
    samples: int,
) -> tuple[float, float, float]:
    endpoints, endpoint_magnetic, endpoint_force = architecture_metrics(analysis, groups)
    worst_magnetic = 0.0
    worst_force = 0.0
    worst_peak = 0.0
    for loss_fraction in np.linspace(0.0, 1.0, samples):
        for group in groups:
            metric = state_metric(analysis, group, float(loss_fraction))
            worst_magnetic = max(worst_magnetic, metric.magnetic_departure)
            worst_force = max(worst_force, metric.force_departure)
            worst_peak = max(worst_peak, metric.endpoint_peak_force_ratio)
    if endpoint_magnetic <= 0 or endpoint_force <= 0 or not endpoints:
        raise PartitionError("trajectory endpoint is degenerate")
    return (
        worst_magnetic / endpoint_magnetic,
        worst_force / endpoint_force,
        worst_peak,
    )


def evaluate(
    kernel_path: Path,
    ceiling_path: Path,
    coil_path: Path,
    boundary_path: Path,
    base_model_path: Path,
    partition_model_path: Path,
) -> dict[str, Any]:
    model = load_model(partition_model_path.read_bytes())
    if _sha256(base_model_path) != model.base_model_sha256:
        raise PartitionError("base model digest does not match fault-partition model")
    kernel = _load_module("ncsx_fault_field_kernel", kernel_path)
    ceiling = _load_module("ncsx_fault_ceiling", ceiling_path)
    base_model = ceiling.load_model(base_model_path.read_bytes())
    boundary = ceiling.boundary_coefficients(boundary_path, base_model)

    reference = analyze(
        ceiling, kernel, coil_path, boundary, base_model, base_model.reference
    )
    selection = select_architectures(reference, model.objective_rounding_places)
    comparator = tuple(
        tuple(physical_index(copy_index, type_index) for copy_index in range(6))
        for type_index in range(3)
    )
    comparator_reference, baseline_magnetic_ref, baseline_force_ref = architecture_metrics(
        reference, comparator
    )
    first_reference, first_magnetic_ref, first_force_ref = architecture_metrics(
        reference, selection.first_groups
    )
    final_reference, final_magnetic_ref, final_force_ref = architecture_metrics(
        reference, selection.final_groups
    )
    first_magnetic_reduction = baseline_magnetic_ref / first_magnetic_ref
    first_force_reduction = baseline_force_ref / first_force_ref

    fine = analyze(ceiling, kernel, coil_path, boundary, base_model, base_model.fine)
    comparator_fine, baseline_magnetic_fine, baseline_force_fine = architecture_metrics(
        fine, comparator
    )
    final_fine, final_magnetic_fine, final_force_fine = architecture_metrics(
        fine, selection.final_groups
    )
    magnetic_reduction_ref = baseline_magnetic_ref / final_magnetic_ref
    force_reduction_ref = baseline_force_ref / final_force_ref
    magnetic_reduction = baseline_magnetic_fine / final_magnetic_fine
    force_reduction = baseline_force_fine / final_force_fine
    magnetic_resolution_difference = abs(magnetic_reduction - magnetic_reduction_ref) / magnetic_reduction
    force_resolution_difference = abs(force_reduction - force_reduction_ref) / force_reduction
    resolution_difference = max(
        magnetic_resolution_difference, force_resolution_difference
    )
    group_spread = max(
        relative_spread(metric.magnetic_departure for metric in final_fine),
        relative_spread(metric.force_departure for metric in final_fine),
    )
    magnetic_overshoot, force_overshoot, peak_force_ratio = trajectory_audit(
        fine, selection.final_groups, model.trajectory_samples
    )
    trajectory_overshoot = max(magnetic_overshoot, force_overshoot)

    first_interpretation = (
        "candidate-justified"
        if first_magnetic_reduction + 1.0e-12 >= model.gate.minimum_magnetic_reduction
        and first_force_reduction + 1.0e-12 >= model.gate.minimum_force_reduction
        else "revise"
    )
    passed = (
        magnetic_reduction + 1.0e-12 >= model.gate.minimum_magnetic_reduction
        and force_reduction + 1.0e-12 >= model.gate.minimum_force_reduction
        and peak_force_ratio <= model.gate.maximum_peak_force_ratio + 1.0e-12
        and trajectory_overshoot
        <= model.gate.maximum_trajectory_overshoot_ratio + 1.0e-12
        and resolution_difference
        <= model.gate.maximum_resolution_relative_difference + 1.0e-12
        and group_spread <= model.gate.maximum_fault_group_relative_spread + 1.0e-12
    )

    def metric_json(metric: GroupMetric) -> dict[str, str]:
        return {
            "magnetic_shape_departure": _exact(metric.magnetic_departure),
            "force_shape_departure": _exact(metric.force_departure),
            "post_fault_normal_fraction": _exact(metric.post_fault_normal_fraction),
            "endpoint_peak_force_ratio": _exact(metric.endpoint_peak_force_ratio),
            "current_share": _exact(metric.current_share),
        }

    return {
        "schema": RESULT_SCHEMA,
        "model_id": model.model_id,
        "source_identity": {
            "coil_sha256": _sha256(coil_path),
            "boundary_sha256": _sha256(boundary_path),
            "base_model_sha256": _sha256(base_model_path),
            "partition_model_sha256": _sha256(partition_model_path),
            "field_kernel_sha256": _sha256(kernel_path),
            "ceiling_library_sha256": _sha256(ceiling_path),
        },
        "search": {
            "architectures_evaluated": selection.architectures_evaluated,
            "first_candidate_groups": [list(group) for group in selection.first_groups],
            "final_candidate_groups": [list(group) for group in selection.final_groups],
            "first_candidate_reference": [metric_json(metric) for metric in first_reference],
            "final_candidate_reference": [metric_json(metric) for metric in final_reference],
        },
        "metrics": {
            "first_magnetic_reduction_factor": _exact(first_magnetic_reduction),
            "first_force_reduction_factor": _exact(first_force_reduction),
            "fine_magnetic_reduction_factor": _exact(magnetic_reduction),
            "fine_force_reduction_factor": _exact(force_reduction),
            "fine_peak_force_ratio": _exact(peak_force_ratio),
            "trajectory_overshoot_ratio": _exact(trajectory_overshoot),
            "resolution_relative_difference": _exact(resolution_difference),
            "fault_group_relative_spread": _exact(group_spread),
            "fine_candidate_post_fault_normal_fraction": _exact(
                max(metric.post_fault_normal_fraction for metric in final_fine)
            ),
            "fine_nominal_normal_fraction": _exact(fine.base_normal_fraction),
            "fine_nominal_force_rms_n_per_m": _exact(fine.nominal_force_rms),
            "fine_nominal_peak_force_n_per_m": _exact(fine.nominal_peak_force),
        },
        "fine": {
            "comparator": [metric_json(metric) for metric in comparator_fine],
            "candidate": [metric_json(metric) for metric in final_fine],
        },
        "iteration_1_interpretation": first_interpretation,
        "partition_validity": "valid",
        "interpretation": "candidate-justified" if passed else "stop",
        "limitations": [
            "Magnetostatic filament and force-density proxies omit inductance, resistance, voltage, thermal propagation, hot spots, eddy currents, structures, stress, and plasma dynamics.",
            "The NCSX geometry is a public numerical proxy; the candidate is a new-design winding partition, not a retrofit prescription or a statement of the historical NCSX circuit topology.",
            "Different base-coil ampere-turns are represented by turn-count scaling at a common circuit current; winding packs and integer turn counts are not designed.",
            "The thresholds were fixed during an interactive reference-grid exploration, not preregistered before all reference results were observed; the fine mesh is only a numerical holdout.",
            "A passing result justifies a coupled circuit/quench model, not hardware, safety, novelty, patentability, or commercial claims.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("field_kernel", type=Path)
    parser.add_argument("ceiling_library", type=Path)
    parser.add_argument("coil_data", type=Path)
    parser.add_argument("boundary_data", type=Path)
    parser.add_argument("base_model", type=Path)
    parser.add_argument("partition_model", type=Path)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    try:
        result = evaluate(
            arguments.field_kernel,
            arguments.ceiling_library,
            arguments.coil_data,
            arguments.boundary_data,
            arguments.base_model,
            arguments.partition_model,
        )
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    except (OSError, PartitionError, ValueError, np.linalg.LinAlgError) as error:
        print(f"fault-partition gate failed: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
