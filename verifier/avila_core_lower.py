#!/usr/bin/env python3
"""Independent lowering of a contract+registry into a compiled snapshot body.

This is the second-implementation seed: it re-derives ``snapshot_sha256``
for a compiled contract without importing or linking any Core code. The
rules are taken from the documented semantic profile and cross-checked
against every ``compiled`` fixture's pinned ``snapshot_sha256`` in
fixtures/semantic-core/types/ (test_verifier.py executes all of them) and
against every committed example-case compilation.

The port covers the *successful lowering path* — the work that decides what
lands inside the compiled body (candidate collection, slot resolution,
topological order, parameter/material-factor lowering, determinism and
seed rules, presentation-gate resolution, requirement lowering into
canonical units, categorical requirements). Every check that would produce
a blocking finding in the compiler is also ported, because a contract that
would be rejected has no legitimate snapshot at all:

- ``WouldReject`` — the port reached a check the compiler reports as a
  blocking finding; any committed snapshot for this pair is bogus.
- ``CannotLower`` — the port hit a construct outside its covered subset
  (for example a registry whose own validation findings this port does not
  reimplement). The verifier reports this as ``not_checked``, never a
  verdict.

What is deliberately not ported: the finding *vocabulary* itself (codes,
pointers, repair candidates — none of it lands in a compiled body), the
non-blocking notices (CORE-A4404/R3601/R3602 — they carry no weight in the
snapshot), and the embedded JSON-Schema validation of role ``input_schema``
values (a shape check; a schema that would fail there means no committed
snapshot to compare anyway).
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass, field
from fractions import Fraction
from typing import Any, Optional

from avila_core_verify import (
    CanonError,
    canonicalize_json,
    kinds_from_registry_doc,
    read_authoritative_exact,
    render_canonical,
)

SEMANTIC_PROFILE = "avila.core/semantic/0.2-draft"
COMPILED_SCHEMA_VERSION = "avila.core/compiled-contract/v0.2-draft"
COMPILER_ID = "avila.core/compiler-rust@0.1.0"
NOT_DEFINED_PLACEHOLDER = "not_defined"
MAX_WORKFLOW_STEPS = 2048


class CannotLower(Exception):
    """The construct is outside this port's covered subset."""


class WouldReject(Exception):
    """A ported check produced a blocking finding: no legitimate snapshot."""


# ---- document shape (deny_unknown_fields + required fields + enums) ---------
#
# The compiler's typed deserialisation refuses an object carrying a key its
# schema does not declare, a missing required field, or an out-of-vocabulary
# enum. These tables port that refusal so a malformed pair raises
# ``WouldReject`` rather than being silently lowered.

_CONTRACT_KEYS = {
    "schema_version", "semantic_profile", "contract_id", "revision", "status",
    "question", "assumptions", "execution_policy", "inputs", "workflow",
    "requirements", "categorical_requirements",
}
_POLICY_KEYS = {
    "permitted_nondeterministic_roles", "permit_nominal_basis",
    "require_qualification", "require_signatures",
}
_INPUT_KEYS = {"input_id", "role", "media_type", "claim_model"}
_STEP_KEYS = {"step_id", "capability_type", "bindings", "parameters",
              "reproducibility", "review"}
_BINDING_KEYS = {"input_slot", "source"}
_REPRO_BIND_KEYS = {"seed", "material_factors"}
_REVIEW_BIND_KEYS = {"reviewer_eligibility_policy", "independence", "instructions"}
_POLICY_REF_KEYS = {"policy_id", "revision", "sha256"}
_INDEPENDENCE_KEYS = {"mode", "requirements"}
_INDEPENDENCE_REQ_KEYS = {"separated_from", "minimum_separation"}
_REQUIREMENT_KEYS = {"requirement_id", "statement", "purpose", "metric",
                     "comparison", "limit", "tolerance", "basis"}
_CAT_REQUIREMENT_KEYS = {"requirement_id", "statement", "purpose", "metric",
                         "predicate"}
_BASIS_KEYS = {"kind", "coverage"}
_QUANTITY_KEYS = {"kind", "value", "unit"}
_VERSIONED_REF_KEYS = {"id", "major"}
_SOURCE_KEYS = {"source", "input_id", "step_id", "output_slot"}
_PREDICATE_KEYS = {"operator", "value", "values"}
_CLAIM_MODEL_KEYS = {"model", "nominal", "side"}

_REGISTRY_KEYS = {"schema_version", "semantic_profile", "registry_id",
                  "revision", "kinds", "purposes", "roles", "capability_types"}
_KIND_KEYS = {"kind_id", "canonical_unit", "unit_class", "owner", "units"}
_UNIT_KEYS = {"symbol", "factor"}
_PURPOSE_KEYS = {"purpose", "owner", "description"}
_ROLE_KEYS = {"role", "owner", "validator", "input_schema", "quantity_kind",
              "unit_class", "accepted_media_types", "permitted_claim_models",
              "categorical_values", "non_claims"}
_CAPABILITY_KEYS = {"capability_type", "owner", "reproducibility", "inputs",
                    "outputs", "parameters", "review", "non_claims"}
_REPRO_DECL_KEYS = {"determinism", "material_factors"}
_FACTOR_KEYS = {"factor_id", "value_type"}
_SLOT_IN_KEYS = {"slot_id", "role", "accepted_media_types", "required"}
_SLOT_OUT_KEYS = {"slot_id", "role", "media_type", "permitted_claim_models",
                  "excluded_purposes"}
_PARAM_KEYS = {"parameter_id", "required", "value_type"}
_PARAM_TYPE_KEYS = {"type", "min", "max", "allowed_values", "kind"}
_BOUND_KEYS = {"value", "inclusive"}
_QUANTITY_VALUE_KEYS = {"value", "unit"}
_REVIEW_DECL_KEYS = {"reviewer_role", "presented_input_slots",
                    "decision_output_slot", "allowed_dispositions"}

_ENUMS = {
    "status": {"draft", "in_review", "approved", "retired"},
    "comparison": {"less_than", "less_than_or_equal", "greater_than",
                   "greater_than_or_equal", "equal"},
    "basis_kind": {"bounded", "enclosure", "nominal"},
    "determinism": {"deterministic", "seeded_stochastic", "nondeterministic"},
    "source_tag": {"contract_input", "step_output"},
    "predicate_operator": {"equals", "in_set"},
    "claim_model": {"exact", "interval", "coverage_interval", "worst_case",
                    "standard_uncertainty", "samples", "distribution",
                    "unquantified"},
    "bound_side": {"lower", "upper"},
    "parameter_type": {"boolean", "integer", "exact_number", "text", "quantity"},
    "independence_mode": {"none", "constraints"},
    "review_party": {"requester", "method_owner", "capability_provider",
                     "executor"},
    "separation_level": {"different_person", "different_organization"},
    "reviewer_role": {"agent"},
    "review_disposition": {"present_to_user", "request_changes", "abstain"},
}


def _shape(obj: Any, keys: set, where: str) -> None:
    if not isinstance(obj, dict):
        raise WouldReject(f"{where} is not an object")
    unknown = set(obj) - keys
    if unknown:
        raise WouldReject(f"{where} carries undeclared field(s) {sorted(unknown)}")


def _enum(value: Any, vocabulary: str, where: str) -> None:
    if value not in _ENUMS[vocabulary]:
        raise WouldReject(f"{where} value {value!r} is outside its vocabulary")


def _check_versioned_ref(ref: Any, where: str) -> None:
    _shape(ref, _VERSIONED_REF_KEYS, where)
    if not isinstance(ref.get("id"), str) or not ref["id"]:
        raise WouldReject(f"{where} id is missing")
    if not isinstance(ref.get("major"), int) or ref["major"] < 1:
        raise WouldReject(f"{where} major is missing or invalid")


def _check_claim_model(model: Any, where: str) -> None:
    _shape(model, _CLAIM_MODEL_KEYS, where)
    _enum(model.get("model"), "claim_model", where)


def _check_source_ref(source: Any, where: str) -> None:
    _shape(source, _SOURCE_KEYS, where)
    tag = source.get("source")
    _enum(tag, "source_tag", where)
    if tag == "contract_input" and "input_id" not in source:
        raise WouldReject(f"{where} contract_input lacks input_id")
    if tag == "step_output" and ("step_id" not in source or "output_slot" not in source):
        raise WouldReject(f"{where} step_output lacks step_id/output_slot")


def _check_quantity(quantity: Any, where: str) -> None:
    _shape(quantity, _QUANTITY_KEYS, where)


def _check_parameter_type(value_type: Any, where: str) -> None:
    _shape(value_type, _PARAM_TYPE_KEYS, where)
    _enum(value_type.get("type"), "parameter_type", where)
    for bound_key in ("min", "max"):
        bound = value_type.get(bound_key)
        if bound is None:
            continue
        _shape(bound, _BOUND_KEYS, f"{where}/{bound_key}")
        if value_type["type"] == "quantity":
            _shape(bound.get("value"), _QUANTITY_VALUE_KEYS,
                   f"{where}/{bound_key}/value")


def _validate_shape(contract: dict, registry: dict) -> None:
    _shape(contract, _CONTRACT_KEYS, "contract")
    for required in ("schema_version", "semantic_profile", "contract_id",
                     "revision", "status", "question", "workflow"):
        if required not in contract:
            raise WouldReject(f"contract lacks required field `{required}`")
    _enum(contract["status"], "status", "contract/status")
    _shape(contract.get("execution_policy", {}), _POLICY_KEYS,
           "contract/execution_policy")
    for index, item in enumerate(contract.get("inputs", [])):
        where = f"contract/inputs/{index}"
        _shape(item, _INPUT_KEYS, where)
        for required in _INPUT_KEYS:
            if required not in item:
                raise WouldReject(f"{where} lacks required field `{required}`")
        _check_versioned_ref(item["role"], f"{where}/role")
        _check_claim_model(item["claim_model"], f"{where}/claim_model")
    for index, step in enumerate(contract["workflow"]):
        where = f"contract/workflow/{index}"
        _shape(step, _STEP_KEYS, where)
        if "step_id" not in step or "capability_type" not in step:
            raise WouldReject(f"{where} lacks step_id/capability_type")
        _check_versioned_ref(step["capability_type"],
                             f"{where}/capability_type")
        for b_index, binding in enumerate(step.get("bindings", [])):
            b_where = f"{where}/bindings/{b_index}"
            _shape(binding, _BINDING_KEYS, b_where)
            if "input_slot" not in binding or "source" not in binding:
                raise WouldReject(f"{b_where} lacks input_slot/source")
            _check_source_ref(binding["source"], f"{b_where}/source")
        _shape(step.get("reproducibility", {}), _REPRO_BIND_KEYS,
               f"{where}/reproducibility")
        review = step.get("review")
        if review is not None:
            _shape(review, _REVIEW_BIND_KEYS, f"{where}/review")
            _shape(review.get("reviewer_eligibility_policy", {}),
                   _POLICY_REF_KEYS, f"{where}/review/reviewer_eligibility_policy")
            independence = review.get("independence", {})
            _shape(independence, _INDEPENDENCE_KEYS, f"{where}/review/independence")
            _enum(independence.get("mode"), "independence_mode",
                  f"{where}/review/independence/mode")
            for r_index, req in enumerate(independence.get("requirements", [])):
                r_where = f"{where}/review/independence/requirements/{r_index}"
                _shape(req, _INDEPENDENCE_REQ_KEYS, r_where)
                _enum(req.get("separated_from"), "review_party",
                      f"{r_where}/separated_from")
                _enum(req.get("minimum_separation"), "separation_level",
                      f"{r_where}/minimum_separation")
    for index, req in enumerate(contract.get("requirements", [])):
        where = f"contract/requirements/{index}"
        _shape(req, _REQUIREMENT_KEYS, where)
        for required in ("requirement_id", "statement", "purpose",
                         "comparison", "limit", "basis"):
            if required not in req:
                raise WouldReject(f"{where} lacks required field `{required}`")
        _check_versioned_ref(req["purpose"], f"{where}/purpose")
        if "metric" in req:
            _check_source_ref(req["metric"], f"{where}/metric")
        _enum(req["comparison"], "comparison", f"{where}/comparison")
        _check_quantity(req["limit"], f"{where}/limit")
        if "tolerance" in req:
            _check_quantity(req["tolerance"], f"{where}/tolerance")
        _shape(req["basis"], _BASIS_KEYS, f"{where}/basis")
        _enum(req["basis"].get("kind"), "basis_kind", f"{where}/basis/kind")
    for index, req in enumerate(contract.get("categorical_requirements", [])):
        where = f"contract/categorical_requirements/{index}"
        _shape(req, _CAT_REQUIREMENT_KEYS, where)
        for required in ("requirement_id", "statement", "purpose", "predicate"):
            if required not in req:
                raise WouldReject(f"{where} lacks required field `{required}`")
        _check_versioned_ref(req["purpose"], f"{where}/purpose")
        if "metric" in req:
            _check_source_ref(req["metric"], f"{where}/metric")
        _shape(req["predicate"], _PREDICATE_KEYS, f"{where}/predicate")
        _enum(req["predicate"].get("operator"), "predicate_operator",
              f"{where}/predicate/operator")

    _shape(registry, _REGISTRY_KEYS, "registry")
    for required in ("schema_version", "semantic_profile", "registry_id",
                     "revision", "kinds", "purposes", "roles",
                     "capability_types"):
        if required not in registry:
            raise WouldReject(f"registry lacks required field `{required}`")
    for index, kind in enumerate(registry["kinds"]):
        where = f"registry/kinds/{index}"
        _shape(kind, _KIND_KEYS, where)
        for u_index, unit in enumerate(kind.get("units", [])):
            _shape(unit, _UNIT_KEYS, f"{where}/units/{u_index}")
    for index, purpose in enumerate(registry["purposes"]):
        _shape(purpose, _PURPOSE_KEYS, f"registry/purposes/{index}")
    for index, role in enumerate(registry["roles"]):
        where = f"registry/roles/{index}"
        _shape(role, _ROLE_KEYS, where)
        for required in ("role", "owner", "validator", "accepted_media_types",
                         "permitted_claim_models"):
            if required not in role:
                raise WouldReject(f"{where} lacks required field `{required}`")
        _check_versioned_ref(role["role"], f"{where}/role")
        for m_index, model in enumerate(role["permitted_claim_models"]):
            _check_claim_model(model, f"{where}/permitted_claim_models/{m_index}")
    for index, capability in enumerate(registry["capability_types"]):
        where = f"registry/capability_types/{index}"
        _shape(capability, _CAPABILITY_KEYS, where)
        for required in ("capability_type", "owner", "reproducibility"):
            if required not in capability:
                raise WouldReject(f"{where} lacks required field `{required}`")
        _check_versioned_ref(capability["capability_type"],
                             f"{where}/capability_type")
        repro = capability["reproducibility"]
        _shape(repro, _REPRO_DECL_KEYS, f"{where}/reproducibility")
        _enum(repro.get("determinism"), "determinism",
              f"{where}/reproducibility/determinism")
        for f_index, factor in enumerate(repro.get("material_factors", [])):
            f_where = f"{where}/reproducibility/material_factors/{f_index}"
            _shape(factor, _FACTOR_KEYS, f_where)
            _check_parameter_type(factor.get("value_type", {}),
                                  f"{f_where}/value_type")
        for s_index, slot in enumerate(capability.get("inputs", [])):
            s_where = f"{where}/inputs/{s_index}"
            _shape(slot, _SLOT_IN_KEYS, s_where)
            for required in ("slot_id", "role", "accepted_media_types"):
                if required not in slot:
                    raise WouldReject(f"{s_where} lacks required field `{required}`")
            _check_versioned_ref(slot["role"], f"{s_where}/role")
        for s_index, slot in enumerate(capability.get("outputs", [])):
            s_where = f"{where}/outputs/{s_index}"
            _shape(slot, _SLOT_OUT_KEYS, s_where)
            for required in ("slot_id", "role", "media_type",
                             "permitted_claim_models"):
                if required not in slot:
                    raise WouldReject(f"{s_where} lacks required field `{required}`")
            _check_versioned_ref(slot["role"], f"{s_where}/role")
            for m_index, model in enumerate(slot["permitted_claim_models"]):
                _check_claim_model(model,
                                   f"{s_where}/permitted_claim_models/{m_index}")
            for p_index, ref in enumerate(slot.get("excluded_purposes", [])):
                _check_versioned_ref(ref, f"{s_where}/excluded_purposes/{p_index}")
        for p_index, parameter in enumerate(capability.get("parameters", [])):
            p_where = f"{where}/parameters/{p_index}"
            _shape(parameter, _PARAM_KEYS, p_where)
            _check_parameter_type(parameter.get("value_type", {}),
                                  f"{p_where}/value_type")
        review = capability.get("review")
        if review is not None:
            r_where = f"{where}/review"
            _shape(review, _REVIEW_DECL_KEYS, r_where)
            for required in ("reviewer_role", "presented_input_slots",
                             "decision_output_slot", "allowed_dispositions"):
                if required not in review:
                    raise WouldReject(f"{r_where} lacks required field `{required}`")
            _enum(review["reviewer_role"], "reviewer_role",
                  f"{r_where}/reviewer_role")
            for d_index, disposition in enumerate(review["allowed_dispositions"]):
                _enum(disposition, "review_disposition",
                      f"{r_where}/allowed_dispositions/{d_index}")


def _sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def _document_identity(raw: bytes) -> str:
    """sha256 of the document's canonical form — the compiler's
    ``source_identities`` rule (read_authoritative_json + re-serialise)."""
    return _sha256(canonicalize_json(raw))


# ---- source references ----------------------------------------------------

# SourceRef Ord: ContractInput < StepOutput (variant order), then fields.


def _source_key(source: dict) -> tuple:
    if source["source"] == "contract_input":
        return (0, source["input_id"], "")
    return (1, source["step_id"], source["output_slot"])


def _source_eq(a: dict, b: dict) -> bool:
    return _source_key(a) == _source_key(b)


def _source_label(source: dict) -> str:
    if source["source"] == "contract_input":
        return f"input:{source['input_id']}"
    return f"step:{source['step_id']}/{source['output_slot']}"


def _is_step_output_of(source: dict, step_id: str) -> bool:
    return source["source"] == "step_output" and source["step_id"] == step_id


# ---- claim models ----------------------------------------------------------


def _claim_model_satisfies(model: dict, basis: dict, comparison: str) -> bool:
    """The compiler's claim-model sufficiency table (compile/requirements.rs)."""
    kind = basis["kind"]
    m = model["model"]
    if kind == "bounded":
        if m in ("exact", "interval", "coverage_interval"):
            return True
        if m == "worst_case":
            if comparison in ("less_than", "less_than_or_equal"):
                return model["side"] == "upper"
            if comparison in ("greater_than", "greater_than_or_equal"):
                return model["side"] == "lower"
            return False
        return False
    if kind == "enclosure":
        return m in ("exact", "interval")
    # nominal
    if m in ("exact", "coverage_interval", "unquantified"):
        return True
    if m in ("interval", "worst_case"):
        return bool(model.get("nominal", False))
    return False


_KERNEL_IRREDUCIBLE = {"standard_uncertainty", "samples", "distribution"}


# ---- registry index --------------------------------------------------------


@dataclass
class RoleDef:
    ref: dict
    accepted_media_types: list
    permitted_claim_models: list
    quantity_kind: Optional[str]
    unit_class: Optional[str]
    categorical_values: list


@dataclass
class CapabilityDef:
    ref: dict
    inputs: list
    outputs: list
    parameters: list
    reproducibility: dict
    review: Optional[dict]


@dataclass
class RegistryIndex:
    doc: dict
    roles: dict = field(default_factory=dict)
    capability_types: dict = field(default_factory=dict)
    purposes: set = field(default_factory=set)
    kinds: dict = field(default_factory=dict)


def _ref_key(ref: dict) -> tuple:
    return (ref["id"], ref["major"])


def _build_index(registry: dict) -> RegistryIndex:
    index = RegistryIndex(doc=registry)
    index.kinds = kinds_from_registry_doc(registry)
    kind_class_names: dict = {}
    kind_ids: set = set()
    # Kernel invariants enforced at registry load (unit.rs): every factor
    # positive, no duplicate unit symbols, the canonical unit present with
    # factor exactly one.
    for kind_record in registry.get("kinds", []):
        kind_id = kind_record.get("kind_id", "")
        canonical_unit = kind_record.get("canonical_unit", "")
        units = kind_record.get("units", [])
        if not kind_id or not canonical_unit or not units:
            raise WouldReject(f"kind `{kind_id}` is missing its id, canonical unit, or units")
        if kind_id in kind_ids:
            raise WouldReject(f"duplicate kind `{kind_id}`")
        kind_ids.add(kind_id)
        kind_class_names[kind_id] = kind_record.get("unit_class", "")
        if not kind_record.get("owner", "").strip():
            raise WouldReject(f"kind `{kind_id}` owner must not be empty")
        symbols: set[str] = set()
        canonical_factor = None
        for unit in units:
            symbol = unit.get("symbol", "")
            if not symbol or symbol in symbols:
                raise WouldReject(f"kind `{kind_id}` declares a duplicate or empty unit")
            symbols.add(symbol)
            if read_authoritative_exact(unit.get("factor", "")) <= 0:
                raise WouldReject(f"kind `{kind_id}` declares a nonpositive unit factor")
            if symbol == canonical_unit:
                canonical_factor = read_authoritative_exact(unit["factor"])
        if canonical_factor is None:
            raise WouldReject(f"kind `{kind_id}` does not include its canonical unit")
        if canonical_factor != 1:
            raise WouldReject(f"kind `{kind_id}` canonical-unit factor is not exactly one")
    for record in registry.get("roles", []):
        role = record["role"]
        if _ref_key(role) in index.roles:
            raise WouldReject(f"duplicate evidence role `{role['id']}@{role['major']}`")
        if not record.get("owner", "").strip() or not record.get("validator", "").strip():
            raise WouldReject(f"role `{role['id']}@{role['major']}` requires a nonempty owner and validator")
        quantity_kind = record.get("quantity_kind")
        unit_class = record.get("unit_class")
        if (quantity_kind is None) != (unit_class is None):
            raise WouldReject(
                f"role `{role['id']}@{role['major']}` must declare both quantity_kind and unit_class")
        if quantity_kind is not None:
            expected_class = kind_class_names.get(quantity_kind)
            if expected_class is None:
                raise WouldReject(f"role `{role['id']}@{role['major']}` references unknown quantity kind `{quantity_kind}`")
            if expected_class != unit_class:
                raise WouldReject(
                    f"role `{role['id']}@{role['major']}` unit class `{unit_class}` does not match kind class `{expected_class}`")
        categorical = record.get("categorical_values", [])
        if categorical and (quantity_kind is not None or unit_class is not None):
            raise WouldReject("a closed categorical role must be non-quantitative")
        if categorical and record.get("permitted_claim_models") != [{"model": "unquantified"}]:
            raise WouldReject(
                "a closed categorical role must permit only the unquantified claim model")
        index.roles[_ref_key(role)] = RoleDef(
            ref=role,
            accepted_media_types=record.get("accepted_media_types", []),
            permitted_claim_models=record.get("permitted_claim_models", []),
            quantity_kind=quantity_kind,
            unit_class=unit_class,
            categorical_values=categorical,
        )
    for record in registry.get("capability_types", []):
        cap = record["capability_type"]
        if _ref_key(cap) in index.capability_types:
            raise WouldReject(f"duplicate capability type `{cap['id']}@{cap['major']}`")
        if not record.get("owner", "").strip():
            raise WouldReject(f"capability type `{cap['id']}@{cap['major']}` owner must not be empty")
        index.capability_types[_ref_key(cap)] = CapabilityDef(
            ref=cap,
            inputs=record.get("inputs", []),
            outputs=record.get("outputs", []),
            parameters=record.get("parameters", []),
            reproducibility=record.get("reproducibility", {}),
            review=record.get("review"),
        )
    for record in registry.get("purposes", []):
        key = _ref_key(record["purpose"])
        if key in index.purposes:
            raise WouldReject(f"duplicate governed purpose `{key[0]}@{key[1]}`")
        if not record.get("owner", "").strip():
            raise WouldReject(f"purpose `{key[0]}@{key[1]}` owner must not be empty")
        index.purposes.add(key)
    return index


# ---- candidates ------------------------------------------------------------


@dataclass
class Candidate:
    source: dict
    role: dict
    media_type: str
    claim_models: list
    excluded_purposes: list


def _collect_sources(contract: dict, index: RegistryIndex) -> list[Candidate]:
    candidates = [
        Candidate(
            source={"source": "contract_input", "input_id": i["input_id"]},
            role=i["role"],
            media_type=i["media_type"],
            claim_models=[i["claim_model"]],
            excluded_purposes=[],
        )
        for i in contract.get("inputs", [])
    ]
    for step in contract["workflow"]:
        capability = index.capability_types.get(_ref_key(step["capability_type"]))
        if capability is None:
            continue
        for output in capability.outputs:
            candidates.append(
                Candidate(
                    source={
                        "source": "step_output",
                        "step_id": step["step_id"],
                        "output_slot": output["slot_id"],
                    },
                    role=output["role"],
                    media_type=output["media_type"],
                    claim_models=output.get("permitted_claim_models", []),
                    excluded_purposes=output.get("excluded_purposes", []),
                )
            )
    candidates.sort(key=lambda c: _source_key(c.source))
    return candidates


def _find_candidate(candidates: list[Candidate], source: dict) -> Optional[Candidate]:
    for candidate in candidates:
        if _source_eq(candidate.source, source):
            return candidate
    return None


# ---- typed-value lowering (parameters and material factors) -----------------


def _reject(reason: str) -> None:
    raise WouldReject(reason)


def _lower_typed_value(definition: dict, authored: Any, kinds: dict) -> dict:
    """Lowers one authored parameter/factor value into its compiled tagged
    form. Any ported failure here is a blocking finding in the compiler."""
    value_type = definition["value_type"]
    ptype = value_type["type"]
    name = definition["parameter_id"]
    if ptype == "boolean":
        if isinstance(authored, bool):
            return {"type": "boolean", "value": authored}
        _reject(f"parameter `{name}` is not a boolean")
    elif ptype == "integer":
        if isinstance(authored, bool) or not isinstance(authored, int):
            _reject(f"parameter `{name}` is not an integer")
        if _outside_bounds(authored, value_type.get("min"), value_type.get("max")):
            _reject(f"parameter `{name}` is outside its declared domain")
        return {"type": "integer", "value": authored}
    elif ptype == "exact_number":
        if not isinstance(authored, str):
            _reject(f"parameter `{name}` is not an exact-number string")
        try:
            value = read_authoritative_exact(authored)
        except CanonError:
            _reject(f"parameter `{name}` is not a canonical exact number")
        if _outside_bounds(value, value_type.get("min"), value_type.get("max"), exact=True):
            _reject(f"parameter `{name}` is outside its declared domain")
        return {"type": "exact_number", "value": render_canonical(value)}
    elif ptype == "text":
        if not isinstance(authored, str):
            _reject(f"parameter `{name}` is not text")
        allowed = value_type.get("allowed_values")
        if allowed is not None and authored not in allowed:
            _reject(f"parameter `{name}` is outside its declared choice set")
        return {"type": "text", "value": authored}
    elif ptype == "quantity":
        if (
            not isinstance(authored, dict)
            or not isinstance(authored.get("value"), str)
            or not isinstance(authored.get("unit"), str)
        ):
            _reject(f"parameter `{name}` is not a quantity object")
        canonical = _scale_quantity(kinds, value_type["kind"], authored)
        if _outside_quantity_bounds(
            canonical[0], kinds, value_type["kind"], value_type.get("min"), value_type.get("max")
        ):
            _reject(f"parameter `{name}` is outside its declared domain")
        return {
            "type": "quantity",
            "kind": value_type["kind"],
            "value": render_canonical(canonical[0]),
            "unit": canonical[1],
        }
    raise CannotLower(f"parameter value type `{ptype}` is not covered")


def _outside_bounds(value, minimum, maximum, exact: bool = False) -> bool:
    for bound, is_min in ((minimum, True), (maximum, False)):
        if bound is None:
            continue
        limit = bound["value"]
        if exact:
            try:
                limit = read_authoritative_exact(limit)
            except CanonError:
                raise CannotLower("a domain bound is not a canonical exact number")
        too_low = value < limit or (value == limit and not bound["inclusive"])
        too_high = value > limit or (value == limit and not bound["inclusive"])
        if (is_min and too_low) or (not is_min and too_high):
            return True
    return False


def _scale_quantity(kinds: dict, kind_id: str, quantity: dict) -> tuple:
    """Returns (canonical Fraction value, canonical unit)."""
    kind = kinds.get(kind_id)
    if kind is None:
        raise CannotLower(f"quantity kind `{kind_id}` is absent from the registry")
    try:
        value = read_authoritative_exact(quantity["value"])
    except CanonError:
        _reject(f"quantity value {quantity['value']!r} is not canonical")
    try:
        return kind.scale(value, quantity["unit"]), kind.canonical_unit
    except Exception as error:
        # A unit belonging to a different kind is CORE-T2102; an unknown
        # symbol is CORE-T2001. Both block compilation either way.
        _reject(f"quantity unit is not admitted: {error}")


def _outside_quantity_bounds(canonical, kinds, kind_id, minimum, maximum) -> bool:
    for bound, is_min in ((minimum, True), (maximum, False)):
        if bound is None:
            continue
        limit, _ = _scale_quantity(kinds, kind_id, bound["value"])
        too_low = canonical < limit or (canonical == limit and not bound["inclusive"])
        too_high = canonical > limit or (canonical == limit and not bound["inclusive"])
        if (is_min and too_low) or (not is_min and too_high):
            return True
    return False


# ---- workflow resolution ----------------------------------------------------


def _resolve_workflow(
    contract: dict, index: RegistryIndex, candidates: list[Candidate],
    invalid_sources: list, unknown_type_steps: set,
) -> tuple[dict, dict]:
    """Per-step resolved bindings and the step->dependency-steps graph."""
    bindings: dict[str, list] = {}
    dependencies: dict[str, set] = {s["step_id"]: set() for s in contract["workflow"]}
    for step in contract["workflow"]:
        capability = index.capability_types.get(_ref_key(step["capability_type"]))
        if capability is None:
            _reject(
                f"capability type {step['capability_type']['id']} is absent from the registry"
            )
        slot_defs = {slot["slot_id"]: slot for slot in capability.inputs}
        explicit = {b["input_slot"]: b for b in step.get("bindings", [])}
        resolved: list[dict] = []
        for slot in capability.inputs:
            binding = explicit.get(slot["slot_id"])
            if binding is not None:
                source = binding["source"]
                if _is_step_output_of(source, step["step_id"]):
                    _reject(f"step `{step['step_id']}` depends on its own output")
                candidate = _find_candidate(candidates, source)
                if candidate is None:
                    if _produced_by_unknown_type(source, unknown_type_steps):
                        continue
                    _reject(f"bound source {_source_label(source)} cannot be resolved")
                if any(_source_eq(candidate.source, s) for s in invalid_sources):
                    continue
                if candidate.role != slot["role"]:
                    _reject(
                        f"source role cannot satisfy slot `{slot['slot_id']}`"
                    )
                if candidate.media_type not in slot["accepted_media_types"]:
                    _reject(
                        f"source media type not accepted by slot `{slot['slot_id']}`"
                    )
                if source["source"] == "step_output":
                    dependencies[step["step_id"]].add(source["step_id"])
                resolved.append({"input_slot": slot["slot_id"], "source": candidate.source})
                continue
            matches = [
                c
                for c in candidates
                if not any(_source_eq(c.source, s) for s in invalid_sources)
                and not _is_step_output_of(c.source, step["step_id"])
                and c.role == slot["role"]
                and c.media_type in slot["accepted_media_types"]
            ]
            blocked = any(
                any(_source_eq(c.source, s) for s in invalid_sources)
                and c.role == slot["role"]
                for c in candidates
            )
            required = slot.get("required", True)
            if len(matches) == 1:
                candidate = matches[0]
                if candidate.source["source"] == "step_output":
                    dependencies[step["step_id"]].add(candidate.source["step_id"])
                resolved.append({"input_slot": slot["slot_id"], "source": candidate.source})
            elif not matches and required and not blocked:
                _reject(f"required input slot `{slot['slot_id']}` has no compatible source")
            elif len(matches) > 1 and required:
                _reject(f"required input slot `{slot['slot_id']}` has multiple compatible sources")
        resolved.sort(key=lambda b: b["input_slot"])
        bindings[step["step_id"]] = resolved
    return bindings, dependencies


def _produced_by_unknown_type(source: dict, unknown_type_steps: set) -> bool:
    return source["source"] == "step_output" and source["step_id"] in unknown_type_steps


def _topological_order(contract: dict, dependencies: dict) -> list[str]:
    """Kahn's algorithm over a lexicographically ordered ready set — the
    compiler's BTreeSet discipline, so the order is deterministic."""
    if len(contract["workflow"]) > MAX_WORKFLOW_STEPS:
        _reject("workflow exceeds the compiler step limit")
    indegree = {step: len(deps) for step, deps in dependencies.items()}
    dependents: dict[str, list] = {}
    for step, deps in dependencies.items():
        for dep in deps:
            dependents.setdefault(dep, []).append(step)
    ready = sorted(step for step, degree in indegree.items() if degree == 0)
    order: list[str] = []
    ready_set = set(ready)
    while ready_set:
        step = min(ready_set)
        ready_set.discard(step)
        order.append(step)
        for dependent in dependents.get(step, []):
            indegree[dependent] -= 1
            if indegree[dependent] == 0:
                ready_set.add(dependent)
    if len(order) != len(indegree):
        _reject("workflow bindings contain a dependency cycle")
    return order


# ---- parameters, reproducibility, presentation gates ------------------------


def _compile_parameters(contract: dict, index: RegistryIndex) -> dict:
    out: dict[str, dict] = {}
    for step in contract["workflow"]:
        capability = index.capability_types.get(_ref_key(step["capability_type"]))
        if capability is None:
            continue
        definitions = {p["parameter_id"]: p for p in capability.parameters}
        for authored_id in step.get("parameters", {}):
            if authored_id not in definitions:
                _reject(f"capability type declares no parameter `{authored_id}`")
        compiled: dict[str, Any] = {}
        for definition in capability.parameters:
            authored = step.get("parameters", {}).get(definition["parameter_id"])
            if authored is None:
                if definition.get("required", False):
                    _reject(f"required parameter `{definition['parameter_id']}` is not bound")
                continue
            if authored == NOT_DEFINED_PLACEHOLDER:
                _reject(f"parameter `{definition['parameter_id']}` remains not defined")
            compiled[definition["parameter_id"]] = _lower_typed_value(
                definition, authored, index.kinds
            )
        out[step["step_id"]] = compiled
    return out


def _compile_reproducibility(contract: dict, index: RegistryIndex) -> dict:
    out: dict[str, dict] = {}
    for step in contract["workflow"]:
        capability = index.capability_types.get(_ref_key(step["capability_type"]))
        if capability is None:
            continue
        declaration = capability.reproducibility
        determinism = declaration.get("determinism", "deterministic")
        policy = contract.get("execution_policy", {})
        permitted = {
            _ref_key(r) for r in policy.get("permitted_nondeterministic_roles", [])
        }
        if determinism == "nondeterministic" and not (
            capability.outputs
            and all(_ref_key(o["role"]) in permitted for o in capability.outputs)
        ):
            _reject("nondeterministic capability is not permitted by the execution policy")
        binding = step.get("reproducibility", {})
        seed = None
        if determinism == "seeded_stochastic":
            authored_seed = binding.get("seed")
            if (
                isinstance(authored_seed, str)
                and authored_seed.strip()
                and authored_seed != NOT_DEFINED_PLACEHOLDER
            ):
                seed = authored_seed
            else:
                _reject("seeded-stochastic capability requires an explicit nonempty seed")
        elif binding.get("seed") is not None:
            _reject(f"a seed is not part of a `{determinism}` invocation identity")

        definitions = {f["factor_id"]: f for f in declaration.get("material_factors", [])}
        for authored_id in binding.get("material_factors", {}):
            if authored_id not in definitions:
                _reject(f"capability type declares no material factor `{authored_id}`")
        factors: dict[str, Any] = {}
        for factor in declaration.get("material_factors", []):
            authored = binding.get("material_factors", {}).get(factor["factor_id"])
            if authored is None or authored == NOT_DEFINED_PLACEHOLDER:
                _reject(f"material execution factor `{factor['factor_id']}` is not bound")
            factors[factor["factor_id"]] = _lower_typed_value(
                {"parameter_id": factor["factor_id"], "value_type": factor["value_type"]},
                authored,
                index.kinds,
            )
        record = {"determinism": determinism, "material_factors": factors}
        if seed is not None:
            record["seed"] = seed
        out[step["step_id"]] = record
    return out


def _compile_presentation_gates(contract: dict, index: RegistryIndex, bindings: dict) -> dict:
    out: dict[str, dict] = {}
    for step in contract["workflow"]:
        capability = index.capability_types.get(_ref_key(step["capability_type"]))
        if capability is None:
            continue
        declaration = capability.review
        binding = step.get("review")
        if declaration is None:
            if binding is not None:
                _reject("capability type does not declare review semantics")
            continue
        if binding is None:
            _reject("review capability requires an explicit eligibility policy")
        policy = binding["reviewer_eligibility_policy"]
        valid = True
        if not str(policy.get("policy_id", "")).strip():
            valid = False
        if not policy.get("revision"):
            valid = False
        if not _is_sha256_identity(policy.get("sha256", "")):
            valid = False
        independence = binding["independence"]
        if independence["mode"] == "constraints":
            requirements = independence.get("requirements", [])
            parties = [r["separated_from"] for r in requirements]
            if not requirements or len(set(parties)) != len(parties):
                valid = False
        instructions = binding.get("instructions", [])
        if not instructions or any(not str(i).strip() for i in instructions):
            valid = False
        output = next(
            (o for o in capability.outputs if o["slot_id"] == declaration["decision_output_slot"]),
            None,
        )
        if output is None:
            raise CannotLower("review decision output slot is absent from the capability")
        resolved = {b["input_slot"]: b for b in bindings.get(step["step_id"], [])}
        presented = [
            resolved[slot]
            for slot in declaration["presented_input_slots"]
            if slot in resolved
        ]
        if len(presented) != len(declaration["presented_input_slots"]):
            continue
        if not valid:
            _reject("review binding is incomplete")
        out[step["step_id"]] = {
            "state": "awaiting_agent",
            "reviewer_role": declaration["reviewer_role"],
            "presented_evidence": presented,
            "decision_output_slot": output["slot_id"],
            "decision_role": output["role"],
            "decision_media_type": output["media_type"],
            "allowed_dispositions": declaration["allowed_dispositions"],
            "reviewer_eligibility_policy": policy,
            "independence": independence,
            "instructions": instructions,
        }
    return out


def _is_sha256_identity(value: str) -> bool:
    if not isinstance(value, str) or not value.startswith("sha256:"):
        return False
    hexpart = value[7:]
    return len(hexpart) == 64 and all(c in "0123456789abcdef" for c in hexpart)


# ---- requirements -----------------------------------------------------------


def _compile_requirements(
    contract: dict, index: RegistryIndex, candidates: list[Candidate],
    invalid_sources: list, unknown_type_steps: set,
) -> list[dict]:
    policy = contract.get("execution_policy", {})
    out: list[dict] = []
    for requirement in contract.get("requirements", []):
        basis = requirement["basis"]
        if basis["kind"] == "nominal" and not policy.get("permit_nominal_basis", False):
            _reject("nominal basis is not permitted by the execution policy")
        coverage = basis.get("coverage")
        if coverage is not None:
            if basis["kind"] != "bounded":
                _reject("coverage is only meaningful for a bounded basis")
            try:
                value = read_authoritative_exact(coverage)
            except CanonError:
                _reject("coverage is not a canonical decimal")
            if not (Fraction(0) < value <= Fraction(1)):
                _reject("coverage must lie in (0, 1]")
        comparison = requirement["comparison"]
        tolerance_authored = requirement.get("tolerance")
        if comparison == "equal" and tolerance_authored is None:
            _reject("an equal comparison requires an exact tolerance quantity")
        if comparison != "equal" and tolerance_authored is not None:
            _reject("a tolerance is only meaningful for an equal comparison")
        metric = requirement.get("metric")
        if metric is None:
            _reject(f"requirement `{requirement['requirement_id']}` names no metric source")
        candidate = _find_candidate(candidates, metric)
        if candidate is None:
            if _produced_by_unknown_type(metric, unknown_type_steps):
                continue
            _reject(f"metric source {_source_label(metric)} cannot be resolved")
        if any(_source_eq(candidate.source, s) for s in invalid_sources):
            continue
        if (
            _ref_key(requirement["purpose"]) in index.purposes
            and _ref_key(requirement["purpose"])
            in {_ref_key(p) for p in candidate.excluded_purposes}
        ):
            _reject("metric source explicitly excludes the governed purpose")
        if not any(
            _claim_model_satisfies(m, basis, comparison) for m in candidate.claim_models
        ):
            _reject("metric source cannot emit a claim model sufficient for this basis")
        role = index.roles.get(_ref_key(candidate.role))
        if role is None:
            raise CannotLower("metric role is absent from the registry")
        if role.quantity_kind is None:
            _reject("metric role is not a quantity role")
        if role.quantity_kind != requirement["limit"]["kind"]:
            _reject("limit kind does not match metric kind")
        limit = _lower_requirement_quantity(requirement["limit"], role.quantity_kind, index.kinds)
        tolerance = None
        if tolerance_authored is not None and comparison == "equal":
            tolerance = _lower_requirement_quantity(
                tolerance_authored, role.quantity_kind, index.kinds
            )
            try:
                if read_authoritative_exact(tolerance_authored["value"]) < 0:
                    _reject("tolerance cannot be negative")
            except CanonError:
                _reject("tolerance value is not canonical")
        compiled = {
            "requirement_id": requirement["requirement_id"],
            "statement": requirement["statement"],
            "purpose": requirement["purpose"],
            "metric": metric,
            "metric_role": candidate.role,
            "comparison": comparison,
            "limit": limit,
            "basis": {k: v for k, v in basis.items()},
        }
        if tolerance is not None:
            compiled["tolerance"] = tolerance
        out.append(compiled)
    out.sort(key=lambda r: r["requirement_id"])
    return out


def _lower_requirement_quantity(quantity: dict, metric_kind: str, kinds: dict) -> dict:
    if quantity["kind"] != metric_kind:
        _reject(f"quantity kind `{quantity['kind']}` does not match metric kind `{metric_kind}`")
    value, canonical_unit = _scale_quantity(kinds, metric_kind, quantity)
    return {
        "kind": metric_kind,
        "value": render_canonical(value),
        "unit": canonical_unit,
    }


def _compile_categorical_requirements(
    contract: dict, index: RegistryIndex, candidates: list[Candidate],
    invalid_sources: list, unknown_type_steps: set,
) -> list[dict]:
    out: list[dict] = []
    for requirement in contract.get("categorical_requirements", []):
        predicate = requirement["predicate"]
        if predicate["operator"] == "equals":
            values = [predicate["value"]]
        else:
            values = predicate["values"]
        values_valid = bool(values) and all(v.strip() for v in values) and len(set(values)) == len(values)
        if not values_valid:
            _reject("categorical predicate values are invalid")
        metric = requirement.get("metric")
        if metric is None:
            _reject(f"categorical requirement `{requirement['requirement_id']}` names no metric")
        candidate = _find_candidate(candidates, metric)
        if candidate is None:
            if _produced_by_unknown_type(metric, unknown_type_steps):
                continue
            _reject(f"categorical metric source {_source_label(metric)} cannot be resolved")
        if any(_source_eq(candidate.source, s) for s in invalid_sources):
            continue
        if (
            _ref_key(requirement["purpose"]) in index.purposes
            and _ref_key(requirement["purpose"])
            in {_ref_key(p) for p in candidate.excluded_purposes}
        ):
            _reject("categorical metric source explicitly excludes the governed purpose")
        if {"model": "unquantified"} not in candidate.claim_models:
            _reject("categorical metric source does not permit the unquantified claim model")
        role = index.roles.get(_ref_key(candidate.role))
        if role is None:
            raise CannotLower("categorical metric role is absent from the registry")
        if role.quantity_kind is not None or role.unit_class is not None:
            _reject("categorical metric role must be non-quantitative")
        if not role.categorical_values:
            _reject("metric role declares no closed categorical vocabulary")
        if any(v not in role.categorical_values for v in values):
            _reject("predicate values are outside the role vocabulary")
        out.append(
            {
                "requirement_id": requirement["requirement_id"],
                "statement": requirement["statement"],
                "purpose": requirement["purpose"],
                "metric": metric,
                "metric_role": candidate.role,
                "predicate": predicate,
            }
        )
    out.sort(key=lambda r: r["requirement_id"])
    return out


# ---- registry-reference validation ------------------------------------------


def _validate_contract_registry_refs(contract: dict, index: RegistryIndex) -> list:
    """Returns the invalid-source list; pushes a WouldReject for each check
    the compiler reports as a blocking finding."""
    invalid: list[dict] = []
    for ref in contract.get("execution_policy", {}).get("permitted_nondeterministic_roles", []):
        if _ref_key(ref) not in index.roles:
            _reject("nondeterminism policy references a role absent from the registry")
    for requirement in contract.get("requirements", []):
        if _ref_key(requirement["purpose"]) not in index.purposes:
            _reject("requirement references a purpose absent from the registry")
    for requirement in contract.get("categorical_requirements", []):
        if _ref_key(requirement["purpose"]) not in index.purposes:
            _reject("categorical requirement references a purpose absent from the registry")
    for input_decl in contract.get("inputs", []):
        role = index.roles.get(_ref_key(input_decl["role"]))
        if role is None:
            invalid.append({"source": "contract_input", "input_id": input_decl["input_id"]})
            _reject("input references a role absent from the registry")
        if input_decl["media_type"] not in role.accepted_media_types:
            invalid.append({"source": "contract_input", "input_id": input_decl["input_id"]})
            _reject("input media type is not accepted by its role")
        if input_decl["claim_model"] not in role.permitted_claim_models:
            invalid.append({"source": "contract_input", "input_id": input_decl["input_id"]})
            _reject("input claim model is not permitted by its role")
    return invalid


# ---- top-level lowering -----------------------------------------------------


def lower_compiled_snapshot(contract_bytes: bytes, registry_bytes: bytes) -> str:
    """Re-derives the compiled-contract ``snapshot_sha256`` for a
    contract+registry byte pair. Raises ``WouldReject`` when a ported check
    shows the compiler would not compile this pair, ``CannotLower`` when the
    pair uses a construct outside this port's covered subset."""
    # Identity first: canonicalise both documents, which also enforces the
    # authoritative profile (no nulls, floats, duplicate keys, or non-NFC
    # strings). A document outside that profile is one the compiler refused
    # before any lowering ran.
    try:
        contract_sha256 = _document_identity(contract_bytes)
        registry_sha256 = _document_identity(registry_bytes)
    except CanonError as error:
        raise WouldReject(f"document is outside the authoritative JSON profile: {error}")
    import json as _json

    contract = _json.loads(contract_bytes)
    registry = _json.loads(registry_bytes)
    if not isinstance(contract, dict) or not isinstance(registry, dict):
        raise CannotLower("documents are not objects")
    _validate_shape(contract, registry)

    index = _build_index(registry)
    invalid_sources = _validate_contract_registry_refs(contract, index)
    unknown_type_steps = {
        s["step_id"]
        for s in contract["workflow"]
        if _ref_key(s["capability_type"]) not in index.capability_types
    }

    parameters = _compile_parameters(contract, index)
    reproducibility = _compile_reproducibility(contract, index)
    candidates = _collect_sources(contract, index)
    bindings, dependencies = _resolve_workflow(
        contract, index, candidates, invalid_sources, unknown_type_steps
    )
    gates = _compile_presentation_gates(contract, index, bindings)
    order = _topological_order(contract, dependencies)
    requirements = _compile_requirements(
        contract, index, candidates, invalid_sources, unknown_type_steps
    )
    categorical = _compile_categorical_requirements(
        contract, index, candidates, invalid_sources, unknown_type_steps
    )

    steps_by_id = {s["step_id"]: s for s in contract["workflow"]}
    workflow: list[dict] = []
    for step_id in order:
        step = steps_by_id[step_id]
        compiled_step = {
            "step_id": step_id,
            "capability_type": step["capability_type"],
            "bindings": bindings.get(step_id, []),
            "parameters": parameters.get(step_id, {}),
            "reproducibility": reproducibility[step_id],
        }
        gate = gates.get(step_id)
        if gate is not None:
            compiled_step["presentation_gate"] = gate
        workflow.append(compiled_step)

    policy_src = contract.get("execution_policy", {})
    execution_policy: dict[str, Any] = {
        "permitted_nondeterministic_roles": policy_src.get(
            "permitted_nondeterministic_roles", []
        )
    }
    for flag in ("permit_nominal_basis", "require_qualification", "require_signatures"):
        if policy_src.get(flag):
            execution_policy[flag] = True

    # Claim models re-serialise with their defaulted fields made explicit:
    # `interval` and `worst_case` carry `nominal: false` in the compiled body
    # even when the authored form omitted it.
    def _normalize_claim_model(model: dict) -> dict:
        out = dict(model)
        if model["model"] in ("interval", "worst_case"):
            out.setdefault("nominal", False)
        return out

    inputs = [
        {
            "input_id": i["input_id"],
            "role": i["role"],
            "media_type": i["media_type"],
            "claim_model": _normalize_claim_model(i["claim_model"]),
        }
        for i in sorted(contract.get("inputs", []), key=lambda i: i["input_id"])
    ]

    body = {
        "schema_version": COMPILED_SCHEMA_VERSION,
        "semantic_profile": SEMANTIC_PROFILE,
        "compiler": COMPILER_ID,
        "contract_id": contract["contract_id"],
        "contract_revision": contract["revision"],
        "contract_sha256": contract_sha256,
        "question": contract["question"],
        "assumptions": contract.get("assumptions", []),
        "registry_id": registry["registry_id"],
        "registry_revision": registry["revision"],
        "registry_sha256": registry_sha256,
        "execution_policy": execution_policy,
        "inputs": inputs,
        "workflow": workflow,
        "requirements": requirements,
    }
    if categorical:
        body["categorical_requirements"] = categorical

    canonical = canonicalize_json(_json.dumps(body).encode("utf-8"))
    return _sha256(canonical)
