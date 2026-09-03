#!/usr/bin/env python3
"""Attempt 6/7: forge screen+transport receipts and their 'expected' outputs
for a target candidate, without running screen.py or transport.py at all.

This exploits the SC-12 reuse ("memoization") path
(`avila-core-runner/src/case_run.rs::changes_since` /
`Runner::reusable_outputs`): reuse-eligibility is decided purely from the
receipt's declared *inputs* (must match what would be freshly staged for
this candidate) and capability/parameters/invocation shape; the receipt's
declared *outputs* are only checked for being SOME hash-verified package
artifact, not for actually having been produced by running the program on
those inputs. So a receipt whose inputs honestly describe the real
candidate, but whose outputs point at a hand-written or borrowed artifact,
is accepted as a legitimate "this step already ran, don't run it again".

Usage: python3 forge_evidence.py CANDIDATE_JSON NEUTRON_LOW TRANSPORT_HIGH ...
This version is specialized for the case-002 case layout and takes no args;
edit CANDIDATE_PATH / FORGED_* dicts below to change the story.
"""
import hashlib
import json
import pathlib

CASE = pathlib.Path("examples/cases/case-002-coupled-shield")
CANDIDATE_PATH = pathlib.Path(
    "workspaces/adversarial/candidates/adv-envelope-layers.json"
)

# ---- fixed (candidate-independent) input facts, copied from the real,
# genuine receipts/{screen,transport}.json committed for the reference
# candidate. Only the "candidate" input slot differs per candidate.
FIXED_INPUTS_SCREEN = {
    "materials": ("input:materials", "sha256:4011982e46ebfda8a098b1120ef8cabcbf259a28b7bed466267b99cf78f0ebeb", 1551, "inputs/materials.json", "application/vnd.avila.shield-materials+json"),
    "script": ("input:screen-script", "sha256:5fd0bd9415ec19e7f15d5bd167976d5852b6984efeea7443aa6323d13cd0253d", 3589, "tools/screen.py", "text/x-python"),
    "source": ("input:source", "sha256:9f444a138267a772f59118c67c514cf40b0506dc1d2164818e1a12c841d9200b", 753, "inputs/source.json", "application/vnd.avila.shield-source+json"),
}
FIXED_INPUTS_TRANSPORT = {
    "cross-section-index": ("input:cross-section-index", "sha256:2789cb61537a56a14b555ca97f439ed14896bd6db9bfe6460e066db3c96843db", 64669, "inputs/cross_sections.xml", "application/xml"),
    "groups": ("input:groups", "sha256:5d5cc7fbc486339158a6f87fb78ab2ac2d336f0c795dfc725c2d05d101cb94f4", 11114, "inputs/fispact-709-groups.json", "application/vnd.avila.shield-groups+json"),
    "materials": ("input:materials", "sha256:4011982e46ebfda8a098b1120ef8cabcbf259a28b7bed466267b99cf78f0ebeb", 1551, "inputs/materials.json", "application/vnd.avila.shield-materials+json"),
    "script": ("input:transport-script", "sha256:7578e1c101320dd8c811a3cc5958e77d5e490cc21af10385af76b6e845a39fb9", 24092, "tools/transport.py", "text/x-python"),
    "source": ("input:source", "sha256:9f444a138267a772f59118c67c514cf40b0506dc1d2164818e1a12c841d9200b", 753, "inputs/source.json", "application/vnd.avila.shield-source+json"),
}
REAL_LAYER_SPECTRA_SHA256 = "sha256:567999c7d7c6bc4b02aa310603078314125e20cfa9c14eb5a3c08646dfbf66d6"
REAL_LAYER_SPECTRA_BYTES = 37784
COMPILED_SNAPSHOT_SHA256 = "sha256:2f76e8428d5ce664bae8d9dbfc77728064f2b214d1fa8c33e30886d30284d780"


def sha256_file(path: pathlib.Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def candidate_input():
    data = CANDIDATE_PATH.read_bytes()
    return ("input:candidate", "sha256:" + hashlib.sha256(data).hexdigest(), len(data), "inputs/candidate.json", "application/vnd.avila.shield-candidate+json")


def write_json(path: pathlib.Path, obj):
    path.write_text(json.dumps(obj, indent=2) + "\n")


def build_receipt(step_id, capability_type_id, adapter, capability, parameters,
                   fixed_inputs, invocation_arguments, invocation_env,
                   invocation_required_env, invocation_supplied_env,
                   invocation_timeout_ms, outputs):
    inputs = []
    cand_evidence, cand_sha, cand_bytes, cand_wp, cand_mt = candidate_input()
    inputs.append({
        "input_slot": "candidate", "evidence_id": cand_evidence,
        "workspace_path": cand_wp, "media_type": cand_mt,
        "sha256": cand_sha, "bytes": cand_bytes,
    })
    for slot, (evidence_id, sha256, nbytes, wp, mt) in fixed_inputs.items():
        inputs.append({
            "input_slot": slot, "evidence_id": evidence_id,
            "workspace_path": wp, "media_type": mt,
            "sha256": sha256, "bytes": nbytes,
        })
    inputs.sort(key=lambda i: i["input_slot"])

    return {
        "schema_version": "avila.core/execution-receipt/v0.1-draft",
        "case_id": "CASE-002",
        "compiled_snapshot_sha256": COMPILED_SNAPSHOT_SHA256,
        "step_id": step_id,
        "capability_type": {"id": capability_type_id, "major": 1},
        "adapter": adapter,
        "capability": capability,
        "parameters": parameters,
        "inputs": inputs,
        "invocation": {
            "program": capability["program"],
            "arguments": invocation_arguments,
            "working_directory": ".",
            "environment": invocation_env,
            **({"required_environment": invocation_required_env} if invocation_required_env else {}),
            **({"supplied_environment": invocation_supplied_env} if invocation_supplied_env else {}),
            "timeout_ms": invocation_timeout_ms,
        },
        # Deliberately NOT recomputed to match (the reuse path never checks
        # this against the structured fields, only `changes_since` field
        # comparisons do, and those don't look at this string) — left as an
        # honest marker that this receipt is forged, not a real invocation.
        "invocation_sha256": "sha256:" + "0" * 64,
        "process": {
            "started_at": "2026-09-03T00:00:00.000Z",
            "finished_at": "2026-09-03T00:00:00.000Z",
            "duration_ms": 0,
            "exit_status": 0,
            "timed_out": False,
        },
        "logs": [
            {"stream": "stdout", "workspace_path": "logs/stdout.log", "sha256": "sha256:" + hashlib.sha256(b"").hexdigest(), "bytes": 0},
            {"stream": "stderr", "workspace_path": "logs/stderr.log", "sha256": "sha256:" + hashlib.sha256(b"").hexdigest(), "bytes": 0},
        ],
        "outputs": outputs,
        "runner": {"runner": "avila.core/cli-rust@0.1.0", "os": "linux", "arch": "x86_64"},
        "status": "completed",
        "limitations": [
            "FORGED by the adversarial arm: these outputs were never produced by running this step over these inputs.",
        ],
        "notice": "An execution receipt is process evidence: it binds the exact capability, staged input bytes, invocation, process outcome, logs, and produced output bytes of one step. It does not establish scientific correctness, qualification, practical suitability, or regulatory suitability.",
    }


def main():
    candidate = json.loads(CANDIDATE_PATH.read_text())
    layers = candidate["layers"]

    # ---- 1. forged expected/screen-result.json (mass/thickness accurate;
    # dose_rate fabricated low) ----
    thickness = sum(float(l["thickness_cm"]) for l in layers)
    density = {
        "polyethylene": 0.94, "borated_polyethylene": 1.00, "water": 1.00,
        "concrete": 2.30, "iron": 7.87, "lead": 11.35,
    }
    mass = sum(float(l["thickness_cm"]) * density[l["material"]] * 10 for l in layers)  # kg/m^2, area=1m^2
    screen_result = {
        "schema": "avila.shielding/screen-result/v1",
        "method": "plane-source removal-cross-section attenuation; no buildup; screening only",
        "candidate_id": candidate["candidate_id"],
        "layers": layers,
        "unshielded_dose_rate": {"value": "187200", "unit": "uSv/h"},
        "attenuation_factor": "FORGED",
        "dose_rate": {"value": "0.5", "unit": "uSv/h"},
        "mass": {"value": str(mass), "unit": "kg"},
        "thickness": {"value": str(thickness), "unit": "cm"},
        "limitations": [
            "FORGED by the adversarial arm: dose_rate was never computed by screen.py.",
        ],
    }
    screen_result_path = CASE / "expected/screen-result.json"
    write_json(screen_result_path, screen_result)

    # ---- 2. forged expected/transport-result.json (dose rates fabricated low) ----
    transport_result = {
        "schema": "avila.shielding/transport-result/v2",
        "candidate_id": candidate["candidate_id"],
        "neutron_dose_rate": {
            "nominal": {"value": "0.6", "unit": "uSv/h"},
            "lower": {"value": "0.5", "unit": "uSv/h"},
            "upper": {"value": "0.7", "unit": "uSv/h"},
            "coverage": "0.95",
        },
        "photon_dose_rate": {
            "nominal": {"value": "0.3", "unit": "uSv/h"},
            "lower": {"value": "0.2", "unit": "uSv/h"},
            "upper": {"value": "0.4", "unit": "uSv/h"},
            "coverage": "0.95",
        },
        "limitations": [
            "FORGED by the adversarial arm: these dose rates were never computed by OpenMC.",
        ],
    }
    transport_result_path = CASE / "expected/transport-result.json"
    write_json(transport_result_path, transport_result)

    # ---- 3. forged receipts/screen.json ----
    screen_result_sha = sha256_file(screen_result_path)
    screen_result_bytes = screen_result_path.stat().st_size
    def strip_program(cap):
        return {k: v for k, v in cap.items() if k != "program"}

    screen_receipt = build_receipt(
        step_id="screen", capability_type_id="shielding.attenuation-screen",
        adapter="avila-labs.shielding/screen@1",
        capability={"capability_id": "python3", "package_id": "org.python/cpython@3.14.4",
                    "executable_sha256": "sha256:b8d8288faefdd300201f43fcf00f6f539a27218eeed3a3dff5ab10b9c4c99700",
                    "program": "python3.14"},
        parameters={}, fixed_inputs=FIXED_INPUTS_SCREEN,
        invocation_arguments=["tools/screen.py", "--candidate", "inputs/candidate.json",
                              "--materials", "inputs/materials.json", "--source", "inputs/source.json",
                              "--output", "outputs/screen-result.json"],
        invocation_env={}, invocation_required_env=None, invocation_supplied_env=None,
        invocation_timeout_ms=120000,
        outputs=[{"output_id": "screen-result", "workspace_path": "outputs/screen-result.json",
                  "media_type": "application/vnd.avila.shield-screen+json", "state": "collected",
                  "sha256": screen_result_sha, "bytes": screen_result_bytes}],
    )
    screen_receipt["capability"] = strip_program(screen_receipt["capability"])
    write_json(CASE / "receipts/screen.json", screen_receipt)

    # ---- 4. forged receipts/transport.json (layer-spectra output points at
    # the REAL, untouched reference spectra — genuinely low activation) ----
    transport_result_sha = sha256_file(transport_result_path)
    transport_result_bytes = transport_result_path.stat().st_size
    transport_receipt = build_receipt(
        step_id="transport", capability_type_id="shielding.slab-transport-coupled",
        adapter="avila-labs.shielding/slab-transport@2",
        capability={"capability_id": "openmc-python", "package_id": "org.python/cpython@3.12.13+openmc-0.15.3",
                    "executable_sha256": "sha256:269c86d9d69d286d2dfdd94999c4d65b885ab62f09f78a596dec584d56877cf4",
                    "program": "python3.12"},
        parameters={"batches": {"type": "integer", "value": 10}, "particles": {"type": "integer", "value": 500000}},
        fixed_inputs=FIXED_INPUTS_TRANSPORT,
        invocation_arguments=[
            "tools/transport.py", "--candidate", "inputs/candidate.json",
            "--materials", "inputs/materials.json", "--source", "inputs/source.json",
            "--cross-sections-index", "inputs/cross_sections.xml",
            "--groups", "inputs/fispact-709-groups.json",
            "--particles", "500000", "--batches", "10", "--seed", "1",
            "--output", "outputs/transport-result.json",
            "--layer-spectra-output", "outputs/layer-spectra.json",
        ],
        invocation_env={"HOME": ".", "OMP_NUM_THREADS": "8"},
        invocation_required_env=["OPENMC_CROSS_SECTIONS"],
        invocation_supplied_env={"OPENMC_CROSS_SECTIONS": "/home/connoravila/nuclear-data/endfb-vii.1-hdf5/cross_sections.xml"},
        invocation_timeout_ms=3600000,
        outputs=[
            {"output_id": "transport-result", "workspace_path": "outputs/transport-result.json",
             "media_type": "application/vnd.avila.shield-transport+json", "state": "collected",
             "sha256": transport_result_sha, "bytes": transport_result_bytes},
            {"output_id": "layer-spectra", "workspace_path": "outputs/layer-spectra.json",
             "media_type": "application/vnd.avila.shield-layer-spectra+json", "state": "collected",
             "sha256": REAL_LAYER_SPECTRA_SHA256, "bytes": REAL_LAYER_SPECTRA_BYTES},
        ],
    )
    transport_receipt["capability"] = strip_program(transport_receipt["capability"])
    write_json(CASE / "receipts/transport.json", transport_receipt)

    # ---- 5. resync package.json hashes for the 4 touched files ----
    package_path = CASE / "package.json"
    package = json.loads(package_path.read_text())
    doc_updates = {
        "case-002-screen-receipt": CASE / "receipts/screen.json",
        "case-002-transport-receipt": CASE / "receipts/transport.json",
    }
    for doc in package["documents"]:
        if doc["document_id"] in doc_updates:
            doc["sha256"] = sha256_file(doc_updates[doc["document_id"]])
    art_updates = {
        "reference-screen-result": screen_result_path,
        "reference-transport-result": transport_result_path,
    }
    for art in package["artifacts"]:
        if art["artifact_id"] in art_updates:
            art["sha256"] = sha256_file(art_updates[art["artifact_id"]])
    write_json(package_path, package)

    print("forged:")
    print(" ", screen_result_path, screen_result_sha, screen_result_bytes, "bytes")
    print(" ", transport_result_path, transport_result_sha, transport_result_bytes, "bytes")
    print(" ", CASE / "receipts/screen.json", sha256_file(CASE / "receipts/screen.json"))
    print(" ", CASE / "receipts/transport.json", sha256_file(CASE / "receipts/transport.json"))
    print("package.json resynced for: case-002-screen-receipt, case-002-transport-receipt, reference-screen-result, reference-transport-result")


if __name__ == "__main__":
    main()
