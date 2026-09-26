#!/usr/bin/env python3
"""Bounded EL-02 API counterexamples, through the thin language CLI.

Build first: cargo build -p avila-core-cli --bin avila-core --locked -j 1
Run: python3 reproduce.py --repo /path/to/project-north-star
Use --check-depth to exercise the parser in a resource-limited child.
Only the output directory is written. No engineering runner is invoked.
"""

import argparse
import copy
import json
from pathlib import Path
import resource
import subprocess


def limited_child():
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2, 384 * 1024**2))
    resource.setrlimit(resource.RLIMIT_CPU, (5, 5))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--out", type=Path, default=Path("/tmp/avila-core-el02-review/results"))
    parser.add_argument("--check-depth", action="store_true")
    args = parser.parse_args()
    repo, out = args.repo.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    examples = repo / "examples/language"
    binary = repo / "target/debug/avila-core"
    read = lambda path: json.loads(path.read_text())
    measurement = read(examples / "libraries/measurement-scaling.v1.json")
    thermal = read(examples / "libraries/thermal-expansion.v1.json")
    mp = read(examples / "programs/positive-measurement-pass.program.json")
    tp = read(examples / "programs/clearance-pass.program.json")
    summaries = {}

    def invoke(pfile, lfile, lifecycle):
        command = [str(binary), "language", "analyze", "--program", str(pfile), "--library", str(lfile)]
        for entry in lifecycle:
            command += ["--lifecycle", entry]
        return subprocess.run(command, capture_output=True, text=True, timeout=8, preexec_fn=limited_child)

    def run(name, program, library, *, repin=False, lifecycle=()):
        program = copy.deepcopy(program)
        pfile, lfile = out / f"{name}.program.json", out / f"{name}.library.json"
        pfile.write_text(json.dumps(program, indent=2) + "\n")
        lfile.write_text(json.dumps(library, indent=2) + "\n")
        result = invoke(pfile, lfile, lifecycle)
        if repin and result.returncode in (0, 1):
            report = json.loads(result.stdout)
            if report.get("library"):
                program["library"]["semantic_sha256"] = report["library"]["semantic_sha256"]
                pfile.write_text(json.dumps(program, indent=2) + "\n")
                result = invoke(pfile, lfile, lifecycle)
        if result.returncode not in (0, 1):
            summary = {"exit": result.returncode, "stderr": result.stderr.strip()}
            summaries[name] = summary
            return summary
        report = json.loads(result.stdout)
        (out / f"{name}.analysis.json").write_text(json.dumps(report, indent=2) + "\n")
        summary = {
            "exit": result.returncode, "plan": report["plan"]["state"],
            "findings": [f["kind"] for f in report["findings"]],
            "values": {k: v.get("value") for k, v in report["bindings"].items() if k in {"combined_rate", "remaining_clearance", "dependent", "displacement"}},
        }
        summaries[name] = summary
        return report

    baseline = run("baseline-measurement", mp, measurement)
    summaries["baseline-measurement"]["support"] = baseline["bindings"]["combined_rate"]

    p = copy.deepcopy(mp)
    p["premises"][0].pop("established_by")
    run("unattributed-independence", p, measurement)

    lib = copy.deepcopy(measurement)
    lib["methods"][0]["implementation"]["body"] = "interval.sub(reading_a, reading_b)"
    report = run("false-postcondition", mp, lib, repin=True)
    summaries["false-postcondition"]["obligations"] = report["obligations"]

    lib = copy.deepcopy(measurement)
    lib["methods"][0]["implementation"]["body"] = "calibration"
    report = run("wrong-body-type", mp, lib, repin=True)
    summaries["wrong-body-type"]["binding"] = report["bindings"]["combined_rate"]

    lib = copy.deepcopy(measurement)
    lib["methods"][0]["ensures"][0]["expression"] = "output = nonexistent_function(reading_a)"
    lib["methods"][0]["ensures"][0]["check"] = "nonexistent_checker"
    run("unrecognized-postcondition", mp, lib, repin=True)

    p = copy.deepcopy(mp)
    p["inputs"][0]["binding"]["value"] = {"kind": "nominal", "value": "1", "unit": "mSv/h"}
    run("nominal-payload-enclosure-type", p, measurement)

    p = copy.deepcopy(mp)
    p["inputs"][0]["binding"]["value"]["unit"] = "kg"
    run("wrong-input-unit", p, measurement)

    p = copy.deepcopy(mp)
    p["requirements"][0]["comparison"] = "potato"
    p["requirements"][0]["limit"]["value"] = "not-a-number"
    run("malformed-requirement", p, measurement)

    p = copy.deepcopy(mp)
    p["requirements"][0]["scope"] = "any"
    run("scope-refinement", p, measurement)

    p = copy.deepcopy(mp)
    duplicate = copy.deepcopy(p["inputs"][0])
    duplicate["binding"]["value"].update(lower="20", upper="21")
    p["inputs"].append(duplicate)
    a = run("duplicate-input-a", p, measurement)
    p["inputs"].reverse()
    b = run("duplicate-input-b", p, measurement)
    summaries["duplicate-input-identity"] = {
        "same_semantic_identity": a["program"]["semantic_sha256"] == b["program"]["semantic_sha256"],
        "a_value": a["bindings"]["combined_rate"]["value"],
        "b_value": b["bindings"]["combined_rate"]["value"],
    }

    p = copy.deepcopy(mp)
    p["premises"] = []
    p["body"].append({"bind": "dependent", "infer": {"rule": "interval.add", "arguments": [{"ref": "combined_rate"}, {"ref": "reading_a"}]}})
    p["requirements"][0]["subject"]["ref"] = "dependent"
    report = run("open-obligation-dependent", p, measurement)
    summaries["open-obligation-dependent"]["requirements"] = report["requirements"]
    summaries["open-obligation-dependent"]["dependent"] = report["bindings"]["dependent"]

    p = copy.deepcopy(tp)
    p["body"] = [{"bind": "remaining_clearance", "import": {
        "type": {"quantity_kind": "length", "claim": "enclosure", "geometry": "bracket@2", "scenario": "thermal-soak-steady", "material": "al-6061-t6"},
        "value": {"kind": "enclosure", "lower": "1", "upper": "1", "unit": "mm"},
        "assumptions": [], "source": {"kind": "certificate", "digest": "sha256:" + "0" * 64, "check": "made-up"},
    }}]
    report = run("unreplayed-certificate-import", p, thermal)
    summaries["unreplayed-certificate-import"]["runtime_obligations"] = report["plan"]["runtime_obligations"]

    lib = copy.deepcopy(thermal)
    lib["methods"][0]["ensures"].append({"kind": "relation", "expression": "output = length", "check": "interval_arithmetic"})
    a = run("ensures-order-a", tp, lib, repin=True)
    lib["methods"][0]["ensures"].reverse()
    b = run("ensures-order-b", tp, lib, repin=True)
    summaries["ensures-order-identity"] = {
        "same_semantic_identity": a["library"]["semantic_sha256"] == b["library"]["semantic_sha256"],
        "a_value": a["bindings"]["displacement"].get("value"),
        "b_value": b["bindings"]["displacement"].get("value"),
    }

    p = copy.deepcopy(tp)
    p["entities"]["scenarios"]["thermal-soak-steady"]["operating_domain"]["unit"] = "kg"
    run("wrong-domain-unit", p, thermal)

    p = copy.deepcopy(tp)
    p["entities"]["scenarios"]["thermal-soak-steady"]["operating_domain"].update(lower="400", upper="300")
    run("inverted-operating-domain", p, thermal)

    p = copy.deepcopy(tp)
    p["entities"]["materials"]["other-material"] = copy.deepcopy(p["entities"]["materials"]["al-6061-t6"])
    p["inputs"][0]["type"]["material"] = "other-material"
    run("conflicting-extra-relation", p, thermal)

    lib = copy.deepcopy(thermal)
    lib["methods"][0]["assumes"].append("undeclared-assumption")
    run("undeclared-method-assumption", tp, lib, repin=True)

    p = copy.deepcopy(mp)
    p["requirements"][0]["scenario"] = "field-survey-1"
    run("requirement-scenario", p, measurement)

    p = copy.deepcopy(mp)
    p["body"].insert(0, {"bind": "derived_a", "infer": {"rule": "interval.add", "arguments": [{"ref": "reading_a"}, {"ref": "reading_a"}]}})
    p["body"][1]["arguments"]["reading_a"]["ref"] = "derived_a"
    p["premises"][0]["arguments"]["over"] = ["derived_a", "reading_b"]
    report = run("derived-provenance", p, measurement)
    summaries["derived-provenance"]["obligations"] = report["obligations"]

    run("unknown-lifecycle-state", tp, thermal, lifecycle=["library:thermal-expansion@1=withdrawnn"])

    p = copy.deepcopy(tp)
    p["body"].append({"bind": "unused", "hole": {"quantity_kind": "length", "claim": "enclosure"}})
    run("unreachable-hole", p, thermal)

    p = copy.deepcopy(tp)
    p["body"].reverse()
    report = run("forward-reference-identity", p, thermal)
    summaries["forward-reference-identity"]["semantic_identity_returned"] = report.get("program", {}).get("semantic_sha256")

    if args.check_depth:
        lib = copy.deepcopy(measurement)
        lib["methods"][0]["implementation"]["body"] = "(" * 10000 + "reading_a" + ")" * 10000
        run("bounded-deep-expression-child", mp, lib)

    (out / "summary.json").write_text(json.dumps(summaries, indent=2) + "\n")
    print(json.dumps(summaries, indent=2))


if __name__ == "__main__":
    main()
