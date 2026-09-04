#!/usr/bin/env python3
"""Golden-output diff for `avila-core run` across a fixed case matrix.

Used while refactoring the case runner (crates/avila-core-runner/src/case_run.rs)
so a pure code-motion change can be checked for zero behaviour change: capture
the CLI's output for a fixed matrix of cases and flags before touching
anything, then after each change re-run the same matrix and diff the
normalized text against the captured baseline. Only operational fields that
are expected to vary run-to-run are normalized away (timestamps, durations,
elapsed times, workspace paths, run/attempt ids); everything else --
findings, verdicts, receipts, digests, coverage, presentation gates -- must
match byte for byte.

Usage:
    # Capture (or refresh) the golden snapshot before editing anything:
    golden_cli_diff.py capture --cli PATH/TO/avila-core --repo PATH/TO/repo \
        --golden DIR

    # After a change, diff fresh output against the captured snapshot:
    golden_cli_diff.py diff --cli PATH/TO/avila-core --repo PATH/TO/repo \
        --golden DIR

Exit status is 0 when every case in the matrix normalizes identically to the
golden snapshot, 1 otherwise (with a unified diff per mismatching case
printed to stdout).

The matrix itself (which cases, which flags) is fixed in MATRIX below so a
capture and a later diff always compare the same commands.
"""

from __future__ import annotations

import argparse
import difflib
import re
import subprocess
import sys
from pathlib import Path

CASES = [
    "case-000-actinv-aftermatter",
    "case-001-shield-search",
    "case-002-coupled-shield",
    "case-003-thermal-spreader",
    "case-008-mode-selective-quench",
    "case-009-ncsx-copper-discharge",
]

# (label, case, extra_cli_args) -- run against every case in text and JSON,
# plus the extra --plan and --input runs the slice calls for.
MATRIX = (
    [(f"{case}.text", case, []) for case in CASES]
    + [(f"{case}.json", case, ["--json"]) for case in CASES]
    + [
        ("case-001-shield-search.plan", "case-001-shield-search", ["--plan"]),
        (
            "case-008-mode-selective-quench.plan",
            "case-008-mode-selective-quench",
            ["--plan"],
        ),
        (
            "case-001-shield-search.input",
            "case-001-shield-search",
            [
                "--input",
                "candidate={repo}/examples/cases/case-001-shield-search/candidates/reference.json",
            ],
        ),
        (
            "case-008-mode-selective-quench.input",
            "case-008-mode-selective-quench",
            [
                "--input",
                "candidate={repo}/examples/cases/case-008-mode-selective-quench/candidates/mode-selective-dump.json",
            ],
        ),
    ]
)

# Matches an operational field that legitimately varies run to run. Applied
# line by line so a JSON body and the concise text view are both covered
# without needing two parsers.
_OPERATIONAL_LINE = re.compile(
    r"""(?ix)
    (
        \b(timestamp|elapsed|duration|elapsed_seconds|elapsed_ms|
           started_at|finished_at|run_id|attempt_id|workspace)\b
    )
    """
)


def normalize(text: str, repo: Path) -> str:
    """Strip only operational fields: timestamps, durations, workspace
    paths, and run ids. Everything else -- every finding, verdict, receipt,
    digest, coverage line, and presentation gate -- must diff exactly."""
    repo_str = str(repo)
    text = text.replace(repo_str, "<REPO>")
    out_lines = []
    for line in text.splitlines():
        if _OPERATIONAL_LINE.search(line):
            key = _OPERATIONAL_LINE.search(line).group(1).lower()
            out_lines.append(f"<{key} normalized>")
            continue
        out_lines.append(line)
    return "\n".join(out_lines) + "\n"


def run_case(cli: Path, repo: Path, case: str, args: list[str]) -> str:
    case_dir = repo / "examples" / "cases" / case
    formatted_args = [a.format(repo=repo) for a in args]
    proc = subprocess.run(
        [str(cli), "run", str(case_dir), *formatted_args],
        capture_output=True,
        text=True,
        check=False,
    )
    body = proc.stdout + proc.stderr
    body += f"\n<exit {proc.returncode}>\n"
    return body


def capture(cli: Path, repo: Path, golden: Path) -> None:
    golden.mkdir(parents=True, exist_ok=True)
    for label, case, args in MATRIX:
        raw = run_case(cli, repo, case, args)
        normalized = normalize(raw, repo)
        (golden / f"{label}.norm").write_text(normalized)
    print(f"captured {len(MATRIX)} golden files under {golden}")


def diff(cli: Path, repo: Path, golden: Path) -> int:
    failures = 0
    for label, case, args in MATRIX:
        golden_file = golden / f"{label}.norm"
        if not golden_file.is_file():
            print(f"MISSING golden file for {label}: {golden_file}")
            failures += 1
            continue
        raw = run_case(cli, repo, case, args)
        normalized = normalize(raw, repo)
        expected = golden_file.read_text()
        if normalized != expected:
            failures += 1
            print(f"=== DIFF: {label} ===")
            sys.stdout.writelines(
                difflib.unified_diff(
                    expected.splitlines(keepends=True),
                    normalized.splitlines(keepends=True),
                    fromfile=f"golden/{label}",
                    tofile=f"current/{label}",
                )
            )
    if failures:
        print(f"{failures}/{len(MATRIX)} case(s) differ from golden")
    else:
        print(f"all {len(MATRIX)} case(s) match golden")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["capture", "diff"])
    parser.add_argument("--cli", required=True, type=Path, help="avila-core binary")
    parser.add_argument("--repo", required=True, type=Path, help="repository root")
    parser.add_argument("--golden", required=True, type=Path, help="golden snapshot dir")
    args = parser.parse_args()

    cli = args.cli.resolve()
    repo = args.repo.resolve()
    golden = args.golden

    if not cli.is_file():
        parser.error(f"--cli not found: {cli}")
    if not (repo / "examples" / "cases").is_dir():
        parser.error(f"--repo does not look like the Avila Core repository: {repo}")

    if args.mode == "capture":
        capture(cli, repo, golden)
        return 0
    return diff(cli, repo, golden)


if __name__ == "__main__":
    raise SystemExit(main())
