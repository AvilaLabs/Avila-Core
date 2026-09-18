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
     and reported as such. The snapshot itself is then *recomputed*:
     ``avila_core_lower`` independently lowers the committed contract and
     registry into the compiled body's identity (canonical-document
     digests, slot resolution, topological order, parameter/material-factor
     lowering, determinism/seed rules, presentation-gate resolution, unit
     scaling, basis and claim-model sufficiency, categorical requirements)
     and the result must equal the recorded ``compiled_snapshot_sha256``.
     A pair the port shows the compiler would have rejected is a
     ``mismatch``, never ``verified``; a construct outside the port's
     covered subset is ``not_checked`` with the uncovered construct named.
     Proved against every compiled fixture's pinned snapshot across the
     five compiler suites in fixtures/semantic-core/types/ and every
     committed example case (test_verifier.py).

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
     a committed value to compare against, and says so. The committed
     report's ``campaign_sha256`` is recomputed over its semantic body
     (everything except the field itself and the informational ``notice``)
     and compared — the digest binds findings and admissions fields this
     profile does not otherwise re-derive.

     Explicitly NOT_CHECKED and named as such: full qualification envelope
     predicate-over-real-facts re-evaluation at the verdict layer (this
     file reads a claim's already-recorded ``qualification.state``,
     exactly as the real evaluator does per CAMPAIGN_EVALUATION.md — "It
     does evaluate qualification positions already carried by claims" —
     rather than re-deriving `inside` / `outside` / `unknown` from real
     facts here; see item 9 for how much of that this profile independently
     re-derives instead, and why not all of it). Claim admission covers
     the SC-11 subset this profile can decide — slot declaredness,
     permitted claim models, per-slot cardinality, numeric shape — plus
     A3's parent-admission cascade over the resolved workflow graph.

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
     margin in a scratch copy of CASE-001 and CASE-003, plus (item 8-9's
     own mutations) a flipped signature byte, a swapped key id, a
     signature over a re-blessed manifest, and a qualification envelope
     term edited to lie — each shown to be named by this verifier; plus
     the positive path on CASE-000, CASE-001, CASE-002, CASE-003,
     CASE-008, and CASE-009 with whichever artifact roots exist on this
     machine (unavailable roots are reported ``not_checked`` by name,
     never silently passed).

  8. ADR-0015 signatures — a from-scratch, standard-library Ed25519 (RFC
     8032 section 5.1: field arithmetic, encoding, key generation, sign,
     verify — no cryptography dependency, no import of ``ed25519-dalek``),
     proved against every RFC 8032 section 7.1 test vector and against
     every one of the nine ADR-0015 signature documents actually committed
     under ``examples/cases/{case-001,002,003}-*/signatures/``. Verifies
     the package manifest's requester signature (the exact digest rule ADR-
     0015's "Implementation notes" state: the manifest with the signature
     document's own ``documents[]`` entry removed, canonicalised), each
     execution receipt's runner signature (target: role
     ``execution_receipt``, document id the step id), and any campaign
     log-line runner signatures present (target: role ``log_line``, the
     canonical form of the line with ``signature`` removed). Reports, per
     signature, exactly one of ``verified`` (naming the signer's key id),
     ``invalid`` (naming the reason — a tampered digest, a wrong or
     unlisted key, or a signature that plain does not verify — this state
     is a strict superset of what a trust root can decide, so a corrupted
     signature is visibly wrong even without one), or ``unsigned`` (no
     signature document names this target at all). ``--trust-root FILE``
     supplies the requester/runner public keys a run accepts
     (``avila.core/trust-root/v0.1-draft``, e.g.
     ``examples/keys/trust-root.json``); without it, every signature is
     reported ``not_checked`` (present and internally consistent, but
     nothing to cryptographically check it against) and never
     ``verified`` — ADR-0015 clause 3.

  9. Qualification envelopes (ADR-0008 as refined by ADR-0018, S-039,
     S-046) — a from-scratch Strong-Kleene predicate evaluator (ADR-0006
     SC-7; the same three-valued ``always`` / ``all`` / ``any`` / ``not`` /
     ``param_in_range`` / ``input_attribute_in`` / ``environment_image_in``
     / ``fact`` grammar the kernel's applicability evaluator implements),
     proved against every vector in
     ``fixtures/semantic-core/vectors/scope-predicates.v1.json``.
     For every qualification-carrying claim, independently re-derives and
     checks: that its recorded qualification identity (id/revision/sha256)
     names a qualification document the package actually binds; that its
     recorded per-term predicate text is, in order, exactly the bound
     record's own scope terms (catches an edited, reordered, or substituted
     term); that the persisted applicability context the claim is required
     to carry (``qualification.context``, ADR-0018) re-evaluates each scope
     term to the recorded per-term result and the recorded aggregate state
     (catches a fact, term result, or state edited after the runner
     assessed them, and a context lifted from another step); that each
     context input's recorded sha256/media_type equals the step receipt's
     bound input for that slot and every fact's source.receipt equals
     ``plan:`` + the receipt's invocation identity (binds the facts to the
     exact planned invocation); and that it never carries an assessment on
     an output slot the record's ``covered_output_slots`` excludes (S-039).
     What is still not proved: that the adapter extracted those facts
     correctly from the staged bytes — the persisted facts are producer
     assertions with provenance, and re-derivation proves the recorded
     assessment is what those facts imply, not that the facts are right.
     ``candidates/outside-envelope*.json`` in CASE-001 are free-input search
     candidates, not runs of their own with a committed campaign report (a
     supplied free input invalidates every step it reaches, per S-023);
     this profile has no committed claims.json to check them against and
     checks none, rather than fabricating one.

     With ``--as-of INSTANT`` (ADR-0006's supplied-snapshot historical
     verification), every qualified claim additionally earns one
     informational ``as_of`` line naming its recorded state beside the
     labeled state it would carry at the supplied instant under the
     supplied material: ``absent`` when the claim's bound record digest is
     not in the material, then ``revoked`` / ``superseded`` / ``expired``
     in campaign-refusal precedence, then the envelope its own recorded
     facts imply at that instant — an unrecognized owner under the
     supplied policy is a suffix, never the label, because recognition is
     a per-requirement gate (nominal basis exempt) rather than an envelope
     state. ``--as-of-material DIR`` supplies the snapshot as another case
     package whose bound records, revocations, signatures, and contract
     policy stand in for the material known then; without it the case's
     own bound set is the material. A divergence between the recorded and
     labeled states is a datum, never a failure — that is what the
     historical question is for.

 10. Requirement-set coverage (S-024) — a case package may bind one
     ``requirement_set`` document and declare in its manifest which
     contract requirements cover each set entry and, for the rest, a
     reason and an accepting owner. A ``run`` refuses to spend evaluation
     on an incomplete declaration (an unstated ``must_state`` omission, or
     coverage only on a basis weaker than the entry's ``minimum_basis``),
     so a package whose committed claims and campaign report exist asserts
     its coverage re-derives ``complete``. This section re-derives the
     assessment — declaration issues, per-entry states, aggregate status —
     from the committed manifest, requirement set, and contract, and
     reports a derived ``incomplete`` as a ``mismatch``. The per-entry
     report is never committed, so the derived status is the only
     committed observable this check can compare; it does not fabricate a
     per-entry diff.

 11. Staged-review records — the optional presentation-gate's committed
     half. For every manifest document of role ``staged_review_record``:
     recomputes the request's ``request_sha256`` and the record's own
     ``record_sha256`` (canonical body minus the digest field, the same
     rule ``campaign_sha256`` uses); binds the request's compiled-snapshot
     and campaign identities against every committed carrier (claims,
     campaign report, recorded log lines — a binding no committed record
     carries is reported ``not_checked`` as an unresolvable reference, not
     ``mismatch``, since it names a run this package does not commit);
     re-derives each ``presented_evidence`` entry from claims.json exactly
     as the run realises it (contract inputs to ``input:{id}`` attestations,
     step outputs to their claim ids, digest and media type compared);
     checks readiness against the recorded missing list, reviewer role, the
     disposition against the request's allowed dispositions, and the
     eligibility-policy digest against a bound ``review_policy`` document.
     Still named out: the agent's prose ``rationale``/``actions``, and
     ``attestation`` — reported, never authenticated.

Explicitly refused (outside this profile, by name, never silently):
  - run-time presentation-gate realisation itself (the transient
    ``presentation_gates`` report a run emits is never committed; only the
    reviewer's committed record is);
  - archive/package-root canonicalisation beyond the flat document+artifact
    list a case-package.v0.1-draft manifest already enumerates;
  - the compiler's finding vocabulary itself — avila_core_lower reproduces
    whether a pair compiles and what its compiled body is, not the codes,
    pointers, and repair candidates a rejected compile would report;
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


# ---------------------------------------------------------------------------
# Report primitives
# ---------------------------------------------------------------------------


@dataclass
class CheckResult:
    """One line of the verifier's report.

    ``status`` is one of ``verified``, ``mismatch``, ``not_checked``,
    ``as_of`` — the last an informational labeled result emitted only by
    ``--as-of`` historical verification (section 10): it never counts as
    verified and never fails the report.
    """

    check: str
    status: str
    detail: str = ""
    reason: str = ""

    def __post_init__(self) -> None:
        if self.status not in ("verified", "mismatch", "not_checked", "as_of"):
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

    def as_of(self, check: str, detail: str = "") -> None:
        self.checks.append(CheckResult(check, "as_of", detail=detail))

    def extend(self, other: "Report") -> None:
        self.checks.extend(other.checks)

    def counts(self) -> dict:
        out = {"verified": 0, "mismatch": 0, "not_checked": 0, "as_of": 0}
        for c in self.checks:
            out[c.status] += 1
        return out

    def exit_code(self) -> int:
        return 1 if any(c.status == "mismatch" for c in self.checks) else 0

    def to_dict(self) -> dict:
        return {
            "schema": "avila.core/independent-verifier-report/v2",
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
            + (f", {counts['as_of']} as_of" if counts["as_of"] else "")
        )
        for c in self.checks:
            marker = {"verified": "OK  ", "mismatch": "FAIL", "not_checked": "N/C ", "as_of": "ASOF"}[c.status]
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
    if non_admitted:
        # Any non-admitted claim settles nothing: the verdict does not
        # evaluate the admitted half (verdict-calculus vectors
        # not_evaluated.mixed-*). The first non-admitted claim decides the
        # rule; every non-admitted claim is named in the reasons.
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
    (CASE-000-R3). ``categorical.in_set.*`` and every edge ordering —
    any non-admitted claim settles the verdict before the admitted half
    is read, duplicates, and a missing category — are proved against the
    ``categorical_vectors`` section of verdict-calculus.v1.json.
    """
    result = VerdictResult(status="not_evaluated", rule="not_evaluated.missing")
    if not evidence:
        result.reasons = [{"code": "CORE-R3301", "owner": "requester"}]
        return result
    admitted = [e for e in evidence if e.state == "admitted"]
    non_admitted = [e for e in evidence if e.state != "admitted"]
    if non_admitted:
        # Same ordering the kernel and numeric evaluator apply: any
        # non-admitted claim means the metric is not settled.
        result.rule = f"not_evaluated.{non_admitted[0].state}"
        result.reasons = [{"evidence_id": e.evidence_id, "state": e.state} for e in non_admitted]
        return result
    if len(admitted) > 1:
        result.rule = "not_evaluated.duplicate_claim"
        result.reasons = [{"code": "CORE-E7301", "evidence_ids": [e.evidence_id for e in admitted]}]
        return result
    category = admitted[0].category
    if category is None:
        result.rule = "not_evaluated.category_missing"
        result.reasons = [{"code": "CORE-R3301", "owner": "executor"}]
        return result
    prefix = "categorical.equals" if operator == "equals" else "categorical.in_set"
    matched = (category == accepted[0]) if operator == "equals" else (category in accepted)
    result.status = "pass" if matched else "fail"
    result.rule = f"{prefix}.{'match' if matched else 'mismatch'}"
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
# snapshot identity equality is checked, then the snapshot is recomputed
# from the committed contract+registry by avila_core_lower (module
# docstring item 4).
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

        check = "claims.compiled_snapshot_recomputation"
        doc_paths = {d["role"]: d["path"] for d in package.get("documents", [])}
        contract_path = case_dir / doc_paths["contract"] if "contract" in doc_paths else None
        registry_path = case_dir / doc_paths["registry"] if "registry" in doc_paths else None
        recorded = claims.get("compiled_snapshot_sha256")
        if recorded is None:
            report.not_checked(check, "claims.json does not bind a compiled snapshot identity")
        elif contract_path is None or registry_path is None or not (contract_path.is_file() and registry_path.is_file()):
            report.not_checked(check, "contract/registry documents are not present to recompute the snapshot")
        else:
            # Imported lazily: avila_core_lower reuses this file's canonical
            # profile and exact-number machinery, so a top-level import here
            # would be circular for whoever is imported second.
            import avila_core_lower

            try:
                recomputed = avila_core_lower.lower_compiled_snapshot(
                    contract_path.read_bytes(), registry_path.read_bytes()
                )
            except avila_core_lower.CannotLower as error:
                report.not_checked(check, f"lowering subset does not cover this pair: {error}")
            except avila_core_lower.WouldReject as error:
                report.mismatch(check, f"the committed contract+registry would not compile: {error}")
            else:
                if recomputed == recorded:
                    report.verified(check, recomputed)
                else:
                    report.mismatch(check, f"recomputed {recomputed}, claims.json binds {recorded}")
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


def apply_qualification_gate(claim: dict, basis_kind: str, require_qualification: bool, recognized_owners: Optional[dict] = None) -> tuple[Optional[VerdictResult], list[dict]]:
    """Returns ``(early_result, extra_reasons)``. If ``early_result`` is not
    None the requirement is NOT_EVALUATED before the kernel runs at all;
    otherwise ``extra_reasons`` (possibly empty) is appended to whatever the
    kernel computes."""
    qualification = claim.get("qualification")
    # Recognition outranks every other qualification state: an unlisted
    # issuer's assessment cannot establish the requirement whatever the
    # record's own terms evaluated — including `inside`. Nominal-basis
    # requirements are unaffected, as in the compiler.
    if qualification is not None and basis_kind != "nominal" and recognized_owners and qualification.get("owner") not in recognized_owners:
        result = VerdictResult(status="not_evaluated", rule="not_evaluated.qualification_not_recognized")
        result.reasons = [
            {"code": "CORE-A4601", "evidence_id": claim["claim_id"], "qualification_owner": qualification.get("owner")}
        ]
        return result, []
    # Lifecycle refusals outrank envelope state too: a withdrawn or
    # superseded record cannot stand whatever its terms said. `revoked_by`
    # and `superseded_by` ride on the claim as the bound set's facts.
    if qualification is not None and basis_kind != "nominal" and qualification.get("revoked_by"):
        result = VerdictResult(status="not_evaluated", rule="not_evaluated.qualification_revoked")
        result.reasons = [
            {"code": "CORE-A4603", "evidence_id": claim["claim_id"], "revoked_by": qualification.get("revoked_by")}
        ]
        return result, []
    if qualification is not None and basis_kind != "nominal" and qualification.get("superseded_by"):
        result = VerdictResult(status="not_evaluated", rule="not_evaluated.qualification_superseded")
        result.reasons = [
            {"code": "CORE-A4101", "evidence_id": claim["claim_id"], "superseded_by": qualification.get("superseded_by")}
        ]
        return result, []
    if qualification is not None and qualification.get("state") in ("outside", "unknown", "expired"):
        state = qualification["state"]
        if state == "expired":
            rule, code = "not_evaluated.qualification_expired", "CORE-A4602"
        elif state == "outside":
            rule, code = "not_evaluated.outside_qualification", "CORE-A4401"
        else:
            rule, code = "not_evaluated.qualification_unknown", "CORE-A4401"
        result = VerdictResult(status="not_evaluated", rule=rule)
        result.reasons = [
            {"code": code, "evidence_id": claim["claim_id"], "qualification_state": state}
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


def _claim_source_key(source: dict) -> tuple:
    if source.get("source") == "step_output":
        return ("step_output", source.get("step_id"), source.get("output_slot"))
    return ("contract_input", source.get("input_id"))


def _claim_shape_ok(claim: dict) -> bool:
    """The numeric shape admission checks (parseable exact bounds, lower
    never above upper) — True when the claim's value is well formed."""
    try:
        r = reduce_claim_value("claim", "admitted", claim)
    except CanonError:
        return False
    return not (r.lower is not None and r.upper is not None and r.lower > r.upper)


def admit_claims_for_metric(
    matches: list[dict],
    permitted_models_for=None,
    cascade: Optional[dict] = None,
) -> list[ReducedEvidence]:
    """Applies the claim-shape admission checks this profile can decide
    without a compiled snapshot (the type-level SC-11 subset
    CAMPAIGN_EVALUATION.md's own admission table names): a claim naming an
    output slot the step's capability type does not declare is dropped
    before admission (CORE-E7002 — proved against
    ``campaign.undeclared-slot.rejected``), a claim whose model the slot's
    ``permitted_claim_models`` does not list is quarantined (A6,
    CORE-E7201 — proved against ``campaign.model-not-permitted.quarantine``
    and ``campaign.model-mismatch.quarantine``), a duplicate claim for one
    output slot quarantines every claim for that slot ("cardinality",
    CORE-E7301 — proved against ``campaign.duplicate-claim.quarantine``),
    and a numeric claim whose lower bound exceeds its upper is refused for
    shape ("bounds are ordered", CORE-E7201 — proved against
    ``campaign.inverted-bounds.quarantine``). ``permitted_models_for``
    resolves the slot's declared models: a set of admitted model names, an
    empty set when the slot is undeclared (the claim drops out as the
    compiler's `continue` does), or None when the registry could not be
    indexed. ``cascade`` maps claim_id → ``admitted``|``quarantined`` from
    A3's parent-admission walk over the resolved dataflow graph; a claim
    whose entry is ``quarantined`` quarantines here (proved against
    ``campaign.parent-missing.not_evaluated``), while a claim absent from
    the map falls through to the local checks.
    """
    reduced: list[ReducedEvidence] = []
    for m in matches:
        if cascade is not None and m["claim_id"] in cascade:
            if cascade[m["claim_id"]] != "admitted":
                reduced.append(
                    ReducedEvidence(
                        evidence_id=m["claim_id"], state="quarantined",
                        model=m["claim"].get("model", "?")))
                continue
        elif permitted_models_for is not None:
            permitted = permitted_models_for(m)
            if permitted is not None and m["claim"].get("model") not in permitted:
                # An undeclared slot drops the claim entirely; a declared
                # slot with an unpermitted model quarantines it.
                if permitted:
                    reduced.append(
                        ReducedEvidence(
                            evidence_id=m["claim_id"], state="quarantined",
                            model=m["claim"].get("model", "?")))
                continue
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
    recognized_owners = contract.get("execution_policy", {}).get("recognized_qualification_owners", {})
    verdicts_by_req = {v["requirement_id"]: v for v in (campaign_report or {}).get("verdicts", [])}

    # A6's slot-declaredness and permitted-claim-model checks ride the
    # lowerer's registry index — the same structures the compiler builds.
    # Imported lazily: avila_core_lower reuses this file's canonical JSON.
    import avila_core_lower

    steps = {s.get("step_id"): s for s in contract.get("workflow", [])}
    try:
        registry_index = avila_core_lower._build_index(registry)
    except Exception:
        registry_index = None

    def permitted_models_for(claim):
        if registry_index is None:
            return None
        step = steps.get(claim.get("step_id"))
        if step is None:
            return None
        capability = registry_index.capability_types.get(
            avila_core_lower._ref_key(step.get("capability_type", {})))
        if capability is None:
            return None
        output = next(
            (o for o in capability.outputs if o.get("slot_id") == claim.get("output_slot")),
            None,
        )
        if output is None:
            return set()
        return {m["model"] for m in output.get("permitted_claim_models", [])}

    # A3's parent-admission cascade: a claim is admitted only when every
    # source its step binds is itself admitted — a contract input by its
    # attestation in claims.inputs, a step output by its claim's own
    # admission. The lowerer's resolved_workflow returns the bindings the
    # compiler would auto-bind and their topological order; walking it in
    # order propagates quarantine downstream exactly as
    # campaign/admission.rs's `admitted` set does. On any resolution
    # failure the cascade is left unset and the local per-claim checks
    # still run.
    cascade = None
    if registry_index is not None:
        try:
            bindings, order = avila_core_lower.resolved_workflow(contract, registry)
        except Exception:
            bindings, order = None, []
        if bindings is not None:
            attested = {
                i["input_id"]
                for i in claims.get("inputs", [])
                if isinstance(i, dict) and isinstance(i.get("input_id"), str)
            }
            source_ok: dict = {}
            for inp in contract.get("inputs", []):
                source_ok[("contract_input", inp["input_id"])] = (
                    inp["input_id"] in attested
                )
            cascade = {}
            for step_id in order:
                step = steps.get(step_id)
                if step is None:
                    continue
                capability = registry_index.capability_types.get(
                    avila_core_lower._ref_key(step.get("capability_type", {}))
                )
                if capability is None:
                    continue
                parents_ok = all(
                    source_ok.get(_claim_source_key(b["source"]), False)
                    for b in bindings.get(step_id, [])
                )
                for output in capability.outputs:
                    slot = output["slot_id"]
                    slot_claims = [
                        c
                        for c in claims.get("claims", [])
                        if c.get("step_id") == step_id
                        and c.get("output_slot") == slot
                    ]
                    permitted = {
                        m["model"]
                        for m in output.get("permitted_claim_models", [])
                    }
                    admitted = parents_ok and len(slot_claims) == 1
                    for claim in slot_claims:
                        # SC-5 clause 6: a `partial` claim is admitted only
                        # on a slot declaring `permits_partial` — elsewhere
                        # it quarantines, exactly as campaign/admission.rs's
                        # CORE-E7201 check does.
                        claim_admitted = (
                            admitted
                            and claim["claim"].get("model") in permitted
                            and _claim_shape_ok(claim["claim"])
                            and (
                                not claim.get("partial")
                                or output.get("permits_partial", False)
                            )
                        )
                        cascade[claim["claim_id"]] = (
                            "admitted" if claim_admitted else "quarantined"
                        )
                    source_ok[("step_output", step_id, slot)] = any(
                        cascade[c["claim_id"]] == "admitted" for c in slot_claims
                    )

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

        reduced_evidence = admit_claims_for_metric(matches, permitted_models_for, cascade)
        gated_result: Optional[VerdictResult] = None
        extra_reasons: list[dict] = []
        if len(reduced_evidence) == 1 and reduced_evidence[0].state == "admitted":
            gated_result, extra_reasons = apply_qualification_gate(matches[0], basis_kind, require_qualification, recognized_owners)

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
        reduced_evidence = admit_claims_for_metric(matches, permitted_models_for, cascade)
        gated_result, extra_reasons = (None, [])
        if len(reduced_evidence) == 1 and reduced_evidence[0].state == "admitted":
            gated_result, extra_reasons = apply_qualification_gate(matches[0], "categorical", require_qualification, recognized_owners)
        if gated_result is not None:
            result = gated_result
        else:
            predicate = creq["predicate"]
            accepted = [predicate["value"]] if predicate["operator"] == "equals" else predicate["values"]
            result = evaluate_categorical_requirement(predicate["operator"], accepted, reduced_evidence)
            result.reasons = list(result.reasons) + extra_reasons
        _compare_verdict(check, result, verdicts_by_req.get(creq["requirement_id"]), report)

    # SC-9 clause 6: the contract's `completion` block is a delivery
    # statement evaluated against the derived verdicts — never a verdict
    # input. `not_evaluated` never fulfills; an `inconclusive` verdict
    # fulfills only under a declared-permitted named reason. campaign/mod.rs
    # `assess_completion` is the authoritative implementation this mirrors.
    block = contract.get("completion")
    if block is None:
        if (campaign_report or {}).get("completion") is not None:
            report.mismatch(
                "completion.block",
                "campaign report carries a completion assessment the contract never declared",
            )
    elif campaign_report is None:
        report.not_checked("completion.block", "no committed campaign-report.json to compare against")
    else:
        fulfilling = set(block.get("fulfilling_verdicts", []))
        permitted_reasons = set(block.get("permitted_inconclusive_reasons", []))
        entries = []
        for v in campaign_report.get("verdicts", []):
            status = v["verdict"]["status"]
            rule = v["verdict"].get("rule", "")
            if status == "not_evaluated":
                fulfilling_entry = False
                reason = "`not_evaluated` never completes a substantive contract"
            elif status == "inconclusive" and "inconclusive" not in fulfilling:
                fulfilling_entry = False
                reason = "`inconclusive` is not a declared fulfilling verdict"
            elif status == "inconclusive" and rule not in permitted_reasons:
                fulfilling_entry = False
                reason = f"inconclusive reason `{rule}` is not permitted"
            elif status in fulfilling:
                fulfilling_entry = True
                reason = f"`{status}` is a declared fulfilling verdict"
            else:
                fulfilling_entry = False
                reason = f"`{status}` is not a declared fulfilling verdict"
            entries.append(
                {
                    "requirement_id": v["requirement_id"],
                    "verdict": status,
                    "fulfilling": fulfilling_entry,
                    "reason": reason,
                }
            )
        derived = {
            "status": "complete" if all(e["fulfilling"] for e in entries) else "incomplete",
            "entries": entries,
        }
        committed = campaign_report.get("completion")
        if committed == derived:
            report.verified(
                "completion.block",
                "re-derived the completion assessment from the declared block and the committed verdicts",
            )
        else:
            report.mismatch("completion.block", f"derived {derived}, committed {committed}")

    if campaign_report is None:
        report.not_checked("verdict.campaign_report", "no committed campaign-report.json supplied to compare against")
        report.not_checked("verdict.campaign_sha256", "no committed campaign-report.json to recompute")
    elif campaign_report.get("campaign_sha256") is None:
        report.not_checked(
            "verdict.campaign_sha256",
            "campaign report carries no campaign_sha256 (a rejected report has none)",
        )
    else:
        # campaign/mod.rs computes the identity over the report's semantic
        # body — schema_version, semantic_profile, status,
        # compiled_snapshot_sha256, claims_sha256, findings, admissions,
        # verdicts — canonicalised; `campaign_sha256` itself and the
        # informational `notice` are outside the body. The identity body
        # always carries `findings`, while the committed report omits the
        # field when empty — reproduced against all ten committed
        # campaign-report.json files, which agree either way.
        body = {k: v for k, v in campaign_report.items() if k not in ("campaign_sha256", "notice")}
        body.setdefault("findings", [])
        recomputed = sha256_bytes(canonicalize_value(body))
        if recomputed == campaign_report["campaign_sha256"]:
            report.verified("verdict.campaign_sha256", "recomputed whole-report canonical identity matches")
        else:
            report.mismatch(
                "verdict.campaign_sha256",
                f"recomputed {recomputed}, committed {campaign_report['campaign_sha256']}",
            )
    report.not_checked(
        "verdict.presentation_gate",
        "a run-time gate realisation is not committed to the package; the committed staged-review records are checked in section 11",
    )


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
# Section 8: Ed25519 (RFC 8032) — the ADR-0015 signature primitive
#
# A from-scratch, standard-library-only Ed25519 (sign, public-key-from-seed,
# verify), re-derived from RFC 8032 section 5.1's own field arithmetic,
# encoding, key-generation, sign, and verify algorithms (not from
# ed25519-dalek, which this profile does not import). Curve constants (d, the
# base point B, the group order L) are computed from RFC 8032's own defining
# relations (d = -121665/121666 mod p; the base point's y-coordinate is 4/5
# mod p, and its x-coordinate is recovered by the same point-decoding
# procedure section 5.1.3 defines) rather than copied as literals, and cross-
# checked once, in TestEd25519RFC8032Vectors, against the literal decimal
# constants RFC 8032's Table 1 states for d, B, and L.
#
# Proved against every one of the five RFC 8032 section 7.1 test vectors
# (TEST 1, TEST 2, TEST 3, TEST 1024, TEST SHA(abc)), embedded verbatim in
# test_verifier.py's RFC8032_ED25519_VECTORS, and — far more relevantly to
# this repository — against every one of the nine ADR-0015 signature
# documents actually committed under examples/cases/*/signatures/, produced
# by the real `ed25519-dalek`-backed `avila-core sign` (see
# TestSignaturesAgainstCommittedDocuments): this file's verify() accepts
# every one of them.
# ---------------------------------------------------------------------------

_ED25519_P = 2**255 - 19
_ED25519_D = (-121665 * pow(121666, _ED25519_P - 2, _ED25519_P)) % _ED25519_P
_ED25519_L = 2**252 + 27742317777372353535851937790883648493


def _ed25519_inv(x: int) -> int:
    return pow(x, _ED25519_P - 2, _ED25519_P)


def _ed25519_xrecover(y: int) -> int:
    """RFC 8032 section 5.1.3's square-root recovery, specialised to
    p = 5 (mod 8) (section 5.1.1's shortcut)."""
    p = _ED25519_P
    xx = (y * y - 1) * _ed25519_inv(_ED25519_D * y * y + 1) % p
    x = pow(xx, (p + 3) // 8, p)
    if (x * x - xx) % p != 0:
        x = (x * pow(2, (p - 1) // 4, p)) % p
    if x % 2 != 0:
        x = p - x
    return x


_ED25519_GY = (4 * _ed25519_inv(5)) % _ED25519_P
_ED25519_GX = _ed25519_xrecover(_ED25519_GY)
# Extended homogeneous coordinates (X, Y, Z, T) per RFC 8032 section 5.1.4.
_ED25519_BASE = (_ED25519_GX, _ED25519_GY, 1, (_ED25519_GX * _ED25519_GY) % _ED25519_P)
_ED25519_NEUTRAL = (0, 1, 1, 0)


def _ed25519_add(p1: tuple, p2: tuple) -> tuple:
    """RFC 8032 section 5.1.4's complete twisted-Edwards addition law."""
    p = _ED25519_P
    x1, y1, z1, t1 = p1
    x2, y2, z2, t2 = p2
    a = (y1 - x1) * (y2 - x2) % p
    b = (y1 + x1) * (y2 + x2) % p
    c = t1 * 2 * _ED25519_D % p * t2 % p
    d = z1 * 2 % p * z2 % p
    e = (b - a) % p
    f = (d - c) % p
    g = (d + c) % p
    h = (b + a) % p
    return (e * f % p, g * h % p, f * g % p, e * h % p)


def _ed25519_scalarmult(point: tuple, scalar: int) -> tuple:
    if scalar == 0:
        return _ED25519_NEUTRAL
    half = _ed25519_scalarmult(point, scalar // 2)
    doubled = _ed25519_add(half, half)
    return _ed25519_add(doubled, point) if scalar & 1 else doubled


def _ed25519_to_affine(point: tuple) -> tuple[int, int]:
    x, y, z, _t = point
    zinv = _ed25519_inv(z)
    return (x * zinv % _ED25519_P, y * zinv % _ED25519_P)


def _ed25519_encode_point(point: tuple) -> bytes:
    """RFC 8032 section 5.1.2: y little-endian over 32 octets, x's parity
    bit copied into the encoding's top bit."""
    x, y = _ed25519_to_affine(point)
    out = bytearray(y.to_bytes(32, "little"))
    if x & 1:
        out[31] |= 0x80
    return bytes(out)


class Ed25519Error(Exception):
    """A key or signature was malformed or did not decode to a valid curve
    point (RFC 8032 section 5.1.3)."""


def _ed25519_decode_point(encoded: bytes) -> tuple:
    if len(encoded) != 32:
        raise Ed25519Error("an encoded point must be exactly 32 bytes")
    p = _ED25519_P
    y = int.from_bytes(encoded, "little")
    sign = (y >> 255) & 1
    y &= (1 << 255) - 1
    if y >= p:
        raise Ed25519Error("y coordinate is not less than p")
    xx = (y * y - 1) * _ed25519_inv(_ED25519_D * y * y + 1) % p
    x = pow(xx, (p + 3) // 8, p)
    if (x * x - xx) % p != 0:
        x = (x * pow(2, (p - 1) // 4, p)) % p
        if (x * x - xx) % p != 0:
            raise Ed25519Error("encoded value is not a valid curve point")
    if x == 0 and sign == 1:
        raise Ed25519Error("invalid point encoding (x=0 with the sign bit set)")
    if (x & 1) != sign:
        x = p - x
    return (x % p, y % p, 1, (x * y) % p)


def _ed25519_clamp(low_32_bytes: bytes) -> int:
    """RFC 8032 section 5.1.5 step 2 ('pruning')."""
    a = bytearray(low_32_bytes)
    a[0] &= 0xF8
    a[31] &= 0x7F
    a[31] |= 0x40
    return int.from_bytes(bytes(a), "little")


def ed25519_public_key_from_seed(seed: bytes) -> bytes:
    """RFC 8032 section 5.1.5: the 32-byte public key for a 32-byte seed."""
    if len(seed) != 32:
        raise Ed25519Error("a seed must be exactly 32 bytes")
    digest = hashlib.sha512(seed).digest()
    scalar = _ed25519_clamp(digest[:32])
    return _ed25519_encode_point(_ed25519_scalarmult(_ED25519_BASE, scalar))


def ed25519_sign(seed: bytes, message: bytes) -> bytes:
    """RFC 8032 section 5.1.6. Used only by this file's own tests, to
    reproduce or mutate a signature for a scratch fixture; the verifier
    itself never signs anything for real."""
    if len(seed) != 32:
        raise Ed25519Error("a seed must be exactly 32 bytes")
    digest = hashlib.sha512(seed).digest()
    scalar = _ed25519_clamp(digest[:32])
    prefix = digest[32:64]
    public_key = _ed25519_encode_point(_ed25519_scalarmult(_ED25519_BASE, scalar))
    r = int.from_bytes(hashlib.sha512(prefix + message).digest(), "little") % _ED25519_L
    r_point_bytes = _ed25519_encode_point(_ed25519_scalarmult(_ED25519_BASE, r))
    k = int.from_bytes(
        hashlib.sha512(r_point_bytes + public_key + message).digest(), "little"
    ) % _ED25519_L
    s = (r + k * scalar) % _ED25519_L
    return r_point_bytes + s.to_bytes(32, "little")


def ed25519_verify(public_key: bytes, message: bytes, signature: bytes) -> bool:
    """RFC 8032 section 5.1.7, unbatched (checks [S]B = R + [k]A', which the
    RFC states is sufficient though not the cofactored form). Returns False
    for any malformed or non-verifying input; raises nothing, so a caller
    never needs a second code path for "well-formed but wrong" versus
    "malformed"."""
    if len(public_key) != 32 or len(signature) != 64:
        return False
    r_bytes = signature[:32]
    s = int.from_bytes(signature[32:64], "little")
    if s >= _ED25519_L:
        return False
    try:
        a_point = _ed25519_decode_point(public_key)
        r_point = _ed25519_decode_point(r_bytes)
    except Ed25519Error:
        return False
    k = int.from_bytes(hashlib.sha512(r_bytes + public_key + message).digest(), "little") % _ED25519_L
    lhs = _ed25519_scalarmult(_ED25519_BASE, s)
    rhs = _ed25519_add(r_point, _ed25519_scalarmult(a_point, k))
    return _ed25519_to_affine(lhs) == _ed25519_to_affine(rhs)


# ---------------------------------------------------------------------------
# Section 9: ADR-0015 signature verification
#
# Rule source: docs/adr/0015-signed-manifests-and-receipts.md clauses 1-6 and
# its "Implementation notes" (the manifest-signing digest rule, the four
# report states, the receipt/log-line signing targets, the donor-receipt
# case_id gap — not re-implemented here since it is a reuse-time SC-12 rule
# with no signature involved), and
# crates/avila-core-evidence/src/signature.rs /
# crates/avila-core-runner/src/case_run/signing.rs, read to learn exactly
# which bytes are signed and which four states are reported (never to copy
# their control flow) — then independently re-derived and proved to agree
# on the real committed artifacts:
#   - the exact manifest-signing digest, cross-checked against every
#     committed signatures/manifest.sig.json's own signed_document.sha256
#     (TestManifestSigningDigest);
#   - every one of the nine committed ADR-0015 signature documents under
#     examples/cases/{case-001,002,003}-*/signatures/, verified against
#     examples/keys/trust-root.json (TestSignaturesAgainstCommittedDocuments).
#
# Without a trust root, a signature is reported "not_checked" (internal
# consistency only) or "invalid" (a tampered or malformed document is
# visibly wrong even without a key); it is never "verified" — ADR-0015
# clause 3, "Implementation notes" report-states paragraph.
# ---------------------------------------------------------------------------

SIGNATURE_SCHEMA_VERSION = "avila.core/signature/v0.1-draft"
TRUST_ROOT_SCHEMA_VERSION = "avila.core/trust-root/v0.1-draft"
ALGORITHM_ED25519 = "ed25519"

_HEX64_RE = re.compile(r"^[a-f0-9]{64}$")
_HEX128_RE = re.compile(r"^[a-f0-9]{128}$")


class TrustRootError(CanonError):
    def __init__(self, message: str):
        super().__init__("CORE-SIG-TRUST", message)


@dataclass
class TrustRoot:
    """The requester/runner public keys one verification run accepts,
    keyed by (key_id, role) exactly as
    avila-core-evidence::signature::TrustRoot::find does — a key listed
    only under one role never satisfies a check for the other."""

    keys: dict[tuple[str, str], str]

    def find(self, key_id: str, role: str) -> Optional[str]:
        return self.keys.get((key_id, role))


def load_trust_root(path: Path) -> TrustRoot:
    doc = load_json(path)
    if doc.get("schema_version") != TRUST_ROOT_SCHEMA_VERSION:
        raise TrustRootError(
            f"trust root has unsupported schema_version {doc.get('schema_version')!r}; expected {TRUST_ROOT_SCHEMA_VERSION!r}"
        )
    keys: dict[tuple[str, str], str] = {}
    for entry in doc.get("keys", []):
        key_id = entry["key_id"]
        role = entry["role"]
        public_key_hex = entry["public_key_hex"]
        if not _HEX64_RE.match(key_id) or not _HEX64_RE.match(public_key_hex):
            raise TrustRootError(f"trust root entry {entry!r} has a malformed key_id or public_key_hex")
        if (key_id, role) in keys:
            raise TrustRootError(f"duplicate key_id {key_id!r} under role {role!r} in trust root")
        keys[(key_id, role)] = public_key_hex
    return TrustRoot(keys=keys)


def digest_from_prefixed(value: str) -> bytes:
    """Decodes a `sha256:`-prefixed hex digest into raw bytes, exactly the
    identity shape every document and artifact in this repository binds."""
    if not isinstance(value, str) or not value.startswith("sha256:"):
        raise CanonError("CORE-SIG-DIGEST", f"digest {value!r} must use the `sha256:` prefix")
    hex_part = value[len("sha256:") :]
    if not _HEX64_RE.match(hex_part):
        raise CanonError("CORE-SIG-DIGEST", f"digest {value!r} is not 64 lowercase hex characters")
    return bytes.fromhex(hex_part)


def manifest_signing_digest(manifest_bytes: bytes, signature_document_id: str) -> bytes:
    """ADR-0015 "Implementation notes": the manifest with the signature
    document's own `documents[]` entry removed, canonicalised as the typed
    struct serialises it. Removing an absent id is a no-op, so this is
    symmetric before and after the entry is bound. Proved against every
    committed signatures/manifest.sig.json (see
    TestManifestSigningDigest) — including that ``json.loads`` +
    ``json.dumps`` round-tripping the manifest first (rather than hashing
    package.json's raw bytes) reproduces the identity the Rust struct
    round-trip normalises to, which is the exact edge this ADR clause
    calls out."""
    manifest = json.loads(manifest_bytes)
    documents = manifest.get("documents")
    if isinstance(documents, list):
        manifest["documents"] = [
            document for document in documents if document.get("document_id") != signature_document_id
        ]
    normalized_bytes = json.dumps(manifest).encode("utf-8")
    canonical = canonicalize_json(normalized_bytes)
    return hashlib.sha256(canonical).digest()


def find_signature_for(
    case_dir: Path, package: dict, target_role: str, target_document_id: str
) -> Optional[tuple[str, dict]]:
    """The bound `signature` document, if any, whose content names exactly
    this (role, document_id) as its signed target — mirrors
    case_run/signing.rs's find_signature_for. Returns the signature
    package document's own `document_id` alongside its parsed content: the
    former is what the manifest signature's own digest rule excludes from
    the manifest it covers."""
    for document in package.get("documents", []):
        if document.get("role") != "signature":
            continue
        path = case_dir / document["path"]
        if not path.is_file():
            continue
        try:
            parsed = load_json(path)
        except (OSError, json.JSONDecodeError):
            continue
        signed = parsed.get("signed_document", {})
        if signed.get("role") == target_role and signed.get("document_id") == target_document_id:
            return document["document_id"], parsed
    return None


def check_signature_internal_consistency(document: dict, expected_digest: bytes) -> Optional[str]:
    """None if internally consistent; otherwise the reason it is not.
    Checkable without any trust root — a tampered signature document is
    visibly wrong even without a key (ADR-0015 "Implementation notes",
    the `invalid` state paragraph)."""
    if document.get("schema_version") != SIGNATURE_SCHEMA_VERSION:
        return f"signature document has unsupported schema_version {document.get('schema_version')!r}; expected {SIGNATURE_SCHEMA_VERSION!r}"
    if document.get("algorithm") != ALGORITHM_ED25519:
        return f"unsupported signature algorithm {document.get('algorithm')!r}; only {ALGORITHM_ED25519!r} is implemented"
    signature_hex = document.get("signature_hex", "")
    if not isinstance(signature_hex, str) or not _HEX128_RE.match(signature_hex):
        return "signature_hex must be exactly 64 bytes, hex encoded"
    try:
        actual_digest = digest_from_prefixed(document["signed_document"]["sha256"])
    except (CanonError, KeyError, TypeError) as error:
        return f"signed_document.sha256 is malformed: {error}"
    if actual_digest != expected_digest:
        return "signed_document.sha256 does not match the digest this target actually re-hashes to"
    return None


def signature_status(
    document: Optional[dict],
    expected_digest: bytes,
    trust_root: Optional[TrustRoot],
    expected_role: str,
) -> dict:
    """The signature state of one target, mirroring
    avila-core-runner::case_run::signing::SignatureStatus: ``unsigned`` (no
    document at all), ``invalid`` (present but inconsistent, wrong key, or
    a signature that plain does not verify — a strict superset of what
    needs a trust root, per ADR-0015's own note that this is deliberate),
    ``not_checked`` (present and consistent, no trust root supplied), or
    ``verified`` (cryptographically checked against a listed key of the
    expected role). ``verified`` never appears without a trust root."""
    if document is None:
        return {"state": "unsigned"}
    reason = check_signature_internal_consistency(document, expected_digest)
    if reason is not None:
        return {"state": "invalid", "reason": reason}
    if trust_root is None:
        return {"state": "not_checked"}
    key_id = document.get("key_id", "")
    public_key_hex = trust_root.find(key_id, expected_role)
    if public_key_hex is None:
        return {
            "state": "invalid",
            "reason": f"key `{key_id}` is not listed under role `{expected_role}` in the supplied trust root",
        }
    verified = ed25519_verify(
        bytes.fromhex(public_key_hex), expected_digest, bytes.fromhex(document["signature_hex"])
    )
    if not verified:
        return {"state": "invalid", "reason": "signature does not verify against the listed key"}
    return {"state": "verified", "signed_by": key_id}


def describe_signature_status(status: dict) -> str:
    state = status["state"]
    if state == "unsigned":
        return "unsigned"
    if state == "not_checked":
        return "signature not checked (no --trust-root supplied)"
    if state == "verified":
        return f"verified, signed by {status['signed_by']}"
    return f"invalid: {status['reason']}"


def manifest_signature_status(
    case_dir: Path, package: dict, manifest_bytes: bytes, trust_root: Optional[TrustRoot]
) -> dict:
    found = find_signature_for(case_dir, package, "manifest", package["case_id"])
    if found is None:
        return {"state": "unsigned"}
    signature_document_id, document = found
    expected_digest = manifest_signing_digest(manifest_bytes, signature_document_id)
    return signature_status(document, expected_digest, trust_root, "requester")


def receipt_signature_status(
    case_dir: Path,
    package: dict,
    step_id: str,
    receipt_document_sha256: str,
    trust_root: Optional[TrustRoot],
) -> dict:
    found = find_signature_for(case_dir, package, "execution_receipt", step_id)
    if found is None:
        return {"state": "unsigned"}
    _, document = found
    try:
        expected_digest = digest_from_prefixed(receipt_document_sha256)
    except CanonError as error:
        return {"state": "invalid", "reason": str(error)}
    return signature_status(document, expected_digest, trust_root, "runner")


def verify_log_line_signature(line: dict, trust_root: Optional[TrustRoot]) -> dict:
    """One campaign log line's own runner signature (ADR-0015 clause 6):
    the canonical form of the line with its `signature` member removed
    must reproduce the digest that member names. Mirrors
    attempt.rs::verify_log_line_signature, minus the lineage check, which
    Section 6 above already re-derives separately."""
    if "signature" not in line:
        return {"state": "unsigned"}
    without_signature = {key: value for key, value in line.items() if key != "signature"}
    canonical = canonicalize_json(json.dumps(without_signature).encode("utf-8"))
    expected_digest = hashlib.sha256(canonical).digest()
    return signature_status(line["signature"], expected_digest, trust_root, "runner")


def verify_log_signatures(log_path: Path, trust_root: Optional[TrustRoot], report: Report) -> None:
    lines = [line for line in log_path.read_bytes().splitlines() if line.strip()]
    check = f"signatures.log.{log_path.parent.name}/{log_path.name}"
    signed_indices: list[int] = []
    verified_count = 0
    invalid: list[str] = []
    for index, raw in enumerate(lines):
        try:
            parsed = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if not isinstance(parsed, dict) or "signature" not in parsed:
            continue
        signed_indices.append(index)
        status = verify_log_line_signature(parsed, trust_root)
        if status["state"] == "verified":
            verified_count += 1
        elif status["state"] == "invalid":
            invalid.append(f"line {index}: {describe_signature_status(status)}")
    if invalid:
        report.mismatch(check, "; ".join(invalid))
    elif not signed_indices:
        report.not_checked(
            check,
            f"none of the {len(lines)} committed lines in this log carry a signature "
            "(ADR-0015 log-line signing needs a live `run --runner-key`; this "
            "repository's committed logs were produced without one)",
        )
    elif trust_root is None:
        report.not_checked(check, f"{len(signed_indices)} of {len(lines)} lines carry a signature, not checked (no --trust-root supplied)")
    else:
        report.verified(check, f"{verified_count} of {len(signed_indices)} signed lines ({len(lines)} total) verified")


def _report_signature_status(report: Report, check: str, status: dict) -> None:
    detail = describe_signature_status(status)
    if status["state"] == "verified":
        report.verified(check, detail)
    elif status["state"] == "invalid":
        report.mismatch(check, detail)
    else:
        report.not_checked(check, detail)


def verify_case_signatures(
    case_dir: Path,
    package: dict,
    manifest_bytes: bytes,
    docs_by_role: dict,
    trust_root: Optional[TrustRoot],
    report: Report,
) -> None:
    status = manifest_signature_status(case_dir, package, manifest_bytes, trust_root)
    _report_signature_status(report, "signatures.manifest", status)

    for receipt_doc in docs_by_role.get("execution_receipt", []):
        step_id = receipt_doc.get("step_id", receipt_doc["document_id"])
        check = f"signatures.receipt.{step_id}"
        path = case_dir / receipt_doc["path"]
        if not path.is_file():
            report.not_checked(check, f"receipt file missing: {path}")
            continue
        status = receipt_signature_status(case_dir, package, step_id, receipt_doc["sha256"], trust_root)
        _report_signature_status(report, check, status)

    for log_name in ("attempts.jsonl", "campaign-log.jsonl"):
        log_path = case_dir / "search" / log_name
        if log_path.is_file():
            verify_log_signatures(log_path, trust_root, report)
    if not any((case_dir / "search" / name).is_file() for name in ("attempts.jsonl", "campaign-log.jsonl")):
        report.not_checked("signatures.log", f"no search/attempts.jsonl or search/campaign-log.jsonl found under {case_dir}")


# ---------------------------------------------------------------------------
# Section 10: qualification envelopes (ADR-0008 as refined by ADR-0018,
# S-039, S-046)
#
# Rule source: crates/avila-core-kernel/src/predicate.rs's documented
# Strong-Kleene semantics (ADR-0006 SC-7) and
# crates/avila-core-compiler/src/qualification.rs's evaluate_envelope (ADR-
# 0008 clauses 2-3, S-039's covered_output_slots refinement), read to learn
# which fields each predicate variant carries and how a scope's top-level
# `all` is split into independently reported terms — never to copy control
# flow — then re-derived and proved to agree on
# fixtures/semantic-core/vectors/scope-predicates.v1.json (every vector;
# see TestScopePredicateVectors) for the generic evaluator itself.
#
# ADR-0018 binds the applicability context itself into the evidence-claims
# schema: every claim's `qualification.context` carries the facts the
# adapter reported (each with its declared source class, source identity,
# validator, and `plan:<invocation>` receipt reference) plus each staged
# input's media type and SHA-256 as slot attributes. So this section DOES
# feed the evaluator a run's real extracted facts: for every
# qualification-carrying claim it re-evaluates each scope term over the
# persisted context and compares per-term results and the aggregate
# Inside/Outside/Unknown state against what the claim records — a recorded
# `inside` whose own facts re-derive `outside` or `unknown` is a mismatch,
# as is a claim whose qualification carries no context at all. It also
# binds the context to the step's committed execution receipt: every
# context input's sha256/media_type must equal the receipt's bound input
# for that slot, and every fact's source.receipt must equal
# `plan:` + the receipt's invocation identity (so a context copied from
# another step or run is detected).
#
# Still structural, checked for every qualification-carrying claim
# (TestQualificationEnvelopeConsistency): that its recorded qualification
# identity (id/revision/sha256) names a qualification document this
# package actually binds; that its recorded per-term predicate text is,
# in order, exactly the bound record's own scope terms (an edited,
# reordered, or substituted term is a mismatch); and that it never carries
# an assessment on an output slot the record's `covered_output_slots`
# excludes (S-039).
#
# Boundary kept: the persisted facts are the adapter's assertions about
# the staged bytes with their declared provenance. Re-deriving the
# envelope from them proves the recorded assessment is what those facts
# imply; it does not prove the adapter read the bytes correctly — that is
# what the receipt's byte-identity binding and the qualification record's
# named validation evidence are for.
#
# --as-of (ADR-0006's supplied-snapshot historical verification) adds one
# informational `as_of` line per qualified claim after the recorded
# checks: the labeled state at the supplied instant under the supplied
# material — the case's own bound set, or `--as-of-material`'s package —
# in campaign-refusal precedence (absent > revoked > superseded > expired
# > the envelope its recorded facts imply). A divergence between recorded
# and labeled states is a datum, never a failure.
# ---------------------------------------------------------------------------


class PredicateError(Exception):
    """A predicate could not be evaluated: a malformed shape, or a fact or
    parameter compared as a quantity with no registered kind. Every such
    error is treated as one term becoming ``unknown`` (matching
    predicate.rs's per-term continue-on-Err path); unlike the Rust
    evaluator, a malformed predicate *shape* is not additionally treated as
    aborting the whole assessment early — a deliberate simplification, since
    every committed qualification record's scope is schema-valid, and this
    divergence can only matter for a deliberately malformed synthetic
    input, never for real data."""


def scope_terms(scope) -> list:
    """Splits a qualification record's `scope` into the terms
    evaluate_envelope reports separately: the items of a top-level non-
    empty `all`, or the whole scope as a single term otherwise."""
    all_items = scope.get("all") if isinstance(scope, dict) else None
    return all_items if isinstance(all_items, list) and all_items else [scope]


def compact_json(value) -> str:
    """Rust's `serde_json::to_string(&Value)` compact form. This
    workspace's serde_json has no `preserve_order` feature, so
    `Value::Object` is BTreeMap-backed and keys serialise in sorted order
    regardless of source order — confirmed byte-for-byte against every
    `qualification.terms[].predicate` string committed in
    examples/cases/{case-001,002,003}-*/claims.json."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _truth_not(value: str) -> str:
    return {"true": "false", "false": "true", "unknown": "unknown"}[value]


def _quantity_to_canonical(value_obj: dict, kind_id: str, kinds: dict[str, Kind]) -> Fraction:
    kind = kinds.get(kind_id)
    if kind is None:
        raise PredicateError(f"kind `{kind_id}` is not in the supplied kind registry")
    magnitude = read_authoritative_exact(value_obj["value"])
    return kind.scale(magnitude, value_obj["unit"])


def _compare_ordering(op: str, a, b) -> str:
    table = {"eq": a == b, "ne": a != b, "lt": a < b, "le": a <= b, "gt": a > b, "ge": a >= b}
    if op not in table:
        raise PredicateError(f"unknown fact operator `{op}`")
    return "true" if table[op] else "false"


def _compare_scalar_eq_ne(op: str, a, b) -> str:
    if op not in ("eq", "ne"):
        raise PredicateError("ordered fact comparison requires an ordered numeric value")
    return _compare_ordering(op, a, b)


def _evaluate_range(range_pred: dict, context: dict, kinds: dict, param_kinds: dict) -> str:
    if range_pred.get("min") is None and range_pred.get("max") is None:
        raise PredicateError("a parameter range requires a minimum or maximum")
    actual = context.get("params", {}).get(range_pred["param"])
    if actual is None:
        return "unknown"
    kind_id = param_kinds.get(range_pred["param"])
    if kind_id is None:
        raise PredicateError(f"parameter `{range_pred['param']}` has no quantity kind")
    actual_scaled = _quantity_to_canonical(actual, kind_id, kinds)
    minimum_spec = range_pred.get("min")
    if minimum_spec is not None:
        minimum = _quantity_to_canonical(minimum_spec, kind_id, kinds)
        if actual_scaled < minimum or (not range_pred.get("min_inclusive", False) and actual_scaled == minimum):
            return "false"
    maximum_spec = range_pred.get("max")
    if maximum_spec is not None:
        maximum = _quantity_to_canonical(maximum_spec, kind_id, kinds)
        if actual_scaled > maximum or (not range_pred.get("max_inclusive", False) and actual_scaled == maximum):
            return "false"
    return "true"


def _evaluate_attribute_set(pred: dict, context: dict) -> str:
    value = context.get("inputs", {}).get(pred["slot"], {}).get("attributes", {}).get(pred["attribute"])
    if not isinstance(value, str):
        return "unknown"
    return "true" if value in pred["values"] else "false"


def _evaluate_fact(pred: dict, context: dict, kinds: dict, fact_kinds: dict) -> str:
    actual = context.get("facts", {}).get(pred["name"])
    if actual is None:
        return "unknown"
    source = actual.get("source", {})
    requirement = pred["source_requirement"]
    if source.get("class") != requirement["class"] or source.get("validator") != requirement["validator"]:
        return "unknown"
    op = pred["op"]
    actual_value = actual["value"]
    expected_value = pred["value"]
    if isinstance(actual_value, dict) and isinstance(expected_value, dict):
        kind_id = fact_kinds.get(pred["name"])
        if kind_id is None:
            raise PredicateError(f"fact `{pred['name']}` has no quantity kind")
        return _compare_ordering(
            op,
            _quantity_to_canonical(actual_value, kind_id, kinds),
            _quantity_to_canonical(expected_value, kind_id, kinds),
        )
    if isinstance(actual_value, bool) and isinstance(expected_value, bool):
        return _compare_scalar_eq_ne(op, actual_value, expected_value)
    if isinstance(actual_value, str) and isinstance(expected_value, str):
        return _compare_scalar_eq_ne(op, actual_value, expected_value)
    if (
        isinstance(actual_value, int)
        and not isinstance(actual_value, bool)
        and isinstance(expected_value, int)
        and not isinstance(expected_value, bool)
    ):
        return _compare_ordering(op, actual_value, expected_value)
    return "unknown"


def evaluate_predicate(predicate: dict, context: dict, kinds: dict, fact_kinds: dict, param_kinds: dict) -> str:
    """Strong-Kleene evaluation of one kernel applicability predicate
    (predicate.rs) over an explicit context, returning "true" / "false" /
    "unknown". Proved against every vector in
    fixtures/semantic-core/vectors/scope-predicates.v1.json — see
    TestScopePredicateVectors."""
    if not isinstance(predicate, dict) or len(predicate) != 1:
        raise PredicateError(f"not a valid predicate: {predicate!r}")
    ((variant, body),) = predicate.items()
    if variant == "always":
        return "true" if body else "false"
    if variant == "not":
        return _truth_not(evaluate_predicate(body, context, kinds, fact_kinds, param_kinds))
    if variant == "all":
        if not body:
            raise PredicateError("`all` requires at least one predicate")
        result = "true"
        for item in body:
            outcome = evaluate_predicate(item, context, kinds, fact_kinds, param_kinds)
            if outcome == "false":
                return "false"
            if outcome == "unknown":
                result = "unknown"
        return result
    if variant == "any":
        if not body:
            raise PredicateError("`any` requires at least one predicate")
        result = "false"
        for item in body:
            outcome = evaluate_predicate(item, context, kinds, fact_kinds, param_kinds)
            if outcome == "true":
                return "true"
            if outcome == "unknown":
                result = "unknown"
        return result
    if variant == "param_in_range":
        return _evaluate_range(body, context, kinds, param_kinds)
    if variant == "input_attribute_in":
        return _evaluate_attribute_set(body, context)
    if variant == "environment_image_in":
        environment = context.get("environment")
        if environment is None:
            return "unknown"
        return "true" if environment.get("image_digest") in body else "false"
    if variant == "fact":
        return _evaluate_fact(body, context, kinds, fact_kinds)
    raise PredicateError(f"unknown predicate variant `{variant}`")


def _aggregate_envelope_state(results: list) -> str:
    if any(result == "false" for result in results):
        return "outside"
    if results and all(result == "true" for result in results):
        return "inside"
    return "unknown"


def _expected_envelope_state(record: dict, term_state: str, evaluated_at: str | None) -> str:
    """Rust's evaluate_envelope applies expiry after term aggregation: a
    record whose ``not_after`` lapsed at the evaluation instant yields
    ``expired`` whatever its terms said. The claim stamps that instant with
    the producing receipt's ``started_at``; both sides are normalized
    ``YYYY-MM-DDTHH:MM:SSZ``, so lexical order is chronological."""
    not_after = record.get("not_after")
    if not_after and evaluated_at is not None and evaluated_at >= not_after:
        return "expired"
    return term_state


def evaluate_envelope_terms(record: dict, context: dict, kinds: dict) -> tuple[str, list, list]:
    """Re-derives qualification.rs's evaluate_envelope: splits the record's
    scope into its top-level terms, evaluates each independently over
    ``context``, and aggregates Inside/Outside/Unknown. Returns
    (state, terms, issues); each term is {"predicate": <compact json>,
    "result": "true"|"false"|"unknown"}."""
    fact_kinds = record.get("fact_kinds", {})
    terms: list[dict] = []
    issues: list[str] = []
    for term in scope_terms(record["scope"]):
        try:
            result = evaluate_predicate(term, context, kinds, fact_kinds, {})
        except PredicateError as error:
            issues.append(str(error))
            result = "unknown"
        terms.append({"predicate": compact_json(term), "result": result})
    return _aggregate_envelope_state([term["result"] for term in terms]), terms, issues


def _check_context_receipt_binding(context: dict, receipt: dict, problems: list) -> None:
    """The persisted applicability context must name the step's own staged
    inputs and its own planned invocation: each context input's recorded
    sha256/media_type equals the receipt's bound input for that slot, and
    every fact's source.receipt is `plan:` + the receipt's invocation
    identity (so a context lifted from another step or run is detected)."""
    staged = {
        entry.get("input_slot"): entry
        for entry in receipt.get("inputs", [])
        if isinstance(entry, dict)
    }
    staged_digests = {entry.get("sha256") for entry in staged.values()}
    for slot, input_ctx in context.get("inputs", {}).items():
        attributes = input_ctx.get("attributes", {}) if isinstance(input_ctx, dict) else {}
        bound = staged.get(slot)
        if bound is None:
            problems.append(
                f"context names input slot {slot!r} that the step's receipt does not stage"
            )
            continue
        for attribute, key in (("sha256", "sha256"), ("media_type", "media_type")):
            if attributes.get(attribute) != bound.get(key):
                problems.append(
                    f"context input {slot!r} records {attribute} {attributes.get(attribute)!r}, "
                    f"the receipt binds {bound.get(key)!r}"
                )
    expected_receipt = f"plan:{receipt.get('invocation_sha256')}"
    for name, fact in context.get("facts", {}).items():
        source = fact.get("source", {}) if isinstance(fact, dict) else {}
        if source.get("receipt") != expected_receipt:
            problems.append(
                f"fact {name!r} cites receipt {source.get('receipt')!r}, this step's "
                f"planned invocation is {expected_receipt!r}"
            )
        identity = source.get("identity")
        if identity != "runner:local" and identity not in staged_digests:
            problems.append(
                f"fact {name!r} cites source identity {identity!r}, which is neither "
                "'runner:local' nor a staged input identity in the step's receipt"
            )


def _load_qualification_material(
    case_dir: Path,
    package: dict,
    docs_by_role: dict,
    recognized_owners: Optional[dict] = None,
) -> tuple[list[tuple[dict, str]], dict[str, str]]:
    """The qualification material one package binds: (record, file sha256)
    pairs plus the target-record-digest → revocation-doc-digest map.

    Under issuer recognition a revocation counts only when it verifies
    under the declared issuer key — the runner ignores it otherwise
    (CORE-X3405); an unrecognized owner's withdrawal is package-asserted.
    The same function serves both the case's own bound set and a supplied
    ``--as-of-material`` package, so historical verification derives
    lifecycle flags from whichever material was supplied."""
    qualification_docs: list[tuple[dict, str]] = []
    for document in docs_by_role.get("qualification", []):
        path = case_dir / document["path"]
        if not path.is_file():
            continue
        raw = path.read_bytes()
        try:
            record = json.loads(raw)
        except json.JSONDecodeError:
            continue
        qualification_docs.append((record, sha256_bytes(raw)))

    owners = recognized_owners or {}
    records_by_sha = {sha: record for record, sha in qualification_docs}
    revoked_by: dict[str, str] = {}
    for document in docs_by_role.get("qualification_revocation", []):
        path = case_dir / document["path"]
        if not path.is_file():
            continue
        raw = path.read_bytes()
        try:
            revocation = json.loads(raw)
        except json.JSONDecodeError:
            continue
        target = revocation.get("record_sha256")
        if not isinstance(target, str):
            continue
        issuer_key = owners.get((records_by_sha.get(target) or {}).get("owner") or "")
        if issuer_key is not None:
            match = find_signature_for(
                case_dir, package, "qualification_revocation", document["document_id"]
            )
            if match is None:
                continue
            _, signature_document = match
            expected_digest = hashlib.sha256(raw).digest()
            if check_signature_internal_consistency(signature_document, expected_digest) is not None:
                continue
            if not ed25519_verify(
                bytes.fromhex(issuer_key),
                expected_digest,
                bytes.fromhex(signature_document["signature_hex"]),
            ):
                continue
        revoked_by.setdefault(target, sha256_bytes(raw))
    return qualification_docs, revoked_by


def verify_case_qualification_envelopes(
    case_dir: Path,
    package: dict,
    docs_by_role: dict,
    claims: Optional[dict],
    kinds: dict,
    receipts_by_step: dict,
    report: Report,
    recognized_owners: Optional[dict] = None,
    as_of: Optional[str] = None,
    as_of_material_dir: Optional[Path] = None,
) -> None:
    qualification_docs, revoked_by = _load_qualification_material(
        case_dir, package, docs_by_role, recognized_owners
    )

    if claims is None:
        return
    qualifying_claims = [claim for claim in claims.get("claims", []) if claim.get("qualification")]

    for claim in qualifying_claims:
        check = f"qualification.{claim['claim_id']}"
        qualification = claim["qualification"]
        match = next(
            (
                (record, actual_sha256)
                for record, actual_sha256 in qualification_docs
                if record.get("qualification_id") == qualification.get("qualification_id")
                and record.get("revision") == qualification.get("revision")
            ),
            None,
        )
        if match is None:
            report.mismatch(
                check,
                f"claim references qualification_id={qualification.get('qualification_id')!r} "
                f"revision={qualification.get('revision')!r}, which is not a qualification document this package binds",
            )
            continue
        record, actual_sha256 = match

        problems = []
        if qualification.get("sha256") != actual_sha256:
            problems.append(
                f"claim binds qualification sha256 {qualification.get('sha256')!r}, "
                f"the bound document is actually {actual_sha256!r}"
            )

        covered_slots = record.get("covered_output_slots")
        if covered_slots is not None and claim.get("output_slot") not in covered_slots:
            problems.append(
                f"claim carries a qualification assessment on output slot {claim.get('output_slot')!r}, "
                f"which record {record.get('qualification_id')!r} does not list in "
                f"covered_output_slots {covered_slots!r} (S-039)"
            )

        if qualification.get("not_after") != record.get("not_after"):
            problems.append(
                f"claim records not_after {qualification.get('not_after')!r}, "
                f"the bound record carries {record.get('not_after')!r}"
            )

        # Claims predate `owner`: an absent field is an older claim, not a
        # mismatch — under a declared recognition policy it simply cannot be
        # recognized (the gate treats it as an unlisted owner).
        if "owner" in qualification and qualification["owner"] != record.get("owner"):
            problems.append(
                f"claim records owner {qualification.get('owner')!r}, "
                f"the bound record declares owner {record.get('owner')!r}"
            )

        # Lifecycle facts are the bound set's assertion, re-derived from the
        # bound documents: a bound record superseding this one, or a bound
        # revocation naming its digest.
        expected_superseder = next(
            (
                candidate_sha256
                for candidate, candidate_sha256 in qualification_docs
                if actual_sha256 in candidate.get("supersedes", [])
            ),
            None,
        )
        if qualification.get("superseded_by") != expected_superseder:
            problems.append(
                f"claim records superseded_by {qualification.get('superseded_by')!r}, "
                f"the bound set derives {expected_superseder!r}"
            )

        expected_revoked_by = revoked_by.get(actual_sha256)
        if qualification.get("revoked_by") != expected_revoked_by:
            problems.append(
                f"claim records revoked_by {qualification.get('revoked_by')!r}, "
                f"the bound set derives {expected_revoked_by!r}"
            )

        # The claim's evaluation instant is its producing receipt's
        # `started_at` — the signed time record expiry is stamped against.
        receipt = receipts_by_step.get(claim.get("step_id"))
        evaluated_at = (receipt or {}).get("process", {}).get("started_at")
        if qualification.get("state") == "expired" and evaluated_at is None:
            problems.append(
                "claim records envelope state 'expired' but no receipt for the "
                "step is bound, so the evaluation instant cannot be re-derived"
            )

        expected_predicates = [compact_json(term) for term in scope_terms(record["scope"])]
        actual_terms = qualification.get("terms", [])
        actual_predicates = [term.get("predicate") for term in actual_terms]
        if actual_predicates != expected_predicates:
            problems.append(
                "claim's recorded term predicates do not match the bound record's own scope "
                f"terms in order: recorded {actual_predicates!r}, expected {expected_predicates!r}"
            )

        recomputed_state = _expected_envelope_state(
            record, _aggregate_envelope_state([term.get("result") for term in actual_terms]), evaluated_at
        )
        if recomputed_state != qualification.get("state"):
            problems.append(
                f"claim's recorded state {qualification.get('state')!r} is not what its own "
                f"recorded per-term results imply ({recomputed_state!r})"
            )

        context = qualification.get("context")
        if not isinstance(context, dict):
            problems.append(
                "claim's qualification carries no persisted applicability context "
                "(ADR-0018): the recorded assessment cannot be re-derived"
            )
        else:
            derived_state, derived_terms, derived_issues = evaluate_envelope_terms(
                record, context, kinds
            )
            derived_state = _expected_envelope_state(record, derived_state, evaluated_at)
            if len(derived_terms) == len(actual_terms):
                for actual_term, derived_term in zip(actual_terms, derived_terms):
                    if actual_term.get("result") != derived_term.get("result"):
                        problems.append(
                            f"term {actual_term.get('predicate')!r} records "
                            f"{actual_term.get('result')!r} but the persisted facts re-derive "
                            f"{derived_term.get('result')!r}"
                        )
            elif not problems:
                problems.append(
                    f"persisted context re-derives {len(derived_terms)} terms but the claim "
                    f"records {len(actual_terms)}"
                )
            if derived_state != qualification.get("state"):
                problems.append(
                    f"persisted facts re-derive envelope state {derived_state!r}, "
                    f"the claim records {qualification.get('state')!r}"
                )
            if derived_issues:
                problems.append(
                    f"persisted context does not re-evaluate cleanly: {'; '.join(derived_issues)}"
                )
            if receipt is not None:
                _check_context_receipt_binding(context, receipt, problems)

        if problems:
            report.mismatch(check, "; ".join(problems))
        else:
            report.verified(
                check,
                f"{len(actual_predicates)} recorded terms match the bound qualification record's "
                f"scope and re-derive {qualification['state']!r} from the persisted facts, "
                "whose input digests and plan identity bind the step's receipt",
            )

    if as_of is None:
        return
    _emit_as_of_envelopes(
        qualifying_claims,
        as_of,
        as_of_material_dir,
        case_dir,
        package,
        docs_by_role,
        recognized_owners or {},
        kinds,
        receipts_by_step,
        report,
    )


def _as_of_state(
    claim: dict,
    material_docs: list[tuple[dict, str]],
    material_revoked: dict[str, str],
    material_owners: dict,
    instant: str,
    kinds: dict,
) -> tuple[str, str]:
    """One qualified claim's labeled state at ``instant`` under the supplied
    material, following the campaign refusal precedence: a record absent
    from the material, then a bound revocation, then supersession, then
    expiry, then the envelope its own recorded facts imply. Owner
    recognition is reported as a suffix, not the label — recognition is a
    per-requirement gate (nominal basis is exempt), not an envelope state.
    Returns (label, suffix)."""
    qualification = claim["qualification"]
    bound_sha = qualification.get("sha256")
    match = next(((record, sha) for record, sha in material_docs if sha == bound_sha), None)
    if match is None:
        same_identity = next(
            (
                record
                for record, _ in material_docs
                if record.get("qualification_id") == qualification.get("qualification_id")
                and record.get("revision") == qualification.get("revision")
            ),
            None,
        )
        note = (
            "a record of the same id+revision but a different digest is bound"
            if same_identity is not None
            else "the claim's bound record digest is not bound"
        )
        return "absent", f" ({note} in the supplied material)"
    record, record_sha = match

    revoker = material_revoked.get(record_sha)
    if revoker is not None:
        return "revoked", f" (revocation {revoker})"
    superseder = next(
        (sha for candidate, sha in material_docs if record_sha in candidate.get("supersedes", [])),
        None,
    )
    if superseder is not None:
        return "superseded", f" (superseding record {superseder})"

    context = qualification.get("context")
    if isinstance(context, dict):
        term_state, _, _ = evaluate_envelope_terms(record, context, kinds)
        source = "the persisted facts"
    else:
        results = [term.get("result") for term in qualification.get("terms", [])]
        if not results:
            return "underivable", " (no persisted context and no recorded terms)"
        term_state = _aggregate_envelope_state(results)
        source = "the recorded terms"
    state = _expected_envelope_state(record, term_state, instant)

    suffix = f" (from {source})"
    if material_owners and qualification.get("owner") not in material_owners:
        suffix += f"; owner {qualification.get('owner')!r} is not recognized under the supplied policy"
    return state, suffix


def _emit_as_of_envelopes(
    qualifying_claims: list,
    as_of: str,
    as_of_material_dir: Optional[Path],
    case_dir: Path,
    package: dict,
    docs_by_role: dict,
    recognized_owners: dict,
    kinds: dict,
    receipts_by_step: dict,
    report: Report,
) -> None:
    """ADR-0006: historical verification is explicitly as_of a supplied
    policy, qualification, revocation, and time snapshot. Each qualified
    claim gets one informational ``as_of`` line naming its recorded state
    and the labeled state it would carry at the supplied instant under the
    supplied material — a divergence is a datum, never a failure."""
    material_dir = as_of_material_dir or case_dir
    if as_of_material_dir is None:
        material_docs, material_revoked = _load_qualification_material(
            case_dir, package, docs_by_role, recognized_owners
        )
        material_owners = recognized_owners
        material_desc = "bound material"
    else:
        material_package_path = material_dir / "package.json"
        if not material_package_path.is_file():
            report.not_checked(
                "qualification.as_of",
                f"--as-of-material {material_dir} names no package.json — the supplied snapshot cannot be read",
            )
            return
        material_package = json.loads(material_package_path.read_bytes())
        material_docs_by_role: dict[str, list[dict]] = {}
        for d in material_package.get("documents", []):
            material_docs_by_role.setdefault(d["role"], []).append(d)
        material_contract_entries = material_docs_by_role.get("contract", [])
        material_contract = None
        if material_contract_entries:
            contract_path = material_dir / material_contract_entries[0]["path"]
            if contract_path.is_file():
                material_contract = load_json(contract_path)
        material_owners = (
            (material_contract or {}).get("execution_policy", {}).get("recognized_qualification_owners") or {}
        )
        material_docs, material_revoked = _load_qualification_material(
            material_dir, material_package, material_docs_by_role, material_owners
        )
        material_desc = f"supplied material {material_dir}"

    if not qualifying_claims:
        report.not_checked(
            "qualification.as_of",
            "--as-of supplied but no claim carries a qualification assessment",
        )
        return

    for claim in qualifying_claims:
        qualification = claim["qualification"]
        check = f"qualification.{claim['claim_id']}.as_of"
        receipt = receipts_by_step.get(claim.get("step_id"))
        evaluated_at = (receipt or {}).get("process", {}).get("started_at") or "unrecorded instant"
        recorded_flags = ""
        if qualification.get("revoked_by"):
            recorded_flags = f" [revoked {qualification['revoked_by'][:19]}…]"
        elif qualification.get("superseded_by"):
            recorded_flags = f" [superseded {qualification['superseded_by'][:19]}…]"
        label, suffix = _as_of_state(
            claim, material_docs, material_revoked, material_owners, as_of, kinds
        )
        report.as_of(
            check,
            f"recorded {qualification.get('state')!r}{recorded_flags} @{evaluated_at}; "
            f"as_of {as_of} under {material_desc}: {label!r}{suffix}",
        )


# ---------------------------------------------------------------------------
# Section 10: requirement-set coverage (S-024)
#
# Rule source: the requirement-set model itself — S-024 states the rule a
# `run` applies: a case package binds one requirement_set document (a
# library's owned list of what any contract in its domain must address)
# and declares in its manifest which contract requirements cover each set
# entry and, for the rest, a reason and an accepting owner; coverage on a
# basis weaker than the entry's `minimum_basis`, or a `must_state` entry
# left uncovered with no stated omission, makes coverage incomplete and
# the run refuses before any evaluation is spent. So a package whose
# committed claims and campaign report exist asserts that its coverage
# declaration re-derived `complete`; this section re-derives the
# assessment from the three committed documents alone and reports a
# derived `incomplete` as a mismatch — artifacts that could not have been
# produced by a conforming run. The per-entry coverage report is written
# to the transient run workspace and never committed, so the derived
# status is the only committed observable; the check names every derived
# issue and does not pretend a per-entry diff against a record that does
# not exist. Cases with no `coverage` declaration have nothing to check
# and get none.
# ---------------------------------------------------------------------------

_BASIS_RANK = {"nominal": 0, "bounded": 1, "enclosure": 2}
_COMPARISONS = {"less_than", "less_than_or_equal", "greater_than", "greater_than_or_equal", "equal"}
_OMISSION_POLICIES = {"must_state", "may_omit"}
REQUIREMENT_SET_SCHEMA_VERSION = "avila.core/requirement-set/v0.1-draft"


def _requirement_set_problems(set_doc: object) -> list:
    """The validation a `run` applies to a bound requirement set before any
    coverage is assessed: an unreadable or invalid set is a refusal, not an
    empty set."""
    problems: list[str] = []
    if not isinstance(set_doc, dict):
        return ["requirement set is not a JSON object"]
    unknown = set(set_doc) - {"schema_version", "set_id", "revision", "owner", "title", "requirements"}
    if unknown:
        problems.append(f"requirement set carries unknown fields {sorted(unknown)}")
    if set_doc.get("schema_version") != REQUIREMENT_SET_SCHEMA_VERSION:
        problems.append(
            f"requirement set schema {set_doc.get('schema_version')!r} is not {REQUIREMENT_SET_SCHEMA_VERSION!r}"
        )
    for field_name in ("set_id", "owner", "title"):
        if not str(set_doc.get(field_name, "")).strip():
            problems.append(f"requirement set `{field_name}` must not be empty")
    if not isinstance(set_doc.get("revision"), int) or set_doc["revision"] < 1:
        problems.append("requirement set revision must be at least 1")
    requirements = set_doc.get("requirements")
    if not isinstance(requirements, list) or not requirements:
        problems.append("requirement set must contain at least one requirement")
        return problems
    seen: set[str] = set()
    for entry in requirements:
        if not isinstance(entry, dict):
            problems.append("a requirement-set entry is not an object")
            continue
        eid = entry.get("set_requirement_id")
        label = eid if isinstance(eid, str) and eid else "<missing id>"
        unknown = set(entry) - {
            "set_requirement_id", "statement", "kind", "comparison",
            "minimum_basis", "omission", "rationale",
        }
        if unknown:
            problems.append(f"requirement set entry `{label}` carries unknown fields {sorted(unknown)}")
        for field_name in ("set_requirement_id", "statement", "kind"):
            if not str(entry.get(field_name, "")).strip():
                problems.append(f"requirement set entry `{label}`: `{field_name}` must not be empty")
        if entry.get("comparison") not in _COMPARISONS:
            problems.append(f"requirement set entry `{label}`: comparison {entry.get('comparison')!r} is not a known comparison")
        if entry.get("minimum_basis") not in _BASIS_RANK:
            problems.append(f"requirement set entry `{label}`: minimum_basis {entry.get('minimum_basis')!r} is not a known basis")
        if entry.get("omission") not in _OMISSION_POLICIES:
            problems.append(f"requirement set entry `{label}`: omission {entry.get('omission')!r} is not a known omission policy")
        if isinstance(eid, str) and eid:
            if eid in seen:
                problems.append(f"requirement set entry `{eid}` is declared twice")
            seen.add(eid)
    return problems


def _assess_coverage(set_doc: dict, declaration: dict, requirements: dict, categorical_ids: set) -> tuple:
    """Re-derives the coverage assessment a `run` computes. ``requirements``
    maps each quantitative contract requirement id to
    ``(limit.kind, comparison, basis.kind)``; ``categorical_ids`` holds the
    contract's categorical requirement ids. Returns ``(issues, entries)``
    where entries carry ``(set_requirement_id, state, issues)``."""
    issues: list[str] = []
    set_ids = {entry["set_requirement_id"] for entry in set_doc["requirements"]}
    mapping = declaration.get("mapping", {})
    if not isinstance(mapping, dict):
        mapping = {}
        issues.append("coverage `mapping` is not an object")
    for key in mapping:
        if key not in set_ids:
            issues.append(f"mapping names `{key}`, which is not in requirement set `{set_doc['set_id']}`")
    omissions: dict[str, dict] = {}
    for omission in declaration.get("omissions", []):
        if not isinstance(omission, dict):
            issues.append("a coverage `omissions` entry is not an object")
            continue
        oid = omission.get("set_requirement_id")
        if oid not in set_ids:
            issues.append(f"omission names `{oid}`, which is not in requirement set `{set_doc['set_id']}`")
            continue
        if not str(omission.get("reason", "")).strip() or not str(omission.get("accepted_by", "")).strip():
            issues.append(f"omission of `{oid}` must state a reason and who accepted it")
            continue
        if mapping.get(oid):
            issues.append(f"`{oid}` is both mapped to contract requirements and declared omitted")
            continue
        if oid in omissions:
            issues.append(f"omission of `{oid}` is declared twice")
        omissions[oid] = omission

    entries: list[tuple[str, str, list]] = []
    for entry in set_doc["requirements"]:
        eid = entry["set_requirement_id"]
        entry_issues: list[str] = []
        covered_by: list[bool] = []
        for requirement_id in mapping.get(eid) or []:
            found = requirements.get(requirement_id)
            if found is None:
                if requirement_id in categorical_ids:
                    entry_issues.append(
                        f"mapped contract requirement `{requirement_id}` is categorical and cannot cover a quantitative requirement-set entry"
                    )
                else:
                    entry_issues.append(f"mapped contract requirement `{requirement_id}` does not exist")
                continue
            kind, comparison, basis = found
            if kind != entry["kind"]:
                entry_issues.append(f"`{requirement_id}` compares kind `{kind}`, the set entry requires `{entry['kind']}`")
                continue
            if comparison != entry["comparison"]:
                entry_issues.append(
                    f"`{requirement_id}` uses comparison `{comparison}`, the set entry requires `{entry['comparison']}`"
                )
                continue
            if basis not in _BASIS_RANK:
                entry_issues.append(f"`{requirement_id}` declares basis `{basis}`, which is not a known basis")
                continue
            covered_by.append(_BASIS_RANK[basis] >= _BASIS_RANK[entry["minimum_basis"]])
        if any(covered_by):
            state = "covered"
        elif covered_by:
            entry_issues.append(
                f"covered only on a basis weaker than the set's minimum `{entry['minimum_basis']}`; a guide is not evidence"
            )
            state = "covered_under_basis"
        elif eid in omissions:
            state = "omitted_stated"
        elif entry["omission"] == "may_omit":
            state = "omissible"
        else:
            entry_issues.append("not covered and no omission is stated; the set requires a reason and an accepting owner")
            state = "omitted_unstated"
        entries.append((eid, state, entry_issues))

    incomplete = bool(issues) or any(
        entry_issues or state in ("covered_under_basis", "omitted_unstated")
        for _eid, state, entry_issues in entries
    )
    return issues, entries, "incomplete" if incomplete else "complete"


# ---------------------------------------------------------------------------
# Section 10.5: provider selection records (ADR-0020, SC-8)
#
# Rule source: crates/avila-core-compiler/src/selection.rs's model and
# crates/avila-core-runner/src/case_run/selection.rs's load_selection, read
# to learn which fields the record carries and which refusal each rule
# produces — never to copy control flow. The engine does not perform
# selection; it verifies the recorded one. This section re-derives every
# check the runner makes: the record's registry snapshot is exactly the
# bound registry, every considered candidate carries a decision and
# reasons, the criteria stay inside the legitimate vocabulary (provider
# payment and Avila margin never appear), at most one candidate is
# selected, and the selected triple is exactly the capability the manifest
# binds for the step. When the contract's execution_policy declares
# provider rules, the record is required and each rule is re-checked —
# deny/allow on the selected type's registry owner, provider independence
# and implementation diversity across steps, the declared maturity floor,
# the self-preference check for an Avila-provided selection, and the cost
# cap's recorded confirmation.
#
# What the runner refuses at bind/plan time this section reports as a
# mismatch — the same truth, checked without trusting the runner.


_LEGITIMATE_SELECTION_CRITERIA = frozenset(
    {"cost", "time", "locality", "technical", "diversity", "preference"}
)
_BANNED_SELECTION_CRITERIA = frozenset({"provider_payment", "avila_margin"})
_MATURITY_ORDER = ["prototype", "development", "qualified", "production"]
_SELECTION_SCHEMA_VERSION = "avila.core/capability-selection/v0.1-draft"


def _selection_policy_active(policy: dict) -> bool:
    return bool(
        policy.get("deny_providers")
        or policy.get("allow_providers")
        or policy.get("require_provider_independence")
        or policy.get("require_diverse_implementations")
        or policy.get("maturity_floor") is not None
        or policy.get("forbid_self_preference")
        or policy.get("cost_cap") is not None
    )


def verify_case_selections(
    case_dir: Path,
    package: dict,
    contract: Optional[dict],
    registry: Optional[dict],
    report: Report,
) -> None:
    policy = (contract or {}).get("execution_policy") or {}
    docs = [d for d in package.get("documents", []) if d.get("role") == "capability_selection"]
    if len(docs) > 1:
        report.mismatch(
            "selection.document",
            f"the package binds {len(docs)} capability_selection documents; one document records the whole package's selections",
        )
        return
    if not docs:
        if _selection_policy_active(policy):
            report.mismatch(
                "selection.document",
                "the contract's execution policy declares provider-selection rules, but no capability_selection document is bound (CORE-P5103)",
            )
        return
    doc = docs[0]
    path = case_dir / doc["path"]
    if not path.is_file():
        report.mismatch("selection.document", f"capability_selection document {doc['path']!r} is missing")
        return
    try:
        selection = load_json(path)
    except (OSError, json.JSONDecodeError) as error:
        report.mismatch("selection.document", f"capability_selection document does not parse: {error}")
        return
    if selection.get("schema_version") != _SELECTION_SCHEMA_VERSION:
        report.mismatch(
            "selection.document",
            f"capability_selection schema_version {selection.get('schema_version')!r} is not {_SELECTION_SCHEMA_VERSION!r}",
        )
        return
    report.verified("selection.document", f"bound {doc['document_id']!r} parses as {_SELECTION_SCHEMA_VERSION}")

    # The record's discovery boundary must be the package's bound registry.
    if registry is not None:
        bound_registry_doc = next(
            (d for d in package.get("documents", []) if d.get("role") == "registry"), None
        )
        snapshot = selection.get("registry_snapshot", {})
        if (
            snapshot.get("registry_id") != registry.get("registry_id")
            or snapshot.get("revision") != registry.get("revision")
            or snapshot.get("sha256") != (bound_registry_doc or {}).get("sha256")
        ):
            report.mismatch(
                "selection.registry_snapshot",
                "the record's registry_snapshot does not equal the package's bound registry — its candidate set is bounded by different material (CORE-P5102)",
            )
        else:
            report.verified(
                "selection.registry_snapshot",
                f"{registry['registry_id']}@{registry['revision']} matches the bound registry digest",
            )

    selections = selection.get("selections", [])
    entries = {entry.get("step_id"): entry for entry in selections if isinstance(entry, dict)}
    executions = package.get("executions", [])
    workflow = {step["step_id"]: step for step in (contract or {}).get("workflow", [])}
    capabilities = {c["capability_id"]: c for c in package.get("capabilities", [])}
    registry_types = {
        c.get("capability_type", {}).get("id"): c for c in (registry or {}).get("capability_types", [])
    }

    # A selection entry for a step the package does not execute is dangling.
    for step_id in entries:
        if not any(execution.get("step_id") == step_id for execution in executions):
            report.mismatch(
                "selection.entries",
                f"the record selects for step {step_id!r}, which the package does not execute",
            )
            break

    selected_owners: list[tuple[str, str]] = []
    selected_executables: list[tuple[str, str]] = []
    for execution in executions:
        step_id = execution["step_id"]
        entry = entries.get(step_id)
        if entry is None:
            if _selection_policy_active(policy):
                report.mismatch(
                    f"selection.{step_id}.entry",
                    f"step {step_id!r} has no selection entry — the policy's rules cannot be checked for it (CORE-P5103)",
                )
            continue

        undecided = [
            f"{c.get('capability_id')}/{c.get('adapter')}"
            for c in entry.get("candidates", [])
            if not c.get("decision") or not c.get("reasons")
        ]
        if undecided:
            report.mismatch(
                f"selection.{step_id}.candidates",
                f"candidates without a recorded decision or reasons: {', '.join(undecided)} (CORE-P5602)",
            )
        else:
            report.verified(
                f"selection.{step_id}.candidates",
                f"{len(entry.get('candidates', []))} candidate(s) all decided and reasoned",
            )

        criteria = entry.get("criteria", [])
        banned = [c for c in criteria if c in _BANNED_SELECTION_CRITERIA]
        unknown = [c for c in criteria if c not in _LEGITIMATE_SELECTION_CRITERIA and c not in _BANNED_SELECTION_CRITERIA]
        if banned or unknown:
            detail = []
            if banned:
                detail.append(f"banned: {', '.join(banned)}")
            if unknown:
                detail.append(f"outside the legitimate vocabulary: {', '.join(unknown)}")
            report.mismatch(
                f"selection.{step_id}.criteria",
                "; ".join(detail) + " (CORE-P5601)",
            )
        else:
            report.verified(
                f"selection.{step_id}.criteria",
                f"criteria {criteria} are legitimate vocabulary" if criteria else "no criteria recorded",
            )

        winners = [c for c in entry.get("candidates", []) if c.get("decision") == "selected"]
        if len(winners) != 1:
            reasons = [
                f"{r.get('rule_id')} ({r.get('detail')})"
                for c in entry.get("candidates", [])
                for r in c.get("reasons", [])
            ]
            report.mismatch(
                f"selection.{step_id}.selected",
                f"{len(winners)} candidates carry decision 'selected' — the run refuses with CORE-P5101: {', '.join(reasons)}",
            )
            continue
        selected = winners[0]

        step = workflow.get(step_id)
        capability = capabilities.get(execution.get("capability_id"), {})
        bound_type = (step or {}).get("capability_type", {})
        differences = []
        if selected.get("capability_type") != bound_type:
            differences.append(f"type {selected.get('capability_type')} != bound {bound_type}")
        if selected.get("capability_id") != execution.get("capability_id"):
            differences.append(
                f"capability {selected.get('capability_id')!r} != bound {execution.get('capability_id')!r}"
            )
        if selected.get("adapter") != execution.get("adapter"):
            differences.append(f"adapter {selected.get('adapter')!r} != bound {execution.get('adapter')!r}")
        if selected.get("executable_sha256") != capability.get("executable_sha256"):
            differences.append(
                f"executable {selected.get('executable_sha256')} != bound {capability.get('executable_sha256')}"
            )
        if differences:
            report.mismatch(
                f"selection.{step_id}.selected",
                "the recorded selection is not what the package binds — " + "; ".join(differences) + " (CORE-P5102)",
            )
            continue
        report.verified(
            f"selection.{step_id}.selected",
            f"the recorded winner is the bound {execution.get('capability_id')}/{execution.get('adapter')}",
        )

        type_id = (selected.get("capability_type") or {}).get("id")
        registry_type = registry_types.get(type_id)
        owner = (registry_type or {}).get("owner")
        if registry_type is None:
            report.mismatch(
                f"selection.{step_id}.provider",
                f"the selected capability type {type_id!r} is not declared in the bound registry",
            )
        else:
            denied = policy.get("deny_providers") or []
            allowed = policy.get("allow_providers") or []
            if owner in denied:
                report.mismatch(
                    f"selection.{step_id}.provider",
                    f"provider {owner!r} is in execution_policy.deny_providers (CORE-P5301)",
                )
            elif allowed and owner not in allowed:
                report.mismatch(
                    f"selection.{step_id}.provider",
                    f"provider {owner!r} is not in execution_policy.allow_providers (CORE-P5301)",
                )
            elif denied or allowed:
                report.verified(f"selection.{step_id}.provider", f"provider {owner!r} is permitted")
            floor = policy.get("maturity_floor")
            if floor is not None:
                declared = registry_type.get("maturity")
                if declared is None or _MATURITY_ORDER.index(declared) < _MATURITY_ORDER.index(floor):
                    report.mismatch(
                        f"selection.{step_id}.maturity",
                        (
                            f"the selected type declares no maturity — the floor {floor!r} cannot be met (CORE-P5303)"
                            if declared is None
                            else f"declared maturity {declared!r} is below the floor {floor!r} (CORE-P5303)"
                        ),
                    )
                else:
                    report.verified(
                        f"selection.{step_id}.maturity",
                        f"declared maturity {declared!r} meets the floor {floor!r}",
                    )
            if owner is not None:
                selected_owners.append((step_id, owner))
        selected_executables.append((step_id, selected.get("executable_sha256")))

        if policy.get("forbid_self_preference") and entry.get("avila_provided"):
            if entry.get("self_preference_check") is None:
                report.mismatch(
                    f"selection.{step_id}.self_preference",
                    "an Avila-provided selection carries no recorded self_preference_check (CORE-P5501)",
                )
            else:
                report.verified(
                    f"selection.{step_id}.self_preference",
                    "the self-preference check is recorded",
                )

        cap = policy.get("cost_cap")
        if cap is not None:
            estimate = entry.get("cost_estimate")
            confirmed = bool(entry.get("cost_confirmed_by"))
            refusal = None
            if estimate is None:
                refusal = "no cost estimate is recorded — the cap cannot be checked"
            elif estimate.get("currency") != cap.get("currency"):
                refusal = f"estimate is in {estimate.get('currency')!r} but the cap is in {cap.get('currency')!r} — the cap cannot be checked across currencies"
            else:
                try:
                    over = read_authoritative_decimal(estimate["value"]) > read_authoritative_decimal(cap["value"])
                except Exception:
                    over = True
                if over:
                    refusal = f"cost estimate {estimate.get('value')} {estimate.get('currency')} exceeds the cap {cap.get('value')} {cap.get('currency')}"
            if refusal is not None and not confirmed:
                report.mismatch(
                    f"selection.{step_id}.cost_cap",
                    f"{refusal} with no recorded confirmation (CORE-P5401)",
                )
            elif refusal is not None:
                report.verified(
                    f"selection.{step_id}.cost_cap",
                    f"{refusal} — confirmed by {entry['cost_confirmed_by']}",
                )
            else:
                report.verified(
                    f"selection.{step_id}.cost_cap",
                    f"cost estimate {estimate.get('value')} {estimate.get('currency')} is within the cap",
                )

    if policy.get("require_provider_independence"):
        seen: dict[str, str] = {}
        violation = None
        for step_id, owner in selected_owners:
            if owner in seen:
                violation = f"steps {seen[owner]!r} and {step_id!r} both selected provider {owner!r} (CORE-P5302)"
                break
            seen[owner] = step_id
        if violation:
            report.mismatch("selection.independence", violation)
        elif selected_owners:
            report.verified(
                "selection.independence",
                f"{len(selected_owners)} step(s) select distinct providers",
            )
    if policy.get("require_diverse_implementations"):
        seen: dict[str, str] = {}
        violation = None
        for step_id, executable in selected_executables:
            if executable in seen:
                violation = f"steps {seen[executable]!r} and {step_id!r} selected the same executable bytes (CORE-P5304)"
                break
            seen[executable] = step_id
        if violation:
            report.mismatch("selection.diversity", violation)
        elif selected_executables:
            report.verified(
                "selection.diversity",
                f"{len(selected_executables)} step(s) select distinct executables",
            )


def verify_case_coverage(case_dir: Path, package: dict, contract: Optional[dict], report: Report) -> None:
    coverage = package.get("coverage")
    if coverage is None:
        return
    check = "coverage.requirement_set"
    if not isinstance(coverage, dict) or not isinstance(coverage.get("requirement_set"), str):
        report.mismatch(check, "manifest `coverage` is malformed: `requirement_set` must name a bound document id")
        return
    set_document = next(
        (d for d in package.get("documents", []) if d.get("document_id") == coverage["requirement_set"]),
        None,
    )
    if set_document is None:
        report.mismatch(
            check,
            f"coverage names requirement_set {coverage['requirement_set']!r}, which is not a document the manifest binds",
        )
        return
    path = case_dir / set_document["path"]
    if not path.is_file():
        report.mismatch(check, f"requirement-set document {set_document['path']!r} is missing")
        return
    try:
        set_doc = load_json(path)
    except (OSError, json.JSONDecodeError) as error:
        report.mismatch(check, f"requirement-set document does not parse: {error}")
        return
    problems = _requirement_set_problems(set_doc)
    if problems:
        report.mismatch(check, "requirement set is invalid — a conforming run refuses it: " + "; ".join(problems))
        return
    if contract is None:
        report.not_checked(check, "contract document unavailable; coverage cannot be re-derived")
        return

    requirements = {
        req["requirement_id"]: (req["limit"]["kind"], req["comparison"], req["basis"]["kind"])
        for req in contract.get("requirements", [])
    }
    categorical_ids = {req["requirement_id"] for req in contract.get("categorical_requirements", [])}
    issues, entries, status = _assess_coverage(set_doc, coverage, requirements, categorical_ids)
    if status == "incomplete":
        detail = issues + [
            f"entry `{eid}` is {state}: {'; '.join(entry_issues)}" if entry_issues else f"entry `{eid}` is {state}"
            for eid, state, entry_issues in entries
            if entry_issues or state in ("covered_under_basis", "omitted_unstated")
        ]
        report.mismatch(
            check,
            "coverage re-derives incomplete — a conforming `run` refuses before evaluation, so the "
            "committed claims and campaign report could not have been produced under this declaration: "
            + "; ".join(detail),
        )
    else:
        counts: dict[str, int] = {}
        for _eid, state, _entry_issues in entries:
            counts[state] = counts.get(state, 0) + 1
        report.verified(
            check,
            f"coverage of requirement set {set_doc['set_id']} revision {set_doc['revision']} re-derives "
            f"complete: "
            + ", ".join(f"{counts.get(state, 0)} {state}" for state in ("covered", "omitted_stated", "omissible")),
        )


# ---------------------------------------------------------------------------
# Section 11: staged-review records (optional presentation gates)
#
# Rule source: CAMPAIGN_EVALUATION.md "Optional presentation gates" and the
# committed avila.core/staged-review-record/v0.1-draft schema, cross-checked
# against examples/cases/case-001-shield-search/reviews/reference.json — the
# one committed record of this shape. A run realizes each compiled
# presentation_gate into a request: presented_evidence is resolved against
# the generated claims document (a `contract_input` source binds the input
# attestation `input:{input_id}`; a `step_output` source binds the claim
# for that step+slot, and the evidence id is the claim id), readiness is
# `ready_for_agent` only when every source resolved, and `request_sha256`
# is sha256 of the canonical request body with the field removed — the
# same digest rule `campaign_sha256` uses, cross-checked against the
# committed record. The reviewer's record then adds its own
# `record_sha256` over the record minus that field.
#
# What this section re-derives: both digests, the request's binding to the
# committed campaign report (compiled snapshot and campaign identities),
# every presented-evidence entry's evidence id, digest, and media type
# against claims.json, readiness against the recorded missing list, the
# reviewer role and disposition vocabulary, and the eligibility-policy
# digest against the bound `review_policy` document.
#
# Named-outs, still: whether the agent's `rationale`/`actions` faithfully
# describe the candidate is prose, not provable identity; and `attestation`
# is the reviewer's self-report — this profile names it, never authenticates
# a reviewer identity. The record is routing history; nothing here makes it
# evidence or a verdict.
# ---------------------------------------------------------------------------

STAGED_REVIEW_SCHEMA_VERSION = "avila.core/staged-review-record/v0.1-draft"
STAGED_REVIEW_DISPOSITIONS = {"present_to_user", "request_changes", "abstain"}


def _canonical_identity(document: dict, field: str) -> Optional[str]:
    """sha256 of the canonical body with `field` removed — the digest rule
    request_sha256, record_sha256, and campaign_sha256 all share."""
    if field not in document:
        return None
    body = {key: value for key, value in document.items() if key != field}
    return sha256_bytes(canonicalize_json(json.dumps(body, separators=(",", ":")).encode()))


def verify_staged_reviews(
    case_dir: Path,
    package: dict,
    docs_by_role: dict[str, list[dict]],
    claims: Optional[dict],
    campaign_report: Optional[dict],
    report: Report,
) -> None:
    records = docs_by_role.get("staged_review_record", [])
    if not records:
        return
    claims_inputs = {i["input_id"]: i for i in (claims or {}).get("inputs", [])}
    claims_outputs = {
        (c["step_id"], c["output_slot"]): c for c in (claims or {}).get("claims", [])
    }
    for document in records:
        document_id = document["document_id"]
        prefix = f"staged_review.{document_id}"
        path = case_dir / document["path"]
        if not path.is_file():
            report.mismatch(prefix, f"staged-review record missing: {document['path']}")
            continue
        try:
            record = load_json(path)
        except (OSError, json.JSONDecodeError) as error:
            report.mismatch(prefix, f"staged-review record does not parse: {error}")
            continue

        if record.get("schema_version") == STAGED_REVIEW_SCHEMA_VERSION:
            report.verified(f"{prefix}.schema_version", STAGED_REVIEW_SCHEMA_VERSION)
        else:
            report.mismatch(
                f"{prefix}.schema_version",
                f"expected {STAGED_REVIEW_SCHEMA_VERSION}, found {record.get('schema_version')!r}",
            )
            continue

        request = record.get("review_request", {})
        # The request binds the run that produced it. A committed carrier is
        # the claims document's and campaign report's snapshot identity, or a
        # campaign_sha256 any recorded log line carries; a binding no
        # committed record carries is unresolvable, not automatically forged —
        # the run it names may simply never have been committed.
        snapshot_carriers = {
            doc.get("compiled_snapshot_sha256")
            for doc in (claims, campaign_report)
            if doc is not None
        } - {None}
        campaign_carriers = {
            (campaign_report or {}).get("campaign_sha256")
        } - {None}
        for log_path in case_dir.glob("**/*.jsonl"):
            for line in log_path.read_bytes().splitlines():
                if not line.strip():
                    continue
                try:
                    row = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(row, dict) and isinstance(row.get("campaign_sha256"), str):
                    campaign_carriers.add(row["campaign_sha256"])
        for field, carriers in (
            ("compiled_snapshot_sha256", snapshot_carriers),
            ("campaign_sha256", campaign_carriers),
        ):
            bound = request.get(field)
            check = f"{prefix}.request.{field}"
            if bound is None:
                report.mismatch(check, f"request carries no {field}")
            elif not carriers:
                report.not_checked(check, "no committed record carries an identity to bind against")
            elif bound in carriers:
                report.verified(check, bound)
            else:
                report.not_checked(
                    check,
                    f"request binds {bound}, which no committed claims/report/log record carries — it names a run this package does not commit",
                )

        if claims is not None:
            for entry in request.get("presented_evidence", []):
                check = f"{prefix}.presented.{entry.get('input_slot', '?')}"
                source = entry.get("source", {})
                bound = None
                if source.get("source") == "contract_input":
                    input_id = source.get("input_id")
                    bound = claims_inputs.get(input_id)
                    want_evidence = f"input:{input_id}"
                elif source.get("source") == "step_output":
                    bound = claims_outputs.get((source.get("step_id"), source.get("output_slot")))
                    want_evidence = bound["claim_id"] if bound is not None else None
                else:
                    report.mismatch(check, f"unknown presented-evidence source kind {source.get('source')!r}")
                    continue
                if bound is None:
                    report.mismatch(check, f"presented evidence resolves to no claims record ({source})")
                    continue
                problems = []
                if entry.get("evidence_id") != want_evidence:
                    problems.append(f"evidence_id {entry.get('evidence_id')!r} != {want_evidence!r}")
                for field in ("sha256", "media_type"):
                    if entry.get(field) != bound["artifact"].get(field):
                        problems.append(f"{field} {entry.get(field)!r} != claims' {bound['artifact'].get(field)!r}")
                if problems:
                    report.mismatch(check, "; ".join(problems))
                else:
                    report.verified(check, entry.get("evidence_id"))
            readiness = request.get("readiness")
            check = f"{prefix}.readiness"
            if readiness == "ready_for_agent" and not request.get("missing_evidence"):
                report.verified(check, "ready_for_agent with no missing evidence")
            elif readiness == "awaiting_evidence" and request.get("missing_evidence"):
                report.verified(
                    check,
                    f"awaiting_evidence names {len(request['missing_evidence'])} unresolved source(s)",
                )
            else:
                report.mismatch(
                    check,
                    f"readiness {readiness!r} contradicts missing_evidence {request.get('missing_evidence')}",
                )
        else:
            report.not_checked(f"{prefix}.presented_evidence", "no claims document committed to rebind against")

        reviewer = record.get("reviewer", {})
        check = f"{prefix}.reviewer_role"
        if reviewer.get("role") == request.get("reviewer_role"):
            report.verified(check, reviewer["role"])
        else:
            report.mismatch(check, f"reviewer role {reviewer.get('role')!r} != request's {request.get('reviewer_role')!r}")

        check = f"{prefix}.disposition"
        disposition = record.get("disposition")
        if disposition in request.get("allowed_dispositions", []):
            report.verified(check, disposition)
        else:
            report.mismatch(check, f"disposition {disposition!r} is not in the request's allowed dispositions")

        policy = request.get("reviewer_eligibility_policy", {})
        policy_doc = next(
            (d for d in docs_by_role.get("review_policy", []) if policy.get("sha256") == d.get("sha256")),
            None,
        )
        check = f"{prefix}.eligibility_policy"
        if not policy:
            report.not_checked(check, "request names no reviewer eligibility policy")
        elif policy_doc is not None:
            report.verified(check, f"policy digest binds manifest document {policy_doc['document_id']}")
        else:
            report.mismatch(check, f"policy digest {policy.get('sha256')!r} binds no manifest review_policy document")

        for field, subject in (("request_sha256", request), ("record_sha256", record)):
            check = f"{prefix}.{field}"
            computed = _canonical_identity(subject, field)
            if computed is None:
                report.mismatch(check, f"record carries no {field}")
            elif computed == subject[field]:
                report.verified(check, computed)
            else:
                report.mismatch(check, f"recomputed {computed}, record binds {subject[field]!r}")

        report.not_checked(
            f"{prefix}.attestation",
            f"attestation {record.get('attestation')!r} is the reviewer's self-report; this profile does not authenticate reviewer identity, and the record is routing history, not evidence or a verdict",
        )


# ---------------------------------------------------------------------------
# Case-level orchestration and CLI
# ---------------------------------------------------------------------------


def verify_case(
    case_dir: Path,
    roots: dict[str, Path],
    trust_root: Optional[TrustRoot] = None,
    as_of: Optional[str] = None,
    as_of_material_dir: Optional[Path] = None,
) -> Report:
    """Runs every applicable section of the profile against one case
    directory (an examples/cases/CASE-NNN-* layout: package.json plus the
    documents it names). ``trust_root``, when supplied, is the ADR-0015
    requester/runner public keys (``load_trust_root``); without one, every
    signature is reported ``not_checked`` or ``invalid``, never
    ``verified``. ``as_of``, a normalized ``YYYY-MM-DDTHH:MM:SSZ`` instant,
    adds one informational labeled state per qualified claim, evaluated
    under ``as_of_material_dir``'s bound qualification material when
    supplied (ADR-0006's supplied-snapshot historical verification) or the
    case's own bound set otherwise."""
    report = Report(str(case_dir))
    package_path = case_dir / "package.json"
    if not package_path.is_file():
        report.not_checked("package.manifest", f"no package.json found at {case_dir}")
        return report
    manifest_bytes = package_path.read_bytes()
    package = json.loads(manifest_bytes)

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

    receipts_by_step: dict[str, dict] = {}
    for receipt_doc in docs_by_role.get("execution_receipt", []):
        step_id = receipt_doc.get("step_id", receipt_doc["document_id"])
        path = case_dir / receipt_doc["path"]
        if not path.is_file():
            report.not_checked(f"receipt.{step_id}", f"receipt file missing: {path}")
            continue
        receipt = load_json(path)
        receipts_by_step[step_id] = receipt
        verify_receipt(case_dir, step_id, receipt, package, claims_by_id, report)

    if contract is not None and registry is not None and claims is not None:
        verify_case_verdicts(contract, registry, claims, campaign_report, report)
    else:
        report.not_checked("verdict.all", "contract.json, registry.json, or claims.json missing/undeclared for this case")

    verify_case_coverage(case_dir, package, contract, report)

    verify_case_qualification_envelopes(
        case_dir, package, docs_by_role, claims, kinds_from_registry_doc(registry) if registry is not None else {},
        receipts_by_step, report,
        (contract or {}).get("execution_policy", {}).get("recognized_qualification_owners") or {},
        as_of=as_of,
        as_of_material_dir=as_of_material_dir,
    )

    verify_case_selections(case_dir, package, contract, registry, report)

    verify_staged_reviews(case_dir, package, docs_by_role, claims, campaign_report, report)

    log_path = case_dir / "search" / "attempts.jsonl"
    if log_path.is_file():
        verify_attempt_log(log_path, report)
    else:
        report.not_checked("lineage.all", f"no attempt log at {log_path}")

    verify_case_signatures(case_dir, package, manifest_bytes, docs_by_role, trust_root, report)
    return report


def normalize_as_of_instant(value: str) -> str:
    """``--as-of`` accepts the same normalized instant form a qualification
    record's ``not_after`` carries — ``YYYY-MM-DDTHH:MM:SSZ`` — or a bare
    ``YYYY-MM-DD`` date, expanded to its first instant ``T00:00:00Z``.
    Normalized instants compare lexically as chronological."""
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
        return f"{value}T00:00:00Z"
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", value):
        return value
    raise ValueError(
        f"--as-of instant {value!r} is not a normalized `YYYY-MM-DDTHH:MM:SSZ` "
        "(or a bare `YYYY-MM-DD` date)"
    )


def cmd_verify_case(args: argparse.Namespace) -> int:
    roots = parse_source_roots(args.source_root or [])
    trust_root = load_trust_root(Path(args.trust_root)) if args.trust_root else None
    try:
        as_of = normalize_as_of_instant(args.as_of) if args.as_of else None
    except ValueError as error:
        raise SystemExit(str(error)) from error
    as_of_material_dir = Path(args.as_of_material) if args.as_of_material else None
    if as_of_material_dir is not None and as_of is None:
        raise SystemExit("--as-of-material requires --as-of: a material snapshot is read at an instant")
    report = verify_case(
        Path(args.case_dir),
        roots,
        trust_root=trust_root,
        as_of=as_of,
        as_of_material_dir=as_of_material_dir,
    )
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
    p_case.add_argument(
        "--trust-root",
        metavar="FILE",
        help="ADR-0015 trust root (avila.core/trust-root/v0.1-draft: requester/runner public keys) to verify "
        "signatures against; without it every signature is reported not_checked or invalid, never verified",
    )
    p_case.add_argument(
        "--as-of",
        metavar="INSTANT",
        help="Historical verification (ADR-0006): report every qualified claim's labeled state at this "
        "instant — normalized `YYYY-MM-DDTHH:MM:SSZ`, or `YYYY-MM-DD` for its first instant — alongside "
        "its recorded state. Informational [ASOF] lines; a divergence is a datum, never a failure",
    )
    p_case.add_argument(
        "--as-of-material",
        metavar="DIR",
        help="Another case package directory whose bound qualification records, revocations, signatures, "
        "and contract policy stand in as the supplied material snapshot for --as-of (requires --as-of); "
        "without it the case's own bound set is the material",
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
