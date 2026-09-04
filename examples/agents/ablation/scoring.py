#!/usr/bin/env python3
"""Pure scoring functions for EXP-002 trials.

Every function here takes already-loaded data (parsed JSON rows, transcript
lines) and returns a plain dict or list; none of them opens a file, runs a
subprocess, or calls Core or `claude`. That split is what makes `score`
deterministic and unit-testable: `harness.py` does all the file I/O and hands
the results to this module.
"""

import json
from fractions import Fraction

FORBIDDEN_WORDS = ("verdict", "margin", "pass", "fail", "coverage", "envelope", "refus")
CORE_TIME_KEYS = ("started_at", "finished_at", "duration_ms")


def _to_float(value):
    if value is None:
        return None
    try:
        return float(Fraction(str(value)))
    except (ValueError, ZeroDivisionError):
        return None


def row_all_pass(row):
    verdicts = row.get("verdicts", [])
    return bool(verdicts) and all(v.get("status") == "pass" for v in verdicts)


def row_mass(row):
    for v in row.get("verdicts", []):
        if v.get("unit") == "kg" and v.get("nominal") is not None:
            return _to_float(v["nominal"])
    return None


def row_out_of_envelope(row):
    return any(v.get("rule") == "not_evaluated.outside_qualification" for v in row.get("verdicts", []))


def row_inconclusive(row):
    return any(v.get("status") == "inconclusive" for v in row.get("verdicts", []))


def row_not_evaluated_other(row):
    """not_evaluated for a reason other than the qualification envelope (a
    step simply not run, e.g. a screen-only call) -- tracked separately so it
    is not confused with a refusal."""
    return any(
        v.get("status") == "not_evaluated" and v.get("rule") != "not_evaluated.outside_qualification"
        for v in row.get("verdicts", [])
    )


def evaluation_log_metrics(rows):
    """Metrics EXP-002 names, computed from one arm's final evaluation-log
    rows (already filtered to the candidates that arm considered final /
    fully evaluated, in the order they were evaluated)."""
    total = len(rows)
    invalid_refused = sum(1 for r in rows if r.get("status") == "rejected")
    out_of_envelope = sum(1 for r in rows if row_out_of_envelope(r))
    inconclusive = sum(1 for r in rows if row_inconclusive(r))
    not_evaluated_other = sum(1 for r in rows if row_not_evaluated_other(r))

    first_pass_index = None
    for i, row in enumerate(rows):
        if row_all_pass(row):
            first_pass_index = i + 1  # 1-indexed: "evaluations to first all-PASS"
            break

    passing_masses = [row_mass(r) for r in rows if row_all_pass(r)]
    passing_masses = [m for m in passing_masses if m is not None]

    return {
        "candidates_evaluated": total,
        "success": first_pass_index is not None,
        "evaluations_to_first_all_pass": first_pass_index,
        "lightest_all_pass_mass_kg_m2": min(passing_masses) if passing_masses else None,
        "invalid_or_refused": invalid_refused,
        "out_of_envelope": out_of_envelope,
        "inconclusive": inconclusive,
        "not_evaluated_other": not_evaluated_other,
    }


def config_hash(config, common_module):
    """Delegates to common.canonical_json_hash so both harness.py and tests
    use the identical definition of "the same config"."""
    return common_module.canonical_json_hash(config)


def parse_bash_calls(transcript_lines, command_substring):
    """From parsed stream-json lines, every Bash tool call whose command
    contains `command_substring`, as {command, start_ts, end_ts, wall_s,
    is_error}, matched by tool_use_id between the assistant's tool_use event
    and the following user tool_result event. Timestamps are ISO 8601
    strings from the stream; wall_s is None when a timestamp is missing."""
    import datetime

    def parse_ts(text):
        if not text:
            return None
        return datetime.datetime.fromisoformat(text.replace("Z", "+00:00"))

    pending = {}
    calls = []
    for entry in transcript_lines:
        etype = entry.get("type")
        if etype == "assistant":
            for block in entry.get("message", {}).get("content", []):
                if block.get("type") == "tool_use" and block.get("name") == "Bash":
                    command = block.get("input", {}).get("command", "")
                    if command_substring in command:
                        pending[block["id"]] = {"command": command, "start_ts": entry.get("timestamp")}
        elif etype == "user":
            for block in entry.get("message", {}).get("content", []):
                tool_use_id = block.get("tool_use_id")
                if tool_use_id in pending:
                    call = pending.pop(tool_use_id)
                    call["end_ts"] = entry.get("timestamp")
                    call["is_error"] = bool(block.get("is_error"))
                    start, end = parse_ts(call["start_ts"]), parse_ts(call["end_ts"])
                    call["wall_s"] = (end - start).total_seconds() if start and end else None
                    calls.append(call)
    return calls


LIVE_FEEDBACK_SUBCOMMANDS = ("propose", "evaluate", "submit", "status")
BOOKEND_SUBCOMMANDS = ("init", "brief", "finish")


def _subcommand(command):
    """The tool subcommand token: the first argument after the script path
    that is not itself a flag, e.g. "propose" from
    "python3 /abs/tool.py propose --out X ...". Returns None if not found."""
    tokens = command.split()
    for i, token in enumerate(tokens):
        if token.endswith(".py"):
            for later in tokens[i + 1:]:
                if not later.startswith("-"):
                    return later
            return None
    return None


def leak_scan(transcript_lines, command_substring, forbidden_words=FORBIDDEN_WORDS, subcommands=LIVE_FEEDBACK_SUBCOMMANDS):
    """Scan only the tool_result content that followed a matched Bash call
    to one of `subcommands` (the arm's own *live, per-candidate* feedback:
    `propose`/`evaluate`/`submit`/`status` by default) for forbidden
    Core-semantics words.

    `brief` and `finish` are excluded by default on purpose, not merely by
    convenience: `brief` legitimately renders the prior constellation with
    full Core verdicts (a fixed factor shared by every arm, not live
    feedback about the arm's own candidates), and `finish` prints this
    tool's own disclaimer sentence ("no verdict ... was computed"), which
    contains the forbidden words while describing their absence. Scanning
    those would produce a false leak on a clean tool by construction, not a
    real one -- confirmed against a real dry-run transcript, see
    `test_ablation.py`. This is the real isolation check: the model's own
    prose may legitimately use an ordinary English word like "pass"; the
    tool's live per-candidate output must not.
    """
    calls_by_id = {}
    hits = []
    pending_ids = set()
    for entry in transcript_lines:
        if entry.get("type") == "assistant":
            for block in entry.get("message", {}).get("content", []):
                if block.get("type") == "tool_use" and block.get("name") == "Bash":
                    command = block.get("input", {}).get("command", "")
                    if command_substring in command and _subcommand(command) in subcommands:
                        pending_ids.add(block["id"])
                        calls_by_id[block["id"]] = command
        elif entry.get("type") == "user":
            for block in entry.get("message", {}).get("content", []):
                tool_use_id = block.get("tool_use_id")
                if tool_use_id in pending_ids:
                    content = block.get("content")
                    text = content if isinstance(content, str) else json.dumps(content)
                    lowered = text.lower()
                    found = [w for w in forbidden_words if w in lowered]
                    if found:
                        hits.append({"tool_use_id": tool_use_id, "command": calls_by_id[tool_use_id], "words": found, "excerpt": text[:400]})
    return {"clean": not hits, "hits": hits}


def result_event(transcript_lines):
    for entry in reversed(transcript_lines):
        if entry.get("type") == "result":
            return entry
    return None
