#!/usr/bin/env python3
"""A scripted designer for the shielding configuration search.

It proposes candidate layered slabs, runs each through Avila Core with the
screening capability only, ranks the candidates the screen and the exact
mass and thickness requirements accept, then runs the best few with the
transport capability supplied so the bounded requirement is evaluated. It
authors candidates and reads reports; it never constructs a verdict, and it
cannot make the screen's nominal PASS count as the bounded one.
"""

import argparse
import json
import random
import subprocess
import sys
from fractions import Fraction
from pathlib import Path

import shield_review


def run_core(core, case, candidate, args, transport, log):
    command = [
        core, "run", str(case), "--json", "--input", f"candidate={candidate}",
        "--source-root", f"case={args.case}",
        "--source-root", f"shielding={args.shielding}",
        "--source-root", f"agents={args.agents}",
        "--source-root", f"nuclear-data={args.nuclear_data}",
        "--capability", f"python3={args.python3}",
        "--log", str(log),
    ]
    if transport:
        command += ["--capability", f"openmc-python={args.openmc_python}", "--env", f"OPENMC_CROSS_SECTIONS={args.cross_sections}"]
    completed = subprocess.run(command, capture_output=True, text=True)
    if not completed.stdout.strip():
        raise SystemExit(f"core produced no report for {candidate}:\n{completed.stderr}")
    return json.loads(completed.stdout)


def margin(report, requirement):
    for entry in report.get("margins", []):
        if entry["requirement_id"] == requirement:
            return entry
    return None


def number(text):
    """Core reports exact canonical values: integers, decimals, or rationals."""
    return Fraction(text)


def show(value):
    """Four significant digits for people; the log keeps the exact value."""
    return "-" if value is None else f"{float(Fraction(value)):.4g}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core", default="target/debug/avila-core")
    parser.add_argument("--case", default="examples/cases/case-001-shield-search")
    parser.add_argument("--shielding", default="examples/capabilities/shielding")
    parser.add_argument("--agents", default="examples/agents")
    parser.add_argument("--nuclear-data", required=True)
    parser.add_argument("--cross-sections", required=True, help="value for OPENMC_CROSS_SECTIONS")
    parser.add_argument("--python3", default="/usr/bin/python3")
    parser.add_argument("--openmc-python", required=True)
    parser.add_argument("--out", default="workspaces/shield-search")
    parser.add_argument("--candidates", type=int, default=200)
    parser.add_argument("--finalists", type=int, default=3)
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()

    out = Path(args.out)
    (out / "candidates").mkdir(parents=True, exist_ok=True)
    (out / "reviews").mkdir(parents=True, exist_ok=True)
    log = out / "campaign-log.jsonl"
    review_log = out / "review-log.jsonl"
    policy_path = Path(args.case, "agent-review-policy.json")
    review_policy, review_policy_sha256 = shield_review.load_policy(policy_path)
    reviewer_sha256 = shield_review.file_identity(shield_review.__file__)
    materials = json.loads(Path(args.shielding, "materials.json").read_text())["materials"]
    names = sorted(materials)
    rng = random.Random(args.seed)

    screened = []
    for index in range(args.candidates):
        # Total thickness near the 100 cm limit, split into one to three
        # layers of whole 5 cm; the designer knows nothing else about shielding.
        total = rng.choice(range(60, 105, 5))
        layer_count = rng.choice([1, 2, 3])
        cuts = sorted(rng.sample(range(5, total, 5), layer_count - 1)) if layer_count > 1 else []
        bounds = [0, *cuts, total]
        layers = [
            {"material": rng.choice(names), "thickness_cm": str(bounds[i + 1] - bounds[i])}
            for i in range(layer_count)
        ]
        candidate = {"schema": "avila.shielding/candidate/v1", "candidate_id": f"c-{index:04d}", "layers": layers}
        path = out / "candidates" / f"c-{index:04d}.json"
        path.write_text(json.dumps(candidate, indent=2) + "\n")
        report = run_core(args.core, args.case, path, args, transport=False, log=log)
        r1, r3, r4 = (margin(report, key) for key in ("SHIELD-R1-screen", "SHIELD-R3-mass", "SHIELD-R4-thickness"))
        feasible = all(entry and entry["status"] == "pass" for entry in (r1, r3, r4))
        screen_dose = r1 and r1.get("nominal")
        mass = r3 and (r3.get("nominal") or r3.get("upper"))
        screened.append({"id": candidate["candidate_id"], "path": str(path), "layers": layers, "status": report["status"],
                         "screen_dose": screen_dose, "mass_kg": mass,
                         "thickness_cm": r4 and r4.get("nominal"), "feasible": feasible})
        print(f"{candidate['candidate_id']}: {'feasible' if feasible else 'rejected'} screen={show(screen_dose)} uSv/h mass={show(mass)} kg", file=sys.stderr)

    # The screen is known to be optimistic, so finalists are the feasible
    # candidates with the most screen margin, not the lightest; mass is a
    # requirement here, not the objective.
    feasible = [entry for entry in screened if entry["feasible"] and entry["screen_dose"] is not None]
    feasible.sort(key=lambda entry: number(entry["screen_dose"]))
    # Two candidates with the same layers are the same design; transport is
    # not spent twice on it.
    finalists, seen, duplicates = [], set(), 0
    for entry in feasible:
        key = tuple((layer["material"], layer["thickness_cm"]) for layer in entry["layers"])
        if key in seen:
            duplicates += 1
            continue
        seen.add(key)
        finalists.append(entry)
        if len(finalists) == args.finalists:
            break
    results = []
    for entry in finalists:
        report = run_core(args.core, args.case, entry["path"], args, transport=True, log=log)
        r2 = margin(report, "SHIELD-R2-transport")
        candidate = json.loads(Path(entry["path"]).read_text())
        review = shield_review.review_candidate(
            report,
            candidate,
            shield_review.file_identity(entry["path"]),
            review_policy,
            review_policy_sha256,
            reviewer_sha256,
        )
        review_path = out / "reviews" / f"{entry['id']}.json"
        review_path.write_text(json.dumps(review, indent=2) + "\n")
        with review_log.open("a") as file:
            file.write(json.dumps({
                "candidate_id": entry["id"],
                "disposition": review["disposition"],
                "actions": review["actions"],
                "review_request_sha256": review["review_request"]["request_sha256"],
                "record_sha256": review["record_sha256"],
                "record": str(review_path),
            }, separators=(",", ":")) + "\n")
        results.append({
            **entry,
            "transport": r2,
            "workspace": report.get("execution", {}).get("workspace"),
            "review": review,
            "review_path": str(review_path),
        })
        print(
            f"{entry['id']}: agent {review['disposition']} after transport "
            f"{r2 and r2['status']} [{show(r2 and r2.get('lower'))}, "
            f"{show(r2 and r2.get('upper'))}] uSv/h",
            file=sys.stderr,
        )

    lines = ["# Shielding configuration search", "", f"{len(screened)} candidates screened; {len(feasible)} passed the screen, mass, and thickness requirements; {len(finalists)} finalists ran transport.", "",
             "| candidate | layers | screen uSv/h | mass kg | transport uSv/h | transport verdict | agent routing |", "| --- | --- | ---: | ---: | ---: | --- | --- |"]
    for entry in results:
        r2 = entry["transport"] or {}
        layers = " + ".join(f"{layer['thickness_cm']} cm {layer['material']}" for layer in entry["layers"])
        review = entry["review"]
        lines.append(f"| {entry['id']} | {layers} | {show(entry['screen_dose'])} | {show(entry['mass_kg'])} | [{show(r2.get('lower'))}, {show(r2.get('upper'))}] | {r2.get('status')} ({r2.get('rule')}) | {review['disposition']} ({len(review['actions'])} actions) |")
    counts = {}
    for entry in results:
        status = (entry["transport"] or {}).get("status") or "not evaluated"
        counts[status] = counts.get(status, 0) + 1
    tally = ", ".join(f"{count} {status}" for status, count in sorted(counts.items()))
    presented = [
        entry for entry in results
        if entry["review"]["disposition"] == "present_to_user"
    ]
    lines += ["", f"Transport verdicts for the {len(results)} finalists: {tally}." if results else "",
              f"User presentation queue: {len(presented)} candidate(s). The optional agent gate returned {len(results) - len(presented)} to the designer.",
              f"{duplicates} feasible candidate(s) skipped as duplicates of a design already sent to transport." if duplicates else "",
              "An `inconclusive` verdict means the statistical interval straddles the limit; more particles narrow it, and a candidate whose nominal sits above the limit is unlikely to pass.", "",
              "Values are shown to four significant digits; the campaign log keeps every exact value and identity. Each staged-review record binds Core's exact review request, dossier identities, policy, instructions, agent implementation, disposition, and actions.", "",
              "The screen's PASS is nominal and establishes nothing; only the transport verdict is bounded, and its interval is statistical only. Practical-review routing controls presentation and never changes a Core verdict."]
    (out / "summary.md").write_text("\n".join(lines) + "\n")
    print((out / "summary.md").read_text())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
