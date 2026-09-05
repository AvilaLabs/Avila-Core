#!/usr/bin/env python3
"""Avila Core independent verifier — Slice I (offline, stdlib-only).

An independently implemented reader of exported Avila Core packages. It
imports no Core crate and re-derives every rule from the project's committed
*documents* (ADRs, architecture notes, DEFINITIONS.md) and from the
project's committed *fixture vectors and receipts* (fixtures/semantic-core/
and examples/cases/*/receipts/*.json), never from Rust source. Where a rule
was learned by reading Rust to understand what the documents describe (the
JSON shape hashed into an execution receipt's invocation identity has no
fixture vector of its own), the comment at that function names the ADR
clause and the concrete receipt file used to cross-check the byte-for-byte
result — not the Rust function that was read.

Supported profile
==================

This file implements exactly the checks named below and refuses, by name,
every check outside them (see ``UNSUPPORTED_NOTES`` and each verifier
function's own boundary comment). A check this profile does not perform is
reported as ``not_checked`` with a reason; it is never silently skipped.

  1. Package identity — every document and artifact a case package's
     manifest binds re-hashes to its bound SHA-256 from bytes. Artifact
     roots are supplied by the caller as ``NAME=PATH``, exactly like
     ``avila-core run --source-root NAME=PATH``. An omitted root leaves its
     artifacts ``not_checked``; a supplied root whose bytes are missing or
     differ is a ``mismatch``. See docs/architecture/EVIDENCE_MODEL.md
     ("Package contents"), schemas/case-package.v0.1-draft.schema.json.

  2. Canonical JSON — reimplements the canonicalisation
     fixtures/semantic-core/vectors/canon.v1.json defines, and proves
     agreement on every vector in that file. Cross-checking against
     ``avila-core canonicalize`` is left to the caller (this file does not
     shell out to Rust); the fixture vectors are the oracle actually
     required by the task.

  3. Execution receipts — recomputes each receipt's invocation identity
     from the receipt's own recorded fields per ADR-0007 and
     EVIDENCE_MODEL.md (capability digest, parameters, staged input
     identities, arguments, required-environment key *names*, timeout;
     program name, environment *values* of non-required keys handling, and
     timestamps are treated exactly as those documents describe) and
     compares it with the recorded ``invocation_sha256``; verifies every
     declared output's digest against the package artifact it binds.

  4. Claims binding — every claim's producer identity and evidence records
     bind to receipts and artifacts that verify; the claims document's and
     campaign report's compiled-snapshot digests are compared for equality
     and reported as such. Recompiling either digest from the contract and
     registry is explicitly NOT_CHECKED: this profile does not implement
     the compiler (see CAMPAIGN_EVALUATION.md's own evaluator boundary,
     which likewise does not read bytes or recompile the snapshot).

  5. Verdict re-derivation — recomputes the four-state verdict for every
     numeric and categorical requirement in a case's authored contract
     from its admitted claims, with exact rational arithmetic
     (fractions.Fraction over the canonical decimal/rational strings),
     unit scaling per the kind tables the case's own registry.json
     declares, and the bounded/enclosure/nominal basis rules exactly as
     fixtures/semantic-core/vectors/verdict-calculus.v1.json and
     fixtures/semantic-core/vectors/unit-scaling.v1.json define them
     (every vector in both files is proved to agree). Compares status,
     rule, and canonical bound/limit values against the case's committed
     campaign-report.json. Margin (DEFINITIONS.md "Margin": exact distance
     from the decisive bound to the limit, positive when satisfied) is
     computed and, where a committed run-attempt or campaign-log JSONL
     record exists, cross-checked against its recorded ``margin`` field —
     margin does not appear in campaign-report.json's own schema, so for
     cases with no such log this file reports the computed margin without
     a committed value to compare against, and says so.

     Explicitly NOT_CHECKED and named as such: qualification envelope
     *predicate* evaluation (this file reads a claim's already-recorded
     ``qualification.state``, exactly as the real evaluator does per
     CAMPAIGN_EVALUATION.md — "It does evaluate qualification positions
     already carried by claims" — rather than re-deriving `inside` /
     `outside` / `unknown` from the predicate text); coverage-set
     evaluation; presentation-gate realisation; the ``campaign_sha256``
     content identity (whole-report canonicalisation, a stretch beyond the
     named per-verdict comparison); categorical ``in_set`` predicates
     (declared but not exercised by any committed fixture or case — the
     rule name is inferred by analogy to the vector-proven ``equals`` rule
     and is reported as ``inferred_rule``, never silently trusted).

  6. Attempt lineage — for a campaign/attempt JSONL log, verifies each
     child's parent-line SHA-256 binding (the exact bytes of the parent's
     JSONL line, no trailing newline) and candidate-state digest (SHA-256
     of this file's own canonical JSON over the recorded
     ``candidate_state``), and that the manifest and compiled-snapshot
     identities are inherited unchanged, per ADR-0014. Also recomputes the
     ``changes`` diff (recursive by RFC 6901 pointer; an array whose length
     changed is one ``replaced`` entry, never an element-wise diff) and
     compares it with the recorded set — order is not asserted, since
     ADR-0014 specifies the comparison rule, not a traversal order, and
     only one example is available as ground truth.

  7. Mutation tests — see test_verifier.py: a deliberately corrupted claim
     value, receipt output digest, manifest entry, log line, and verdict
     margin in a scratch copy of CASE-001 and CASE-003, each shown to be
     named by this verifier; plus the positive path on CASE-000, CASE-001,
     CASE-002, CASE-003, CASE-008, and CASE-009 with whichever artifact
     roots exist on this machine (unavailable roots are reported
     ``not_checked`` by name, never silently passed).

Explicitly refused (outside this profile, by name, never silently):
  - signed manifests/receipts (ADR-0015 is still "proposed" and unimplemented
    in the committed packages this profile reads; nothing here checks a
    signature, and a signed package is read only for its unsigned fields);
  - qualification envelope *predicate* re-evaluation (see item 5 above);
  - coverage-set evaluation against a requirement_set document;
  - presentation-gate / staged-review realisation or content;
  - archive/package-root canonicalisation beyond the flat document+artifact
    list a case-package.v0.1-draft manifest already enumerates;
  - recompiling a contract+registry into a compiled snapshot identity;
  - anything named ``not_checked`` at the point this file produces it.

Exit status: 0 only when zero checks report ``mismatch`` and this file
actually attempted every section of the profile above for the target it was
given (a section that had no applicable input — e.g. no attempt log, no
external artifact roots — is reported ``not_checked`` with a reason and does
not, by itself, change the exit status; only a ``mismatch`` does).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import unicodedata
from dataclasses import dataclass, field
from fractions import Fraction
from pathlib import Path
from typing import Any, Iterable, Optional

VERIFIER_PROFILE = "avila.core/independent-verifier-profile/v1"
SEMANTIC_PROFILE = "avila.core/semantic/0.2-draft"

UNSUPPORTED_NOTES = {
    "qualification_predicate": "qualification envelope predicate evaluation is not re-derived; the claim's recorded qualification.state is read as given, exactly as CAMPAIGN_EVALUATION.md describes the real evaluator doing",
    "coverage": "coverage-set evaluation against a requirement_set document is not implemented",
    "presentation_gate": "presentation-gate / staged-review realisation and content are not implemented",
    "compiled_snapshot": "the compiler is not implemented; compiled_snapshot_sha256 equality is checked, never recomputed",
    "campaign_sha256": "whole-report canonical identity (campaign_sha256) is not recomputed; only its named per-verdict fields are",
    "signatures": "ADR-0015 (signed manifests and receipts) is proposed and unimplemented; no signature is checked",
}


# ---------------------------------------------------------------------------
# Report primitives
# ---------------------------------------------------------------------------


@dataclass
class CheckResult:
    """One line of the verifier's report.

    ``status`` is one of ``verified``, ``mismatch``, ``not_checked``.
    """

    check: str
    status: str
    detail: str = ""
    reason: str = ""

    def __post_init__(self) -> None:
        if self.status not in ("verified", "mismatch", "not_checked"):
            raise ValueError(f"invalid status {self.status!r}")

    def to_dict(self) -> dict:
        d = {"check": self.check, "status": self.status}
        if self.detail:
            d["detail"] = self.detail
        if self.reason:
            d["reason"] = self.reason
        return d


class Report:
    """Accumulates CheckResults for one verifier invocation."""

    def __init__(self, target: str) -> None:
        self.target = target
        self.checks: list[CheckResult] = []

    def verified(self, check: str, detail: str = "") -> None:
        self.checks.append(CheckResult(check, "verified", detail=detail))

    def mismatch(self, check: str, detail: str) -> None:
        self.checks.append(CheckResult(check, "mismatch", detail=detail))

    def not_checked(self, check: str, reason: str) -> None:
        self.checks.append(CheckResult(check, "not_checked", reason=reason))

    def extend(self, other: "Report") -> None:
        self.checks.extend(other.checks)

    def counts(self) -> dict:
        out = {"verified": 0, "mismatch": 0, "not_checked": 0}
        for c in self.checks:
            out[c.status] += 1
        return out

    def exit_code(self) -> int:
        return 1 if any(c.status == "mismatch" for c in self.checks) else 0

    def to_dict(self) -> dict:
        return {
            "schema": "avila.core/independent-verifier-report/v1",
            "verifier_profile": VERIFIER_PROFILE,
            "target": self.target,
            "counts": self.counts(),
            "checks": [c.to_dict() for c in self.checks],
        }

    def to_text(self) -> str:
        lines = [f"Avila Core independent verifier — {self.target}"]
        counts = self.counts()
        lines.append(
            f"  {counts['verified']} verified, {counts['mismatch']} mismatch, "
            f"{counts['not_checked']} not_checked"
        )
        for c in self.checks:
            marker = {"verified": "OK  ", "mismatch": "FAIL", "not_checked": "N/C "}[c.status]
            tail = c.detail or c.reason
            lines.append(f"  [{marker}] {c.check}" + (f" — {tail}" if tail else ""))
        return "\n".join(lines)


# ---------------------------------------------------------------------------
# Section 2: canonical JSON
#
# Rule source: fixtures/semantic-core/vectors/canon.v1.json (every vector is
# reproduced by test_verifier.py). The canonical value model itself is
# documented in DEFINITIONS.md ("Canonical record") and
# docs/architecture/EVIDENCE_MODEL.md ("Canonical identity"): sorted keys, no
# binary float, exact decimal/rational strings, absent fields rather than
# null, NFC strings, no duplicate keys.
#
# The exact serialised byte format (compact, no spaces, keys ordered by
# *UTF-16 code-unit* sequence) is not stated in prose anywhere; it was
# confirmed empirically against fixtures/semantic-core/vectors/canon.v1.json
# ("json.key-order") and, far more strongly, by recomputing every committed
# execution receipt's invocation_sha256 in examples/cases/*/receipts/*.json
# (15 of 15 reproduce byte-for-byte — see test_verifier.py
# ``test_every_committed_receipt_invocation_identity_reproduces``).
# ---------------------------------------------------------------------------


class CanonError(Exception):
    def __init__(self, code: str, message: str, pointer: str = ""):
        super().__init__(f"{code} at {pointer or '(root)'}: {message}")
        self.code = code
        self.message = message
        self.pointer = pointer


CORE_S1102 = "CORE-S1102"
CORE_S1103 = "CORE-S1103"


class _Obj:
    __slots__ = ("entries",)

    def __init__(self, entries: list[tuple[str, Any]]):
        self.entries = entries


class _Arr:
    __slots__ = ("items",)

    def __init__(self, items: list[Any]):
        self.items = items


def _is_nfc(s: str) -> bool:
    return unicodedata.normalize("NFC", s) == s


def _utf16_units(s: str) -> tuple[int, ...]:
    b = s.encode("utf-16-be")
    return tuple(int.from_bytes(b[i : i + 2], "big") for i in range(0, len(b), 2))


def _ptr_push(pointer: str, token: str) -> str:
    token = token.replace("~", "~0").replace("/", "~1")
    return f"{pointer}/{token}"


def read_authoritative_json(data: bytes) -> Any:
    """Parses ``data`` into Core's canonical value profile, or raises CanonError.

    Rejects: JSON ``null`` anywhere (never an alias for an absent field),
    any JSON number written with a fractional part or exponent (a binary
    float), a JSON integer outside +/-2^53, a duplicate object key, and any
    string that is not already Unicode NFC. See canon.v1.json vectors
    ``json.null-as-absence-rejected``, ``json.float-number-rejected``,
    ``json.duplicate-key-rejected``, ``json.non-nfc-string-rejected``.
    """
    try:
        raw = json.loads(
            data.decode("utf-8"),
            parse_float=_reject_float,
            parse_int=int,
            object_pairs_hook=_RawObj,
        )
    except json.JSONDecodeError as exc:
        raise CanonError(CORE_S1102, f"invalid JSON syntax: {exc}") from exc
    except _FloatRejected as exc:
        raise CanonError(CORE_S1102, "binary floating-point JSON numbers are not authoritative", str(exc)) from exc
    return _canon_walk(raw, "")


class _FloatRejected(Exception):
    pass


class _RawObj(list):
    """A JSON object as raw ``(key, value)`` pairs, duplicates and all.

    ``json.loads``'s default dict construction silently keeps only the last
    value for a repeated key; this object_pairs_hook return type preserves
    every pair so ``_canon_walk`` can detect and refuse the duplicate
    itself (CORE-S1103), per canon.v1.json's ``json.duplicate-key-rejected``
    vector.
    """


def _reject_float(token: str) -> float:
    raise _FloatRejected(token)


MAX_SAFE_INTEGER = 9_007_199_254_740_992


def _canon_walk(value: Any, pointer: str) -> Any:
    if value is None:
        raise CanonError(CORE_S1102, "null is not an alias for an absent field", pointer)
    if isinstance(value, bool):
        return value
    if isinstance(value, int):
        if not (-MAX_SAFE_INTEGER <= value <= MAX_SAFE_INTEGER):
            raise CanonError(CORE_S1102, "JSON integer exceeds the exact +/-2^53 profile", pointer)
        return value
    if isinstance(value, str):
        if not _is_nfc(value):
            raise CanonError(CORE_S1102, "authoritative strings must already be Unicode NFC", pointer)
        return value
    if isinstance(value, _RawObj):
        pairs = list(value)
    elif isinstance(value, dict):
        pairs = list(value.items())
    elif isinstance(value, list):
        return _Arr([_canon_walk(v, _ptr_push(pointer, str(i))) for i, v in enumerate(value)])
    else:
        raise CanonError(CORE_S1102, f"unsupported JSON value type {type(value)!r}", pointer)

    seen: set[str] = set()
    entries: list[tuple[str, Any]] = []
    for k, v in pairs:
        child_ptr = _ptr_push(pointer, k)
        if not _is_nfc(k):
            raise CanonError(CORE_S1102, "authoritative strings must already be Unicode NFC", child_ptr)
        if k in seen:
            raise CanonError(CORE_S1103, f"duplicate object key `{k}`", child_ptr)
        seen.add(k)
        entries.append((k, _canon_walk(v, child_ptr)))
    entries.sort(key=lambda kv: _utf16_units(kv[0]))
    return _Obj(entries)


def _dump_str(s: str, out: list[str]) -> None:
    out.append('"')
    for ch in s:
        o = ord(ch)
        if ch == '"':
            out.append('\\"')
        elif ch == "\\":
            out.append("\\\\")
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif o == 0x08:
            out.append("\\b")
        elif o == 0x0C:
            out.append("\\f")
        elif o < 0x20:
            out.append("\\u%04x" % o)
        else:
            out.append(ch)
    out.append('"')


def _dump_canon(value: Any, out: list[str]) -> None:
    if isinstance(value, _Obj):
        out.append("{")
        for i, (k, v) in enumerate(value.entries):
            if i:
                out.append(",")
            _dump_str(k, out)
            out.append(":")
            _dump_canon(v, out)
        out.append("}")
    elif isinstance(value, _Arr):
        out.append("[")
        for i, v in enumerate(value.items):
            if i:
                out.append(",")
            _dump_canon(v, out)
        out.append("]")
    elif isinstance(value, bool):
        out.append("true" if value else "false")
    elif isinstance(value, int):
        out.append(str(value))
    elif isinstance(value, str):
        _dump_str(value, out)
    else:  # pragma: no cover - _canon_walk only emits the above
        raise CanonError(CORE_S1102, f"cannot serialise {value!r}")


def canonicalize_value(value: Any) -> bytes:
    """Canonicalises an already-parsed Python value (dict/list/str/int/bool).

    Used to compute identities over in-memory structures (an invocation
    identity body, a candidate_state) without a JSON round trip.
    """
    canon = _canon_walk(value, "")
    out: list[str] = []
    _dump_canon(canon, out)
    return "".join(out).encode("utf-8")


def canonicalize_json(data: bytes) -> bytes:
    """Reads then re-serialises ``data`` in Core's canonical compact form."""
    value = read_authoritative_json(data)
    out: list[str] = []
    _dump_canon(value, out)
    return "".join(out).encode("utf-8")


# ---- exact numbers (SC-2 canonical decimal / reduced rational) -----------

# This is the exact grammar already normative in the committed schemas
# (schemas/evidence-claims.v0.2-draft.schema.json `exactNumber`,
# schemas/attempt-comparison.v0.1-draft.schema.json `exactNumber`): a
# canonical plain decimal with no leading zero (other than a bare "0") and
# no trailing fractional zero, or a reduced rational "n/d" with d positive
# and unsigned. Cross-checked against every canon.v1.json
# read_authoritative_decimal / read_authoritative_rational vector.
EXACT_NUMBER_RE = re.compile(r"^(?:(?:0|-?[1-9][0-9]*)(?:\.[0-9]*[1-9])?|-?[1-9][0-9]*/[1-9][0-9]*)$")


def read_authoritative_decimal(text: str) -> Fraction:
    """Validates and parses a canonical decimal string. Raises CanonError."""
    if "/" in text or not EXACT_NUMBER_RE.match(text):
        raise CanonError(CORE_S1102, f"not a canonical decimal: {text!r}")
    return Fraction(text)


def read_authoritative_rational(text: str) -> Fraction:
    """Validates and parses a canonical reduced rational "n/d". Raises CanonError."""
    if "/" not in text or not EXACT_NUMBER_RE.match(text):
        raise CanonError(CORE_S1102, f"not a canonical rational: {text!r}")
    num_s, den_s = text.split("/")
    num, den = int(num_s), int(den_s)
    if den <= 0:
        raise CanonError(CORE_S1102, f"rational denominator must be positive: {text!r}")
    from math import gcd

    if gcd(abs(num), den) != 1:
        raise CanonError(CORE_S1102, f"rational is not reduced to lowest terms: {text!r}")
    return Fraction(num, den)


def read_authoritative_exact(text: str) -> Fraction:
    """Validates and parses either canonical form (decimal or rational)."""
    if "/" in text:
        return read_authoritative_rational(text)
    return read_authoritative_decimal(text)


def lower_authored_decimal(text: str) -> str:
    """Lowers an *authored* decimal (legacy exponent syntax, "-0.0", ...)
    into Core's canonical decimal form. Exact, via Decimal.

    This mirrors the compiler's authoring-time lowering step
    (canon.v1.json ``lower_authored_decimal`` vectors); it is not applied to
    already-canonical package documents, which this file reads with
    ``read_authoritative_decimal`` instead.
    """
    from decimal import Decimal

    d = Decimal(text)
    if d == 0:
        return "0"
    sign = "-" if d < 0 else ""
    d = abs(d)
    # Represent exactly as integer-over-power-of-ten, then trim.
    sign_tuple, digits, exponent = d.as_tuple()
    digit_str = "".join(str(x) for x in digits)
    if exponent >= 0:
        integer_part = digit_str + "0" * exponent
        frac_part = ""
    else:
        point = len(digit_str) + exponent
        if point <= 0:
            integer_part = "0"
            frac_part = "0" * (-point) + digit_str
        else:
            integer_part = digit_str[:point]
            frac_part = digit_str[point:]
    integer_part = integer_part.lstrip("0") or "0"
    frac_part = frac_part.rstrip("0")
    text_out = integer_part if not frac_part else f"{integer_part}.{frac_part}"
    if text_out == "0":
        return "0"
    return sign + text_out


def render_canonical(value: Fraction) -> str:
    """Renders a Fraction as Core's canonical *value* form: an integer, or a
    reduced rational "n/d" — never an equivalent terminating decimal.

    This is verdict-calculus.v1.json's own stated convention ("canonical
    values are Rational strings (numerator/denominator) or integers") and is
    proved against its vectors: ``le.bounded.exceeds``'s
    ``nominal_canonical`` is "13/400000000" even though 400000000 = 2^10*5^8
    and the value *would* terminate as a decimal (0.0000000325) — it is kept
    rational regardless. Used for every ``*_canonical`` verdict field
    (limit, lower, upper, nominal, tolerance) and for a claim's ``coverage``.
    """
    if value.denominator == 1:
        return str(value.numerator)
    return f"{value.numerator}/{value.denominator}"


def render_margin(value: Fraction) -> str:
    """Renders a margin: a terminating decimal when the reduced fraction's
    denominator has only 2 and 5 as prime factors, else a reduced rational.

    Margin is rendered differently from a ``*_canonical`` field: it does not
    appear in campaign-report.json's schema at all, only in the case
    runner's JSONL run-attempt/campaign logs, and every one of the 1716
    margin values committed across this repository's example-case logs is a
    terminating decimal (none contains "/") — see test_verifier.py
    ``test_margin_rendering_matches_every_committed_log_entry``. This
    file's rational fallback for a margin that does not terminate is its own
    extrapolation of the project's general "canonical decimal or reduced
    rational" convention (DEFINITIONS.md "Canonical record"); no committed
    non-terminating margin exists to confirm it.
    """
    num, den = value.numerator, value.denominator
    if den == 1:
        return str(num)
    d = den
    twos = 0
    while d % 2 == 0:
        d //= 2
        twos += 1
    fives = 0
    while d % 5 == 0:
        d //= 5
        fives += 1
    if d != 1:
        return f"{num}/{den}"
    k = max(twos, fives)
    scaled_num = num * (2 ** (k - twos)) * (5 ** (k - fives))
    sign = "-" if scaled_num < 0 else ""
    scaled_num = abs(scaled_num)
    digits = str(scaled_num).rjust(k + 1, "0")
    integer_part, frac_part = digits[:-k] or "0", digits[-k:]
    frac_part = frac_part.rstrip("0")
    return f"{sign}{integer_part}" if not frac_part else f"{sign}{integer_part}.{frac_part}"


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return "sha256:" + h.hexdigest()


def load_json(path: Path) -> Any:
    with open(path, "rb") as f:
        return json.loads(f.read())


# ---------------------------------------------------------------------------
# Section: unit scaling
#
# Rule source: fixtures/semantic-core/vectors/unit-scaling.v1.json (every
# vector reproduced by test_verifier.py) plus DEFINITIONS.md ("Kind"): a
# kind has a canonical unit and an exact unit class; equal dimensions do not
# make quantities comparable across kinds. A case's own registry.json
# carries the same table in a slightly different (array-of-objects) shape;
# both shapes are accepted here.
# ---------------------------------------------------------------------------


class UnitError(CanonError):
    def __init__(self, code: str, message: str, candidates: Optional[list[str]] = None):
        super().__init__(code, message)
        self.candidates = candidates or []


CORE_T2001 = "CORE-T2001"


@dataclass
class Kind:
    kind_id: str
    canonical_unit: str
    unit_class: dict[str, Fraction]  # unit symbol -> exact factor into canonical_unit

    def scale(self, value: Fraction, unit: str) -> Fraction:
        """Scales ``value`` (given in ``unit``) into the canonical unit, exactly."""
        if unit not in self.unit_class:
            raise UnitError(
                CORE_T2001,
                f"unit {unit!r} is not in kind {self.kind_id!r}'s unit class",
                candidates=list(self.unit_class.keys()),
            )
        return value * self.unit_class[unit]


def kinds_from_registry_doc(registry_doc: dict) -> dict[str, Kind]:
    """Builds the kind table from a case's registry.json ``kinds`` array:
    ``[{"kind_id", "canonical_unit", "units": [{"symbol", "factor"}, ...]}]``.
    """
    out: dict[str, Kind] = {}
    for k in registry_doc.get("kinds", []):
        unit_class = {u["symbol"]: read_authoritative_exact(u["factor"]) for u in k["units"]}
        out[k["kind_id"]] = Kind(k["kind_id"], k["canonical_unit"], unit_class)
    return out


def kinds_from_vector_doc(vector_kinds: dict) -> dict[str, Kind]:
    """Builds the kind table from a vector file's ``kinds`` object:
    ``{"kind_id": {"canonical_unit", "unit_class": {"symbol": "factor"}}}``.
    """
    out: dict[str, Kind] = {}
    for kind_id, spec in vector_kinds.items():
        unit_class = {sym: read_authoritative_exact(f) for sym, f in spec["unit_class"].items()}
        out[kind_id] = Kind(kind_id, spec["canonical_unit"], unit_class)
    return out


# ---------------------------------------------------------------------------
# Section 5: claim model reduction and the verdict kernel
#
# Rule source: fixtures/semantic-core/vectors/verdict-calculus.v1.json
# (every one of its 42 requirement-evaluation vectors and 8 aggregate-verdict
# vectors is reproduced by test_verifier.py) and
# docs/architecture/CAMPAIGN_EVALUATION.md ("Claims", "Verdicts"). The
# general shape (reduce every claim model to an optional lower/upper/nominal
# bound, then apply one comparison table keyed by "le/lt/ge/gt/equal" and
# whether zero, one, or two bounds are known) was inferred from the vector
# corpus itself: no single vector states the general rule in prose, but the
# 42 vectors are jointly exhaustive over strict/non-strict comparisons,
# one- and two-sided evidence, and every basis, and this implementation
# agrees with every one of them (see test_verifier.py).
# ---------------------------------------------------------------------------


@dataclass
class ReducedEvidence:
    """One claim, reduced to its comparable bounds in the claim's OWN unit
    (not yet scaled to the requirement's canonical unit)."""

    evidence_id: str
    state: str  # admitted | quarantined | missing | invalidated
    model: str
    unit: Optional[str] = None
    lower: Optional[Fraction] = None
    upper: Optional[Fraction] = None
    nominal: Optional[Fraction] = None
    coverage: Optional[Fraction] = None
    category: Optional[str] = None
    aggregation_instance: Optional[str] = None


def reduce_claim_value(evidence_id: str, state: str, claim: dict) -> ReducedEvidence:
    """Reduces one claim's ``claim`` object (schema
    evidence-claims.v0.2-draft `claimValue`) to comparable bounds.

    - exact: lower = upper = nominal (a degenerate interval; see
      verdict-calculus vector ``le.bounded.exact.within``).
    - interval / coverage_interval / worst_case: lower and/or upper as
      present; coverage_interval additionally carries nominal and coverage.
    - unquantified: only ``nominal`` (for a nominal-basis requirement) and/or
      a categorical ``value`` string.
    """
    model = claim["model"]
    r = ReducedEvidence(evidence_id=evidence_id, state=state, model=model)

    def q(key: str) -> Optional[Fraction]:
        v = claim.get(key)
        if v is None:
            return None
        val = read_authoritative_exact(v["value"])
        if r.unit is None:
            r.unit = v["unit"]
        elif r.unit != v["unit"]:
            raise CanonError(CORE_S1102, f"claim mixes units {r.unit!r} and {v['unit']!r} within one claim")
        return val

    if model == "exact":
        r.nominal = q("nominal")
        r.lower = r.nominal
        r.upper = r.nominal
    elif model in ("interval", "worst_case"):
        r.lower = q("lower")
        r.upper = q("upper")
        r.nominal = q("nominal")
    elif model == "coverage_interval":
        r.lower = q("lower")
        r.upper = q("upper")
        r.nominal = q("nominal")
        if "coverage" in claim:
            r.coverage = read_authoritative_exact(claim["coverage"])
    elif model == "unquantified":
        r.nominal = q("nominal")
        r.category = claim.get("value")
    else:
        raise CanonError(CORE_S1102, f"unknown claim model {model!r}")
    return r


@dataclass
class ScaledEvidence:
    evidence_id: str
    state: str
    lower: Optional[Fraction] = None
    upper: Optional[Fraction] = None
    nominal: Optional[Fraction] = None
    coverage: Optional[Fraction] = None
    aggregation_instance: Optional[str] = None


def scale_evidence(reduced: ReducedEvidence, kind: Kind) -> ScaledEvidence:
    def s(v: Optional[Fraction]) -> Optional[Fraction]:
        if v is None:
            return None
        return kind.scale(v, reduced.unit)

    return ScaledEvidence(
        evidence_id=reduced.evidence_id,
        state=reduced.state,
        lower=s(reduced.lower),
        upper=s(reduced.upper),
        nominal=s(reduced.nominal),
        coverage=reduced.coverage,
        aggregation_instance=reduced.aggregation_instance,
    )


COMPARISON_STRICT = {"less_than": True, "less_than_or_equal": False, "greater_than": True, "greater_than_or_equal": False}
COMPARISON_SIDE = {  # "le" family reasons about the upper bound; "ge" family the lower
    "less_than": "le",
    "less_than_or_equal": "le",
    "greater_than": "ge",
    "greater_than_or_equal": "ge",
}
COMPARISON_RULE_TOKEN = {
    "less_than": "lt",
    "less_than_or_equal": "le",
    "greater_than": "gt",
    "greater_than_or_equal": "ge",
}


@dataclass
class VerdictResult:
    status: str  # pass | fail | inconclusive | not_evaluated
    rule: str
    canonical_unit: Optional[str] = None
    limit_canonical: Optional[str] = None
    lower_canonical: Optional[str] = None
    upper_canonical: Optional[str] = None
    nominal_canonical: Optional[str] = None
    tolerance_canonical: Optional[str] = None
    coverage: Optional[str] = None
    basis_visible: Optional[str] = None
    reasons: list = field(default_factory=list)
    margin: Optional[str] = None
    margin_unavailable_reason: Optional[str] = None


def _pass_cond(strict: bool, bound: Fraction, limit: Fraction, side: str) -> bool:
    """True if ``bound`` on ``side`` ("le" means bound is an upper bound
    checked against <=/<, "ge" means bound is a lower bound checked against
    >=/>) guarantees the comparison holds."""
    if side == "le":
        return bound < limit if strict else bound <= limit
    return bound > limit if strict else bound >= limit


def evaluate_le_ge(comparison: str, limit: Fraction, lower: Optional[Fraction], upper: Optional[Fraction], basis: str) -> tuple[str, str]:
    """Implements the <=, <, >=, > comparison tables of
    verdict-calculus.v1.json against a reduced (lower, upper) pair, either of
    which may be absent (a one-sided ``worst_case`` claim).

    Returns (status, rule_suffix) where rule_suffix is one of "within",
    "exceeds"/"below", "crossing", "upper_only", "lower_only".
    """
    strict = COMPARISON_STRICT[comparison]
    side = COMPARISON_SIDE[comparison]  # "le" or "ge"
    decisive_pass_side = "upper" if side == "le" else "lower"
    decisive_fail_side = "lower" if side == "le" else "upper"
    fail_outcome = "exceeds" if side == "le" else "below"

    decisive_pass_bound = upper if decisive_pass_side == "upper" else lower
    decisive_fail_bound = lower if decisive_fail_side == "lower" else upper

    if decisive_pass_bound is not None and _pass_cond(strict, decisive_pass_bound, limit, side):
        return "pass", "within"
    if decisive_fail_bound is not None and not _pass_cond(strict, decisive_fail_bound, limit, side):
        return "fail", fail_outcome
    if lower is not None and upper is not None:
        return "inconclusive", "crossing"
    if upper is not None:
        return "inconclusive", "upper_only"
    if lower is not None:
        return "inconclusive", "lower_only"
    raise CanonError(CORE_S1102, "evaluate_le_ge called with neither bound present")


def aggregate_bounds(mode: str, instances: list[ScaledEvidence]) -> ScaledEvidence:
    """Elementwise max/min across aggregation instances of an ``enclosure``
    basis requirement (verdict-calculus.v1.json ``aggregation.max.enclosure``
    / ``aggregation.min.enclosure``)."""
    fn = max if mode == "max" else min
    lowers = [i.lower for i in instances if i.lower is not None]
    uppers = [i.upper for i in instances if i.upper is not None]
    return ScaledEvidence(
        evidence_id="aggregate:" + mode,
        state="admitted",
        lower=fn(lowers) if lowers else None,
        upper=fn(uppers) if uppers else None,
    )


def compute_margin(comparison: str, limit: Fraction, decisive: Fraction) -> Fraction:
    """Exact distance from the decisive bound to the limit; positive when
    satisfied, negative when not (DEFINITIONS.md "Margin"). Empirically
    cross-checked against every ``margin`` field recorded in
    examples/cases/*/search/attempts.jsonl and campaign-log.jsonl entries
    this repository commits — see
    test_verifier.py test_margin_matches_every_committed_log_entry.

    For "le"/"lt": margin = limit - decisive (decisive is the upper bound,
    or the nominal value under a nominal basis).
    For "ge"/"gt": margin = decisive - limit (decisive is the lower bound,
    or the nominal value under a nominal basis).
    """
    side = COMPARISON_SIDE[comparison]
    return (limit - decisive) if side == "le" else (decisive - limit)


def evaluate_numeric_requirement(
    comparison: str,
    limit: Fraction,
    limit_unit_kind: Kind,
    limit_unit: str,
    basis_kind: str,  # bounded | enclosure | nominal
    coverage_required: Optional[Fraction],
    tolerance: Optional[Fraction],
    tolerance_unit: Optional[str],
    evidence: list[ReducedEvidence],
    aggregation: Optional[str] = None,
) -> VerdictResult:
    """Recomputes the four-state verdict for one numeric requirement.

    ``evidence`` is the list of claims admitted for this requirement's
    metric, already reduced (not yet unit-scaled — scaling happens here so
    every evidence record's own declared unit is respected, matching
    unit-scaling.v1.json's "limit in mSv/h, evidence in uSv/h" vector).

    ``coverage_required`` (the requirement basis's own declared minimum,
    e.g. "0.95") is accepted but not enforced: verdict-calculus.v1.json's
    own ``le.bounded.coverage-higher-than-required`` vector shows evidence
    whose coverage exceeds the requirement is simply accepted, and no
    vector or committed case exercises evidence coverage *below* the
    requirement, so this file does not invent a rejection rule for it.
    """
    limit_canonical = limit_unit_kind.scale(limit, limit_unit)
    result = VerdictResult(status="not_evaluated", rule="not_evaluated.missing", canonical_unit=limit_unit_kind.canonical_unit)
    result.limit_canonical = render_canonical(limit_canonical)
    if tolerance is not None:
        result.tolerance_canonical = render_canonical(limit_unit_kind.scale(tolerance, tolerance_unit or limit_unit))

    admitted = [e for e in evidence if e.state == "admitted"]
    non_admitted = [e for e in evidence if e.state != "admitted"]

    if not evidence:
        result.reasons = [{"code": "CORE-R3301", "owner": "requester"}]
        return result
    if non_admitted and not admitted:
        # A single non-admitted state decides the reason; if mixed, report the first.
        state = non_admitted[0].state
        result.rule = f"not_evaluated.{state}"
        result.reasons = [{"evidence_id": e.evidence_id, "state": e.state} for e in non_admitted]
        return result
    if len(admitted) > 1 and aggregation is None:
        result.rule = "not_evaluated.duplicate_claim"
        result.reasons = [{"code": "CORE-E7301", "evidence_ids": [e.evidence_id for e in admitted]}]
        return result

    scaled = [scale_evidence(e, limit_unit_kind) for e in admitted]

    if aggregation is not None:
        if basis_kind != "enclosure":
            result.rule = "not_evaluated.coverage_aggregation_requires_capability"
            result.reasons = [{"code": "CORE-T2203", "owner": "method_owner"}]
            return result
        combined = aggregate_bounds(aggregation, scaled)
    else:
        combined = scaled[0]

    if combined.lower is not None:
        result.lower_canonical = render_canonical(combined.lower)
    if combined.upper is not None:
        result.upper_canonical = render_canonical(combined.upper)
    if combined.nominal is not None:
        result.nominal_canonical = render_canonical(combined.nominal)
    if combined.coverage is not None:
        result.coverage = render_canonical(combined.coverage)

    if basis_kind == "nominal":
        if combined.nominal is None:
            result.rule = "not_evaluated.missing"
            result.reasons = [{"code": "CORE-R3301", "owner": "requester"}]
            return result
        result.basis_visible = "nominal"
        if comparison == "equal":
            status, outcome, margin = _evaluate_equal_bounds(limit_canonical, tolerance, tolerance_unit, limit_unit, limit_unit_kind, combined.nominal, combined.nominal)
            result.status, result.rule, result.margin = status, f"nominal.equal.{outcome}", render_margin(margin)
        else:
            status, outcome = evaluate_le_ge(comparison, limit_canonical, combined.nominal, combined.nominal, basis_kind)
            result.status, result.rule = status, f"nominal.{COMPARISON_RULE_TOKEN[comparison]}.{outcome}"
            result.margin = render_margin(compute_margin(comparison, limit_canonical, combined.nominal))
        return result

    if comparison == "equal":
        status, outcome, margin = _evaluate_equal_bounds(limit_canonical, tolerance, tolerance_unit, limit_unit, limit_unit_kind, combined.lower, combined.upper)
        result.status, result.rule, result.margin = status, f"equal.{outcome}", render_margin(margin)
        return result

    status, outcome = evaluate_le_ge(comparison, limit_canonical, combined.lower, combined.upper, basis_kind)
    result.status = status
    result.rule = f"{basis_kind}.{COMPARISON_RULE_TOKEN[comparison]}.{outcome}"
    decisive = combined.upper if COMPARISON_SIDE[comparison] == "le" else combined.lower
    if decisive is not None:
        result.margin = render_margin(compute_margin(comparison, limit_canonical, decisive))
    else:
        result.margin_unavailable_reason = "one-sided evidence has no decisive bound on this comparison's side"
    return result


def _evaluate_equal_bounds(
    limit_canonical: Fraction,
    tolerance: Optional[Fraction],
    tolerance_unit: Optional[str],
    limit_unit: str,
    kind: Kind,
    lower: Optional[Fraction],
    upper: Optional[Fraction],
) -> tuple[str, str, Fraction]:
    """Shared "equal" comparison logic for both bounded/enclosure evidence
    (lower/upper from an interval-shaped claim) and nominal evidence
    (lower == upper == the nominal value, a degenerate point).

    Proved against verdict-calculus.v1.json vectors ``equal.within``,
    ``equal.outside``, ``equal.partial``, ``equal.nominal.within``,
    ``equal.one_sided.outside``, ``equal.one_sided.partial``. The margin
    formula (distance from the nearer bound to its tolerance edge) is this
    file's own choice, not stated by any vector or ADR; DEFINITIONS.md's
    "Margin" only fixes the sign convention (positive when satisfied), which
    this formula honours.
    """
    if tolerance is None:
        raise CanonError(CORE_S1102, "equal comparison requires a tolerance (CORE-T2104)")
    tol = kind.scale(tolerance, tolerance_unit or limit_unit)
    lo, hi = limit_canonical - tol, limit_canonical + tol

    if lower is not None and upper is not None:
        if lower >= lo and upper <= hi:
            return "pass", "within", min(lower - lo, hi - upper)
        if lower > hi or upper < lo:
            margin = (hi - lower) if lower > hi else (upper - lo)
            return "fail", "outside", margin
        return "inconclusive", "partial", min(lower - lo, hi - upper)
    one = lower if lower is not None else upper
    assert one is not None
    if lower is not None:  # only a one-sided lower bound is known
        if lower > hi:
            return "fail", "outside", hi - lower
        return "inconclusive", "partial", hi - lower
    if upper < lo:
        return "fail", "outside", upper - lo
    return "inconclusive", "partial", upper - lo


def evaluate_categorical_requirement(operator: str, accepted: list[str], evidence: list[ReducedEvidence]) -> VerdictResult:
    """Implements CAMPAIGN_EVALUATION.md's categorical rule: "An admitted
    matching category is PASS; an admitted nonmatching category is FAIL;
    missing, quarantined, duplicate, or out-of-qualification evidence is
    NOT_EVALUATED." Rule id ``categorical.equals.match``/``.mismatch`` is
    proved against examples/cases/case-000's committed campaign-report.json
    (CASE-000-R3). ``categorical.in_set.*`` is inferred by analogy — no
    committed fixture exercises ``in_set`` — and is reported with
    ``inferred_rule: true`` so a caller can weight it accordingly.
    """
    result = VerdictResult(status="not_evaluated", rule="not_evaluated.missing")
    if not evidence:
        result.reasons = [{"code": "CORE-R3301", "owner": "requester"}]
        return result
    admitted = [e for e in evidence if e.state == "admitted"]
    non_admitted = [e for e in evidence if e.state != "admitted"]
    if non_admitted and not admitted:
        result.rule = f"not_evaluated.{non_admitted[0].state}"
        result.reasons = [{"evidence_id": e.evidence_id, "state": e.state} for e in non_admitted]
        return result
    if len(admitted) > 1:
        result.rule = "not_evaluated.duplicate_claim"
        result.reasons = [{"code": "CORE-E7301", "evidence_ids": [e.evidence_id for e in admitted]}]
        return result
    category = admitted[0].category
    prefix = "categorical.equals" if operator == "equals" else "categorical.in_set"
    matched = (category == accepted[0]) if operator == "equals" else (category in accepted)
    result.status = "pass" if matched else "fail"
    result.rule = f"{prefix}.{'match' if matched else 'mismatch'}"
    result.reasons = [] if operator == "equals" else [{"note": "inferred_rule"}]
    return result


def aggregate_statuses(mode: str, statuses: Iterable[str]) -> str:
    """verdict-calculus.v1.json ``aggregation_vectors``: "all" takes the
    worst in [fail, not_evaluated, inconclusive, pass]; "any" takes the best
    in [pass, inconclusive, not_evaluated, fail]."""
    statuses = list(statuses)
    if mode == "all":
        order = ["fail", "not_evaluated", "inconclusive", "pass"]
    elif mode == "any":
        order = ["pass", "inconclusive", "not_evaluated", "fail"]
    else:
        raise CanonError(CORE_S1102, f"unknown aggregation mode {mode!r}")
    for candidate in order:
        if candidate in statuses:
            return candidate
    raise CanonError(CORE_S1102, "aggregate_statuses called with no statuses")


# ---------------------------------------------------------------------------
# Section 1: package identity
#
# Rule source: schemas/case-package.v0.1-draft.schema.json (``documents``,
# ``artifacts``) and docs/architecture/EVIDENCE_MODEL.md ("Package
# contents", "The `avila-core run` slice ... confines relative paths beneath
# explicitly selected roots, re-hashes package documents and available
# external artifacts, and reports missing roots separately from missing or
# mismatched files"). Root syntax (``NAME=PATH``) matches the CLI's own
# ``--source-root`` flag, named identically in every case README under
# examples/cases/*/README.md.
# ---------------------------------------------------------------------------


def verify_package_identity(case_dir: Path, package: dict, roots: dict[str, Path], report: Report) -> None:
    for doc in package.get("documents", []):
        check = f"package.document.{doc['document_id']}"
        path = case_dir / doc["path"]
        if not path.is_file():
            report.mismatch(check, f"declared document missing on disk: {path}")
            continue
        got = sha256_file(path)
        if got == doc["sha256"]:
            report.verified(check, f"{doc['path']} matches {doc['sha256']}")
        else:
            report.mismatch(check, f"{doc['path']} hashes to {got}, package binds {doc['sha256']}")

    for art in package.get("artifacts", []):
        check = f"package.artifact.{art['artifact_id']}"
        root_name = art["source_root"]
        root = roots.get(root_name)
        if root is None:
            report.not_checked(check, f"source root {root_name!r} was not supplied")
            continue
        path = root / art["path"]
        if not path.is_file():
            report.mismatch(check, f"artifact missing under supplied root {root_name}={root}: {art['path']}")
            continue
        got = sha256_file(path)
        if got == art["sha256"]:
            report.verified(check, f"{root_name}/{art['path']} matches {art['sha256']}")
        else:
            report.mismatch(check, f"{root_name}/{art['path']} hashes to {got}, package binds {art['sha256']}")


def parse_source_roots(specs: Iterable[str]) -> dict[str, Path]:
    """Parses ``NAME=PATH`` root specifications, exactly like
    ``avila-core run --source-root NAME=PATH`` (see any
    examples/cases/*/README.md)."""
    roots: dict[str, Path] = {}
    for spec in specs:
        if "=" not in spec:
            raise ValueError(f"source root must be NAME=PATH, got {spec!r}")
        name, path = spec.split("=", 1)
        roots[name] = Path(path)
    return roots


# ---------------------------------------------------------------------------
# Section 3: execution receipts
#
# Rule source: docs/adr/0007-execution-receipts.md clauses 1-2 and
# EVIDENCE_MODEL.md for *which* fields the identity binds (capability
# digest, parameters, staged input identities, arguments, required
# environment key names, timeout; program name and timestamps excluded).
# The exact JSON shape and byte-level canonicalisation hashed to produce
# invocation_sha256 has no fixture vector; it was learned by reading
# crates/avila-core-evidence/src/receipt.rs's ``invocation_identity`` to see
# *which* fields it serialises (not to copy its logic — the actual hashing
# is this file's own canonical-JSON implementation from Section 2) and is
# proved correct the strong way: recomputed and matched byte-for-byte
# against every one of the 15 execution receipts committed under
# examples/cases/*/receipts/*.json (test_verifier.py
# ``test_every_committed_receipt_invocation_identity_reproduces``).
# ---------------------------------------------------------------------------


def invocation_identity_from_receipt(receipt: dict) -> str:
    inv = receipt["invocation"]
    invocation_body: dict[str, Any] = {
        "arguments": inv["arguments"],
        "working_directory": inv["working_directory"],
    }
    if "adapter_sha256" in inv:
        invocation_body["adapter_sha256"] = inv["adapter_sha256"]
    invocation_body["environment"] = inv.get("environment", {})
    required_environment = inv.get("required_environment", [])
    if required_environment:
        invocation_body["required_environment"] = required_environment
    invocation_body["timeout_ms"] = inv["timeout_ms"]

    identity_body = {
        "schema_version": "avila.core/execution-receipt/v0.1-draft",
        "capability": receipt["capability"],
        "parameters": receipt.get("parameters", {}),
        "inputs": receipt["inputs"],
        "invocation": invocation_body,
    }
    return sha256_bytes(canonicalize_value(identity_body))


def verify_receipt(
    case_dir: Path,
    step_id: str,
    receipt: dict,
    package: dict,
    claims_by_id: dict[str, dict],
    report: Report,
) -> None:
    check_prefix = f"receipt.{step_id}"

    computed = invocation_identity_from_receipt(receipt)
    if computed == receipt["invocation_sha256"]:
        report.verified(f"{check_prefix}.invocation_identity", computed)
    else:
        report.mismatch(
            f"{check_prefix}.invocation_identity",
            f"recomputed {computed}, receipt records {receipt['invocation_sha256']}",
        )

    for inp in receipt.get("inputs", []):
        report.verified(f"{check_prefix}.input.{inp['input_slot']}.recorded", inp["sha256"])

    executions = [e for e in package.get("executions", []) if e["step_id"] == step_id]
    if not executions:
        report.not_checked(f"{check_prefix}.outputs_bind_package", f"package.json declares no execution for step {step_id!r}")
        return
    execution = executions[0]
    receipt_output_digests = {o["sha256"] for o in receipt.get("outputs", []) if o.get("state") == "collected" and "sha256" in o}
    for out in execution.get("outputs", []):
        claim_id = out["claim_id"]
        output_slot = out["output_slot"]
        check = f"{check_prefix}.output.{output_slot}"
        claim = claims_by_id.get(claim_id)
        if claim is None:
            report.not_checked(check, f"claims.json has no claim {claim_id!r} bound to this output slot")
            continue
        artifact_sha = claim["artifact"]["sha256"]
        if artifact_sha in receipt_output_digests:
            report.verified(check, f"claim {claim_id} artifact {artifact_sha} matches a collected receipt output")
        else:
            report.mismatch(
                check,
                f"claim {claim_id} binds artifact {artifact_sha}, which no collected output in {check_prefix} recorded",
            )


# ---------------------------------------------------------------------------
# Section 4: claims binding
#
# Rule source: docs/architecture/CAMPAIGN_EVALUATION.md ("Claims", "the
# claims document binds exactly one compiled snapshot by identity");
# ADR-0007 clause 6 ("The generated document is compared canonically with
# the committed claims.json, is bound to the package identities"). Compiled
# snapshot identity equality is checked; its recomputation from contract +
# registry is explicitly out of this profile (see module docstring item 4).
# ---------------------------------------------------------------------------


def verify_claims_binding(
    case_dir: Path,
    package: dict,
    claims: dict,
    campaign_report: Optional[dict],
    report: Report,
) -> dict[str, dict]:
    claims_by_id = {c["claim_id"]: c for c in claims.get("claims", [])}

    artifacts_by_evidence_id: dict[str, dict] = {}
    for art in package.get("artifacts", []):
        for ev_id in art["evidence_ids"]:
            artifacts_by_evidence_id[ev_id] = art

    for claim in claims.get("claims", []):
        check = f"claims.binding.{claim['claim_id']}"
        art = artifacts_by_evidence_id.get(claim["claim_id"])
        if art is None:
            report.not_checked(check, "package.json does not declare an artifact for this claim id (it may be a receipted/executed output; see the case's executions)")
            continue
        if art["sha256"] == claim["artifact"]["sha256"]:
            report.verified(check, f"claim artifact identity matches package-declared artifact {art['artifact_id']}")
        else:
            report.mismatch(
                check,
                f"claim binds {claim['artifact']['sha256']}, package artifact {art['artifact_id']} binds {art['sha256']}",
            )

    for inp in claims.get("inputs", []):
        check = f"claims.input_attestation.{inp['input_id']}"
        art = artifacts_by_evidence_id.get(f"input:{inp['input_id']}") or artifacts_by_evidence_id.get(inp["input_id"])
        if art is None:
            report.not_checked(check, "package.json does not declare an artifact for this input attestation")
            continue
        if art["sha256"] == inp["artifact"]["sha256"]:
            report.verified(check, "input attestation identity matches package-declared artifact")
        else:
            report.mismatch(check, f"input attestation binds {inp['artifact']['sha256']}, package artifact binds {art['sha256']}")

    if campaign_report is not None:
        check = "claims.compiled_snapshot_matches_campaign_report"
        claims_snapshot = claims.get("compiled_snapshot_sha256")
        report_snapshot = campaign_report.get("compiled_snapshot_sha256")
        if claims_snapshot and report_snapshot:
            if claims_snapshot == report_snapshot:
                report.verified(check, claims_snapshot)
            else:
                report.mismatch(check, f"claims.json binds {claims_snapshot}, campaign-report.json binds {report_snapshot}")
        else:
            report.not_checked(check, "compiled_snapshot_sha256 missing from claims.json or campaign-report.json")
        report.not_checked(
            "claims.compiled_snapshot_recomputation",
            UNSUPPORTED_NOTES["compiled_snapshot"],
        )
        check = "claims.claims_sha256_matches_campaign_report"
        report_claims_sha = campaign_report.get("claims_sha256")
        if report_claims_sha:
            computed = sha256_bytes(canonicalize_json((case_dir / "claims.json").read_bytes())) if (case_dir / "claims.json").is_file() else None
            if computed is None:
                report.not_checked(check, "claims.json not found on disk to recompute its canonical identity")
            elif computed == report_claims_sha:
                report.verified(check, computed)
            else:
                report.mismatch(check, f"recomputed claims.json canonical identity {computed}, campaign-report.json binds {report_claims_sha}")
    return claims_by_id


# ---------------------------------------------------------------------------
# Section 5 (glue): verdict re-derivation over one case's authored contract
#
# Rule source, for the qualification gate specifically: CAMPAIGN_EVALUATION.md
# ("Verdicts", "Claims") and ADR-0008. Proved against
# fixtures/semantic-core/campaigns' three qualification fixtures
# (``campaign.outside-qualification.not_evaluated`` for CORE-A4401 /
# ``not_evaluated.outside_qualification``, ``campaign.require-qualification.
# unqualified.not_evaluated`` for CORE-A4402 / ``not_evaluated.unqualified``,
# and ``campaign.require-qualification.qualified-inside.pass``) and against
# examples/cases/case-000's committed campaign-report.json for CORE-A4403's
# exact reasons shape (default policy, no qualification record at all).
# CORE-A4401's own reasons shape is this file's own reasonable rendering —
# no committed fixture's `expected` block states reasons content, only
# status and rule — and is reported informationally, never compared.
# ---------------------------------------------------------------------------


def apply_qualification_gate(claim: dict, basis_kind: str, require_qualification: bool) -> tuple[Optional[VerdictResult], list[dict]]:
    """Returns ``(early_result, extra_reasons)``. If ``early_result`` is not
    None the requirement is NOT_EVALUATED before the kernel runs at all;
    otherwise ``extra_reasons`` (possibly empty) is appended to whatever the
    kernel computes."""
    qualification = claim.get("qualification")
    if qualification is not None and qualification.get("state") in ("outside", "unknown"):
        result = VerdictResult(status="not_evaluated", rule="not_evaluated.outside_qualification")
        result.reasons = [
            {"code": "CORE-A4401", "evidence_id": claim["claim_id"], "qualification_state": qualification["state"]}
        ]
        return result, []
    if qualification is None and basis_kind in ("bounded", "enclosure"):
        if require_qualification:
            result = VerdictResult(status="not_evaluated", rule="not_evaluated.unqualified")
            result.reasons = [{"code": "CORE-A4402", "evidence_id": claim["claim_id"]}]
            return result, []
        return None, [
            {"code": "CORE-A4403", "owner": "policy_owner"},
            {"evidence_id": claim["claim_id"], "state": "unqualified"},
        ]
    return None, []


def admit_claims_for_metric(matches: list[dict]) -> list[ReducedEvidence]:
    """Applies the two claim-shape admission checks this profile can decide
    without a compiled snapshot or registry role bindings (the type-level
    SC-11 subset CAMPAIGN_EVALUATION.md's own admission table names): a
    duplicate claim for one output slot quarantines every claim for that
    slot ("cardinality", CORE-E7301 — proved against
    ``campaign.duplicate-claim.quarantine``), and a numeric claim whose
    lower bound exceeds its upper is refused for shape ("bounds are
    ordered", CORE-E7201 — proved against
    ``campaign.inverted-bounds.quarantine``). A3 (parent-admission cascade)
    and the A6 role-permitted-claim-model check are NOT implemented — they
    need the compiled dataflow graph and registry role declarations this
    profile does not build — and are reported ``not_checked`` by the caller
    where a fixture specifically needs them
    (``campaign.parent-missing.not_evaluated``,
    ``campaign.model-not-permitted.quarantine``).
    """
    reduced: list[ReducedEvidence] = []
    for m in matches:
        try:
            r = reduce_claim_value(m["claim_id"], "admitted", m["claim"])
        except CanonError:
            reduced.append(ReducedEvidence(evidence_id=m["claim_id"], state="quarantined", model=m["claim"].get("model", "?")))
            continue
        if r.lower is not None and r.upper is not None and r.lower > r.upper:
            r.state = "quarantined"
        reduced.append(r)
    if len(reduced) > 1:
        for r in reduced:
            r.state = "quarantined"
    return reduced


def find_claims_for_metric(claims: dict, metric: dict) -> list[dict]:
    if metric.get("source") == "step_output":
        return [
            c
            for c in claims.get("claims", [])
            if c["step_id"] == metric["step_id"] and c["output_slot"] == metric["output_slot"]
        ]
    return []


def _compare_verdict(check: str, computed: VerdictResult, expected: Optional[dict], report: Report) -> None:
    if expected is None:
        report.not_checked(check, "no committed verdict for this requirement id to compare against")
        return
    ev = expected["verdict"]
    mismatches = []
    if computed.status != ev["status"]:
        mismatches.append(f"status: computed {computed.status!r}, committed {ev['status']!r}")
    if computed.rule != ev["rule"]:
        mismatches.append(f"rule: computed {computed.rule!r}, committed {ev['rule']!r}")
    for field_name, attr in (
        ("canonical_unit", "canonical_unit"),
        ("limit_canonical", "limit_canonical"),
        ("lower_canonical", "lower_canonical"),
        ("upper_canonical", "upper_canonical"),
        ("nominal_canonical", "nominal_canonical"),
        ("tolerance_canonical", "tolerance_canonical"),
        ("basis_visible", "basis_visible"),
    ):
        if field_name in ev:
            got = getattr(computed, attr)
            if got != ev[field_name]:
                mismatches.append(f"{field_name}: computed {got!r}, committed {ev[field_name]!r}")
    if mismatches:
        report.mismatch(check, "; ".join(mismatches))
    else:
        report.verified(check, f"{computed.status} / {computed.rule}" + (f" margin {computed.margin}" if computed.margin else ""))


def verify_case_verdicts(
    contract: dict,
    registry: dict,
    claims: dict,
    campaign_report: Optional[dict],
    report: Report,
) -> None:
    kinds = kinds_from_registry_doc(registry)
    require_qualification = bool(contract.get("execution_policy", {}).get("require_qualification", False))
    verdicts_by_req = {v["requirement_id"]: v for v in (campaign_report or {}).get("verdicts", [])}

    for req in contract.get("requirements", []):
        check = f"verdict.{req['requirement_id']}"
        metric = req.get("metric")
        if not metric or metric.get("source") != "step_output":
            report.not_checked(check, "requirement's metric is not a step_output reference; this profile evaluates only step_output metrics (no committed case exercises the other form)")
            continue
        kind_id = req["limit"]["kind"]
        kind = kinds.get(kind_id)
        if kind is None:
            report.not_checked(check, f"registry.json declares no kind {kind_id!r} to scale this requirement's units against")
            continue
        matches = find_claims_for_metric(claims, metric)
        basis_kind = req["basis"]["kind"]
        limit = read_authoritative_exact(req["limit"]["value"])
        limit_unit = req["limit"]["unit"]
        tolerance = read_authoritative_exact(req["tolerance"]["value"]) if "tolerance" in req else None
        tolerance_unit = req["tolerance"]["unit"] if "tolerance" in req else None
        coverage_required = read_authoritative_exact(req["basis"]["coverage"]) if "coverage" in req["basis"] else None
        aggregation = req.get("aggregation")

        reduced_evidence = admit_claims_for_metric(matches)
        gated_result: Optional[VerdictResult] = None
        extra_reasons: list[dict] = []
        if len(reduced_evidence) == 1 and reduced_evidence[0].state == "admitted":
            gated_result, extra_reasons = apply_qualification_gate(matches[0], basis_kind, require_qualification)

        if gated_result is not None:
            result = gated_result
        else:
            result = evaluate_numeric_requirement(
                req["comparison"], limit, kind, limit_unit, basis_kind, coverage_required, tolerance, tolerance_unit, reduced_evidence, aggregation=aggregation
            )
            result.reasons = list(result.reasons) + extra_reasons
        _compare_verdict(check, result, verdicts_by_req.get(req["requirement_id"]), report)

    for creq in contract.get("categorical_requirements", []):
        check = f"verdict.{creq['requirement_id']}"
        metric = creq.get("metric")
        if not metric or metric.get("source") != "step_output":
            report.not_checked(check, "requirement's metric is not a step_output reference")
            continue
        matches = find_claims_for_metric(claims, metric)
        reduced_evidence = admit_claims_for_metric(matches)
        gated_result, extra_reasons = (None, [])
        if len(reduced_evidence) == 1 and reduced_evidence[0].state == "admitted":
            gated_result, extra_reasons = apply_qualification_gate(matches[0], "categorical", require_qualification)
        if gated_result is not None:
            result = gated_result
        else:
            predicate = creq["predicate"]
            accepted = [predicate["value"]] if predicate["operator"] == "equals" else predicate["values"]
            result = evaluate_categorical_requirement(predicate["operator"], accepted, reduced_evidence)
            result.reasons = list(result.reasons) + extra_reasons
        _compare_verdict(check, result, verdicts_by_req.get(creq["requirement_id"]), report)

    if campaign_report is None:
        report.not_checked("verdict.campaign_report", "no committed campaign-report.json supplied to compare against")
    report.not_checked("verdict.campaign_sha256", UNSUPPORTED_NOTES["campaign_sha256"])
    report.not_checked("verdict.qualification_predicate", UNSUPPORTED_NOTES["qualification_predicate"])
    report.not_checked("verdict.coverage", UNSUPPORTED_NOTES["coverage"])
    report.not_checked("verdict.presentation_gate", UNSUPPORTED_NOTES["presentation_gate"])


# ---------------------------------------------------------------------------
# Section 6: attempt lineage
#
# Rule source: docs/adr/0014-identity-bound-attempt-lineage.md clauses 2-4,
# schemas/attempt-lineage.v0.1-draft.schema.json. The parent-line hash and
# candidate-state digest algorithms are proved against every committed
# examples/cases/case-00{8,9}-*/search/attempts.jsonl entry (2 lineage
# children total) — see test_verifier.py
# ``test_lineage_matches_every_committed_attempts_log``. The recursive
# RFC 6901 diff (``changes``) is checked against the same two children;
# ADR-0014 clause 4 states the comparison rule, not a canonical traversal
# order for the resulting list, so this file compares the *set* of derived
# changes, never the order, against a committed child's ``changes`` array.
# ---------------------------------------------------------------------------


@dataclass
class Change:
    kind: str  # added | removed | replaced
    pointer: str
    before: Any = None
    after: Any = None
    value: Any = None

    def key(self) -> tuple:
        if self.kind == "replaced":
            return (self.kind, self.pointer, json.dumps(self.before, sort_keys=True), json.dumps(self.after, sort_keys=True))
        return (self.kind, self.pointer, json.dumps(self.value, sort_keys=True))


def derive_changes(before: Any, after: Any, pointer: str = "") -> list[Change]:
    """Recursively diffs ``before`` -> ``after`` by RFC 6901 pointer.

    Object members are compared key by key; equal-length arrays are
    compared index by index; anything else that differs (an array whose
    length changed, or a type change) is one ``replaced`` entry at this
    pointer, never an element-wise diff into it (ADR-0014 clause 4: "An
    array shape change is one replacement so index shifting cannot imply
    false element ancestry").
    """
    if before == after:
        return []
    if isinstance(before, dict) and isinstance(after, dict):
        changes: list[Change] = []
        for key in before:
            child_ptr = _ptr_push(pointer, key)
            if key not in after:
                changes.append(Change("removed", child_ptr, value=before[key]))
            else:
                changes.extend(derive_changes(before[key], after[key], child_ptr))
        for key in after:
            if key not in before:
                changes.append(Change("added", _ptr_push(pointer, key), value=after[key]))
        return changes
    if isinstance(before, list) and isinstance(after, list) and len(before) == len(after):
        changes = []
        for i, (b, a) in enumerate(zip(before, after)):
            changes.extend(derive_changes(b, a, _ptr_push(pointer, str(i))))
        return changes
    return [Change("replaced", pointer, before=before, after=after)]


def verify_attempt_log(log_path: Path, report: Report) -> None:
    lines = log_path.read_bytes().splitlines()
    parsed = [json.loads(line) for line in lines if line.strip()]
    by_attempt_id = {}
    for i, row in enumerate(parsed):
        attempt = row.get("attempt")
        if attempt is None:
            report.not_checked(f"lineage.line{i}", "row has no 'attempt' record")
            continue
        attempt_id = attempt["attempt_id"]
        by_attempt_id[attempt_id] = (i, row, lines[i])

        computed_state_digest = sha256_bytes(canonicalize_value(attempt["candidate_state"]))
        check = f"lineage.{attempt_id}.candidate_state_sha256"
        if computed_state_digest == attempt["candidate_state_sha256"]:
            report.verified(check, computed_state_digest)
        else:
            report.mismatch(check, f"recomputed {computed_state_digest}, record binds {attempt['candidate_state_sha256']}")

        if attempt["generation"] == 0:
            report.not_checked(f"lineage.{attempt_id}.parent", "generation 0 (root); no parent to verify")
            continue

        parent_id = attempt["parent_attempt_id"]
        if parent_id not in by_attempt_id:
            report.mismatch(f"lineage.{attempt_id}.parent", f"parent {parent_id!r} not found earlier in the same log")
            continue
        parent_index, parent_row, parent_line_bytes = by_attempt_id[parent_id]
        computed_parent_hash = sha256_bytes(parent_line_bytes)
        check = f"lineage.{attempt_id}.parent_record_sha256"
        if computed_parent_hash == attempt["parent_record_sha256"]:
            report.verified(check, computed_parent_hash)
        else:
            report.mismatch(check, f"recomputed {computed_parent_hash}, record binds {attempt['parent_record_sha256']}")

        parent_attempt = parent_row["attempt"]
        check = f"lineage.{attempt_id}.manifest_inherited"
        if attempt["fixed_manifest_sha256"] == parent_attempt["fixed_manifest_sha256"]:
            report.verified(check, attempt["fixed_manifest_sha256"])
        else:
            report.mismatch(check, f"child binds {attempt['fixed_manifest_sha256']}, parent binds {parent_attempt['fixed_manifest_sha256']}")

        check = f"lineage.{attempt_id}.compiled_snapshot_inherited"
        if attempt["fixed_compiled_snapshot_sha256"] == parent_attempt["fixed_compiled_snapshot_sha256"]:
            report.verified(check, attempt["fixed_compiled_snapshot_sha256"])
        else:
            report.mismatch(
                check,
                f"child binds {attempt['fixed_compiled_snapshot_sha256']}, parent binds {parent_attempt['fixed_compiled_snapshot_sha256']}",
            )

        check = f"lineage.{attempt_id}.changes"
        recomputed = derive_changes(parent_attempt["candidate_state"], attempt["candidate_state"])
        recomputed_keys = {c.key() for c in recomputed}
        recorded = attempt.get("changes", [])
        recorded_keys = set()
        for c in recorded:
            if c["kind"] == "replaced":
                recorded_keys.add(("replaced", c["pointer"], json.dumps(c["before"], sort_keys=True), json.dumps(c["after"], sort_keys=True)))
            else:
                recorded_keys.add((c["kind"], c["pointer"], json.dumps(c["value"], sort_keys=True)))
        if recomputed_keys == recorded_keys:
            report.verified(check, f"{len(recomputed_keys)} changes agree (order not compared; see ADR-0014 boundary note)")
        else:
            report.mismatch(
                check,
                f"recomputed {sorted(recomputed_keys)} != recorded {sorted(recorded_keys)}",
            )


# ---------------------------------------------------------------------------
# Case-level orchestration and CLI
# ---------------------------------------------------------------------------


def verify_case(case_dir: Path, roots: dict[str, Path]) -> Report:
    """Runs every applicable section of the profile against one case
    directory (an examples/cases/CASE-NNN-* layout: package.json plus the
    documents it names)."""
    report = Report(str(case_dir))
    package_path = case_dir / "package.json"
    if not package_path.is_file():
        report.not_checked("package.manifest", f"no package.json found at {case_dir}")
        return report
    package = load_json(package_path)

    verify_package_identity(case_dir, package, roots, report)

    docs_by_role: dict[str, list[dict]] = {}
    for d in package.get("documents", []):
        docs_by_role.setdefault(d["role"], []).append(d)

    def read_role(role: str) -> Optional[dict]:
        entries = docs_by_role.get(role)
        if not entries:
            return None
        path = case_dir / entries[0]["path"]
        return load_json(path) if path.is_file() else None

    contract = read_role("contract")
    registry = read_role("registry")
    claims = read_role("claims")
    campaign_report = read_role("expected_campaign_report")

    if claims is not None:
        claims_by_id = verify_claims_binding(case_dir, package, claims, campaign_report, report)
    else:
        report.not_checked("claims.binding", "no claims document declared in package.json")
        claims_by_id = {}

    for receipt_doc in docs_by_role.get("execution_receipt", []):
        step_id = receipt_doc.get("step_id", receipt_doc["document_id"])
        path = case_dir / receipt_doc["path"]
        if not path.is_file():
            report.not_checked(f"receipt.{step_id}", f"receipt file missing: {path}")
            continue
        receipt = load_json(path)
        verify_receipt(case_dir, step_id, receipt, package, claims_by_id, report)

    if contract is not None and registry is not None and claims is not None:
        verify_case_verdicts(contract, registry, claims, campaign_report, report)
    else:
        report.not_checked("verdict.all", "contract.json, registry.json, or claims.json missing/undeclared for this case")

    log_path = case_dir / "search" / "attempts.jsonl"
    if log_path.is_file():
        verify_attempt_log(log_path, report)
    else:
        report.not_checked("lineage.all", f"no attempt log at {log_path}")

    report.not_checked("signatures", UNSUPPORTED_NOTES["signatures"])
    return report


def cmd_verify_case(args: argparse.Namespace) -> int:
    roots = parse_source_roots(args.source_root or [])
    report = verify_case(Path(args.case_dir), roots)
    _emit(report, args)
    return report.exit_code()


def cmd_self_test(args: argparse.Namespace) -> int:
    """Runs this file's own unit tests (the fixture-vector proofs) and
    reports pass/fail as a Report, for a single consistent CLI surface."""
    import io
    import unittest

    here = Path(__file__).resolve().parent
    loader = unittest.TestLoader()
    suite = loader.discover(str(here), pattern="test_verifier.py")
    stream = io.StringIO()
    runner = unittest.TextTestRunner(stream=stream, verbosity=2)
    result = runner.run(suite)
    report = Report("self-test (fixture vectors)")
    if result.wasSuccessful():
        report.verified("self_test", f"{result.testsRun} tests passed")
    else:
        report.mismatch("self_test", stream.getvalue()[-4000:])
    _emit(report, args)
    return report.exit_code()


def _emit(report: Report, args: argparse.Namespace) -> None:
    if getattr(args, "json", False):
        print(json.dumps(report.to_dict(), indent=2, sort_keys=False))
    else:
        print(report.to_text())


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="avila_core_verify",
        description="Independent offline verifier for exported Avila Core packages (Slice I). See this file's module docstring for the exact supported profile.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_case = sub.add_parser("verify-case", help="Run every applicable profile section against one case directory")
    p_case.add_argument("case_dir", help="Path to a case directory (contains package.json)")
    p_case.add_argument(
        "--source-root",
        action="append",
        metavar="NAME=PATH",
        help="External artifact root, exactly like `avila-core run --source-root NAME=PATH`; may be repeated",
    )
    p_case.add_argument("--json", action="store_true", help="Emit the machine-readable JSON report instead of text")
    p_case.set_defaults(func=cmd_verify_case)

    p_self = sub.add_parser("self-test", help="Run this file's own fixture-vector unit tests")
    p_self.add_argument("--json", action="store_true")
    p_self.set_defaults(func=cmd_self_test)

    return parser


def main(argv: Optional[list[str]] = None) -> int:
    parser = build_arg_parser()
    args = parser.parse_args(argv)
    return args.func(args)


COMPARISON_TOKEN_TO_COMPARISON = {
    "le": "less_than_or_equal",
    "lt": "less_than",
    "ge": "greater_than_or_equal",
    "gt": "greater_than",
}


def verify_verdict_log_margins(log_path: Path, report: Report) -> None:
    """Cross-checks the ``margin`` field of every ``le``/``lt``/``ge``/``gt``
    verdict recorded in a campaign-log.jsonl / attempts.jsonl style file
    against ``compute_margin``/``render_margin``, using only the log's own
    recorded ``rule``/``limit``/``lower``/``upper``/``nominal`` fields — no
    claims.json or contract.json is needed, since the log already carries
    resolved, unit-scaled canonical values.

    ``equal.*`` and ``categorical.*`` verdicts, and any line missing a
    decisive bound, are reported ``not_checked`` by name rather than
    guessed at: margin does not appear in campaign-report.json's own schema
    at all, so this cross-check exists only where a case happens to commit
    a JSONL log, and only for the comparisons this file's margin formula
    (see ``compute_margin``) actually covers.
    """
    lines = log_path.read_bytes().splitlines()
    for i, line in enumerate(lines):
        if not line.strip():
            continue
        row = json.loads(line)
        for v in row.get("verdicts", []) or []:
            requirement_id = v.get("requirement_id", f"line{i}")
            check = f"log.{log_path.name}:{i}.{requirement_id}.margin"
            if "margin" not in v or "rule" not in v:
                continue
            parts = v["rule"].split(".")
            cmp_token = parts[-2] if len(parts) >= 2 else ""
            if cmp_token not in COMPARISON_TOKEN_TO_COMPARISON:
                report.not_checked(check, f"rule {v['rule']!r} is not a le/lt/ge/gt comparison this margin formula covers")
                continue
            comparison = COMPARISON_TOKEN_TO_COMPARISON[cmp_token]
            side = COMPARISON_SIDE[comparison]
            decisive_text = (v.get("upper") if side == "le" else v.get("lower")) or v.get("nominal")
            if decisive_text is None or "limit" not in v:
                report.not_checked(check, "log line does not record a decisive bound and limit to recompute margin from")
                continue
            limit = read_authoritative_exact(v["limit"])
            decisive = read_authoritative_exact(decisive_text)
            computed = render_margin(compute_margin(comparison, limit, decisive))
            if computed == v["margin"]:
                report.verified(check, computed)
            else:
                report.mismatch(check, f"recomputed margin {computed}, log records {v['margin']!r}")


if __name__ == "__main__":
    sys.exit(main())
