#!/usr/bin/env python3
"""Attempt 8: reuse GENUINE evidence from a real, honestly-passing, heavier
candidate's real run, misattributed to a much lighter candidate via forged
receipts (screen + transport only; activation is left to run for real on
the borrowed real spectra, so it is not forged at all).

Donor: workspaces/CASE-002/20260903T163604700Z-1456062, a real run of
campaign-rev2/posthoc/poly105-lead5.json (105 cm polyethylene + 5 cm lead,
genuinely all-PASS per RESULTS.md and reconfirmed live).
Target: candidates/adv-cheat-light.json (1 cm bare polyethylene).
"""
import hashlib
import json
import pathlib
import shutil

CASE = pathlib.Path("examples/cases/case-002-coupled-shield")
CANDIDATE_PATH = pathlib.Path("workspaces/adversarial/candidates/adv-cheat-light.json")
DONOR = pathlib.Path("workspaces/CASE-002/20260903T163604700Z-1456062")

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
COMPILED_SNAPSHOT_SHA256 = "sha256:2f76e8428d5ce664bae8d9dbfc77728064f2b214d1fa8c33e30886d30284d780"


def sha256_file(path: pathlib.Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def candidate_input():
    data = CANDIDATE_PATH.read_bytes()
    return ("input:candidate", "sha256:" + hashlib.sha256(data).hexdigest(), len(data), "inputs/candidate.json", "application/vnd.avila.shield-candidate+json")


def write_json(path, obj):
    path.write_text(json.dumps(obj, indent=2) + "\n")


def strip_program(cap):
    return {k: v for k, v in cap.items() if k != "program"}


def build_receipt(step_id, capability_type_id, adapter, capability, parameters,
                   fixed_inputs, invocation_arguments, invocation_env,
                   invocation_required_env, invocation_supplied_env,
                   invocation_timeout_ms, outputs):
    inputs = []
    cand_evidence, cand_sha, cand_bytes, cand_wp, cand_mt = candidate_input()
    inputs.append({"input_slot": "candidate", "evidence_id": cand_evidence,
                    "workspace_path": cand_wp, "media_type": cand_mt,
                    "sha256": cand_sha, "bytes": cand_bytes})
    for slot, (evidence_id, sha256, nbytes, wp, mt) in fixed_inputs.items():
        inputs.append({"input_slot": slot, "evidence_id": evidence_id,
                        "workspace_path": wp, "media_type": mt,
                        "sha256": sha256, "bytes": nbytes})
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
        "invocation_sha256": "sha256:" + "0" * 64,
        "process": {"started_at": "2026-09-03T00:00:00.000Z", "finished_at": "2026-09-03T00:00:00.000Z",
                    "duration_ms": 0, "exit_status": 0, "timed_out": False},
        "logs": [
            {"stream": "stdout", "workspace_path": "logs/stdout.log", "sha256": "sha256:" + hashlib.sha256(b"").hexdigest(), "bytes": 0},
            {"stream": "stderr", "workspace_path": "logs/stderr.log", "sha256": "sha256:" + hashlib.sha256(b"").hexdigest(), "bytes": 0},
        ],
        "outputs": outputs,
        "runner": {"runner": "avila.core/cli-rust@0.1.0", "os": "linux", "arch": "x86_64"},
        "status": "completed",
        "limitations": ["FORGED by the adversarial arm: these outputs are genuine, but were produced by a real run of a different candidate (105 cm polyethylene + 5 cm lead), not by running this step over these inputs."],
        "notice": "An execution receipt is process evidence: it binds the exact capability, staged input bytes, invocation, process outcome, logs, and produced output bytes of one step. It does not establish scientific correctness, qualification, practical suitability, or regulatory suitability.",
    }


def main():
    # 1. copy the donor's REAL outputs over the reference candidate's
    #    committed "expected" files (overwrite in place -- adding new
    #    artifacts with unrecognized evidence_ids fails verify_bindings, so
    #    this must reuse the existing evidence_id bindings, as in Attempt 6).
    screen_result_path = CASE / "expected/screen-result.json"
    transport_result_path = CASE / "expected/transport-result.json"
    layer_spectra_path = CASE / "expected/layer-spectra.json"
    shutil.copyfile(DONOR / "screen/outputs/screen-result.json", screen_result_path)
    shutil.copyfile(DONOR / "transport/outputs/transport-result.json", transport_result_path)
    shutil.copyfile(DONOR / "transport/outputs/layer-spectra.json", layer_spectra_path)

    screen_sha, screen_bytes = sha256_file(screen_result_path), screen_result_path.stat().st_size
    transport_sha, transport_bytes = sha256_file(transport_result_path), transport_result_path.stat().st_size
    spectra_sha, spectra_bytes = sha256_file(layer_spectra_path), layer_spectra_path.stat().st_size

    # 2. forge receipts/screen.json (inputs honestly describe the target
    #    candidate; output points at the donor's real screen-result)
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
                  "sha256": screen_sha, "bytes": screen_bytes}],
    )
    screen_receipt["capability"] = strip_program(screen_receipt["capability"])
    write_json(CASE / "receipts/screen.json", screen_receipt)

    # 3. forge receipts/transport.json (both outputs point at the donor's
    #    real transport-result and real layer-spectra)
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
             "sha256": transport_sha, "bytes": transport_bytes},
            {"output_id": "layer-spectra", "workspace_path": "outputs/layer-spectra.json",
             "media_type": "application/vnd.avila.shield-layer-spectra+json", "state": "collected",
             "sha256": spectra_sha, "bytes": spectra_bytes},
        ],
    )
    transport_receipt["capability"] = strip_program(transport_receipt["capability"])
    write_json(CASE / "receipts/transport.json", transport_receipt)

    # 4. resync package.json (3 artifacts + 2 documents touched;
    #    receipts/activation.json and its artifacts are untouched on purpose
    #    -- the spectra hash changed, so activation will genuinely re-execute)
    package_path = CASE / "package.json"
    package = json.loads(package_path.read_text())
    doc_updates = {"case-002-screen-receipt": CASE / "receipts/screen.json",
                   "case-002-transport-receipt": CASE / "receipts/transport.json"}
    for doc in package["documents"]:
        if doc["document_id"] in doc_updates:
            doc["sha256"] = sha256_file(doc_updates[doc["document_id"]])
    art_updates = {"reference-screen-result": screen_result_path,
                   "reference-transport-result": transport_result_path,
                   "reference-layer-spectra": layer_spectra_path}
    for art in package["artifacts"]:
        if art["artifact_id"] in art_updates:
            art["sha256"] = sha256_file(art_updates[art["artifact_id"]])
    write_json(package_path, package)

    print("forged (genuine-evidence variant):")
    print(" screen-result <- donor, ", screen_sha, screen_bytes, "bytes")
    print(" transport-result <- donor, ", transport_sha, transport_bytes, "bytes")
    print(" layer-spectra <- donor, ", spectra_sha, spectra_bytes, "bytes")
    print(" receipts/screen.json, receipts/transport.json rewritten; package.json resynced (5 fields)")
    print(" receipts/activation.json left untouched -> should force real re-execution on borrowed real spectra")


if __name__ == "__main__":
    main()
