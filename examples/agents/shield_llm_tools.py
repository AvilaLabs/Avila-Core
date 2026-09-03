#!/usr/bin/env python3
"""Tools for a language-model designer that searches through Avila Core.

The designer (a connected agent) never runs Core directly and never writes a
verdict. It calls these subcommands, reads what Core reported, and decides
what to propose next. Every candidate, every rationale the designer gives,
and every Core report are recorded, so the arm is reviewable like the
scripted ones.

    init       fix the case, roots, capabilities, budgets, and prior logs for an arm
    brief      print the contract's requirements, the material table, the
               constellation so far (prior logs plus this arm), and the rules
    propose    screen a batch of proposals (JSON list) through Core, python3 only
    transport  run chosen candidates through every step (transport, activation)
    status     budget used, best candidates so far
    finish     write summary.md for the arm

All numbers printed here are copied from Core's --json reports or --log lines.
"""

import argparse
import json
import sys
from decimal import Decimal
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import shield_common as core  # noqa: E402

HEAVY_DENSITY = Decimal("2.0")
THICKNESS_KEY = "thickness_cm"


def use_config(config):
    """Adopt the arm's thickness field name so every helper reads and writes the right key."""
    global THICKNESS_KEY
    THICKNESS_KEY = config.get("thickness_key") or "thickness_cm"


def load_config(out):
    return json.loads((Path(out) / "config.json").read_text())


def save_state(out, state):
    (Path(out) / "state.json").write_text(json.dumps(state, indent=2) + "\n")


def load_state(out):
    return json.loads((Path(out) / "state.json").read_text())


def fmt(value, digits=3):
    if value is None:
        return "-"
    return f"{float(Fraction(str(value))):.{digits}g}"


def layers_text(layers):
    unit = THICKNESS_KEY.split("_")[-1]
    return " + ".join(f"{l[THICKNESS_KEY]} {unit} {l['material']}" for l in layers)


def canonical_layers(layers):
    merged = []
    for layer in layers:
        thickness = Decimal(str(layer.get(THICKNESS_KEY, layer.get("thickness_cm", layer.get("thickness_mm", "0")))))
        if thickness <= 0:
            continue
        if merged and merged[-1]["material"] == layer["material"]:
            merged[-1][THICKNESS_KEY] = str(Decimal(merged[-1][THICKNESS_KEY]) + thickness)
        else:
            merged.append({"material": layer["material"], THICKNESS_KEY: str(thickness)})
    for layer in merged:
        text = format(Decimal(layer[THICKNESS_KEY]), "f")
        layer[THICKNESS_KEY] = text.rstrip("0").rstrip(".") if "." in text else text
    return merged


def signature(layers):
    return tuple((l["material"], l[THICKNESS_KEY]) for l in layers)


def read_log(path):
    path = Path(path)
    if not path.is_file():
        return []
    rows = []
    for line in path.read_text().splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def candidate_layers(row, log_path):
    for supplied in row.get("supplied_inputs", []):
        if supplied.get("input_id") != "candidate":
            continue
        path = Path(supplied.get("path", ""))
        if not path.is_file():
            path = Path(log_path).parent / "candidates" / path.name
        if path.is_file():
            try:
                return canonical_layers(json.loads(path.read_text())["layers"])
            except (KeyError, ValueError):
                return None
    return None


def transported(row):
    """A row counts as fully evaluated when every declared step ran or was
    reused; a screen-only run leaves its later steps `not_run`."""
    steps = row.get("steps", [])
    return bool(steps) and all(list(s)[1] in ("executed", "reused") for s in steps)


def verdict_map(row):
    return {v["requirement_id"]: v for v in row.get("verdicts", [])}


def all_pass(row):
    verdicts = row.get("verdicts", [])
    return bool(verdicts) and all(v.get("status") == "pass" for v in verdicts)


def interval_text(v):
    if v is None:
        return "-"
    status = v.get("status", "?")
    if v.get("lower") is not None:
        return f"[{fmt(v['lower'])}, {fmt(v['upper'])}] {status}"
    if v.get("nominal") is not None:
        return f"{fmt(v['nominal'])} {status}"
    return status


def constellation_rows(config, out):
    rows = []
    sources = [(p, "prior") for p in config.get("prior_logs", [])] + [(str(Path(out) / "campaign-log.jsonl"), "this arm")]
    for log_path, origin in sources:
        for row in read_log(log_path):
            layers = candidate_layers(row, log_path)
            if layers is None or not transported(row):
                continue
            rows.append({"origin": origin, "layers": layers, "verdicts": verdict_map(row), "all_pass": all_pass(row)})
    return rows


def worst_margin(verdicts):
    values = []
    for v in verdicts.values():
        if v.get("status") in ("pass", "fail", "inconclusive") and v.get("margin") is not None:
            values.append(float(Fraction(str(v["margin"]))))
    return min(values) if values else None


def mass_of(verdicts):
    for rid, v in verdicts.items():
        if v.get("unit") == "kg" and v.get("nominal") is not None:
            return float(Fraction(str(v["nominal"])))
    return None


def print_table(rows):
    if not rows:
        print("(none)")
        return
    ids = sorted({rid for r in rows for rid in r["verdicts"]})
    short = {rid: rid.replace("SHIELD-", "") for rid in ids}
    header = ["origin", "design"] + [short[rid] for rid in ids] + ["worst margin", "all PASS"]
    print(" | ".join(header))
    for r in sorted(rows, key=lambda r: -(worst_margin(r["verdicts"]) if worst_margin(r["verdicts"]) is not None else -1e9)):
        cells = [r["origin"], layers_text(r["layers"])] + [interval_text(r["verdicts"].get(rid)) for rid in ids]
        cells += [fmt(worst_margin(r["verdicts"])), "yes" if r["all_pass"] else "no"]
        print(" | ".join(cells))


def run(config, candidate_path, transport, out):
    capabilities = dict(config["screen_capabilities"])
    environment = None
    if transport:
        capabilities.update(config["transport_capabilities"])
        environment = config["environment"] or None
    extra = ["--expect-manifest", config["manifest_sha256"]] if config.get("manifest_sha256") else None
    return core.run_core(
        config["core"], config["case"], candidate_path,
        source_roots=config["source_roots"], capabilities=capabilities,
        environment=environment, log=Path(out) / "campaign-log.jsonl", extra_args=extra,
    )


def cmd_init(args):
    out = Path(args.out)
    (out / "candidates").mkdir(parents=True, exist_ok=True)
    source_roots = {}
    for spec in args.source_root:
        name, _, path = spec.partition("=")
        source_roots[name] = path
    manifest = Path(args.case) / "package.json"
    manifest_sha256 = "sha256:" + __import__("hashlib").sha256(manifest.read_bytes()).hexdigest()
    transport_capabilities = {}
    if args.openmc_python:
        transport_capabilities["openmc-python"] = args.openmc_python
    for spec in args.capability:
        name, _, path = spec.partition("=")
        transport_capabilities[name] = path
    environment = {}
    if args.cross_sections and args.cross_sections != "none":
        environment["OPENMC_CROSS_SECTIONS"] = args.cross_sections
    for spec in args.env:
        key, _, value = spec.partition("=")
        environment[key] = value
    config = {
        "core": args.core, "case": args.case, "source_roots": source_roots, "manifest_sha256": manifest_sha256,
        "screen_capabilities": {"python3": args.python3},
        "transport_capabilities": transport_capabilities,
        "environment": environment,
        "prior_logs": args.prior_log, "materials": args.materials,
        "screen_budget": args.screen_budget, "transport_budget": args.transport_budget,
        "candidate_schema": args.candidate_schema, "thickness_key": args.thickness_key, "probe_thickness": args.probe_thickness,
    }
    (out / "config.json").write_text(json.dumps(config, indent=2) + "\n")
    save_state(str(out), {"screens": 0, "transports": 0, "next_index": 0, "screened": {}})
    print(f"initialized {out}: transport budget {args.transport_budget}, screen budget {args.screen_budget}, {len(args.prior_log)} prior log(s); package pinned at {manifest_sha256}")


def cmd_brief(args):
    out = args.out
    config = load_config(out)
    use_config(config)
    state = load_state(out)
    materials = core.load_materials(Path(config["materials"]) / "materials.json")
    # A bootstrap screen run exposes the compiled requirements.
    probe = Path(out) / "candidates" / "brief-probe.json"
    core.write_candidate(probe, "brief-probe", [(sorted(materials)[0], config.get("probe_thickness", "5"))],
                         schema=config.get("candidate_schema"), thickness_key=config.get("thickness_key"))
    report = run(config, probe, False, out)
    compiled = report.get("compile", {}).get("compiled", {})
    print("# Contract")
    print(compiled.get("question", "").strip())
    print()
    print("# Requirements (id | comparison | limit | basis | statement)")
    for req in compiled.get("requirements", []):
        limit = req.get("limit", {})
        print(f"{req.get('requirement_id')} | {req.get('comparison')} | {limit.get('value')} {limit.get('unit')} | {req.get('basis', {}).get('kind')} | {req.get('statement', '')}")
    print()
    if all("density_g_cm3" in spec and "composition" in spec for spec in materials.values()):
        print("# Materials (name | density g/cm3 | heavy? | composition)")
        for name, spec in sorted(materials.items()):
            density = Decimal(spec["density_g_cm3"])
            print(f"{name} | {density} | {'heavy' if density > HEAVY_DENSITY else 'moderator'} | {json.dumps(spec['composition']['elements'])}")
    else:
        print("# Materials (name | properties as the table declares them)")
        for name, spec in sorted(materials.items()):
            print(f"{name} | " + ", ".join(f"{k} {v}" for k, v in spec.items() if not isinstance(v, (dict, list))))
    print()
    print("# Constellation: every transported design on record, best worst-case margin first")
    print_table(constellation_rows(config, out))
    print()
    print(f"# Budget: {state['transports']}/{config['transport_budget']} transports used, {state['screens']}/{config['screen_budget']} screens used")
    print()
    print("# Rules")
    unit = THICKNESS_KEY.split("_")[-1]
    print(f"- Propose candidates as a JSON list of {{\"layers\": [{{\"material\": ..., \"{THICKNESS_KEY}\": ...}}, ...], \"rationale\": ...}}; thicknesses in whole {unit}, layers listed from the source outward.")
    print("- The screen establishes nothing; only transport decides the bounded requirements. Choose which screened candidates to transport yourself.")
    print("- Core's verdicts are final. You may not edit the contract, the package, or any file; you may only propose and choose.")
    print("- Every proposal's rationale is recorded. Say what evidence in the constellation it rests on.")


def cmd_propose(args):
    out = args.out
    config = load_config(out)
    use_config(config)
    state = load_state(out)
    proposals = json.loads(Path(args.proposals).read_text())
    materials = core.load_materials(Path(config["materials"]) / "materials.json")
    notes = Path(out) / "designer-notes.jsonl"
    results = []
    for proposal in proposals:
        if state["screens"] >= config["screen_budget"]:
            print("screen budget exhausted")
            break
        layers = canonical_layers(proposal["layers"])
        unknown = [l["material"] for l in layers if l["material"] not in materials]
        if unknown or not layers:
            print(f"rejected proposal (unknown material {unknown} or empty): {proposal}")
            continue
        sig = signature(layers)
        key = json.dumps(sig)
        if key in state["screened"]:
            print(f"already screened as {state['screened'][key]}: {layers_text(layers)}")
            continue
        candidate_id = f"l-{state['next_index']:04d}"
        state["next_index"] += 1
        path = Path(out) / "candidates" / f"{candidate_id}.json"
        core.write_candidate(path, candidate_id, [(l["material"], l[THICKNESS_KEY]) for l in layers],
                             schema=config.get("candidate_schema"), thickness_key=config.get("thickness_key"))
        report = run(config, path, False, out)
        state["screens"] += 1
        state["screened"][key] = candidate_id
        margins = {m["requirement_id"]: m for m in report.get("margins", [])}
        with notes.open("a") as handle:
            handle.write(json.dumps({"candidate_id": candidate_id, "layers": layers, "rationale": proposal.get("rationale", ""),
                                     "stage": "screen", "status": report.get("status"),
                                     "margins": {k: {kk: v.get(kk) for kk in ("status", "nominal", "margin", "unit")} for k, v in margins.items()}}) + "\n")
        results.append((candidate_id, layers, margins, report.get("status")))
    save_state(out, state)
    print("id | design | status | " + " | ".join(sorted({rid.replace('SHIELD-', '') for _, _, m, _ in results for rid in m})))
    for candidate_id, layers, margins, status in results:
        cells = [candidate_id, layers_text(layers), status]
        for rid in sorted(margins):
            m = margins[rid]
            cells.append(f"{m.get('status')} {fmt(m.get('nominal'))} (margin {fmt(m.get('margin'))})" if m.get("status") in ("pass", "fail", "inconclusive") else m.get("status", "-"))
        print(" | ".join(cells))
    print(f"screens used {state['screens']}/{config['screen_budget']}")


def cmd_transport(args):
    out = args.out
    config = load_config(out)
    use_config(config)
    state = load_state(out)
    notes = Path(out) / "designer-notes.jsonl"
    for candidate_id in args.ids:
        if state["transports"] >= config["transport_budget"]:
            print("transport budget exhausted")
            break
        path = Path(out) / "candidates" / f"{candidate_id}.json"
        if not path.is_file():
            print(f"no such candidate: {candidate_id}")
            continue
        candidate = json.loads(path.read_text())
        report = run(config, path, True, out)
        state["transports"] += 1
        margins = {m["requirement_id"]: m for m in report.get("margins", [])}
        with notes.open("a") as handle:
            handle.write(json.dumps({"candidate_id": candidate_id, "layers": candidate["layers"], "rationale": args.rationale or "",
                                     "stage": "transport", "status": report.get("status"), "campaign_sha256": report.get("campaign", {}).get("campaign_sha256"),
                                     "margins": {k: {kk: v.get(kk) for kk in ("status", "lower", "upper", "nominal", "margin", "unit")} for k, v in margins.items()}}) + "\n")
        print(f"{candidate_id} | {layers_text(candidate['layers'])} | {report.get('status')}")
        for rid in sorted(margins):
            print(f"   {rid}: {interval_text(margins[rid])} margin {fmt(margins[rid].get('margin'))} {margins[rid].get('unit', '')}")
        verdicts = margins
        print(f"   all PASS: {'yes' if verdicts and all(v.get('status') == 'pass' for v in verdicts.values()) else 'no'}")
    save_state(out, state)
    print(f"transports used {state['transports']}/{config['transport_budget']}")


def cmd_status(args):
    out = args.out
    config = load_config(out)
    use_config(config)
    state = load_state(out)
    rows = [r for r in constellation_rows(config, out) if r["origin"] == "this arm"]
    passing = sorted((mass_of(r["verdicts"]), layers_text(r["layers"])) for r in rows if r["all_pass"] and mass_of(r["verdicts"]) is not None)
    print(f"transports used {state['transports']}/{config['transport_budget']}; screens used {state['screens']}/{config['screen_budget']}")
    print(f"all-PASS designs this arm: {len(passing)}")
    for mass, text in passing:
        print(f"  {mass:.1f} kg  {text}")
    print("transported this arm, best worst-case margin first:")
    print_table(rows)


def cmd_finish(args):
    out = args.out
    config = load_config(out)
    use_config(config)
    state = load_state(out)
    rows = [r for r in constellation_rows(config, out) if r["origin"] == "this arm"]
    passing = sorted((mass_of(r["verdicts"]), layers_text(r["layers"])) for r in rows if r["all_pass"] and mass_of(r["verdicts"]) is not None)
    lines = ["# Language-model designer arm", "",
             f"{state['screens']} candidates screened; {state['transports']} sent to transport; {len(passing)} passed every requirement Core evaluated.", ""]
    if passing:
        lines += ["Lightest all-PASS design: " + f"{passing[0][1]} at {passing[0][0]:.1f} kg.", ""]
    lines += ["| design | worst real margin | all PASS |", "| --- | ---: | --- |"]
    for r in sorted(rows, key=lambda r: -(worst_margin(r["verdicts"]) or -1e9)):
        lines.append(f"| {layers_text(r['layers'])} | {fmt(worst_margin(r['verdicts']))} | {'yes' if r['all_pass'] else 'no'} |")
    lines += ["", "The designer proposed candidates and chose finalists; every status, interval, and margin above is copied from Avila Core's reports. Its rationales are in `designer-notes.jsonl`."]
    (Path(out) / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("init")
    p.add_argument("--out", required=True)
    p.add_argument("--core", default="target/debug/avila-core")
    p.add_argument("--case", default="examples/cases/case-002-coupled-shield")
    p.add_argument("--materials", default="examples/capabilities/shield-coupled")
    p.add_argument("--source-root", action="append", default=[], metavar="NAME=PATH")
    p.add_argument("--python3", default="/usr/bin/python3")
    p.add_argument("--openmc-python", default=None, help="shielding shortcut: the OpenMC interpreter as capability `openmc-python`")
    p.add_argument("--cross-sections", default=None, help="shielding shortcut: value for OPENMC_CROSS_SECTIONS")
    p.add_argument("--capability", action="append", default=[], metavar="NAME=PATH", help="a capability the full evaluation needs beyond python3 (repeatable)")
    p.add_argument("--env", action="append", default=[], metavar="KEY=VALUE", help="an environment value an execution declares (repeatable)")
    p.add_argument("--prior-log", action="append", default=[])
    p.add_argument("--screen-budget", type=int, default=400)
    p.add_argument("--transport-budget", type=int, default=40)
    p.add_argument("--candidate-schema", default=None, help="candidate schema id when the case is not the shielding one")
    p.add_argument("--thickness-key", default=None, help="layer thickness field name when it is not thickness_cm")
    p.add_argument("--probe-thickness", default="5")
    p.set_defaults(func=cmd_init)
    for name, func in (("brief", cmd_brief), ("status", cmd_status), ("finish", cmd_finish)):
        p = sub.add_parser(name)
        p.add_argument("--out", required=True)
        p.set_defaults(func=func)
    p = sub.add_parser("propose")
    p.add_argument("--out", required=True)
    p.add_argument("--proposals", required=True, help="JSON list of {layers, rationale}")
    p.set_defaults(func=cmd_propose)
    p = sub.add_parser("transport")
    p.add_argument("--out", required=True)
    p.add_argument("--rationale", default="")
    p.add_argument("ids", nargs="+")
    p.set_defaults(func=cmd_transport)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
