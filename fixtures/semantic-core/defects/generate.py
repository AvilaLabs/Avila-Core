#!/usr/bin/env python3
"""Generates defects.v1.json by seeding one realistic defect into a copy of
the real CASE-001 contract+registry pair and capturing the compiler's
actual findings as the pinned expectation.

Not shipped machinery: a corpus authoring aid. Run from the repository root
after `cargo build -p avila-core-cli`:

    python3 fixtures/semantic-core/defects/generate.py
"""

import copy
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
BIN = ROOT.parents[2] / "target" / "debug" / "avila-core"

def canonical(data: bytes) -> bytes:
    import sys
    sys.path.insert(0, str(ROOT.parents[2] / "verifier"))
    from avila_core_verify import canonicalize_json
    return canonicalize_json(data)

def sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()

def resolve(obj, pointer: str):
    tokens = [t.replace("~1", "/").replace("~0", "~") for t in pointer.split("/")[1:]]
    for token in tokens[:-1]:
        obj = obj[int(token)] if isinstance(obj, list) else obj[token]
    return obj, tokens[-1]

def apply(doc, mutations):
    doc = copy.deepcopy(doc)
    for mutation in mutations:
        parent, leaf = resolve(doc, mutation["pointer"])
        op = mutation["op"]
        if op == "set":
            if isinstance(parent, list):
                parent[int(leaf)] = mutation["value"]
            else:
                parent[leaf] = mutation["value"]
        elif op == "remove":
            if isinstance(parent, list):
                del parent[int(leaf)]
            else:
                del parent[leaf]
        else:
            raise ValueError(op)
    return doc

def run_compile(contract: dict, registry: dict):
    out = subprocess.run(
        [str(BIN), "compile", "--contract", "/dev/stdin", "--registry", "/dev/stdin"],
        input=json.dumps(contract).encode() + b"\x00" + json.dumps(registry).encode(),
        capture_output=True,
    )
    # --contract/--registry take file paths; feed via temp files instead.
    raise RuntimeError("use files")

def compile_pair(contract_bytes: bytes, registry_bytes: bytes) -> dict:
    import tempfile
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as c, \
         tempfile.NamedTemporaryFile(suffix=".json", delete=False) as r:
        c.write(contract_bytes); r.write(registry_bytes)
        c.flush(); r.flush()
        out = subprocess.run(
            [str(BIN), "compile", "--contract", c.name, "--registry", r.name],
            capture_output=True, text=True,
        )
    # The compiler exits nonzero when status is "rejected"; the report is
    # still on stdout either way.
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        print(out.stdout, out.stderr)
        raise SystemExit("compile invocation failed")

BASE_CONTRACT = (ROOT / "base.contract.json").read_bytes()
BASE_REGISTRY = (ROOT / "base.registry.json").read_bytes()
CONTRACT = json.loads(BASE_CONTRACT)
REGISTRY = json.loads(BASE_REGISTRY)

# (fixture_id, defect prose, contract_mutations, registry_mutations)
DEFECTS = [
    (
        "defect.contract.typo-field",
        "requester misspells `comparison` on a requirement",
        [
            {"op": "set", "pointer": "/requirements/1/comparision", "value": "less_than_or_equal"},
            {"op": "remove", "pointer": "/requirements/1/comparison"},
        ],
        [],
    ),
    (
        "defect.contract.unresolvable-metric",
        "a requirement's metric names a step that does not exist",
        [
            {"op": "set", "pointer": "/requirements/1/metric/step_id", "value": "trasnport"},
        ],
        [],
    ),
    (
        "defect.contract.unresolvable-binding",
        "a binding names a contract input that does not exist",
        [
            {"op": "set", "pointer": "/workflow/0/bindings/0/source",
             "value": {"source": "contract_input", "input_id": "scree-script"}},
        ],
        [],
    ),
    (
        "defect.contract.dropped-binding",
        "a binding is deleted; the single matching candidate auto-binds and the contract still compiles",
        [
            {"op": "remove", "pointer": "/workflow/0/bindings/0"},
        ],
        [],
    ),
    (
        "defect.contract.role-mismatched-binding",
        "a step's script slot is bound to the candidate input (wrong role)",
        [
            {"op": "set", "pointer": "/workflow/0/bindings/0/source",
             "value": {"source": "contract_input", "input_id": "candidate"}},
        ],
        [],
    ),
    (
        "defect.contract.media-mismatched-binding",
        "a step's candidate slot is bound to the screen script (wrong media type)",
        [
            {"op": "set", "pointer": "/workflow/0/bindings/1/source",
             "value": {"source": "contract_input", "input_id": "screen-script"}},
        ],
        [],
    ),
    (
        "defect.contract.unknown-unit",
        "a limit is authored in a unit the kind does not admit",
        [
            {"op": "set", "pointer": "/requirements/1/limit/unit", "value": "uSv/hr"},
        ],
        [],
    ),
    (
        "defect.contract.limit-kind-mismatch",
        "a limit's quantity kind does not match the metric's role kind",
        [
            {"op": "set", "pointer": "/requirements/2/limit/kind", "value": "core.length"},
        ],
        [],
    ),
    (
        "defect.contract.undeclared-parameter",
        "a step binds a parameter its capability type does not declare",
        [
            {"op": "set", "pointer": "/workflow/1/parameters/batch_size", "value": 10},
        ],
        [],
    ),
    (
        "defect.contract.missing-required-parameter",
        "a required parameter binding is deleted",
        [
            {"op": "remove", "pointer": "/workflow/1/parameters/particles"},
        ],
        [],
    ),
    (
        "defect.contract.parameter-type",
        "an integer parameter is bound to text",
        [
            {"op": "set", "pointer": "/workflow/1/parameters/particles", "value": "many"},
        ],
        [],
    ),
    (
        "defect.contract.missing-seed",
        "a seeded-stochastic step's seed is deleted",
        [
            {"op": "remove", "pointer": "/workflow/1/reproducibility/seed"},
        ],
        [],
    ),
    (
        "defect.contract.seed-on-deterministic",
        "a deterministic step is given a seed",
        [
            {"op": "set", "pointer": "/workflow/0/reproducibility", "value": {"seed": "7"}},
        ],
        [],
    ),
    (
        "defect.contract.review-policy-identity",
        "the reviewer eligibility policy's sha256 is malformed",
        [
            {"op": "set", "pointer": "/workflow/2/review/reviewer_eligibility_policy/sha256",
             "value": "sha256:not-hex"},
        ],
        [],
    ),
    (
        "defect.contract.empty-instructions",
        "the optional review's practical instructions are empty",
        [
            {"op": "set", "pointer": "/workflow/2/review/instructions", "value": []},
        ],
        [],
    ),
    (
        "defect.contract.nominal-without-permit",
        "a requirement switches to nominal basis while the policy's nominal permit is dropped",
        [
            {"op": "set", "pointer": "/requirements/1/basis/kind", "value": "nominal"},
            {"op": "remove", "pointer": "/execution_policy/permit_nominal_basis"},
        ],
        [],
    ),
    (
        "defect.contract.equal-without-tolerance",
        "an equal comparison omits its tolerance",
        [
            {"op": "set", "pointer": "/requirements/0/comparison", "value": "equal"},
        ],
        [],
    ),
    (
        "defect.contract.coverage-out-of-range",
        "a bounded basis carries a coverage outside (0, 1]",
        [
            {"op": "set", "pointer": "/requirements/1/basis/coverage", "value": "1.5"},
        ],
        [],
    ),
    (
        "defect.contract.unknown-purpose",
        "a requirement names a purpose absent from the registry",
        [
            {"op": "set", "pointer": "/requirements/0/purpose/id", "value": "shielding.bogus"},
        ],
        [],
    ),
    (
        "defect.contract.unknown-capability",
        "a step names a capability type absent from the registry",
        [
            {"op": "set", "pointer": "/workflow/1/capability_type/id", "value": "shielding.slab-trasport"},
        ],
        [],
    ),
    (
        "defect.contract.self-dependency",
        "a step binds one of its own outputs as an input",
        [
            {"op": "set", "pointer": "/workflow/2/bindings/0/source",
             "value": {"source": "step_output", "step_id": "practical-review", "output_slot": "decision"}},
        ],
        [],
    ),
    (
        "defect.registry.role-dropped",
        "the registry drops a role an input still references",
        [],
        [
            {"op": "remove", "pointer": "/roles/0"},
        ],
    ),
    (
        "defect.registry.unit-factor-rewritten",
        "a registry unit's conversion factor is silently changed",
        [],
        # Find the uSv/h unit inside its kind and rewrite its factor; the
        # pointer is resolved at generation time below.
        [{"op": "__unit_factor", "kind": "nuclear.ambient-dose-equivalent-rate",
          "symbol": "uSv/h", "value": "0.0000000005"}],
    ),
    (
        "defect.registry.missing-validator",
        "a role loses its declared validator",
        [],
        [
            {"op": "set", "pointer": "/roles/0/validator", "value": ""},
        ],
    ),
    (
        "defect.registry.duplicate-role",
        "a second role reuses the first role's versioned identity",
        [],
        [
            {"op": "set", "pointer": "/roles/1/role",
             "value": {"id": "shielding.candidate", "major": 1}},
        ],
    ),
    (
        "defect.registry.role-unit-class-mismatch",
        "a quantity role names a unit class that does not match its kind's class",
        [],
        [
            {"op": "set", "pointer": "/roles/6/unit_class", "value": "core.length.units@1"},
        ],
    ),
    (
        "defect.registry.duplicate-purpose",
        "a second purpose reuses the first purpose's id",
        [],
        [
            {"op": "set", "pointer": "/purposes/1/purpose/id",
             "value": "avila-labs.shielding.design-search"},
        ],
    ),
    (
        "defect.registry.empty-owner",
        "a role loses its owner",
        [],
        [
            {"op": "set", "pointer": "/roles/0/owner", "value": ""},
        ],
    ),
    (
        "defect.registry.duplicate-unit",
        "a kind declares the same unit symbol twice",
        [],
        [
            {"op": "set", "pointer": "/kinds/0/units/1/symbol", "value": "uSv/h"},
        ],
    ),
    (
        "defect.contract.json-float-limit",
        "a limit is authored as a JSON float instead of an exact decimal string",
        [
            {"op": "set", "pointer": "/requirements/1/limit/value", "value": 25.0},
        ],
        [],
    ),
    (
        "defect.contract.duplicate-input",
        "two inputs reuse the same input_id",
        [
            {"op": "set", "pointer": "/inputs/1/input_id", "value": "candidate"},
        ],
        [],
    ),
    (
        "defect.contract.prefix-case",
        "a limit's unit has the wrong prefix case — MSv is not mSv",
        [
            {"op": "set", "pointer": "/requirements/1/limit/unit", "value": "MSv/h"},
        ],
        [],
    ),
    (
        "defect.contract.role-major-version",
        "an input references the right role at the wrong major version",
        [
            {"op": "set", "pointer": "/inputs/0/role/major", "value": 2},
        ],
        [],
    ),
    (
        "defect.contract.negative-zero-limit",
        "a limit is authored as -0, which has no canonical decimal",
        [
            {"op": "set", "pointer": "/requirements/1/limit/value", "value": "-0"},
        ],
        [],
    ),
]


def main() -> int:
    fixtures = []
    for fixture_id, defect, contract_mutations, registry_mutations in DEFECTS:
        contract = apply(CONTRACT, contract_mutations)
        registry = copy.deepcopy(REGISTRY)
        resolved_registry_mutations = []
        for mutation in registry_mutations:
            if mutation["op"] == "__unit_factor":
                for k_index, kind in enumerate(registry["kinds"]):
                    if kind["kind_id"] == mutation["kind"]:
                        for u_index, unit in enumerate(kind["units"]):
                            if unit["symbol"] == mutation["symbol"]:
                                unit["factor"] = mutation["value"]
                                resolved_registry_mutations.append({
                                    "op": "set",
                                    "pointer": f"/kinds/{k_index}/units/{u_index}/factor",
                                    "value": mutation["value"],
                                })
            else:
                resolved_registry_mutations.append(mutation)
                registry = apply(registry, [mutation])
        report = compile_pair(
            json.dumps(contract, indent=1).encode(),
            json.dumps(registry, indent=1).encode(),
        )
        entry = {
            "fixture_id": fixture_id,
            "defect": defect,
            "contract_mutations": contract_mutations,
            "registry_mutations": resolved_registry_mutations,
            "expected": {
                "status": report["status"],
                "findings": [
                    {
                        "code": f["code"],
                        "class": f["class"],
                        "owner": f["owner"],
                        "primary": f["primary"],
                        "repairs": [r["applicability"] for r in f.get("repairs", [])],
                    }
                    for f in report["findings"]
                ],
            },
        }
        if report["status"] == "compiled":
            entry["expected"]["snapshot_sha256"] = report["compiled"]["snapshot_sha256"]
        fixtures.append(entry)
        print(f"{fixture_id}: {report['status']} ({len(report['findings'])} findings)")

    suite = {
        "fixture_set": "compiler-defects",
        "version": 1,
        "semantic_profile": "avila.core/semantic/0.2-draft",
        "contract": {
            "path": "base.contract.json",
            "sha256": sha256(canonical(BASE_CONTRACT)),
        },
        "registry": {
            "path": "base.registry.json",
            "sha256": sha256(canonical(BASE_REGISTRY)),
        },
        "fixtures": fixtures,
    }
    (ROOT / "defects.v1.json").write_text(json.dumps(suite, indent=1) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
