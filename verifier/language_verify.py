#!/usr/bin/env python3
"""Avila Core language-evaluation replay — EL-04 (offline, stdlib-only).

Independently re-derives what an ``avila.core/language-evaluation`` record
claims, from the supplied program, library, execution plan, and observation
document — never from Rust source behavior. The rules reconstructed here
come from the language specification (spec §7 [O1]/[O2], §8 premise
admissibility, §10 identity) and the document schemas:

  * canonical JSON identity of program/library documents, and the
    *semantic* projection identity (annotation fields dropped at declared
    positions, declared sets sorted by projected canonical bytes)
  * plan identity (``plan_sha256`` over the canonical body) and the
    invocation set a ``ready`` plan must carry — the staged input digests
    re-derived from the verifier's own binding replay
  * observation binding (O1): receipt re-hash, plan/site/executable
    agreement, staged input digest-set equality, invocation identity,
    completed status, output digest, typed admission of the output
  * postcondition replay (O2): ``output = <expr>`` ensures relations
    evaluated with exact-rational interval arithmetic
  * premise discharge: ``assumes`` instantiated at the applied scope stay
    residual unless an admissible premise asserts them, whose own
    assumptions expand the cone to a fixpoint
  * requirement verdicts: ``bounded.ge`` / ``bounded.le`` over the
    subject's established value, keeping the ``pass`` / ``fail`` /
    ``inconclusive`` / ``not_evaluated`` state distinctions
  * record identity (``context_sha256``, ``evaluation_sha256``)

A record that replays differently is reported ``mismatch``. Material this
port does not model (e.g. analysis identity when the analysis document is
not supplied) is ``not_checked`` by name rather than silently trusted.

Usage:
  python3 language_verify.py verify-evaluation \
      --program P.json --library L.json --plan plan.json \
      --observations obs.json --evaluation eval.json [--analysis a.json]

  python3 language_verify.py explain \
      --program P.json --evaluation-old A.json --evaluation-new B.json
"""

import json
import sys
from collections import Counter
from dataclasses import dataclass, field
from fractions import Fraction
from pathlib import Path
from typing import Any, Optional

sys.path.insert(0, str(Path(__file__).resolve().parent))

from avila_core_verify import (  # noqa: E402
    Report,
    canonicalize_json,
    read_authoritative_exact,
    sha256_bytes,
)

PLAN_SCHEMA = "avila.core/execution-plan/v0.1-draft"
OBSERVATIONS_SCHEMA = "avila.core/language-observations/v0.1-draft"
EVALUATION_SCHEMA = "avila.core/language-evaluation/v0.1-draft"
LANGUAGE_PROFILE = "avila.core/language/0.1-draft"

# The closed replayable-check vocabulary — a `check` name outside it can
# never discharge a postcondition or admit a certificate.
SUPPORTED_CHECKS = ("interval_arithmetic",)


def canon_bytes(value: Any) -> bytes:
    """Canonical JSON bytes of a plain Python structure — nulls stripped
    first (the canonical grammar carries only present fields)."""

    def strip(v: Any) -> Any:
        if isinstance(v, dict):
            return {k: strip(x) for k, x in v.items() if x is not None}
        if isinstance(v, list):
            return [strip(x) for x in v]
        return v

    return canonicalize_json(
        json.dumps(strip(value), ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    )


def canon_sha256(value: Any) -> str:
    return sha256_bytes(canon_bytes(value))


def _hex(digest_with_prefix: str) -> str:
    return digest_with_prefix.split(":", 1)[1]


def valid_sha256_digest(digest) -> bool:
    """`sha256:` followed by exactly 64 lowercase hexadecimal characters."""
    return (
        isinstance(digest, str)
        and digest.startswith("sha256:")
        and len(digest) == 7 + 64
        and all(c in "0123456789abcdef" for c in digest[7:])
    )


def load_json(path: str) -> Any:
    return json.loads(Path(path).read_bytes().decode("utf-8"))


def canonically_admitted(path: str) -> Optional[bytes]:
    """The authoritative-JSON admission gate: duplicate keys, floats, nulls,
    and non-NFC strings make the document malformed — `json.loads` accepts
    them silently, the canonical grammar does not."""
    try:
        return canonicalize_json(Path(path).read_bytes())
    except Exception:
        return None


# ---------------------------------------------------------------------------
# Semantic projection (spec §10.1) — ported from the declared role grammar:
# annotation fields drop inside declared structs, declared sets sort by the
# canonical bytes of each *projected* element, declared sequences keep order,
# user-defined map keys are semantic identifiers and always preserved.
# ---------------------------------------------------------------------------

NODE = ("node",)
ANNOTATION = ("annotation",)
RELATION_KEYS = ("map", NODE)


def _struct(*fields):
    return ("struct", list(fields))


def _map(role):
    return ("map", role)


def _set(role):
    return ("set", role)


def _seq(role):
    return ("seq", role)


PROVENANCE = _struct(
    ("kind", NODE), ("party", NODE), ("edge", NODE), ("digest", NODE),
    ("check", NODE), ("statement", ANNOTATION), ("note", ANNOTATION),
    ("reason", ANNOTATION),
)
VALUE = _struct(
    ("kind", NODE), ("value", NODE), ("lower", NODE), ("upper", NODE),
    ("unit", NODE),
)
SLOT = _struct(
    ("quantity_kind", NODE), ("claim", NODE), ("geometry", NODE),
    ("scenario", NODE), ("material", NODE),
)
SCOPED_ASSERTION = _struct(("proposition", NODE), ("at", RELATION_KEYS))
KIND_PRODUCT = _struct(
    ("lhs_kind", NODE), ("rhs_kind", NODE), ("result_kind", NODE),
    ("result_unit", NODE), ("label", ANNOTATION), ("note", ANNOTATION),
    ("reason", ANNOTATION),
)
PROPOSITION_DECL = _struct(("params", _set(NODE)), ("gloss", ANNOTATION))
QUANTITY_KIND_DECL = _struct(("canonical_unit", NODE), ("gloss", ANNOTATION))
REQUIRES = _struct(
    ("kind", NODE), ("domain", NODE), ("within", NODE), ("subject", NODE),
    ("scope", NODE), ("over", _set(NODE)), ("label", ANNOTATION),
    ("note", ANNOTATION), ("reason", ANNOTATION),
)
ENSURES = _struct(
    ("kind", NODE), ("expression", NODE), ("check", NODE),
    ("label", ANNOTATION), ("note", ANNOTATION), ("reason", ANNOTATION),
)
PRODUCES = _struct(("quantity_kind", NODE), ("unit", NODE))
IMPLEMENTATION = _struct(
    ("kind", NODE), ("executable", NODE), ("produces", PRODUCES),
    ("body", NODE), ("label", ANNOTATION), ("note", ANNOTATION),
    ("reason", ANNOTATION),
)
METHOD = _struct(
    ("id", NODE), ("label", ANNOTATION), ("note", ANNOTATION),
    ("reason", ANNOTATION), ("variables", _map(NODE)),
    ("inputs", _map(SLOT)), ("output", SLOT), ("projects", _set(NODE)),
    ("requires", _set(REQUIRES)), ("ensures", _set(ENSURES)),
    ("assumes", _set(NODE)), ("effects", _set(NODE)),
    ("implementation", IMPLEMENTATION),
)
LIBRARY_HEADER = _struct(("name", NODE), ("revision", NODE))
LIBRARY = _struct(
    ("schema_version", NODE), ("profile", NODE), ("library", LIBRARY_HEADER),
    ("title", ANNOTATION), ("description", ANNOTATION),
    ("quantity_kinds", _map(QUANTITY_KIND_DECL)),
    ("kind_products", _set(KIND_PRODUCT)),
    ("propositions", _map(PROPOSITION_DECL)),
    ("methods", _set(METHOD)),
)
GEOMETRY_ENTITY = _struct(
    ("source", PROVENANCE), ("label", ANNOTATION), ("note", ANNOTATION),
    ("reason", ANNOTATION),
)
DOMAIN = _struct(
    ("quantity_kind", NODE), ("unit", NODE), ("lower", NODE), ("upper", NODE),
)
SCENARIO_ENTITY = _struct(
    ("scope", NODE), ("operating_domain", DOMAIN), ("source", PROVENANCE),
    ("label", ANNOTATION), ("note", ANNOTATION), ("reason", ANNOTATION),
)
INTERVAL = _struct(("unit", NODE), ("lower", NODE), ("upper", NODE))
MATERIAL_ENTITY = _struct(
    ("applicability", _map(INTERVAL)), ("source", PROVENANCE),
    ("label", ANNOTATION), ("note", ANNOTATION), ("reason", ANNOTATION),
)
ENTITIES = _struct(
    ("geometries", _map(GEOMETRY_ENTITY)),
    ("scenarios", _map(SCENARIO_ENTITY)),
    ("materials", _map(MATERIAL_ENTITY)),
)
BINDING = _struct(
    ("state", NODE), ("value", VALUE), ("source", PROVENANCE),
    ("party", NODE), ("reason", ANNOTATION),
)
INPUT = _struct(
    ("id", NODE), ("type", SLOT), ("binding", BINDING),
    ("label", ANNOTATION), ("note", ANNOTATION), ("reason", ANNOTATION),
)
ASSUMPTION = _struct(
    ("id", NODE), ("asserts", NODE), ("denies", NODE), ("at", RELATION_KEYS),
    ("source", PROVENANCE), ("label", ANNOTATION), ("note", ANNOTATION),
    ("reason", ANNOTATION),
)
PREMISE_ARGUMENTS = _struct(("over", _set(NODE)),)
PREMISE = _struct(
    ("id", NODE), ("proposition", NODE), ("arguments", PREMISE_ARGUMENTS),
    ("at", RELATION_KEYS), ("established_by", PROVENANCE),
    ("assumptions", _set(SCOPED_ASSERTION)), ("label", ANNOTATION),
    ("note", ANNOTATION), ("reason", ANNOTATION),
)
ARG_REF = _struct(("ref", NODE),)
INFER = _struct(("rule", NODE), ("arguments", _seq(ARG_REF)))
IMPORT = _struct(
    ("type", SLOT), ("value", VALUE),
    ("assumptions", _set(SCOPED_ASSERTION)), ("source", PROVENANCE),
)
STEP = _struct(
    ("bind", NODE), ("apply", NODE), ("arguments", _map(ARG_REF)),
    ("infer", INFER), ("import", IMPORT), ("hole", SLOT), ("goal", SLOT),
    ("label", ANNOTATION), ("note", ANNOTATION), ("reason", ANNOTATION),
)
REQUIREMENT = _struct(
    ("id", NODE), ("subject", ARG_REF), ("comparison", NODE),
    ("quantity_kind", NODE), ("limit", VALUE), ("scope", NODE),
    ("scenario", NODE), ("label", ANNOTATION), ("note", ANNOTATION),
    ("reason", ANNOTATION),
)
PROGRAM_LIBRARY_PIN = _struct(
    ("name", NODE), ("revision", NODE), ("semantic_sha256", NODE),
)
PROGRAM = _struct(
    ("schema_version", NODE), ("profile", NODE), ("id", NODE),
    ("title", ANNOTATION), ("description", ANNOTATION),
    ("library", PROGRAM_LIBRARY_PIN), ("entities", ENTITIES),
    ("inputs", _set(INPUT)), ("assumptions", _set(ASSUMPTION)),
    ("premises", _set(PREMISE)), ("body", _seq(STEP)),
    ("requirements", _set(REQUIREMENT)),
)


class ProjectionError(Exception):
    pass


def project(value: Any, role: tuple, path: str) -> Any:
    kind = role[0]
    if kind == "node":
        return value
    if kind == "struct":
        if not isinstance(value, dict):
            raise ProjectionError(f"shape mismatch at {path}: expected object")
        out = {}
        declared = set()
        for name, field_role in role[1]:
            declared.add(name)
            if name not in value or field_role == ANNOTATION:
                continue
            out[name] = project(value[name], field_role, f"{path}/{name}")
        for key in value:
            if key not in declared:
                raise ProjectionError(f"undeclared field `{key}` at {path}")
        return out
    if kind == "map":
        if not isinstance(value, dict):
            raise ProjectionError(f"shape mismatch at {path}: expected object")
        return {k: project(v, role[1], f"{path}/{k}") for k, v in value.items()}
    if kind in ("set", "seq"):
        if not isinstance(value, list):
            raise ProjectionError(f"shape mismatch at {path}: expected array")
        items = [project(v, role[1], f"{path}/{i}") for i, v in enumerate(value)]
        if kind == "set":
            items.sort(key=lambda e: canon_bytes(e))
        return items
    raise ProjectionError(f"shape mismatch at {path}")


def semantic_sha256(doc: dict, document: str) -> str:
    role = PROGRAM if document == "program" else LIBRARY
    return "sha256:" + _hex(sha256_bytes(canon_bytes(project(doc, role, ""))))


def canonical_order(items: list, role: tuple) -> list:
    """`canonical_order`: declared sets iterate by each element's projected
    canonical bytes (annotation-blind); on projection failure the raw
    serialized bytes keep the key content-addressed. Returns the ORIGINAL
    source indices alongside — witness/finding iteration depends on them."""

    def key(pair):
        try:
            return canon_bytes(project(pair[1], role, ""))
        except Exception:
            return canon_bytes(pair[1])

    return sorted(enumerate(items), key=key)


def canonical_sort(items: list, role: tuple) -> list:
    return [item for _, item in canonical_order(items, role)]


# ---------------------------------------------------------------------------
# Numbers and values
# ---------------------------------------------------------------------------


def exact_number(text) -> Fraction:
    """The `ExactNumber` grammar: canonical rationals `p/q`, else the
    canonical decimal form — leading `+`, leading zeroes, trailing decimal
    zeroes, exponent notation, and unreduced rationals are all rejected."""
    if not isinstance(text, str):
        raise ValueError(f"not an exact-number literal: {text!r}")
    return read_authoritative_exact(text)


def canonical_rational(value: Fraction) -> str:
    if value.denominator == 1:
        return str(value.numerator)
    return f"{value.numerator}/{value.denominator}"


CLAIMS = ("exact", "enclosure", "nominal")


def claim_of(text: Optional[str]) -> str:
    """`ClaimModel::parse(..).unwrap_or(Nominal)` — an unrecognized declared
    claim parses to nominal (the malformed finding is raised separately)."""
    return text if text in CLAIMS else "nominal"


def claim_satisfies(claim: str, required: str) -> bool:
    return (claim, required) in {
        ("exact", "exact"),
        ("exact", "enclosure"),
        ("enclosure", "enclosure"),
        ("nominal", "nominal"),
    }


@dataclass
class Numeric:
    """exact → a point; enclosure → [lower, upper]."""

    exact: Optional[Fraction] = None
    lower: Optional[Fraction] = None
    upper: Optional[Fraction] = None

    def bounds(self) -> tuple[Fraction, Fraction]:
        if self.exact is not None:
            return self.exact, self.exact
        assert self.lower is not None and self.upper is not None
        return self.lower, self.upper

    def __eq__(self, other):
        if not isinstance(other, Numeric):
            return NotImplemented
        return (
            self.exact == other.exact
            and self.lower == other.lower
            and self.upper == other.upper
        )


@dataclass
class QuantityType:
    quantity_kind: str
    claim: str
    relations: dict = field(default_factory=dict)


@dataclass
class SemVal:
    ty: QuantityType
    unit: str
    value: Optional[Numeric]
    state: str  # "established" | "declared" | "unestablished"
    assumptions: list = field(default_factory=list)
    edges: set = field(default_factory=set)

    def value_text(self) -> Optional[str]:
        if self.value is None:
            return None
        if self.value.exact is not None:
            return canonical_rational(self.value.exact)
        return (
            f"[{canonical_rational(self.value.lower)},"
            f" {canonical_rational(self.value.upper)}]"
        )


@dataclass(frozen=True)
class ScopedProposition:
    proposition: str
    at: tuple  # sorted (relation, entity) pairs

    @staticmethod
    def of(proposition: str, at: dict) -> "ScopedProposition":
        return ScopedProposition(proposition, tuple(sorted(at.items())))

    def render(self) -> str:
        if not self.at:
            return self.proposition
        entities = ", ".join(entity for _, entity in self.at)
        return f"{self.proposition}@({entities})"


def render_subject(subject: SemVal, with_unit: bool = True) -> str:
    if subject.value is None:
        base = f"{subject.ty.claim} <unestablished>"
    elif subject.value.exact is not None:
        base = f"{subject.ty.claim} {canonical_rational(subject.value.exact)}"
    else:
        base = (
            f"{subject.ty.claim} [{canonical_rational(subject.value.lower)},"
            f" {canonical_rational(subject.value.upper)}]"
        )
    return f"{base} {subject.unit}" if with_unit and subject.unit else base


def value_decl_of(value: SemVal) -> Optional[dict]:
    """The `ValueDecl` a semantic value stages to — the staged-bytes recipe."""
    if value.value is None:
        return None
    decl = {"kind": value.ty.claim, "unit": value.unit}
    if value.value.exact is not None:
        decl["value"] = canonical_rational(value.value.exact)
    else:
        decl["lower"] = canonical_rational(value.value.lower)
        decl["upper"] = canonical_rational(value.value.upper)
    return decl


def staged_input_sha256(value: SemVal) -> Optional[str]:
    decl = value_decl_of(value)
    if decl is None:
        return None
    return "sha256:" + _hex(sha256_bytes(canon_bytes(decl)))


def admit_value(decl: dict, ty: QuantityType, library_kinds: dict) -> Optional[Numeric]:
    """Typed admission of a `ValueDecl`: the payload's claim must satisfy the
    declared claim and the unit must be the kind's canonical unit."""
    claim = decl.get("kind")
    if claim not in CLAIMS or not claim_satisfies(claim, ty.claim):
        return None
    canonical_unit = library_kinds.get(ty.quantity_kind, {}).get("canonical_unit")
    if canonical_unit is not None and decl.get("unit") != canonical_unit:
        return None
    try:
        if claim in ("exact", "nominal"):
            text = decl.get("value")
            if text is None:
                return None
            return Numeric(exact=exact_number(text))
        lower_s, upper_s = decl.get("lower"), decl.get("upper")
        if lower_s is None or upper_s is None:
            return None
        lower, upper = exact_number(lower_s), exact_number(upper_s)
        if lower > upper:
            return None
        return Numeric(lower=lower, upper=upper)
    except Exception:
        return None


# ---------------------------------------------------------------------------
# Expression replay — `output = <expr>` over exact-rational enclosures
# ---------------------------------------------------------------------------

EXPR_RULES = {"interval.add": "add", "interval.sub": "sub", "interval.mul": "mul"}


class EvalFailure(Exception):
    """Carries the finding kind the failure maps to (`budget`/`malformed`/
    `unsupported`/`type_mismatch`) — the same taxonomy `parse_failure` and
    `rule_failure` use when they publish the failure."""

    def __init__(self, kind: str, detail: str = ""):
        super().__init__(detail)
        self.kind = kind


MAX_EXPR_DEPTH = 128
MAX_EXPR_TERMS = 4096


def parse_expression(text: str):
    """`expr := term (('+'|'-') term)*`, `term := factor ('*' factor)*`,
    `factor := '(' expr ')' | name | name '(' args ')'`. Depth and term
    counts are bounded — a document beyond the profile's limits is refused,
    not recursed on."""
    pos = 0
    n = len(text)
    depth = 0
    terms = [0]

    def skip_ws():
        nonlocal pos
        # `u8::is_ascii_whitespace` — space, tab, LF, FF, CR (not VT, never
        # Unicode whitespace).
        while pos < n and text[pos] in " \t\n\r\x0c":
            pos += 1

    def term():
        terms[0] += 1
        if terms[0] > MAX_EXPR_TERMS:
            raise EvalFailure("budget", "expression exceeds the term bound")

    def parse_expr():
        nonlocal pos
        left = parse_term()
        while True:
            skip_ws()
            if pos < n and text[pos] in "+-":
                op = "add" if text[pos] == "+" else "sub"
                pos += 1
                left = ("call", op, [left, parse_term()])
            else:
                return left

    def parse_term():
        nonlocal pos
        left = parse_factor()
        while True:
            skip_ws()
            if pos < n and text[pos] == "*":
                pos += 1
                left = ("call", "mul", [left, parse_factor()])
            else:
                return left

    def parse_factor():
        nonlocal pos, depth
        skip_ws()
        if depth >= MAX_EXPR_DEPTH:
            raise EvalFailure("budget", "expression nesting exceeds the depth bound")
        if pos >= n:
            raise EvalFailure("malformed", "expected an identifier or `(`")
        c = text[pos]
        if c == "(":
            pos += 1
            depth += 1
            inner = parse_expr()
            depth -= 1
            skip_ws()
            if pos >= n or text[pos] != ")":
                raise EvalFailure("malformed", "expected `)`")
            pos += 1
            return inner
        if c.isascii() and (c.isalpha() or c == "_"):
            start = pos
            while pos < n and text[pos].isascii() and (
                text[pos].isalnum() or text[pos] in "_-.@/:"
            ):
                pos += 1
            name = text[start:pos]
            skip_ws()
            if pos < n and text[pos] == "(":
                pos += 1
                depth += 1
                args = parse_arguments()
                depth -= 1
                if name not in EXPR_RULES:
                    raise EvalFailure("unsupported", f"unknown primitive rule `{name}`")
                term()
                return ("call", EXPR_RULES[name], args)
            term()
            return ("name", name)
        raise EvalFailure("malformed", "expected an identifier or `(`")

    def parse_arguments():
        nonlocal pos
        args = []
        while True:
            skip_ws()
            if pos < n and text[pos] == ")":
                pos += 1
                return args
            args.append(parse_expr())
            skip_ws()
            if pos < n and text[pos] == ",":
                pos += 1
                continue
            if pos < n and text[pos] == ")":
                pos += 1
                return args
            raise EvalFailure("malformed", "expected `,` or `)`")

    expression = parse_expr()
    skip_ws()
    if pos != n:
        raise EvalFailure("malformed", f"trailing input at byte {pos} of `{text}`")
    return expression


def parse_ensures(expression: str):
    if "=" not in expression:
        raise EvalFailure("malformed", f"ensures expression `{expression}` has no `=`")
    lhs, rhs = expression.split("=", 1)
    if lhs.strip() != "output":
        raise EvalFailure(
            "malformed",
            f"ensures expression `{expression}` must bind `output` on the left",
        )
    return parse_expression(rhs.strip())


def eval_expression(expr, operands: dict, kind_products: dict, _depth: int = 0) -> SemVal:
    """interval.add/sub/mul — the two-operand primitive rules with exact /
    enclosure arithmetic, quantity-kind checks, and relation merging."""
    if _depth >= MAX_EXPR_DEPTH:
        raise EvalFailure("budget", "expression evaluation exceeds the depth bound")
    kind = expr[0]
    if kind == "name":
        name = expr[1]
        if name not in operands:
            raise EvalFailure("unsupported", f"unbound operand `{name}`")
        return operands[name]
    _, rule, args = expr
    if len(args) != 2:
        raise EvalFailure("unsupported", f"{rule} takes 2 operands; {len(args)} supplied")
    left = eval_expression(args[0], operands, kind_products, _depth + 1)
    right = eval_expression(args[1], operands, kind_products, _depth + 1)
    if left.ty.claim == "nominal" or right.ty.claim == "nominal":
        raise EvalFailure("unsupported", "nominal operand")
    if rule in ("add", "sub"):
        if left.ty.quantity_kind != right.ty.quantity_kind:
            raise EvalFailure("type_mismatch", "kind mismatch")
        quantity_kind, unit = left.ty.quantity_kind, left.unit
    else:
        pair = kind_products.get((left.ty.quantity_kind, right.ty.quantity_kind))
        if pair is None:
            raise EvalFailure("unsupported", "no kind_products row")
        quantity_kind, unit = pair
    relations = dict(left.ty.relations)
    for key, entity in right.ty.relations.items():
        if relations.get(key, entity) != entity:
            raise EvalFailure("type_mismatch", "relation conflict")
        relations[key] = entity
    claim = "exact" if left.ty.claim == right.ty.claim == "exact" else "enclosure"
    a = None if left.value is None else left.value.bounds()
    b = None if right.value is None else right.value.bounds()
    numeric = None
    if a is not None and b is not None:
        if rule == "add":
            numeric = (a[0] + b[0], a[1] + b[1])
        elif rule == "sub":
            numeric = (a[0] - b[1], a[1] - b[0])
        else:
            products = [a[i] * b[j] for i in (0, 1) for j in (0, 1)]
            numeric = (min(products), max(products))
    value = None
    if numeric is not None:
        if claim == "exact" and numeric[0] == numeric[1]:
            value = Numeric(exact=numeric[0])
        else:
            value = Numeric(lower=numeric[0], upper=numeric[1])
    # The state join is over operand *states*, not value presence: two
    # established operands establish; an unestablished operand poisons; a
    # declared operand keeps the result declared — the value exists but the
    # observation that would establish it has not arrived.
    if left.state == "established" and right.state == "established":
        state = "established"
    elif left.state == "unestablished" or right.state == "unestablished":
        state = "unestablished"
    else:
        state = "declared"
    return SemVal(
        ty=QuantityType(quantity_kind, claim, relations),
        unit=unit,
        value=value,
        state=state,
        assumptions=merge_assumptions([], [left.assumptions, right.assumptions]),
        edges=left.edges | right.edges,
    )


def merge_assumptions(base: list, others) -> list:
    out = list(base)
    for seq in others:
        for item in seq:
            if item not in out:
                out.append(item)
    return out


# ---------------------------------------------------------------------------
# The replay analyzer — enough of `analyze` to re-derive eval claims
# ---------------------------------------------------------------------------


def slot_relations(slot: dict) -> dict:
    return {
        key: slot[key]
        for key in ("geometry", "scenario", "material")
        if key in slot
    }


class Replay:
    """Replays program + library (+ admitted observations) to the semantic
    spine an evaluation record claims: binding states, runtime obligation
    outcomes, requirement verdicts."""

    def __init__(self, program: dict, library: dict, lifecycle=None):
        self.program = program
        self.library = library
        self.lifecycle = lifecycle or []  # [(key, state)] — caller material
        self.kinds = library.get("quantity_kinds", {})
        self.kind_products = {
            (p["lhs_kind"], p["rhs_kind"]): (p["result_kind"], p["result_unit"])
            for p in library.get("kind_products", [])
        }
        self.methods = {m["id"]: m for m in library.get("methods", [])}
        self.propositions = library.get("propositions", {})
        self.env: dict[str, SemVal] = {}
        self.blocked: dict[str, str] = {}
        self.deps: dict[str, list[str]] = {}
        self.edges: dict[str, set] = {}
        # obligation id → (kind, state) — both replay-derived and
        # compared against the record (`rule` + `state` fields).
        self.obligation_states: dict[str, tuple] = {}
        self.observed_values: dict[int, Optional[Numeric]] = {}
        self.observed_sites: set[int] = set()
        self.contradicted: set = set()
        self.site_map: dict[int, dict] = {}
        # at → rejection reason — drives the `observations[]` outcome rows.
        self.rejected_observations: dict[str, str] = {}
        self.expected_plan_sha256: Optional[str] = None
        self.denied: set = set()
        # Emitted as the analyzer's eval_events: `observe` rows per binding
        # attempt and runtime obligation, `discharge` per external ensures.
        self.events: list[tuple] = []
        # Program-scoped finding kinds emitted during the run — the
        # admission gate needs them: a document publishing any
        # ADMISSION_KINDS finding carries no semantic identity.
        self.finding_kinds: set[str] = set()

    def pfind(self, kind: str):
        self.finding_kinds.add(kind)

    def check_entity(self, relation: str, entity):
        known = {
            "geometry": (self.program.get("entities") or {}).get("geometries") or {},
            "scenario": (self.program.get("entities") or {}).get("scenarios") or {},
            "material": (self.program.get("entities") or {}).get("materials") or {},
        }.get(relation)
        if known is not None and entity not in known:
            self.pfind("undeclared_entity")

    def type_of(self, slot: dict) -> QuantityType:
        claim = claim_of(slot.get("claim"))
        if slot.get("claim") not in CLAIMS:
            self.pfind("malformed")
        if slot.get("quantity_kind") not in self.kinds:
            self.pfind("malformed")
        relations = {}
        for key, entity in slot_relations(slot).items():
            self.check_entity(key, entity)
            relations[key] = entity
        return QuantityType(slot.get("quantity_kind"), claim, relations)

    def admit(self, decl: dict, ty: QuantityType) -> Optional[Numeric]:
        """`admit_value` with its failure kinds: unknown kind → malformed,
        unsatisfying claim → type_mismatch, unit/payload failures →
        malformed."""
        claim = decl.get("kind")
        if claim not in CLAIMS:
            self.pfind("malformed")
            return None
        if not claim_satisfies(claim, ty.claim):
            self.pfind("type_mismatch")
            return None
        value = admit_value(decl, ty, self.kinds)
        if value is None:
            self.pfind("malformed")
        return value

    # -- premise admissibility (spec §8/A3) --------------------------------

    def compute_static_edges(self):
        """Every binding's recorded source edges: the union of its operands'
        edges (and an import's own), computed before the body runs."""
        for step in self.program.get("body", []):
            edges = set()
            import_decl = step.get("import")
            if import_decl and (import_decl.get("source") or {}).get("edge"):
                edges.add(import_decl["source"]["edge"])
            for ref in self.step_references(step):
                edges |= self.edges.get(ref, set())
            if edges:
                self.edges[step["bind"]] = edges

    def step_references(self, step: dict) -> list:
        """`step_references`: the step's argument map in canonical key order
        (all supplied slots — including undeclared ones — carry provenance
        edges), then `infer` arguments in position order."""
        return [ref for _, ref in _step_references(step)]

    def check_provenance(self, source: dict) -> bool:
        """`provenance_admissible` — emits the same program-scoped finding
        kinds the reference does (malformed for shape, `unsupported` for an
        unreplayable certificate check), and returns admissibility."""
        kind = source.get("kind")
        if kind in ("assertion", "declared"):
            if source.get("party") is None:
                self.pfind("malformed")
                return False
            return True
        if kind == "certificate":
            ok = True
            if not valid_sha256_digest(source.get("digest", "")):
                self.pfind("malformed")
                ok = False
            check = source.get("check")
            if check is None:
                self.pfind("malformed")
                ok = False
            elif check not in SUPPORTED_CHECKS:
                self.pfind("unsupported")
                ok = False
            return ok
        self.pfind("malformed")
        return False

    def admissible_witnesses(self) -> dict:
        """(proposition, at-map-key) → admissible premise indices: needs
        an admissible `established_by`, not denied, not in a cyclic
        assumption group, and `provenance_disjoint` must not cover operands
        sharing static edges."""
        premises = self.program.get("premises", [])
        # Program assumptions both asserted and denied at one scope are
        # contradicted — dependent bindings are blocked after discharge.
        asserts, denies = set(), set()
        for a in self.program.get("assumptions", []):
            at = tuple(sorted(a.get("at", {}).items()))
            if a.get("asserts"):
                asserts.add((a["asserts"], at))
            if a.get("denies"):
                denies.add((a["denies"], at))
        self.contradicted = asserts & denies
        denied = denies
        inadmissible = set()
        for i, p in enumerate(premises):
            if not p.get("established_by") or not self.check_provenance(
                p["established_by"]
            ):
                inadmissible.add(i)
            key = (p["proposition"], tuple(sorted(p.get("at", {}).items())))
            if key in denied:
                inadmissible.add(i)
        prop_map: dict = {}
        # Canonical order — the witness lists' order lands in the discharged
        # support's assumption order; source order must not shape it.
        for i, p in canonical_order(premises, PREMISE):
            prop_map.setdefault(
                (p["proposition"], tuple(sorted(p.get("at", {}).items()))), []
            ).append(i)
        dep_edges = [
            {
                j
                for s in p.get("assumptions", [])
                for j in prop_map.get(
                    (s["proposition"], tuple(sorted(s.get("at", {}).items()))), []
                )
            }
            for p in premises
        ]
        # Witness chains are depth-bounded: a traversal that does not
        # terminate inside the bound emits `budget` (an admission finding)
        # and contributes an empty reach set — never an unbounded search.
        reach = []
        for i in range(len(premises)):
            seen, stack, steps = set(), list(dep_edges[i]), 0
            exceeded = False
            while stack:
                node = stack.pop()
                steps += 1
                if steps > MAX_WITNESS_DEPTH * MAX_WITNESS_DEPTH:
                    exceeded = True
                    break
                if node not in seen:
                    seen.add(node)
                    stack.extend(dep_edges[node])
            if exceeded:
                self.pfind("budget")
                seen = set()
            reach.append(seen)
        cyclic = [i for i in range(len(premises)) if i in reach[i]]
        grouped = set()
        for i in cyclic:
            if i not in grouped:
                members = [j for j in cyclic if j in reach[i] and i in reach[j]]
                grouped.update(members)
                inadmissible.update(members)
        # provenance_disjoint: static edges over `over` operands must be
        # disjoint — a shared edge makes the premise inadmissible.
        for i, p in enumerate(premises):
            if p["proposition"] == "provenance_disjoint" and i not in inadmissible:
                bound = (p.get("arguments") or {}).get("over") or []
                sets = [self.edges.get(b) for b in bound]
                if all(sets) and any(
                    sets[a] & sets[b]
                    for a in range(len(sets))
                    for b in range(a + 1, len(sets))
                ):
                    inadmissible.add(i)
        self.inadmissible_premises = inadmissible
        return {
            key: [i for i in idxs if i not in inadmissible]
            for key, idxs in prop_map.items()
            if any(i not in inadmissible for i in idxs)
        }

    def _premise_inadmissible(self, index: int) -> bool:
        return index in getattr(self, "inadmissible_premises", set())

    def effective_edges(self, name: str, admissible, verifying: set) -> set:
        edges = set(self.edges.get(name, set()))
        value = self.env.get(name)
        if value is None:
            return edges
        edges |= value.edges
        residual, cone = self._support_cone_edges(
            value.assumptions, admissible, verifying
        )
        edges |= cone
        return edges

    def _support_cone_edges(self, roots: list, admissible, verifying: set):
        residual, edges = [], set()
        seen = set(roots)
        for root in roots:
            stack = [root]
            while stack:
                prop = stack.pop()
                witnesses = admissible.get((prop.proposition, prop.at), [])
                if not witnesses:
                    residual.append(prop)
                    continue
                fired = False
                for i in witnesses:
                    premise = self.program["premises"][i]
                    if premise["proposition"] == "provenance_disjoint":
                        if i in verifying:
                            continue
                        bound = (premise.get("arguments") or {}).get("over") or []
                        if any(b not in self.env for b in bound):
                            continue
                        verifying.add(i)
                        sets = [
                            self.effective_edges(b, admissible, verifying)
                            for b in bound
                        ]
                        verifying.discard(i)
                        if all(sets) and any(
                            sets[a] & sets[b]
                            for a in range(len(sets))
                            for b in range(a + 1, len(sets))
                        ):
                            continue
                    fired = True
                    edge = (premise.get("established_by") or {}).get("edge")
                    if edge:
                        edges.add(edge)
                    for s in premise.get("assumptions", []):
                        nxt = ScopedProposition.of(
                            s["proposition"], s.get("at", {})
                        )
                        if nxt != prop and nxt not in seen:
                            seen.add(nxt)
                            stack.append(nxt)
                if not fired:
                    residual.append(prop)
        return residual, edges

    def discharge_all(self, admissible):
        for name, value in self.env.items():
            residual, cone = self._support_cone_edges(
                value.assumptions, admissible, set()
            )
            value.assumptions = residual
            value.edges |= cone
            self.edges.setdefault(name, set()).update(value.edges)

    # -- body replay -------------------------------------------------------

    def run(
        self,
        evaluating: bool = False,
        site_map: Optional[dict] = None,
        expected_plan_sha256=None,
    ):
        self.site_map = site_map or {}
        self.expected_plan_sha256 = expected_plan_sha256
        seen = set()
        for inp in canonical_sort(self.program.get("inputs", []), INPUT):
            # A duplicate input id keeps the FIRST canonical binding — the
            # duplicate was already an admission finding.
            if inp["id"] in seen:
                continue
            seen.add(inp["id"])
            ty = self.type_of(inp["type"])
            binding = inp.get("binding")
            value = None
            state = "unestablished"
            edges = set()
            if binding is None:
                self.pfind("hole")
            else:
                # Provenance is checked before the state arm — a malformed
                # `source` on an `unavailable` binding still counts.
                if binding.get("source") is not None:
                    self.check_provenance(binding["source"])
                state_v = binding.get("state")
                if state_v == "bound":
                    decl = binding.get("value")
                    if decl is None:
                        self.pfind("malformed")
                    else:
                        value = self.admit(decl, ty)
                        if value is not None:
                            state = "established"
                            # The source edge enters `edges` only when the
                            # payload itself admits — a refused value
                            # carries no provenance.
                            edge = (binding.get("source") or {}).get("edge")
                            if edge:
                                edges.add(edge)
                elif state_v != "unavailable":
                    self.pfind("malformed")
            unit = ((binding or {}).get("value") or {}).get("unit", "")
            self.env[inp["id"]] = SemVal(ty, unit, value, state, [], edges)
            if edges:
                self.edges[inp["id"]] = set(edges)
        self.compute_static_edges()
        admissible = self.admissible_witnesses()
        for index, step in enumerate(self.program.get("body", [])):
            self.step(index, step, evaluating, admissible)
        self.discharge_all(admissible)
        # A binding whose residual assumptions contain a contradicted
        # proposition can ground no verdict — every dependent use is
        # blocked (runs after discharge; the check reads residuals).
        for name, value in self.env.items():
            if any(
                (a.proposition, a.at) in self.contradicted
                for a in value.assumptions
            ):
                self.blocked[name] = "not_evaluated.contradiction"

    def step(self, index: int, step: dict, evaluating: bool, admissible: dict):
        bind = step["bind"]
        kinds = sum(
            1 for k in ("apply", "infer", "import", "hole", "goal") if k in step
        )
        if kinds != 1:
            # `step` must carry exactly one operation — a malformed step
            # produces no binding rather than dispatching on first match.
            self.pfind("malformed")
            return
        if "apply" in step:
            self.apply(index, bind, step["apply"], step.get("arguments") or {},
                       evaluating, admissible)
        elif "infer" in step:
            self.infer(index, bind, step["infer"])
        elif "import" in step:
            self.do_import(index, bind, step["import"])
        elif "hole" in step or "goal" in step:
            slot = step.get("hole") or step.get("goal")
            ty = self.type_of(slot)
            if "hole" in step:
                self.pfind("hole")
            else:
                needed = set(ty.relations)
                # A candidate's output must satisfy the goal's claim — an
                # unparseable claim excludes the method rather than
                # defaulting to nominal.
                candidates = [
                    m
                    for m in self.methods.values()
                    if m.get("output", {}).get("quantity_kind")
                    == ty.quantity_kind
                    and (
                        (m.get("output") or {}).get("claim") in CLAIMS
                        and claim_satisfies(
                            (m.get("output") or {}).get("claim"), ty.claim
                        )
                    )
                    and needed
                    <= {k for k, _ in slot_relations(m.get("output") or {})}
                ]
                self.pfind("ambiguity" if len(candidates) > 1 else "hole")
            self.env[bind] = SemVal(ty, "", None, "unestablished")

    def propagated_block(self, references) -> Optional[str]:
        for r in references:
            if r in self.blocked:
                return self.blocked[r]
        return None

    def poison(self, bind: str, ty: QuantityType):
        self.env[bind] = SemVal(ty, "", None, "unestablished", [], set())

    def infer(self, index: int, bind: str, infer: dict):
        rule = EXPR_RULES.get(infer.get("rule"))
        if rule is None:
            self.pfind("unsupported")
            return
        references = [a["ref"] for a in infer.get("arguments", [])]
        self.deps[bind] = references
        inherited = self.propagated_block(references)
        operands = {}
        for r in references:
            v = self.env.get(r)
            if v is None:
                self.pfind("malformed")
                return
            operands[r] = v
        expr = ("call", rule, [("name", r) for r in references])
        try:
            value = eval_expression(expr, operands, self.kind_products)
        except EvalFailure as failure:
            self.pfind(failure.kind)
            return
        if inherited:
            self.blocked[bind] = inherited
        self.env[bind] = value

    def do_import(self, index: int, bind: str, import_decl: dict):
        ty = self.type_of(import_decl["type"])
        value = self.admit(import_decl["value"], ty)
        malformed = value is None
        assumptions = []
        # `assumptions` present-vs-absent is semantic: absence is not an
        # empty declaration, it is a malformed import.
        if "assumptions" not in import_decl:
            malformed = True
            self.pfind("malformed_import")
        else:
            assumptions = [
                ScopedProposition.of(s["proposition"], s.get("at", {}))
                for s in canonical_sort(
                    import_decl.get("assumptions"), SCOPED_ASSERTION
                )
            ]
        src = import_decl.get("source")
        if src is not None:
            if src.get("kind") not in ("assertion", "certificate", "declared"):
                self.pfind("malformed_import")
                malformed = True
            elif src["kind"] == "certificate":
                # A certificate without a replayable payload is
                # `unsupported` — the import refuses to establish, but the
                # program still admits (this kind is not an admission kind).
                if not (
                    valid_sha256_digest(src.get("digest", ""))
                    and src.get("check") in SUPPORTED_CHECKS
                ):
                    self.pfind("unsupported")
                    malformed = True
            elif src.get("party") is None:
                self.pfind("malformed_import")
                malformed = True
        edges = set()
        if malformed:
            self.env[bind] = SemVal(ty, "", None, "unestablished")
            return
        if src and src.get("edge"):
            edges.add(src["edge"])
        self.deps[bind] = []
        self.env[bind] = SemVal(
            ty,
            import_decl["value"].get("unit", ""),
            value,
            "established",
            assumptions,
            edges,
        )
        if edges:
            self.edges[bind] = set(edges)

    LIFECYCLE_STATES = ("active", "superseded", "expired", "withdrawn")

    def check_lifecycle(self, index: int, method_id: str) -> str:
        """Per-application lifecycle gate: `library:<name>@<rev>` and
        `method:<lib>/<method>@<rev>` keys. Distinct states on one key are a
        conflict; `expired`/`withdrawn` refuse; `superseded` is a notice."""
        lib = self.library.get("library", {})
        keys = (
            f"library:{lib.get('name', '')}@{lib.get('revision', '')}",
            f"method:{lib.get('name', '')}/{method_id}@{lib.get('revision', '')}",
        )
        states = []
        for key in keys:
            seen = {
                s
                for k, s in self.lifecycle
                if k == key and s in self.LIFECYCLE_STATES
            }
            if len(seen) > 1:
                return "conflict"
            if seen:
                states.append(seen.pop())
        if any(s in ("expired", "withdrawn") for s in states):
            return "refused"
        return "usable"

    def apply(self, index: int, bind: str, method_id: str, arguments: dict,
              evaluating: bool, admissible: dict):
        method = self.methods.get(method_id)
        if method is None:
            self.pfind("undeclared_method")
            return
        at = f"body[{index}]"
        # `deps`/block propagation walk the *declared* slots only — a
        # missing slot contributes no edge; an undeclared argument is
        # flagged malformed but never becomes a dependency.
        operand_refs = []
        slot_values: dict[str, SemVal] = {}
        bad = False
        variables: dict[str, str] = {}
        # Signature slots iterate in canonical (sorted) order — operand
        # resolution order shapes the merged assumption order, not the
        # document's field order.
        for slot, decl in sorted(method.get("inputs", {}).items()):
            arg = arguments.get(slot)
            if arg is None:
                self.pfind("missing_argument")
                bad = True
                continue
            operand_refs.append(arg["ref"])
            value = self.env.get(arg["ref"])
            if value is None:
                self.pfind("malformed")
                bad = True
                continue
            # The operand's claim must satisfy the slot's claim and the
            # quantity kinds must agree — mismatches poison the binding.
            if not claim_satisfies(value.ty.claim, claim_of(decl.get("claim"))):
                self.pfind("type_mismatch")
                bad = True
            if value.ty.quantity_kind != decl["quantity_kind"]:
                self.pfind("type_mismatch")
                bad = True
            missing = False
            for relation, var in slot_relations(decl).items():
                entity = value.ty.relations.get(relation)
                if entity is None:
                    missing = True
                    continue
                self.check_entity(relation, entity)
                if var in variables and variables[var] != entity:
                    self.pfind("type_mismatch")
                    bad = True
                else:
                    variables[var] = entity
            if missing:
                self.pfind("relation_absent")
                bad = True
            slot_values[slot] = value
        if arguments:
            for slot in arguments:
                if slot not in method.get("inputs", {}):
                    self.pfind("malformed")
                    bad = True
        self.deps[bind] = operand_refs
        inherited = self.propagated_block(operand_refs)

        # `output_type`: declared output relations resolved through the
        # (possibly partial) variables map — the poisoned binding still
        # carries the scope the signature could resolve.
        def poison_ty():
            return QuantityType(
                method["output"]["quantity_kind"],
                claim_of(method["output"].get("claim")),
                {
                    key: variables[var]
                    for key, var in slot_relations(method["output"]).items()
                    if var in variables
                },
            )

        output_ty = QuantityType(
            method["output"]["quantity_kind"],
            claim_of(method["output"].get("claim")),
            {},
        )
        if bad:
            self.poison(bind, poison_ty())
            return

        usability = self.check_lifecycle(index, method_id)
        if usability == "refused":
            self.blocked[bind] = "not_evaluated.lifecycle_refused"
            self.pfind("lifecycle_refused")
            self.poison(bind, poison_ty())
            return
        if usability == "conflict":
            self.pfind("lifecycle_conflict")
            self.poison(bind, poison_ty())
            return

        # Output relations: operand union minus `projects`; conflicts poison.
        output_relations: dict[str, str] = {}
        relation_conflict = False
        for value in slot_values.values():
            for key, entity in value.ty.relations.items():
                if output_relations.get(key, entity) != entity:
                    relation_conflict = True
                output_relations[key] = entity
        for key in method.get("projects", []):
            output_relations.pop(key, None)
        if relation_conflict:
            self.pfind("type_mismatch")
            self.poison(bind, poison_ty())
            return
        output_ty.relations = output_relations

        # Generated `requires` obligations — checked statically. Emission
        # order is canonical; the blocking rule is the LAST blocked
        # obligation's (each overwrites), matching the analyzer's fold.
        block = inherited
        refuted = False
        witness_support_assumptions: list = []
        witness_support_edges: set = set()
        for position, requirement in enumerate(
            canonical_sort(method.get("requires", []), REQUIRES)
        ):
            oid = f"{at}.requires[{position}]"
            state, support_a, support_e, detail = self.check_requires(
                method, requirement, variables, slot_values, arguments, admissible
            )
            self.obligation_states[oid] = (
                requirement.get("kind", ""), state, detail
            )
            witness_support_assumptions = merge_assumptions(
                witness_support_assumptions, [support_a]
            )
            witness_support_edges |= support_e
            if state == "refuted":
                refuted = True
                block = "not_evaluated.obligation_refuted"
            elif state == "open":
                block = "not_evaluated.obligation_unmet"

        impl = method.get("implementation") or {}
        impl_kind = impl.get("kind")
        value = None
        unit = ""
        state = "unestablished"
        if impl_kind == "primitive":
            try:
                computed = eval_expression(
                    parse_expression(impl.get("body") or ""),
                    slot_values,
                    self.kind_products,
                )
                ok = (
                    computed.ty.quantity_kind == output_ty.quantity_kind
                    and claim_satisfies(computed.ty.claim, output_ty.claim)
                )
                value, unit = computed.value, computed.unit
                state = computed.state if ok else "unestablished"
                if not ok:
                    self.pfind("type_mismatch")
            except EvalFailure as failure:
                self.pfind(failure.kind)
        elif impl_kind == "external":
            declared_value = None
            conflicted = False
            for ensures in canonical_sort(method.get("ensures", []), ENSURES):
                # A declared value that cannot even replay is ignored
                # silently — the library checker already reported the
                # defect; only a *conflict* between replayable claims is a
                # program finding here.
                try:
                    computed = eval_expression(
                        parse_ensures(ensures["expression"]),
                        slot_values,
                        self.kind_products,
                    )
                except EvalFailure:
                    continue
                if computed.value is not None:
                    if declared_value is not None and declared_value != computed.value:
                        conflicted = True
                    elif declared_value is None:
                        # First claimed value wins — a conflicting later
                        # ensures refutes the signature but does not
                        # rebind the declared result.
                        declared_value = computed.value
            if conflicted:
                self.pfind("malformed")
            unit = (impl.get("produces") or {}).get("unit", "")
            if conflicted or any(
                v.state == "unestablished" for v in slot_values.values()
            ):
                declared_state = "unestablished"
            else:
                declared_state = "declared"
            observed = None
            if evaluating and index in self.site_map:
                observed = self.bind_observation(
                    index, method, slot_values, variables, self.site_map[index]
                )
            if observed is not None:
                self.observed_sites.add(index)
                self.observed_values[index] = observed.value
                value, unit, state = observed.value, observed.unit, "established"
            else:
                value, unit, state = declared_value, unit, declared_state

        # Ensures obligations: canonical (set) order; each obligation's
        # `blocked_rule` OVERWRITES `block` — the rule that lands in
        # `blocked` is the last non-discharged obligation's, matching the
        # analyzer's fold over this step's obligation rows.
        for position, (_, ensures) in enumerate(
            (p, e)
            for p, e in enumerate(
                canonical_sort(method.get("ensures", []), ENSURES)
            )
        ):
            oid = f"{at}.ensures[{position}]"
            check_ok = ensures.get("check") in SUPPORTED_CHECKS
            if impl_kind == "primitive":
                o_state = "open"
                # A supported check + a parseable expression gate the
                # replay; parse failures were already reported at library
                # admission — silent here.
                parsed = None
                try:
                    parsed = parse_ensures(ensures["expression"])
                except EvalFailure:
                    pass
                if check_ok and parsed is not None:
                    try:
                        expected = eval_expression(
                            parsed, slot_values, self.kind_products
                        ).value
                        if expected is not None and value is not None:
                            o_state = "discharged" if expected == value else "refuted"
                    except EvalFailure as failure:
                        self.pfind(failure.kind)
            elif not evaluating:
                o_state = "runtime"
            elif index not in self.observed_sites:
                o_state = "open"
            else:
                o_state = "open"
                parsed = None
                try:
                    parsed = parse_ensures(ensures["expression"])
                except EvalFailure:
                    pass
                if parsed is not None:
                    try:
                        expected = eval_expression(
                            parsed, slot_values, self.kind_products
                        ).value
                        got = self.observed_values.get(index)
                        if expected is not None and got is not None:
                            o_state = "discharged" if expected == got else "refuted"
                    except EvalFailure as failure:
                        self.pfind(failure.kind)
            self.obligation_states[oid] = (
                "postcondition",
                o_state,
                f"`{ensures.get('expression', '')}` checked by "
                f"{ensures.get('check', '')}",
            )
            if o_state == "refuted":
                self.pfind("obligation_refuted")
                refuted = True
                block = "not_evaluated.obligation_refuted"
            elif o_state == "open":
                block = "not_evaluated.obligation_unmet"
            if impl_kind == "external":
                # The discharge event's subject is the application-scope
                # `at` (`body[i] {method}`), not the obligation id.
                self.events.append(
                    (
                        "discharge",
                        f"{at} {method_id}",
                        o_state,
                        f"postcondition `{ensures.get('expression', '')}` "
                        f"{o_state} by {ensures.get('check', '')}",
                    )
                )

        # Runtime obligations: `body[i].observation` when no ensures are
        # declared — emitted after the ensures loop, so its `blocked_rule`
        # lands last in the fold.
        if impl_kind == "external" and not method.get("ensures"):
            oid = f"{at}.observation"
            if not evaluating:
                ob_state = "runtime"
            elif index in self.observed_sites:
                ob_state = "discharged"
            else:
                ob_state = "open"
                block = "not_evaluated.obligation_unmet"
            self.obligation_states[oid] = (
                "observation",
                ob_state,
                f"output `{bind}` must be observed at execution; "
                f"`{method_id}` declares no postcondition — nothing to replay",
            )
            if evaluating:
                self.events.append(
                    (
                        "observe",
                        at,
                        ob_state,
                        f"observation obligation for `{bind}`",
                    )
                )

        if block:
            self.blocked[bind] = block
        if refuted:
            self.poison(bind, poison_ty())
            return

        # Instantiated `assumes` at the applied scope.
        assumptions = []
        for name in canonical_sort(method.get("assumes", []), NODE):
            params = self.propositions.get(name, {}).get("params", [])
            at_map = {}
            for param in params:
                # `variables` iterates in canonical order — the first var
                # naming this relation wins.
                for var, relation in sorted(
                    method.get("variables", {}).items()
                ):
                    if relation == param and var in variables:
                        at_map[param] = variables[var]
                        break
            assumptions.append(ScopedProposition.of(name, at_map))
        assumptions = merge_assumptions(
            assumptions, [v.assumptions for v in slot_values.values()]
        )
        assumptions = merge_assumptions(
            assumptions, [witness_support_assumptions]
        )
        edges = set(witness_support_edges)
        for v in slot_values.values():
            edges |= v.edges
        if index in self.observed_sites:
            edges.add(f"observation:{self.site_map[index]['receipt_sha256']}")
        if unit == "":
            unit = self.kinds.get(output_ty.quantity_kind, {}).get(
                "canonical_unit", ""
            )
        self.env[bind] = SemVal(output_ty, unit, value, state, assumptions, edges)
        self.edges[bind] = set(edges)

    def check_requires(self, method, requirement, variables, slot_values,
                       arguments, admissible) -> tuple:
        """→ (state, support_assumptions, support_edges, detail). Witness
        support is `independence` discharging premises' own assumptions —
        they join the conclusion's residual cone."""
        kind = requirement.get("kind")
        if kind == "domain_containment":
            detail = (
                f"`{requirement.get('domain') or ''}` ⊆ "
                f"`{requirement.get('within') or ''}`"
            )

            def entity_for(path):
                if "." not in (path or ""):
                    return None
                relation, field = (path or "").split(".", 1)
                # The signature's var→relation map is ordered; the first
                # variable carrying the relation resolves the path.
                for var, rel in sorted(method.get("variables", {}).items()):
                    if rel == relation:
                        entity = variables.get(var)
                        return (field, entity) if entity else None
                return None

            d = entity_for(requirement.get("domain"))
            w = entity_for(requirement.get("within"))
            if d is None or w is None:
                self.pfind("obligation_unmet")
                return (
                    "open", [], set(),
                    "domain or applicability path does not resolve through "
                    "the signature",
                )
            domain = (
                self.program.get("entities", {})
                .get("scenarios", {})
                .get(d[1], {})
                .get("operating_domain")
            )
            within = (
                self.program.get("entities", {})
                .get("materials", {})
                .get(w[1], {})
                .get("applicability")
            )
            if domain is None or within is None:
                self.pfind("obligation_unmet")
                return (
                    "open", [], set(),
                    f"`{d[1]}` or `{w[1]}` declares no operating "
                    "domain/applicability",
                )
            within_interval = within.get(domain.get("quantity_kind"))
            if within_interval is None:
                self.pfind("obligation_unmet")
                return (
                    "open", [], set(),
                    f"applicability of `{w[1]}` declares no "
                    f"`{domain.get('quantity_kind')}` interval",
                )
            if within_interval.get("unit") != domain.get("unit"):
                self.pfind("type_mismatch")
                self.pfind("obligation_unmet")
                return (
                    "open", [], set(),
                    "domain and applicability units are incompatible",
                )
            try:
                dl, du = exact_number(domain["lower"]), exact_number(domain["upper"])
                wl, wu = (
                    exact_number(within_interval["lower"]),
                    exact_number(within_interval["upper"]),
                )
            except Exception:
                self.pfind("malformed")
                self.pfind("obligation_unmet")
                return (
                    "open", [], set(),
                    "domain or applicability interval is malformed",
                )
            # An inverted interval is malformed, not refuted — the
            # obligation stays open rather than fabricating a verdict.
            if dl > du or wl > wu:
                self.pfind("malformed")
                self.pfind("obligation_unmet")
                return (
                    "open", [], set(),
                    "domain or applicability interval is malformed",
                )
            if wl <= dl <= du <= wu:
                return "discharged", [], set(), detail
            self.pfind("precondition_refuted")
            return "refuted", [], set(), detail
        if kind == "scope_check":
            detail = (
                f"scope of `{requirement.get('subject') or ''}` must refine "
                f"`{requirement.get('scope') or ''}`"
            )
            slot = requirement.get("subject")
            arg = arguments.get(slot) if slot else None
            value = self.env.get(arg["ref"]) if arg else None
            scope_entity = (
                value.ty.relations.get("scenario") if value else None
            )
            scope = (
                self.program.get("entities", {})
                .get("scenarios", {})
                .get(scope_entity, {})
                .get("scope")
                if scope_entity
                else None
            )
            required = requirement.get("scope", "")
            if scope is None:
                self.pfind("obligation_unmet")
                return "open", [], set(), detail
            if scope_refines(scope, required):
                return "discharged", [], set(), detail
            self.pfind("precondition_refuted")
            return "refuted", [], set(), detail
        if kind == "provenance_disjoint":
            bound = sorted(
                {
                    arguments[s].get("ref")
                    for s in requirement.get("over", [])
                    if isinstance(arguments.get(s), dict)
                    and arguments[s].get("ref") is not None
                }
            )
            detail = "over " + ", ".join(bound)
            edge_sets = [
                self.effective_edges(b, admissible, set()) for b in bound
            ]
            if not all(edge_sets):
                self.pfind("obligation_unmet")
                return "open", [], set(), detail
            shared = any(
                edge_sets[a] & edge_sets[b]
                for a in range(len(edge_sets))
                for b in range(a + 1, len(edge_sets))
            )
            if shared:
                self.pfind("obligation_refuted")
                return "refuted", [], set(), detail
            return "discharged", [], set(), detail
        if kind == "independence":
            bound = {
                arguments[s]["ref"]
                for s in requirement.get("over", [])
                if isinstance(arguments.get(s), dict) and "ref" in arguments[s]
            }
            bound_s = ", ".join(sorted(bound))
            scope = {
                method["variables"][var]: entity
                for var, entity in variables.items()
                if var in method.get("variables", {})
            }
            scope_key = tuple(sorted(scope.items()))
            admissible_for = admissible.get(("independent", scope_key), [])
            # Witnesses iterate in canonical premise order so their support
            # lands deterministically; an unattributed premise cannot
            # discharge even if it survived admissibility otherwise.
            witnesses = []
            for idx, p in canonical_order(self.program.get("premises", []), PREMISE):
                if (
                    p["proposition"] == "independent"
                    and p.get("at", {}) == scope
                    and set((p.get("arguments") or {}).get("over", [])) == bound
                    and idx in admissible_for
                    and p.get("established_by") is not None
                ):
                    witnesses.append(idx)
            if not witnesses:
                # The record names inadmissible witnesses at this scope —
                # why the obligation stayed open is part of the narration.
                inadmissible_named = [
                    p["id"]
                    for _, p in canonical_order(
                        self.program.get("premises", []), PREMISE
                    )
                    if p["proposition"] == "independent"
                    and p.get("at", {}) == scope
                    and self._premise_inadmissible(_)
                ]
                detail = (
                    f"no premise attests independence over {bound_s}"
                )
                if inadmissible_named:
                    detail += (
                        "; premise "
                        + ", ".join(inadmissible_named)
                        + " exists but is inadmissible"
                    )
                self.pfind("obligation_unmet")
                return "open", [], set(), detail
            names = ", ".join(
                self.program["premises"][i]["id"] for i in witnesses
            )
            detail = f"discharged by premise {names}"
            support_a, support_e = [], set()
            for i in witnesses:
                premise = self.program["premises"][i]
                for s in canonical_sort(
                    premise.get("assumptions", []), SCOPED_ASSERTION
                ):
                    prop = ScopedProposition.of(
                        s["proposition"], s.get("at", {})
                    )
                    if prop not in support_a:
                        support_a.append(prop)
                edge = (premise.get("established_by") or {}).get("edge")
                if edge:
                    support_e.add(edge)
            return "discharged", support_a, support_e, detail
        self.pfind("malformed")
        return "open", [], set(), ""  # unknown kinds stay open — never discharged

    # -- O1 binding (spec §7) ----------------------------------------------

    def bind_observation(
        self,
        index: int,
        method: dict,
        slot_values: dict,
        variables: dict,
        record: dict,
    ) -> Optional[SemVal]:
        """The O1 chain. Each `reject` emits an `observe → rejected` event
        and the outcome-row detail; a typed-admission failure returns None
        *silently* — no event, no outcome row — matching `?` in the
        reference implementation."""
        at = f"body[{index}]"
        receipt = record.get("receipt") or {}

        def reject(reason):
            self.events.append(("observe", at, "rejected", reason))
            self.rejected_observations[at] = reason
            return None

        if record.get("at") != at:
            return reject(
                f"observation declares site `{record.get('at')}` — "
                f"supplied for `{at}`"
            )
        if "sha256:" + _hex(sha256_bytes(canon_bytes(receipt))) != record.get(
            "receipt_sha256"
        ):
            return reject(
                "receipt bytes do not re-hash to the declared `receipt_sha256`"
            )
        declared_executable = (
            method.get("implementation") or {}
        ).get("executable", "")
        # The receipt AND the record-level field must both name the declared
        # executable — a receipt cut for another invocation is foreign even
        # when internally consistent.
        if (
            receipt.get("plan_sha256") != (self.expected_plan_sha256 or "")
            or receipt.get("at") != at
            or receipt.get("executable") != declared_executable
            or record.get("executable") != declared_executable
        ):
            return reject(
                "receipt does not belong to this plan, site, or executable"
            )
        expected_inputs = {
            slot: sha
            for slot, v in slot_values.items()
            if (sha := staged_input_sha256(v)) is not None
        }
        record_inputs = dict(record.get("inputs") or {})
        receipt_input_rows = receipt.get("inputs", [])
        receipt_inputs = {i["slot"]: i["sha256"] for i in receipt_input_rows}
        if (
            len(record_inputs) != len(expected_inputs)
            or any(
                expected_inputs.get(slot) != digest
                for slot, digest in record_inputs.items()
            )
            or len(receipt_input_rows) != len(expected_inputs)
            or receipt_inputs != expected_inputs
        ):
            return reject(
                "observation input digests do not match the invocation's operands"
            )
        identity = canon_sha256(
            {
                "executable": receipt.get("executable"),
                "executable_sha256": receipt.get("executable_sha256"),
                "inputs": receipt.get("inputs", []),
                "invocation": receipt.get("invocation"),
            }
        )
        if receipt.get("invocation_sha256") != identity:
            return reject(
                "receipt `invocation_sha256` does not re-derive from its parts"
            )
        if receipt.get("status") != "completed" or record.get("output") is None:
            return reject("the invocation did not produce a completed output")
        output_decl = record.get("output")
        output_sha = "sha256:" + _hex(sha256_bytes(canon_bytes(output_decl)))
        output_row = next(
            (
                o
                for o in receipt.get("outputs") or []
                if o.get("output_id") == "output"
            ),
            None,
        )
        if output_sha != record.get("output_sha256") or (
            output_row is None
            or output_row.get("state") != "collected"
            or output_row.get("sha256") != record.get("output_sha256")
        ):
            return reject(
                "output digest does not cover the supplied output document"
            )
        output_ty = QuantityType(
            method["output"]["quantity_kind"],
            claim_of(method["output"].get("claim")),
            {
                key: variables[var]
                for key, var in slot_relations(method["output"]).items()
                if var in variables
            },
        )
        numeric = self.admit(output_decl, output_ty)
        if numeric is None:
            # Typed admission failure: no event, no outcome row — the
            # binding simply never establishes.
            return None
        self.events.append(
            (
                "observe",
                at,
                "established",
                "invocation digests verified; receipt "
                f"{record.get('receipt_sha256')} re-hashes",
            )
        )
        return SemVal(
            output_ty,
            ((method.get("implementation") or {}).get("produces") or {}).get(
                "unit", ""
            ),
            numeric,
            "established",
        )

    # -- requirement verdicts ------------------------------------------------

    def requirement_verdict(self, requirement: dict, subject: SemVal) -> dict:
        """bounded.ge / bounded.le over the subject's established value."""
        if subject.value is None:
            return {
                "status": "not_evaluated",
                "rule": "not_evaluated.missing_evidence",
                "detail": self.unestablished_detail(requirement["subject"]["ref"]),
            }
        limit_text = requirement.get("limit", {}).get("value")
        try:
            limit = exact_number(limit_text)
        except Exception:
            return {
                "status": "not_evaluated",
                "rule": "not_evaluated.malformed",
                "detail": "the requirement limit is not an exact number",
            }
        lower, upper = subject.value.bounds()
        symbol = requirement["comparison"]
        if symbol not in ("ge", "le"):
            # Anything but `ge`/`le` is malformed at admission — no verdict
            # may be inferred by treating it as `le`.
            return {
                "status": "not_evaluated",
                "rule": "not_evaluated.malformed",
                "detail": f"unknown comparison `{symbol}`",
            }
        if symbol == "ge":
            if lower >= limit:
                status = "pass"
                detail = (
                    f"{render_subject(subject)} satisfies >= "
                    f"{canonical_rational(limit)} {requirement['limit'].get('unit','')}: "
                    f"{canonical_rational(lower)} >= {canonical_rational(limit)}"
                )
            elif upper < limit:
                status = "fail"
                detail = (
                    f"upper bound {canonical_rational(upper)} < limit "
                    f"{canonical_rational(limit)}"
                )
            else:
                status = "inconclusive"
                detail = (
                    f"{render_subject(subject, with_unit=False)} straddles limit "
                    f"{canonical_rational(limit)}"
                )
        else:
            if upper <= limit:
                status = "pass"
                detail = (
                    f"{render_subject(subject)} satisfies <= "
                    f"{canonical_rational(limit)} {requirement['limit'].get('unit','')}: "
                    f"{canonical_rational(upper)} <= {canonical_rational(limit)}"
                )
            elif lower > limit:
                status = "fail"
                detail = (
                    f"lower bound {canonical_rational(lower)} > limit "
                    f"{canonical_rational(limit)}"
                )
            else:
                status = "inconclusive"
                detail = (
                    f"{render_subject(subject, with_unit=False)} straddles limit "
                    f"{canonical_rational(limit)}"
                )
        if subject.assumptions:
            conditional = ", ".join(a.render() for a in subject.assumptions)
            sep = ", " if status == "pass" else "; "
            detail = f"{detail}{sep}conditional on {conditional}"
        self.events.append(
            (
                f"bounded.{symbol}",
                f"requirements[{requirement['id']}]",
                status,
                detail,
            )
        )
        return {
            "status": status,
            "rule": f"bounded.{symbol}",
            "detail": detail,
        }

    def requirement_malformed(self, requirement: dict, bound: set) -> bool:
        """Requirement admission (§7.B): comparison vocabulary, limit shape,
        declared kinds and entities, a bound subject. A requirement that
        fails admission is `not_evaluated.malformed` — never evaluated."""
        if requirement.get("comparison") not in ("ge", "le"):
            return True
        if requirement.get("quantity_kind") not in self.kinds:
            return True
        limit = requirement.get("limit") or {}
        if limit.get("kind") != "exact":
            return True
        try:
            exact_number(limit.get("value"))
        except Exception:
            return True
        # A non-canonical limit unit is a `type_mismatch` finding — not a
        # malformed requirement — and does not short-circuit evaluation.
        scope = requirement.get("scope")
        if scope is not None and scope not in ("steady-state", "transient", "any"):
            return True
        scenario = requirement.get("scenario")
        if scenario is not None and scenario not in (
            self.program.get("entities", {}).get("scenarios", {})
        ):
            return True
        if requirement.get("subject", {}).get("ref") not in bound:
            return True
        return False

    def requirement_reports(self) -> dict:
        reports = {}
        scenarios = self.program.get("entities", {}).get("scenarios", {})
        # Sequential single-assignment universe: inputs plus every step bind.
        bound = {i["id"] for i in self.program.get("inputs", [])} | {
            s["bind"] for s in self.program.get("body", [])
        }
        for requirement in canonical_sort(
            self.program.get("requirements", []), REQUIREMENT
        ):
            rid = requirement["id"]
            if self.requirement_malformed(requirement, bound):
                reports[rid] = {
                    "state": "not_evaluated",
                    "verdict": {
                        "status": "not_evaluated",
                        "rule": "not_evaluated.malformed",
                        "detail": "the requirement itself failed admission",
                    },
                    "declared_value": None,
                }
                continue
            subject_ref = requirement["subject"]["ref"]
            subject = self.env.get(subject_ref)

            def ne(rule, detail, declared=None):
                return {
                    "state": "not_evaluated",
                    "verdict": {"status": "not_evaluated", "rule": rule,
                                "detail": detail},
                    "declared_value": declared,
                }

            if subject is None:
                reports[rid] = ne(
                    "not_evaluated.missing_evidence",
                    "subject has no established value",
                )
                continue
            block_rule = self.blocked.get(subject_ref)
            if subject.state == "unestablished":
                rule = (
                    block_rule
                    if block_rule
                    in (
                        "not_evaluated.obligation_refuted",
                        "not_evaluated.precondition_refuted",
                        "not_evaluated.lifecycle_refused",
                        "not_evaluated.contradiction",
                    )
                    else (
                        "not_evaluated.unestablished"
                        if subject.ty.claim == "nominal"
                        else "not_evaluated.missing_evidence"
                    )
                )
                detail = (
                    self.unestablished_detail(subject_ref)
                    if rule == "not_evaluated.missing_evidence"
                    else "subject has no established value"
                )
                reports[rid] = ne(rule, detail, subject.value_text())
                continue
            if block_rule:
                reports[rid] = ne(
                    block_rule,
                    "a generated obligation blocks this subject",
                    subject.value_text(),
                )
                continue
            if subject.ty.quantity_kind != requirement["quantity_kind"]:
                reports[rid] = ne(
                    "not_evaluated.type_mismatch",
                    "subject kind does not match the requirement",
                    subject.value_text(),
                )
                continue
            subject_scenario = subject.ty.relations.get("scenario")
            if requirement.get("scenario") and subject_scenario != requirement["scenario"]:
                reports[rid] = ne(
                    "not_evaluated.coverage",
                    "required scenario identity is not carried by the subject",
                    subject.value_text(),
                )
                continue
            if requirement.get("scope"):
                covered = (
                    subject_scenario in scenarios
                    and scope_refines(
                        scenarios[subject_scenario].get("scope", ""),
                        requirement["scope"],
                    )
                )
                if not covered:
                    reports[rid] = ne(
                        "not_evaluated.coverage",
                        "requirement scope is not covered by the subject's scenario",
                        subject.value_text(),
                    )
                    continue
            if subject.state == "declared":
                if subject.ty.claim == "nominal":
                    reports[rid] = ne(
                        "not_evaluated.unestablished",
                        "subject is a nominal assertion; the requirement needs an established claim",
                        subject.value_text(),
                    )
                else:
                    reports[rid] = ne(
                        "not_evaluated.missing_evidence",
                        "the runtime observation that would establish the subject did not bind",
                        subject.value_text(),
                    )
                continue
            if subject.ty.claim == "nominal":
                reports[rid] = ne(
                    "not_evaluated.unestablished",
                    "subject claim cannot satisfy a bounded requirement",
                    subject.value_text(),
                )
                continue
            verdict = self.requirement_verdict(requirement, subject)
            reports[rid] = {
                "state": "evaluated",
                "verdict": verdict,
                "declared_value": subject.value_text(),
            }
        return reports

    def unestablished_detail(self, subject: str) -> str:
        cone = set()
        frontier = [subject]
        while frontier:
            name = frontier.pop()
            if name not in cone:
                cone.add(name)
                frontier.extend(self.deps.get(name, []))
        clauses, named = [], set()
        for inp in self.program.get("inputs", []):
            if (
                (inp.get("binding") or {}).get("state") == "unavailable"
                and inp["id"] in cone
            ):
                clauses.append(f"input {inp['id']} declared unavailable")
                named.add(inp["id"])
        for name in sorted(cone):
            if name == subject or name in named:
                continue
            value = self.env.get(name)
            if value is not None and value.state == "unestablished":
                clauses.append(f"{name} has no established value")
        if not clauses:
            clauses.append(f"{subject} has no established value")
        return "; ".join(clauses)


def scope_refines(actual: str, required: str) -> bool:
    return required == "any" or actual == required


# ---------------------------------------------------------------------------
# verify-evaluation
# ---------------------------------------------------------------------------


def identifier_charset_ok(text) -> bool:
    """Declared identifiers are single tokens: no whitespace, none of the
    structural delimiters `[] , . / :` (Rust `identifier_charset_ok`)."""
    return isinstance(text, str) and bool(text) and all(
        not c.isspace() and c not in "[],./:" for c in text
    )


def _ids_ok(ids) -> bool:
    seen = set()
    for i in ids:
        if not identifier_charset_ok(i) or i in seen:
            return False
        seen.add(i)
    return True


# Profile bounds (spec §11) — the mechanical admission subset the replay
# can decide. Semantic admission findings (`undeclared_*`, `malformed`
# payload checks) are NOT reproduced here — a record claiming identity `""`
# for a program that fails only those still mismatches, which fails closed.
MAX_BODY_STEPS = 512
MAX_INPUTS = 512
MAX_PREMISES = 256
MAX_ASSUMPTIONS = 512
MAX_REQUIREMENTS = 256
MAX_ENTITIES = 1024
MAX_METHODS = 256

MAX_WITNESS_DEPTH = 256  # == MAX_PREMISES in the reference
MAX_OVER_MEMBERS = 32
MAX_LIFECYCLE_ENTRIES = 128

ADMISSION_KINDS = {
    "malformed",
    "malformed_import",
    "undeclared_proposition",
    "undeclared_method",
    "undeclared_entity",
    "budget",
}

RELATION_NAMES = ("geometry", "scenario", "material")
SCOPE_KINDS = ("steady-state", "transient", "any")
SUPPORTED_CHECKS = ("interval_arithmetic",)
BUILTIN_PROPOSITIONS = ("independent", "provenance_disjoint")
LIBRARY_SCHEMA_VERSION = "avila.core/method-library/v0.1-draft"
PROGRAM_SCHEMA_VERSION = "avila.core/language-program/v0.1-draft"


def expr_names(expr) -> set:
    """Every `Name` in the expression tree — the admission check requires
    each to resolve to a declared input slot."""
    if expr[0] == "name":
        return {expr[1]}
    out = set()
    for a in expr[2]:
        out |= expr_names(a)
    return out


def proposition_known(library: dict, name) -> bool:
    return name in (library.get("propositions") or {}) or (
        name in BUILTIN_PROPOSITIONS
    )


def proposition_params(library: dict, name) -> list:
    decl = (library.get("propositions") or {}).get(name)
    if decl is None:
        return list(RELATION_NAMES)
    return decl.get("params") or []


def _step_references(step: dict):
    """Canonical (position, reference) pairs in sorted-argument order —
    matches `step_references`: apply arguments first (a slot-keyed map),
    then infer arguments' ordered refs."""
    refs = []
    args = step.get("arguments")
    if isinstance(args, dict):
        refs += [(f"arguments.{k}", args[k]["ref"]) for k in sorted(args)
                 if isinstance(args.get(k), dict) and "ref" in args[k]]
    infer = step.get("infer")
    if isinstance(infer, dict):
        refs += [
            (f"infer.arguments[{i}]", a["ref"])
            for i, a in enumerate(infer.get("arguments") or [])
            if isinstance(a, dict) and "ref" in a
        ]
    return refs


def program_admission_ok(program: dict, library: dict, finding_kinds) -> bool:
    """Program admission == 'no ADMISSION_KINDS finding, static or
    analyze-run': the mechanical statics are replayed here and the
    semantic-emission kinds come from the baseline replay's collected
    `finding_kinds`."""
    if program.get("schema_version") != PROGRAM_SCHEMA_VERSION:
        return False
    if program.get("profile") != LANGUAGE_PROFILE:
        return False
    entities = program.get("entities") or {}
    if (
        len(program.get("body") or []) > MAX_BODY_STEPS
        or len(program.get("inputs") or []) > MAX_INPUTS
        or len(program.get("premises") or []) > MAX_PREMISES
        or len(program.get("assumptions") or []) > MAX_ASSUMPTIONS
        or len(program.get("requirements") or []) > MAX_REQUIREMENTS
        or sum(
            len(entities.get(group) or {})
            for group in ("geometries", "scenarios", "materials")
        )
        > MAX_ENTITIES
    ):
        return False
    for collection in ("inputs", "premises", "assumptions", "requirements"):
        if not _ids_ok(x.get("id", "") for x in program.get(collection) or []):
            return False
    if not identifier_charset_ok(program.get("id", "")):
        return False
    if not all(
        identifier_charset_ok(x.get("bind", ""))
        for x in program.get("body") or []
    ):
        return False
    for group in ("geometries", "scenarios", "materials"):
        if not all(identifier_charset_ok(k) for k in (entities.get(group) or {})):
            return False

    kinds = library.get("quantity_kinds") or {}

    def prop_check(prop, at):
        """Scoped proposition reference: the name must resolve, and every
        `at` key must be one of its declared params."""
        if not proposition_known(library, prop):
            return False
        return all(k in proposition_params(library, prop) for k in at)

    # Assumptions: exactly one of asserts/denies; proposition refs and
    # scope keys; source provenance.
    for a in program.get("assumptions") or []:
        if bool(a.get("asserts")) == bool(a.get("denies")):
            return False
        if not prop_check(a.get("asserts") or a.get("denies"), a.get("at") or {}):
            return False
        if a.get("source") is not None and provenance_admission_bad(a["source"]):
            return False
    # Premises: attribution required; `over` bound; proposition + scope
    # keys; support assertion props; established_by provenance.
    bound_names = {i.get("id") for i in program.get("inputs") or []}
    bound_names |= {s.get("bind") for s in program.get("body") or []}
    for pr in program.get("premises") or []:
        if pr.get("established_by") is None:
            return False
        if provenance_admission_bad(pr["established_by"]):
            return False
        if len((pr.get("arguments") or {}).get("over") or []) > MAX_OVER_MEMBERS:
            return False
        if not all(
            member in bound_names
            for member in (pr.get("arguments") or {}).get("over") or []
        ):
            return False
        if not prop_check(pr.get("proposition"), pr.get("at") or {}):
            return False
        for s in pr.get("assumptions") or []:
            if not prop_check(s.get("proposition"), s.get("at") or {}):
                return False
    # Import-step assumption propositions too.
    for step in program.get("body") or []:
        imp = step.get("import")
        if isinstance(imp, dict):
            for s in imp.get("assumptions") or []:
                if not prop_check(s.get("proposition"), s.get("at") or {}):
                    return False
    # Entities: scope vocabulary + interval-decl shape (declared kind,
    # canonical rational bounds, non-inverted; unit must equal canonical).
    scenarios = entities.get("scenarios") or {}
    materials = entities.get("materials") or {}
    for sc in scenarios.values():
        if (sc.get("scope") or "") not in SCOPE_KINDS:
            return False
        d = sc.get("operating_domain")
        if d is not None and not _interval_decl_ok(d, d.get("quantity_kind"), kinds):
            return False
    for mt in materials.values():
        for kind_name, d in (mt.get("applicability") or {}).items():
            if not _interval_decl_ok(d, kind_name, kinds):
                return False
    # Sequential single-assignment: every reference names an input or an
    # earlier binding; binds may not shadow.
    bound = {i.get("id") for i in program.get("inputs") or []}
    for step in program.get("body") or []:
        for _, ref in _step_references(step):
            if ref not in bound:
                return False
        if step.get("bind") in bound:
            return False
        bound.add(step.get("bind"))
    # Requirement admission.
    for r in program.get("requirements") or []:
        if r.get("comparison") not in ("ge", "le"):
            return False
        if r.get("quantity_kind") not in kinds:
            return False
        if (r.get("limit") or {}).get("kind") != "exact":
            return False
        try:
            exact_number((r.get("limit") or {}).get("value") or "")
        except Exception:
            return False
        if r.get("subject", {}).get("ref") not in bound:
            return False
        if r.get("scope") is not None and r["scope"] not in SCOPE_KINDS:
            return False
        if r.get("scenario") is not None and r["scenario"] not in scenarios:
            return False
    # Entity + input-binding provenance.
    for group in ("geometries", "scenarios", "materials"):
        for e in (entities.get(group) or {}).values():
            if e.get("source") is not None and provenance_admission_bad(e["source"]):
                return False
    for inp in program.get("inputs") or []:
        src = ((inp.get("binding") or {}).get("source"))
        if src is not None and provenance_admission_bad(src):
            return False
    # The analyze run must not have published an admission-kind finding.
    if finding_kinds & ADMISSION_KINDS:
        return False
    return True


def provenance_admission_bad(source: dict) -> bool:
    """True when `provenance_admissible` emits an ADMISSION kind —
    `unsupported` certificate checks do NOT unadmit the document (the
    premise/binding just cannot discharge)."""
    kind = source.get("kind")
    if kind in ("assertion", "declared"):
        return source.get("party") is None
    if kind == "certificate":
        if not valid_sha256_digest(source.get("digest", "")):
            return True
        return source.get("check") is None  # unsupported ≠ malformed
    return True


def _interval_decl_ok(interval: dict, kind_name, kinds) -> bool:
    """check_interval_decl: declared kind, canonical-rational bounds,
    non-inverted. The unit-vs-canonical check is a `type_mismatch` — not an
    admission kind — so it is deliberately not gated here."""
    if kind_name not in kinds:
        return False
    try:
        lo = exact_number(interval.get("lower") or "")
        hi = exact_number(interval.get("upper") or "")
    except Exception:
        return False
    if lo > hi:
        return False
    return True


def library_admission_ok(library: dict) -> bool:
    """The full `LibraryChecker::run` admission: schema/profile, budgets,
    identifier charset + duplicate ids, method-signature checks (declared
    kinds and claims, slot→variable consistency, `projects` vocabulary,
    `assumes` propositions and params, requires/ensures declaration shape
    and expression names), `kind_products` shape."""
    if library.get("schema_version") != LIBRARY_SCHEMA_VERSION:
        return False
    if library.get("profile") != LANGUAGE_PROFILE:
        return False
    methods = library.get("methods") or []
    if len(methods) > MAX_METHODS:
        return False
    kinds = library.get("quantity_kinds") or {}
    if not identifier_charset_ok((library.get("library") or {}).get("name", "")):
        return False
    for collection in ("quantity_kinds", "propositions"):
        if not all(
            identifier_charset_ok(k)
            for k in (library.get(collection) or {})
        ):
            return False
    method_ids = set()
    for _, method in canonical_order(methods, METHOD):
        if not identifier_charset_ok(method.get("id", "")):
            return False
        for key in list(method.get("inputs") or {}) + list(
            method.get("variables") or {}
        ):
            if not identifier_charset_ok(key):
                return False
        if method.get("id") in method_ids:
            return False
        method_ids.add(method.get("id"))
        declared_vars = method.get("variables") or {}
        inputs = method.get("inputs") or {}
        for slot in list(inputs.values()) + [method.get("output") or {}]:
            if slot.get("quantity_kind") not in kinds:
                return False
            if slot.get("claim") not in CLAIMS:
                return False
            for relation, var in slot_relations(slot).items():
                if declared_vars.get(var) != relation:
                    return False
        for relation in method.get("projects") or []:
            if relation not in RELATION_NAMES:
                return False
        for name in method.get("assumes") or []:
            if not proposition_known(library, name):
                return False
            declared_relations = set(declared_vars.values())
            for param in proposition_params(library, name):
                if param not in declared_relations:
                    return False
        for requirement in canonical_sort(
            method.get("requires") or [], REQUIRES
        ):
            if not _requires_decl_ok(method, requirement):
                return False
        for ensures in canonical_sort(method.get("ensures") or [], ENSURES):
            if not _ensures_decl_ok(method, ensures):
                return False
        impl = method.get("implementation")
        if impl is not None:
            ik = impl.get("kind")
            if ik == "primitive":
                body = impl.get("body")
                if body is None:
                    return False
                try:
                    expr = parse_expression(body)
                except EvalFailure as failure:
                    if failure.kind in ADMISSION_KINDS:
                        return False
                    expr = None
                if expr is not None and not expr_names(expr) <= set(inputs):
                    return False
            elif ik == "external":
                produces = impl.get("produces")
                if produces is None:
                    return False
                if produces.get("quantity_kind") not in kinds:
                    return False
            else:
                return False
    product_keys = set()
    for _, product in canonical_order(
        library.get("kind_products") or [], KIND_PRODUCT
    ):
        for f in ("lhs_kind", "rhs_kind", "result_kind"):
            if product.get(f) not in kinds:
                return False
        key = (product.get("lhs_kind"), product.get("rhs_kind"))
        if key in product_keys:
            return False
        product_keys.add(key)
        canonical = (kinds.get(product.get("result_kind")) or {}).get(
            "canonical_unit"
        )
        if canonical is not None and product.get("result_unit") != canonical:
            return False
    return True


def _resolve_field_path(method: dict, path: str) -> bool:
    """`scenario.operating_domain`/`material.applicability` — `rel.field`
    where `rel` is declared in the signature's variables."""
    if not isinstance(path, str) or "." not in path:
        return False
    relation, field = path.split(".", 1)
    declared = {"scenario": "operating_domain", "material": "applicability"}.get(
        relation
    )
    if field != declared:
        return False
    return relation in set((method.get("variables") or {}).values())


def _requires_decl_ok(method: dict, requirement: dict) -> bool:
    kind = requirement.get("kind")
    inputs = method.get("inputs") or {}
    if kind == "domain_containment":
        return all(
            _resolve_field_path(method, requirement.get(f) or "")
            for f in ("domain", "within")
        )
    if kind == "scope_check":
        subject, scope = requirement.get("subject"), requirement.get("scope")
        if subject is None or scope is None:
            return False
        if scope not in SCOPE_KINDS:
            return False
        return subject in inputs
    if kind in ("independence", "provenance_disjoint"):
        over = requirement.get("over")
        if over is None or len(over) > MAX_OVER_MEMBERS:
            return False
        return all(s in inputs for s in over)
    return False


def _ensures_decl_ok(method: dict, ensures: dict) -> bool:
    """`kind`, `check`, parse, and names — the `check` unsupported finding
    is not an admission kind, so it is deliberately not gated here."""
    if ensures.get("kind") != "relation":
        return False
    try:
        expr = parse_ensures(ensures.get("expression") or "")
    except EvalFailure as failure:
        return failure.kind not in ADMISSION_KINDS
    return expr_names(expr) <= set(method.get("inputs") or {})


def reachable_bindings(program: dict, replay) -> set:
    """Requirement subjects + goal binds, walked back through `deps` — the
    plan emits invocations only for reachable external steps."""
    roots = {
        r["subject"]["ref"] for r in program.get("requirements", [])
    }
    for step in program.get("body", []):
        if step.get("goal") is not None:
            roots.add(step["bind"])
    reachable = set(roots)
    stack = list(roots)
    while stack:
        for dep in replay.deps.get(stack.pop(), ()):
            if dep not in reachable:
                reachable.add(dep)
                stack.append(dep)
    return reachable


def external_sites(program: dict, library: dict, reachable: set) -> set:
    methods = {m["id"]: m for m in library.get("methods", [])}
    return {
        f"body[{i}]"
        for i, step in enumerate(program.get("body", []))
        if step["bind"] in reachable
        and step.get("goal") is None
        and step.get("hole") is None
        and "apply" in step
        and (methods.get(step.get("apply", ""), {}).get("implementation") or {})
        .get("kind")
        == "external"
    }


def verify_evaluation(args) -> Report:
    report = Report("language-evaluation")
    program = load_json(args.program)
    library = load_json(args.library)
    # The evaluation record is the claim under test — it must itself be
    # canonically admissible (no duplicate keys, floats, nulls), or a forged
    # field would digest identically to its absent self.
    if canonically_admitted(args.evaluation) is None:
        report.mismatch(
            "evaluation-admission",
            "the evaluation document fails canonical admission "
            "(duplicate keys, floats, or nulls present)",
        )
        return report
    evaluation = load_json(args.evaluation)
    plan_admitted = canonically_admitted(args.plan) is not None
    if not plan_admitted:
        report.mismatch(
            "plan-admission",
            "the plan document fails canonical admission",
        )
    plan = load_json(args.plan)
    observations_admitted = canonically_admitted(args.observations) is not None
    observations = load_json(args.observations) if observations_admitted else None
    schema_ok = (
        observations is not None
        and observations.get("schema_version") == OBSERVATIONS_SCHEMA
        and observations.get("profile") == "avila.core/language/0.1-draft"
    )
    if not observations_admitted:
        # Malformed observations are foreign in toto — every site stays
        # unobserved and the record must say so.
        report.verified(
            "observations-admission",
            "observations document is malformed — all sites stay unobserved",
        )
    elif not schema_ok:
        # Decodes but isn't an observations document — foreign in toto.
        # The supplied digest still binds it in the context, and the
        # record must carry the schema finding — this is an honest record
        # of a foreign document, not a mismatch.
        report.verified(
            "observations-admission",
            f"schema `{observations.get('schema_version')}` / profile "
            f"`{observations.get('profile')}` is not an observations document "
            "— foreign in toto",
        )
    else:
        report.verified(
            "observations-admission",
            "canonical observations document of the declared schema",
        )
    if observations is None or not schema_ok:
        observations = {
            "schema_version": OBSERVATIONS_SCHEMA,
            "profile": "avila.core/language/0.1-draft",
            "plan_sha256": "",
            "observations": [],
        }

    # -- semantic + document identity ---------------------------------------
    try:
        program_sem = semantic_sha256(program, "program")
        library_sem = semantic_sha256(library, "library")
    except ProjectionError as e:
        report.mismatch("semantic-identity", str(e))
        return report
    context = evaluation.get("context") or {}
    # The record's own identity fields: it must claim the evaluation
    # schema and the language profile, and the context must carry the
    # profile — a record under any other vocabulary is not this profile's.
    for name, got, want in (
        ("evaluation-schema", evaluation.get("schema_version"), EVALUATION_SCHEMA),
        ("evaluation-profile", evaluation.get("profile"), LANGUAGE_PROFILE),
        ("context-profile", context.get("semantic_profile"), LANGUAGE_PROFILE),
    ):
        if got == want:
            report.verified(name, want)
        else:
            report.mismatch(name, f"record claims {got!r}, expected {want!r}")
    # Supplied lifecycle state is caller material — the record's context
    # lists it, and the replay must apply the same gates to reproduce the
    # derivation it claims.
    lifecycle = []
    for entry in getattr(args, "lifecycle", []) or []:
        key, sep, state = entry.partition("=")
        if not sep:
            report.mismatch(
                "lifecycle-admission",
                f"lifecycle entry `{entry}` is not `key=state`",
            )
            break
        lifecycle.append((key, state))
    claimed_lifecycle = context.get("lifecycle") or []
    supplied_lifecycle = [f"{k}={s}" for k, s in lifecycle]
    if claimed_lifecycle == supplied_lifecycle:
        report.verified(
            "context-lifecycle",
            f"{len(claimed_lifecycle)} lifecycle entr{'y' if len(claimed_lifecycle) == 1 else 'ies'}",
        )
    else:
        report.mismatch(
            "context-lifecycle",
            f"context claims {claimed_lifecycle}; "
            f"{len(supplied_lifecycle)} supplied: {supplied_lifecycle}",
        )

    # Plan-time replay (analysis mode) — run after lifecycle material is
    # known: the runtime finding kinds it publishes feed the program
    # admission gate, and it re-derives the staged input digests the plan
    # binds.
    plan_replay = Replay(program, library, lifecycle)
    plan_replay.run(evaluating=False, site_map={})

    # Admission decides whether Rust publishes a semantic identity at all:
    # an inadmissible document's context field is `""` — and a record
    # claiming `""` for an admissible document is forged.
    program_admitted = (
        canonically_admitted(args.program) is not None
        and program_admission_ok(program, library, plan_replay.finding_kinds)
    )
    library_admitted = (
        canonically_admitted(args.library) is not None
        and library_admission_ok(library)
    )
    expected_program_sha = program_sem if program_admitted else ""
    expected_library_sha = library_sem if library_admitted else ""
    for name, got, want in (
        ("program-semantic-identity", context.get("program_sha256"), expected_program_sha),
        ("library-semantic-identity", context.get("library_sha256"), expected_library_sha),
    ):
        if got == want:
            report.verified(name, want or "inadmissible — no identity published")
        else:
            report.mismatch(name, f"record claims {got}, rederived {want}")
    pin = program.get("library") or {}
    pin_ok = (
        pin.get("semantic_sha256") == library_sem
        and pin.get("name") == (library.get("library") or {}).get("name")
        and pin.get("revision") == (library.get("library") or {}).get("revision")
    )
    if not library_admitted:
        report.not_checked(
            "library-pin",
            "the supplied library is not admitted; its pin cannot be checked",
        )
    elif pin_ok:
        report.verified("library-pin", "program's pin matches the supplied library")
    else:
        report.mismatch(
            "library-pin",
            f"program pins {pin.get('name')}/{pin.get('revision')}@"
            f"{pin.get('semantic_sha256')}; supplied library rederives "
            f"{(library.get('library') or {}).get('name')}/"
            f"{(library.get('library') or {}).get('revision')}@{library_sem}",
        )

    # -- plan identity + invocations -----------------------------------------
    for name, got, want in (
        ("plan-schema", plan.get("schema_version"), PLAN_SCHEMA),
        ("plan-profile", plan.get("profile"), LANGUAGE_PROFILE),
    ):
        if got == want:
            report.verified(name, want)
        else:
            report.mismatch(name, f"plan claims {got!r}, expected {want!r}")
    plan_body = {k: v for k, v in plan.items() if k != "plan_sha256"}
    plan_sha = canon_sha256(plan_body)
    if plan.get("plan_sha256") == plan_sha:
        report.verified("plan-identity", plan_sha)
    else:
        report.mismatch(
            "plan-identity",
            f"declared {plan.get('plan_sha256')}, recomputed {plan_sha}",
        )
    # The plan's own context binds the *projection* digests (present even
    # when admission later refuses the document) — a plan cut against
    # other material is foreign even if its self-digest is honest.
    plan_ctx = plan.get("context") or {}
    for field, want in (
        ("program_sha256", program_sem),
        ("library_sha256", library_sem),
    ):
        got = plan_ctx.get(field)
        if got == want:
            report.verified(f"plan-context.{field}", want)
        else:
            report.mismatch(
                f"plan-context.{field}",
                f"plan claims {got}; the supplied document projects to {want}",
            )
    if args.analysis:
        analysis_doc = load_json(args.analysis)
        # A refused plan publishes no analysis identity — `""` — while a
        # ready plan binds the canonical bytes of the baseline analysis.
        analysis_sha = (
            "" if plan.get("state") == "refused" else canon_sha256(analysis_doc)
        )
        if plan_ctx.get("analysis_sha256") == analysis_sha:
            report.verified("plan-context.analysis_sha256", analysis_sha)
        else:
            report.mismatch(
                "plan-context.analysis_sha256",
                f"plan claims {plan_ctx.get('analysis_sha256')}; "
                f"supplied analysis recomputes {analysis_sha}",
            )
    if context.get("plan_sha256") == plan_sha:
        report.verified("context-plan-link", "context binds the recomputed plan digest")
    else:
        report.mismatch(
            "context-plan-link",
            f"context claims {context.get('plan_sha256')}, recomputed {plan_sha}",
        )

    if plan.get("state") != "ready":
        report.not_checked(
            "plan-invocations",
            f"plan is `{plan.get('state')}` — no invocation set to check",
        )
    else:
        expected_sites = external_sites(
            program, library, reachable_bindings(program, plan_replay)
        )
        declared_sites = {i["at"] for i in plan.get("invocations", [])}
        if declared_sites == expected_sites:
            report.verified(
                "plan-invocations", f"{len(declared_sites)} external site(s)"
            )
        else:
            report.mismatch(
                "plan-invocations",
                f"declared {sorted(declared_sites)}, expected {sorted(expected_sites)}",
            )
        for inv in plan.get("invocations", []):
            site = inv["at"]
            index = int(site[len("body["):-1])
            step = program["body"][index]
            method = plan_replay.methods.get(step.get("apply", ""), {})
            problems = []
            if inv.get("bind") != step.get("bind"):
                problems.append("bind")
            if inv.get("method") != step.get("apply"):
                problems.append("method")
            if inv.get("executable") != (
                method.get("implementation") or {}
            ).get("executable"):
                problems.append("executable")
            if sorted(inv.get("effects", [])) != sorted(
                canonical_sort(method.get("effects", []), NODE)
            ):
                problems.append("effects")
            produces = (method.get("implementation") or {}).get("produces")
            if inv.get("produces") != produces:
                problems.append("produces")
            arguments = step.get("arguments") or {}
            for slot, pin_decl in (inv.get("inputs") or {}).items():
                ref = arguments.get(slot, {}).get("ref")
                operand = plan_replay.env.get(ref)
                if pin_decl.get("state") == "unstaged":
                    if operand is not None and operand.value is not None:
                        problems.append(f"input {slot} marked unstaged but a value exists")
                    continue
                if operand is None or staged_input_sha256(operand) != pin_decl.get("sha256"):
                    problems.append(f"input {slot} digest")
            if sorted((inv.get("inputs") or {}).keys()) != sorted(arguments.keys()):
                problems.append("input slots")
            if problems:
                report.mismatch(
                    f"plan.invocation[{site}]",
                    "mismatched fields: " + ", ".join(problems),
                )
            else:
                report.verified(
                    f"plan.invocation[{site}]",
                    "bind/method/executable/inputs recompute",
                )

    # -- observation admission (O1) -------------------------------------------
    if observations_admitted:
        # The evaluation context binds the *whole* canonical document —
        # including whatever self-digest field it declared.
        obs_full_sha = sha256_bytes(canonicalize_json(
            Path(args.observations).read_bytes()))
        if context.get("observations_sha256") == obs_full_sha:
            report.verified("context-observations-link", obs_full_sha)
        else:
            report.mismatch(
                "context-observations-link",
                f"context claims {context.get('observations_sha256')}, "
                f"supplied document hashes to {obs_full_sha}",
            )
        if schema_ok:
            # A real observations document's own digest covers its body
            # minus the self-digest field.
            obs_body = {
                k: v for k, v in observations.items()
                if k != "observations_sha256"
            }
            obs_sha = canon_sha256(obs_body)
            if observations.get("observations_sha256") == obs_sha:
                report.verified("observations-identity", obs_sha)
            else:
                report.mismatch(
                    "observations-identity",
                    f"declared {observations.get('observations_sha256')}, "
                    f"recomputed {obs_sha}",
                )
        else:
            # A document that decodes but is not an observations document
            # explains itself via a document finding, not a self-digest.
            if evaluation.get("document_findings"):
                report.verified(
                    "document-findings",
                    "the foreign document explains itself in the record",
                )
            else:
                report.mismatch(
                    "document-findings",
                    "a schema-foreign observations document must carry a "
                    "document finding",
                )
    else:
        # Malformed: the record must carry the empty context digest and a
        # document finding.
        if context.get("observations_sha256") == "":
            report.verified(
                "context-observations-link",
                "empty digest — the record binds a malformed document honestly",
            )
        else:
            report.mismatch(
                "context-observations-link",
                "record claims a digest for a malformed observations document",
            )
        if evaluation.get("document_findings"):
            report.verified(
                "document-findings",
                "the malformed document explains itself in the record",
            )
        else:
            report.mismatch(
                "document-findings",
                "malformed observations supplied but no document finding records it",
            )
    context_ok = (
        observations.get("plan_sha256") == plan.get("plan_sha256")
        and plan.get("state") == "ready"
    )
    site_map: dict[int, dict] = {}
    # Expected `observations[]` outcome rows: (at, bind, state, detail).
    expected_outcomes: list[tuple] = []
    claimed: dict[int, list] = {}
    invocation_sites = {i["at"] for i in plan.get("invocations", [])}
    for record in observations.get("observations", []):
        at = record.get("at", "")
        index = None
        if at.startswith("body[") and at.endswith("]"):
            try:
                index = int(at[5:-1])
            except ValueError:
                pass
        if index is None or not context_ok or at not in invocation_sites:
            # Rejected before binding — the outcome row carries the
            # record's own claimed `bind`.
            expected_outcomes.append((
                at,
                record.get("bind", ""),
                "rejected",
                "observation names no external-invocation site in this plan"
                if context_ok
                else "the supplied observations do not belong to this plan",
            ))
            continue
        claimed.setdefault(index, []).append(record)
    for index, records in claimed.items():
        at = f"body[{index}]"
        if len(records) > 1:
            expected_outcomes.append((
                at,
                "",
                "rejected",
                f"{len(records)} observation records claim this site — "
                "ambiguous, none admitted",
            ))
        else:
            site_map[index] = records[0]

    replay = Replay(program, library, lifecycle)
    # O1 binds against the plan digest the record *declares* — the
    # recomputation above is a separate check.
    replay.run(
        evaluating=True,
        site_map=site_map,
        expected_plan_sha256=plan.get("plan_sha256"),
    )
    for index in site_map:
        at = f"body[{index}]"
        if index in replay.observed_sites:
            continue  # an admitted record leaves no outcome row
        if at in replay.rejected_observations:
            expected_outcomes.append(
                (at, "", "rejected", replay.rejected_observations[at])
            )
        # A silent admission failure (`admit_value` returns None) leaves no
        # row and no event — the record is honest in saying nothing.
    # Absent-observation outcomes: every planned invocation no supplied
    # record answered — rejected ones still count as supplied.
    supplied_ats = {o.get("at") for o in observations.get("observations", [])}
    for inv in plan.get("invocations", []):
        if inv["at"] not in supplied_ats:
            expected_outcomes.append((
                inv["at"],
                inv.get("bind", ""),
                "absent",
                "no observation supplied for this invocation — its "
                "obligations stay open",
            ))
    recorded_outcomes = Counter(
        (o.get("at"), o.get("bind"), o.get("state"), o.get("detail"))
        for o in evaluation.get("observations", [])
    )
    expected_outcomes_c = Counter(expected_outcomes)
    if recorded_outcomes != expected_outcomes_c:
        report.mismatch(
            "observation-outcomes",
            f"missing: {sorted((expected_outcomes_c - recorded_outcomes).elements())}; "
            f"extra: {sorted((recorded_outcomes - expected_outcomes_c).elements())}",
        )
    else:
        report.verified(
            "observation-outcomes",
            f"{len(expected_outcomes)} observation outcome row(s) recompute",
        )
    outcome_rows = {o.get("at"): o for o in evaluation.get("observations", [])}
    observe_events = {
        (e.get("subject"), e.get("state"))
        for e in evaluation.get("applications", [])
        if e.get("rule") == "observe"
    }
    findings = evaluation.get("findings") or []
    # Per-site verification: admitted → no row + established event; rejected
    # → row + rejected event + observation_foreign finding; silent (typed
    # admission failed) → no row, no event, no foreign finding.
    for index in sorted(site_map):
        at = f"body[{index}]"
        if index in replay.observed_sites:
            if (at, "established") in observe_events:
                report.verified(
                    f"observation[{at}]", "admitted (observe → established)"
                )
            else:
                report.mismatch(
                    f"observation[{at}]",
                    "replay admits this record but no `observe → "
                    "established` event is recorded",
                )
        elif at in replay.rejected_observations:
            if (at, "rejected") not in observe_events:
                report.mismatch(
                    f"observation[{at}]",
                    "replay rejects this record but no `observe → rejected` "
                    "event is recorded",
                )
            elif any(
                f.get("kind") == "observation_foreign" and f.get("at") == at
                for f in findings
            ):
                report.verified(
                    f"observation[{at}]", "rejected — observation_foreign recorded"
                )
            else:
                report.mismatch(
                    f"observation-finding[{at}]",
                    "a record rejected at the digest binding carries no "
                    "observation_foreign finding",
                )
        else:
            # Silent admission failure — the honest record carries neither a
            # row nor an event for this site.
            if (at, "established") not in observe_events and (
                at, "rejected"
            ) not in observe_events:
                report.verified(
                    f"observation[{at}]",
                    "typed admission failed — the site stays unobserved",
                )
            else:
                report.mismatch(
                    f"observation[{at}]",
                    "a silently-refused record still carries an observe event",
                )

    # -- obligation + requirement replay --------------------------------------
    # Completeness is symmetric: every replay-derived obligation must be
    # present in the record AND every recorded row must be one the replay
    # produced. A record may neither omit an open obligation nor invent one.
    for obligation in evaluation.get("obligations", []):
        oid = obligation.get("subject")
        rederived = replay.obligation_states.get(oid)
        if oid is None:
            report.mismatch(
                "obligation[?]",
                "the record carries an obligation row with no subject",
            )
            continue
        if rederived is None:
            report.mismatch(
                f"obligation[{oid}]",
                "the record claims an obligation the replay never produced",
            )
        elif (
            obligation.get("state"),
            obligation.get("rule"),
            obligation.get("detail"),
        ) != (rederived[1], rederived[0], rederived[2]):
            report.mismatch(
                f"obligation[{oid}]",
                f"record claims `{obligation.get('rule')}`/"
                f"`{obligation.get('state')}` `{obligation.get('detail')}`, "
                f"replay derives `{rederived[0]}`/`{rederived[1]}` "
                f"`{rederived[2]}`",
            )
        else:
            report.verified(
                f"obligation[{oid}]", f"{rederived[0]}: {rederived[1]}"
            )
    recorded_obligation_ids = {
        o.get("subject") for o in evaluation.get("obligations", [])
    }
    for oid in sorted(set(replay.obligation_states) - recorded_obligation_ids):
        report.mismatch(
            f"obligation[{oid}]",
            "replay derives this obligation but the record omits it",
        )

    verdicts = replay.requirement_reports()
    for rid, recorded in (evaluation.get("requirements") or {}).items():
        expected = verdicts.get(rid)
        if expected is None:
            report.mismatch(
                f"requirement[{rid}]",
                "the record claims a requirement the replay never produced",
            )
            continue
        want = expected["verdict"]
        same = (
            recorded.get("status") == want["status"]
            and recorded.get("rule") == want["rule"]
            and recorded.get("detail") == want["detail"]
            and recorded.get("subject_value") == expected["declared_value"]
        )
        if same:
            report.verified(
                f"requirement[{rid}]",
                f"{want['status']}: {want['rule']}",
            )
        else:
            report.mismatch(
                f"requirement[{rid}]",
                f"record claims {recorded.get('status')}/{recorded.get('rule')} "
                f"`{recorded.get('detail')}`; replay derives "
                f"{want['status']}/{want['rule']} `{want['detail']}`",
            )
    for rid in sorted(set(verdicts) - set(evaluation.get("requirements") or {})):
        report.mismatch(
            f"requirement[{rid}]",
            "replay derives this requirement verdict but the record omits it",
        )

    # -- application events ----------------------------------------------------
    # The record's `applications` rows are the claimed event log: `observe`
    # rows per binding attempt and runtime obligation, `discharge` rows for
    # each external-method ensures. Compare as a multiset — a forged or
    # missing row means the record's narration does not follow from the
    # derivation it claims.
    expected_events = replay.events
    recorded_events = [
        (e.get("rule"), e.get("subject"), e.get("state"), e.get("detail"))
        for e in evaluation.get("applications", [])
    ]
    if Counter(recorded_events) == Counter(expected_events):
        report.verified(
            "application-events",
            f"{len(expected_events)} application event(s) recompute",
        )
    else:
        missing = Counter(expected_events) - Counter(recorded_events)
        extra = Counter(recorded_events) - Counter(expected_events)
        report.mismatch(
            "application-events",
            f"record's application log diverges — missing: {sorted(missing.elements())}; "
            f"extra: {sorted(extra.elements())}",
        )

    # -- record identities ----------------------------------------------------
    context_sha = canon_sha256(context)
    if evaluation.get("context_sha256") == context_sha:
        report.verified("context-identity", context_sha)
    else:
        report.mismatch(
            "context-identity",
            f"declared {evaluation.get('context_sha256')}, recomputed {context_sha}",
        )
    eval_body = {k: v for k, v in evaluation.items() if k != "evaluation_sha256"}
    eval_sha = canon_sha256(eval_body)
    if evaluation.get("evaluation_sha256") == eval_sha:
        report.verified("evaluation-identity", eval_sha)
    else:
        report.mismatch(
            "evaluation-identity",
            f"declared {evaluation.get('evaluation_sha256')}, recomputed {eval_sha}",
        )
    if args.analysis:
        analysis_doc = load_json(args.analysis)
        analysis_sha = canon_sha256(analysis_doc)
        if context.get("analysis_sha256") == analysis_sha:
            report.verified("analysis-identity", analysis_sha)
        else:
            report.mismatch(
                "analysis-identity",
                f"context claims {context.get('analysis_sha256')}, "
                f"supplied analysis recomputes {analysis_sha}",
            )
    else:
        report.not_checked(
            "analysis-identity",
            "the analysis document was not supplied; its digest cannot be recomputed",
        )
    return report


# ---------------------------------------------------------------------------
# explain — semantic-edge diffing between two evaluations of one program
# ---------------------------------------------------------------------------


def explain(args) -> Report:
    report = Report("language-explain")
    old = load_json(args.evaluation_old)
    new = load_json(args.evaluation_new)
    program = load_json(args.program)

    for rid in sorted(
        set(old.get("requirements", {})) | set(new.get("requirements", {}))
    ):
        a = {
            k: v
            for k, v in (old.get("requirements", {}).get(rid) or {}).items()
            if k in ("status", "rule", "subject_value")
        }
        b = {
            k: v
            for k, v in (new.get("requirements", {}).get(rid) or {}).items()
            if k in ("status", "rule", "subject_value")
        }
        if a != b:
            subject = next(
                (
                    r["subject"]["ref"]
                    for r in program.get("requirements", [])
                    if r["id"] == rid
                ),
                "?",
            )
            report.verified(
                f"change.requirement[{rid}]",
                f"subject `{subject}`: "
                f"{a.get('status')}/{a.get('rule')} → "
                f"{b.get('status')}/{b.get('rule')}",
            )
    for oid in sorted(
        {o["subject"] for o in old.get("obligations", [])}
        | {o["subject"] for o in new.get("obligations", [])}
    ):
        a = next((o for o in old["obligations"] if o["subject"] == oid), {})
        b = next((o for o in new["obligations"] if o["subject"] == oid), {})
        if a.get("state") != b.get("state"):
            report.verified(
                f"change.obligation[{oid}]",
                f"{a.get('state')} → {b.get('state')}",
            )
    for at in sorted(
        {o.get("at") for o in old.get("observations", [])}
        | {o.get("at") for o in new.get("observations", [])}
    ):
        a = next((o for o in old.get("observations", []) if o.get("at") == at), {})
        b = next((o for o in new.get("observations", []) if o.get("at") == at), {})
        if a.get("state") != b.get("state"):
            report.verified(
                f"change.observation[{at}]",
                f"{a.get('state')} → {b.get('state')}",
            )
    return report


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def render(report: Report) -> str:
    lines = [f"verify {report.target}"]
    for c in report.checks:
        suffix = (
            f" — {c.detail}" if c.detail else (f" ({c.reason})" if c.reason else "")
        )
        lines.append(f"  {c.status:12} {c.check}{suffix}")
    counts = report.counts()
    lines.append("  totals: " + ", ".join(f"{k}={v}" for k, v in counts.items() if v))
    return "\n".join(lines)


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(prog="language_verify")
    sub = parser.add_subparsers(dest="command", required=True)
    v = sub.add_parser("verify-evaluation")
    for name in ("program", "library", "plan", "observations", "evaluation"):
        v.add_argument(f"--{name}", required=True)
    v.add_argument("--analysis", required=False, default=None)
    # Supplied lifecycle state as `key=state`, repeatable — mirrors the
    # analyzer's caller-supplied material, e.g.
    # `--lifecycle library:thermal-expansion@1=withdrawn`.
    v.add_argument("--lifecycle", action="append", default=[])
    e = sub.add_parser("explain")
    for name in ("program", "evaluation-old", "evaluation-new"):
        e.add_argument(f"--{name}", required=True)
    args = parser.parse_args()
    report = (
        verify_evaluation(args) if args.command == "verify-evaluation" else explain(args)
    )
    print(render(report))
    return 1 if any(c.status == "mismatch" for c in report.checks) else 0


if __name__ == "__main__":
    sys.exit(main())
