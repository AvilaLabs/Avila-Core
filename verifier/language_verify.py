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
from dataclasses import dataclass, field
from fractions import Fraction
from pathlib import Path
from typing import Any, Optional

sys.path.insert(0, str(Path(__file__).resolve().parent))

from avila_core_verify import (  # noqa: E402
    Report,
    canonicalize_json,
    read_authoritative_rational,
    sha256_bytes,
)

PLAN_SCHEMA = "avila.core/execution-plan/v0.1-draft"
OBSERVATIONS_SCHEMA = "avila.core/language-observations/v0.1-draft"


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


def canonical_sort(items: list, role: tuple) -> list:
    """`canonical_order`: declared sets iterate by each element's projected
    canonical bytes (annotation-blind); on projection failure the raw
    serialized bytes keep the key content-addressed."""

    def key(item):
        try:
            return canon_bytes(project(item, role, ""))
        except Exception:
            return canon_bytes(item)

    return sorted(items, key=key)


# ---------------------------------------------------------------------------
# Numbers and values
# ---------------------------------------------------------------------------


def exact_number(text: str) -> Fraction:
    """The `ExactNumber` grammar: canonical rationals `p/q`, else decimal."""
    if "/" in text:
        return read_authoritative_rational(text)
    if text.strip() != text or not text:
        raise ValueError("not an exact-number literal")
    if all(c.isdigit() or c == "-" or c == "+" for c in text):
        return Fraction(int(text))
    if not any(c.isdigit() for c in text):
        raise ValueError(f"not an exact-number literal: {text!r}")
    return Fraction(text)  # decimal digit strings — Fraction parses exactly


def canonical_rational(value: Fraction) -> str:
    if value.denominator == 1:
        return str(value.numerator)
    return f"{value.numerator}/{value.denominator}"


CLAIMS = ("exact", "enclosure", "nominal")


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
    pass


def parse_expression(text: str):
    """`expr := term (('+'|'-') term)*`, `term := factor ('*' factor)*`,
    `factor := '(' expr ')' | name | name '(' args ')'`."""
    pos = 0
    n = len(text)

    def skip_ws():
        nonlocal pos
        while pos < n and text[pos].isspace():
            pos += 1

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
        nonlocal pos
        skip_ws()
        if pos >= n:
            raise EvalFailure("expected an identifier or `(`")
        c = text[pos]
        if c == "(":
            pos += 1
            inner = parse_expr()
            skip_ws()
            if pos >= n or text[pos] != ")":
                raise EvalFailure("expected `)`")
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
                args = parse_arguments()
                if name not in EXPR_RULES:
                    raise EvalFailure(f"unknown primitive rule `{name}`")
                return ("call", EXPR_RULES[name], args)
            return ("name", name)
        raise EvalFailure("expected an identifier or `(`")

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
            raise EvalFailure("expected `,` or `)`")

    expression = parse_expr()
    skip_ws()
    if pos != n:
        raise EvalFailure(f"trailing input at byte {pos} of `{text}`")
    return expression


def parse_ensures(expression: str):
    if "=" not in expression:
        raise EvalFailure(f"ensures expression `{expression}` has no `=`")
    lhs, rhs = expression.split("=", 1)
    if lhs.strip() != "output":
        raise EvalFailure(
            f"ensures expression `{expression}` must bind `output` on the left"
        )
    return parse_expression(rhs.strip())


def eval_expression(expr, operands: dict, kind_products: dict) -> SemVal:
    """interval.add/sub/mul — the two-operand primitive rules with exact /
    enclosure arithmetic, quantity-kind checks, and relation merging."""
    kind = expr[0]
    if kind == "name":
        name = expr[1]
        if name not in operands:
            raise EvalFailure(f"unbound operand `{name}`")
        return operands[name]
    _, rule, args = expr
    if len(args) != 2:
        raise EvalFailure(f"{rule} takes 2 operands; {len(args)} supplied")
    left = eval_expression(args[0], operands, kind_products)
    right = eval_expression(args[1], operands, kind_products)
    if left.ty.claim == "nominal" or right.ty.claim == "nominal":
        raise EvalFailure("nominal operand")
    if rule in ("add", "sub"):
        if left.ty.quantity_kind != right.ty.quantity_kind:
            raise EvalFailure("kind mismatch")
        quantity_kind, unit = left.ty.quantity_kind, left.unit
    else:
        pair = kind_products.get((left.ty.quantity_kind, right.ty.quantity_kind))
        if pair is None:
            raise EvalFailure("no kind_products row")
        quantity_kind, unit = pair
    relations = dict(left.ty.relations)
    for key, entity in right.ty.relations.items():
        if relations.get(key, entity) != entity:
            raise EvalFailure("relation conflict")
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
    return SemVal(
        ty=QuantityType(quantity_kind, claim, relations),
        unit=unit,
        value=value,
        state="established" if value is not None else "unestablished",
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

    def __init__(self, program: dict, library: dict):
        self.program = program
        self.library = library
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
        self.obligation_states: dict[str, str] = {}
        self.observed_values: dict[int, Optional[Numeric]] = {}
        self.observed_sites: set[int] = set()
        self.site_map: dict[int, dict] = {}
        self.expected_plan_sha256: Optional[str] = None
        self.denied: set = set()

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
        refs = []
        if "apply" in step:
            method = self.methods.get(step["apply"])
            if method:
                for slot in method.get("inputs", {}):
                    arg = (step.get("arguments") or {}).get(slot)
                    if arg:
                        refs.append(arg["ref"])
        elif "infer" in step:
            refs = [a["ref"] for a in step["infer"].get("arguments", [])]
        return refs

    def admissible_witnesses(self) -> dict:
        """(proposition, at-map-key) → admissible premise indices: needs
        `established_by`, not denied, not in a cyclic assumption group, and
        `provenance_disjoint` must not cover operands sharing static edges."""
        premises = self.program.get("premises", [])
        denied = {
            (a["denies"], tuple(sorted(a.get("at", {}).items())))
            for a in self.program.get("assumptions", [])
            if a.get("denies")
        }
        inadmissible = set()
        for i, p in enumerate(premises):
            if not p.get("established_by"):
                inadmissible.add(i)
            key = (p["proposition"], tuple(sorted(p.get("at", {}).items())))
            if key in denied:
                inadmissible.add(i)
        prop_map: dict = {}
        for i, p in enumerate(premises):
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
        reach = []
        for i in range(len(premises)):
            seen, stack = set(), list(dep_edges[i])
            while stack:
                node = stack.pop()
                if node not in seen:
                    seen.add(node)
                    stack.extend(dep_edges[node])
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
        return {
            key: [i for i in idxs if i not in inadmissible]
            for key, idxs in prop_map.items()
            if any(i not in inadmissible for i in idxs)
        }

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

    def run(self, evaluating: bool, site_map: dict, expected_plan_sha256=None):
        self.site_map = site_map
        self.expected_plan_sha256 = expected_plan_sha256
        for inp in canonical_sort(self.program.get("inputs", []), INPUT):
            ty = QuantityType(
                inp["type"]["quantity_kind"],
                inp["type"].get("claim", "nominal"),
                slot_relations(inp["type"]),
            )
            binding = inp.get("binding") or {}
            value = None
            state = "unestablished"
            edges = set()
            if binding.get("state") == "bound" and binding.get("value") is not None:
                value = admit_value(binding["value"], ty, self.kinds)
                if value is not None:
                    state = "established"
            unit = (binding.get("value") or {}).get("unit", "")
            if (binding.get("source") or {}).get("edge"):
                edges.add(binding["source"]["edge"])
            self.env[inp["id"]] = SemVal(ty, unit, value, state, [], edges)
            if edges:
                self.edges[inp["id"]] = set(edges)
        self.compute_static_edges()
        admissible = self.admissible_witnesses()
        for index, step in enumerate(self.program.get("body", [])):
            self.step(index, step, evaluating, admissible)
        self.discharge_all(admissible)

    def step(self, index: int, step: dict, evaluating: bool, admissible: dict):
        bind = step["bind"]
        if "apply" in step:
            self.apply(index, bind, step["apply"], step.get("arguments") or {},
                       evaluating, admissible)
        elif "infer" in step:
            self.infer(index, bind, step["infer"])
        elif "import" in step:
            self.do_import(index, bind, step["import"])
        elif "hole" in step or "goal" in step:
            slot = step.get("hole") or step.get("goal")
            ty = QuantityType(
                slot["quantity_kind"],
                slot.get("claim", "nominal"),
                slot_relations(slot),
            )
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
            return
        references = [a["ref"] for a in infer.get("arguments", [])]
        self.deps[bind] = references
        inherited = self.propagated_block(references)
        operands = {}
        for r in references:
            v = self.env.get(r)
            if v is None:
                return
            operands[r] = v
        expr = ("call", rule, [("name", r) for r in references])
        try:
            value = eval_expression(expr, operands, self.kind_products)
        except EvalFailure:
            return
        if inherited:
            self.blocked[bind] = inherited
        self.env[bind] = value

    def do_import(self, index: int, bind: str, import_decl: dict):
        ty = QuantityType(
            import_decl["type"]["quantity_kind"],
            import_decl["type"].get("claim", "nominal"),
            slot_relations(import_decl["type"]),
        )
        value = admit_value(import_decl["value"], ty, self.kinds)
        assumptions = [
            ScopedProposition.of(s["proposition"], s.get("at", {}))
            for s in canonical_sort(
                import_decl.get("assumptions") or [], SCOPED_ASSERTION
            )
        ]
        edges = set()
        src = import_decl.get("source") or {}
        if src.get("edge"):
            edges.add(src["edge"])
        malformed = (
            value is None
            or "assumptions" not in import_decl
            or not src
            or src.get("kind") not in ("assertion", "certificate", "declared")
            or (src.get("kind") == "certificate" and not src.get("digest"))
        )
        self.env[bind] = SemVal(
            ty,
            import_decl["value"].get("unit", ""),
            value,
            "unestablished" if malformed else "established",
            assumptions,
            edges,
        )

    def apply(self, index: int, bind: str, method_id: str, arguments: dict,
              evaluating: bool, admissible: dict):
        method = self.methods.get(method_id)
        if method is None:
            return
        at = f"body[{index}]"
        operand_refs = []
        slot_values: dict[str, SemVal] = {}
        bad = False
        variables: dict[str, str] = {}
        for slot, decl in method.get("inputs", {}).items():
            arg = arguments.get(slot)
            if arg is None:
                bad = True
                continue
            operand_refs.append(arg["ref"])
            value = self.env.get(arg["ref"])
            if value is None:
                bad = True
                continue
            for relation, var in slot_relations(decl).items():
                entity = value.ty.relations.get(relation)
                if entity is None:
                    bad = True
                    continue
                if var in variables and variables[var] != entity:
                    bad = True
                variables[var] = entity
            slot_values[slot] = value
        if arguments:
            for slot in arguments:
                if slot not in method.get("inputs", {}):
                    bad = True
        self.deps[bind] = operand_refs
        inherited = self.propagated_block(operand_refs)

        output_ty = QuantityType(
            method["output"]["quantity_kind"],
            method["output"].get("claim", "nominal"),
            {},
        )
        if bad:
            self.poison(bind, output_ty)
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
            self.poison(bind, output_ty)
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
            state, support_a, support_e = self.check_requires(
                method, requirement, variables, slot_values, arguments, admissible
            )
            self.obligation_states[oid] = state
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
            except EvalFailure:
                pass
        elif impl_kind == "external":
            declared_value = None
            conflicted = False
            for ensures in canonical_sort(method.get("ensures", []), ENSURES):
                try:
                    computed = eval_expression(
                        parse_ensures(ensures["expression"]),
                        slot_values,
                        self.kind_products,
                    )
                    if computed.value is not None:
                        if declared_value is not None and declared_value != computed.value:
                            conflicted = True
                        declared_value = computed.value
                except EvalFailure:
                    pass
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
                    index, method, slot_values, self.site_map[index]
                )
            if observed is not None:
                self.observed_sites.add(index)
                self.observed_values[index] = observed.value
                value, unit, state = observed.value, observed.unit, "established"
            else:
                value, unit, state = declared_value, unit, declared_state

        # Runtime obligations: `body[i].observation` when no ensures are
        # declared, else each `ensures[i]` replayed under evaluation.
        if impl_kind == "external" and not method.get("ensures"):
            oid = f"{at}.observation"
            if not evaluating:
                ob_state = "runtime"
            elif index in self.observed_sites:
                ob_state = "discharged"
            else:
                ob_state = "open"
                block = block or "not_evaluated.obligation_unmet"
            self.obligation_states[oid] = ob_state
        for position, (_, ensures) in enumerate(
            (p, e)
            for p, e in enumerate(
                canonical_sort(method.get("ensures", []), ENSURES)
            )
        ):
            oid = f"{at}.ensures[{position}]"
            if impl_kind == "primitive":
                o_state = "open"
                try:
                    expected = eval_expression(
                        parse_ensures(ensures["expression"]),
                        slot_values,
                        self.kind_products,
                    ).value
                    if expected is not None and value is not None:
                        o_state = "discharged" if expected == value else "refuted"
                except EvalFailure:
                    pass
            elif not evaluating:
                o_state = "runtime"
            elif index not in self.observed_sites:
                o_state = "open"
            else:
                o_state = "open"
                try:
                    expected = eval_expression(
                        parse_ensures(ensures["expression"]),
                        slot_values,
                        self.kind_products,
                    ).value
                    got = self.observed_values.get(index)
                    if expected is not None and got is not None:
                        o_state = "discharged" if expected == got else "refuted"
                except EvalFailure:
                    pass
            self.obligation_states[oid] = o_state
            if o_state == "refuted":
                refuted = True
                block = block or "not_evaluated.obligation_refuted"
            elif o_state == "open":
                block = block or "not_evaluated.obligation_unmet"

        if block:
            self.blocked[bind] = block
        if refuted:
            self.poison(bind, output_ty)
            return

        # Instantiated `assumes` at the applied scope.
        assumptions = []
        for name in canonical_sort(method.get("assumes", []), NODE):
            params = self.propositions.get(name, {}).get("params", [])
            at_map = {}
            for param in params:
                for var, relation in method.get("variables", {}).items():
                    if relation == param and var in variables:
                        at_map[param] = variables[var]
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
        """→ (state, support_assumptions, support_edges). Witness support is
        `independence` discharging premises' own assumptions — they join the
        conclusion's residual cone."""
        if requirement.get("kind") == "domain_containment":
            def entity_for(path):
                if "." not in (path or ""):
                    return None
                relation, field = (path or "").split(".", 1)
                for var, rel in method.get("variables", {}).items():
                    if rel == relation:
                        entity = variables.get(var)
                        return (field, entity) if entity else None
                return None

            d = entity_for(requirement.get("domain"))
            w = entity_for(requirement.get("within"))
            if d is None or w is None:
                return "open", [], set()
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
                return "open", [], set()
            within_interval = within.get(domain.get("quantity_kind"))
            if within_interval is None or within_interval.get("unit") != domain.get("unit"):
                return "open", [], set()
            try:
                dl, du = exact_number(domain["lower"]), exact_number(domain["upper"])
                wl, wu = (
                    exact_number(within_interval["lower"]),
                    exact_number(within_interval["upper"]),
                )
            except Exception:
                return "open", [], set()
            return ("discharged" if wl <= dl <= du <= wu else "refuted"), [], set()
        if requirement.get("kind") == "scope_check":
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
                return "open", [], set()
            return (
                "discharged" if scope_refines(scope, required) else "refuted"
            ), [], set()
        if requirement.get("kind") == "provenance_disjoint":
            bound = sorted(
                {
                    arguments[s]["ref"]
                    for s in requirement.get("over", [])
                    if s in arguments
                }
            )
            edge_sets = [
                self.effective_edges(b, admissible, set()) for b in bound
            ]
            if not all(edge_sets):
                return "open", [], set()
            shared = any(
                edge_sets[a] & edge_sets[b]
                for a in range(len(edge_sets))
                for b in range(a + 1, len(edge_sets))
            )
            return ("refuted" if shared else "discharged"), [], set()
        if requirement.get("kind") == "independence":
            bound = {
                arguments[s]["ref"]
                for s in requirement.get("over", [])
                if s in arguments
            }
            scope = {
                method["variables"][var]: entity
                for var, entity in variables.items()
                if var in method.get("variables", {})
            }
            scope_key = tuple(sorted(scope.items()))
            admissible_for = admissible.get(("independent", scope_key), [])
            witnesses = [
                i
                for i, p in enumerate(self.program.get("premises", []))
                if p["proposition"] == "independent"
                and p.get("at", {}) == scope
                and set((p.get("arguments") or {}).get("over", [])) == bound
                and i in admissible_for
            ]
            if not witnesses:
                return "open", [], set()
            support_a, support_e = [], set()
            for i in witnesses:
                premise = self.program["premises"][i]
                for s in premise.get("assumptions", []):
                    prop = ScopedProposition.of(
                        s["proposition"], s.get("at", {})
                    )
                    if prop not in support_a:
                        support_a.append(prop)
                edge = (premise.get("established_by") or {}).get("edge")
                if edge:
                    support_e.add(edge)
            return "discharged", support_a, support_e
        return "open", [], set()  # unknown kinds stay open — never discharged

    # -- O1 binding (spec §7) ----------------------------------------------

    def bind_observation(
        self, index: int, method: dict, slot_values: dict, record: dict
    ) -> Optional[SemVal]:
        at = f"body[{index}]"
        receipt = record.get("receipt") or {}
        if "sha256:" + _hex(sha256_bytes(canon_bytes(receipt))) != record.get(
            "receipt_sha256"
        ):
            return None
        if record.get("at") != at or receipt.get("at") != at:
            return None
        if receipt.get("plan_sha256") != (self.expected_plan_sha256 or ""):
            return None
        declared_executable = (
            method.get("implementation") or {}
        ).get("executable", "")
        if receipt.get("executable") != declared_executable:
            return None
        expected_inputs = {
            slot: sha
            for slot, v in slot_values.items()
            if (sha := staged_input_sha256(v)) is not None
        }
        if dict(record.get("inputs") or {}) != expected_inputs:
            return None
        receipt_inputs = {
            i["slot"]: i["sha256"] for i in receipt.get("inputs", [])
        }
        if receipt_inputs != dict(record.get("inputs") or {}):
            return None
        identity = canon_sha256(
            {
                "executable": receipt.get("executable"),
                "executable_sha256": receipt.get("executable_sha256"),
                "inputs": receipt.get("inputs", []),
                "invocation": receipt.get("invocation"),
            }
        )
        if receipt.get("invocation_sha256") != identity:
            return None
        if receipt.get("status") != "completed":
            return None
        output_decl = record.get("output")
        if output_decl is None:
            return None
        output_sha = "sha256:" + _hex(sha256_bytes(canon_bytes(output_decl)))
        if output_sha != record.get("output_sha256"):
            return None
        outputs = receipt.get("outputs") or []
        if not outputs or outputs[0].get("sha256") != output_sha:
            return None
        output_ty = QuantityType(
            method["output"]["quantity_kind"],
            method["output"].get("claim", "nominal"),
            {},
        )
        numeric = admit_value(output_decl, output_ty, self.kinds)
        if numeric is None:
            return None
        return SemVal(
            output_ty,
            (impl_produces := (method.get("implementation") or {}).get("produces") or {}).get("unit", ""),
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
        return {
            "status": status,
            "rule": f"bounded.{symbol}",
            "detail": detail,
        }

    def requirement_reports(self) -> dict:
        reports = {}
        scenarios = self.program.get("entities", {}).get("scenarios", {})
        for requirement in canonical_sort(
            self.program.get("requirements", []), REQUIREMENT
        ):
            rid = requirement["id"]
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


def external_sites(program: dict, library: dict) -> set:
    methods = {m["id"]: m for m in library.get("methods", [])}
    return {
        f"body[{i}]"
        for i, step in enumerate(program.get("body", []))
        if (methods.get(step.get("apply", ""), {}).get("implementation") or {})
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
        # Decodes but isn't an observations document — foreign in toto, and
        # its canonical digest still binds in the context.
        report.mismatch(
            "observations-admission",
            f"schema `{observations.get('schema_version')}` / profile "
            f"`{observations.get('profile')}` is not an observations document",
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
    for name, got, want in (
        ("program-semantic-identity", context.get("program_sha256"), program_sem),
        ("library-semantic-identity", context.get("library_sha256"), library_sem),
    ):
        if got == want:
            report.verified(name, want)
        else:
            report.mismatch(name, f"record claims {got}, rederived {want}")
    pin = program.get("library") or {}
    if pin.get("semantic_sha256") == library_sem:
        report.verified("library-pin", "program's pin matches the supplied library")
    else:
        report.mismatch(
            "library-pin",
            f"program pins {pin.get('semantic_sha256')}; supplied library rederives {library_sem}",
        )

    # -- plan identity + invocations -----------------------------------------
    plan_body = {k: v for k, v in plan.items() if k != "plan_sha256"}
    plan_sha = canon_sha256(plan_body)
    if plan.get("plan_sha256") == plan_sha:
        report.verified("plan-identity", plan_sha)
    else:
        report.mismatch(
            "plan-identity",
            f"declared {plan.get('plan_sha256')}, recomputed {plan_sha}",
        )
    if context.get("plan_sha256") == plan_sha:
        report.verified("context-plan-link", "context binds the recomputed plan digest")
    else:
        report.mismatch(
            "context-plan-link",
            f"context claims {context.get('plan_sha256')}, recomputed {plan_sha}",
        )

    # Plan-time replay (analysis mode) re-derives staged input digests.
    plan_replay = Replay(program, library)
    plan_replay.run(evaluating=False, site_map={})
    if plan.get("state") != "ready":
        report.not_checked(
            "plan-invocations",
            f"plan is `{plan.get('state')}` — no invocation set to check",
        )
    else:
        expected_sites = external_sites(program, library)
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
        obs_body = {k: v for k, v in observations.items() if k != "observations_sha256"}
        obs_sha = canon_sha256(obs_body)
        if observations.get("observations_sha256") == obs_sha:
            report.verified("observations-identity", obs_sha)
        else:
            report.mismatch(
                "observations-identity",
                f"declared {observations.get('observations_sha256')}, recomputed {obs_sha}",
            )
        # The evaluation context binds the *whole* document — including the
        # declared self-digest — while the field above covers the body alone.
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
    outcomes_expected: dict[str, str] = {}
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
            outcomes_expected[at] = "rejected"
            continue
        claimed.setdefault(index, []).append(record)
    for index, records in claimed.items():
        at = f"body[{index}]"
        if len(records) > 1:
            outcomes_expected[at] = "rejected"
        else:
            site_map[index] = records[0]
    for inv in plan.get("invocations", []):
        idx = int(inv["at"][5:-1])
        if idx not in site_map and outcomes_expected.get(inv["at"]) != "rejected":
            outcomes_expected[inv["at"]] = "absent"

    replay = Replay(program, library)
    # O1 binds against the plan digest the record *declares* — the
    # recomputation above is a separate check.
    replay.run(
        evaluating=True,
        site_map=site_map,
        expected_plan_sha256=plan.get("plan_sha256"),
    )
    for index in site_map:
        at = f"body[{index}]"
        outcomes_expected[at] = (
            "admitted" if index in replay.observed_sites else "rejected"
        )
    outcome_rows = {o.get("at"): o for o in evaluation.get("observations", [])}
    observe_events = {
        (e.get("subject"), e.get("state"))
        for e in evaluation.get("applications", [])
        if e.get("rule") == "observe"
    }
    for at, expected_state in sorted(outcomes_expected.items()):
        row = outcome_rows.get(at)
        if expected_state == "admitted":
            # An admitted record leaves no outcome row — the `observe →
            # established` application event and discharged obligations are
            # its evidence.
            if (at, "established") in observe_events:
                report.verified(
                    f"observation[{at}]", "admitted (observe → established)"
                )
            else:
                report.mismatch(
                    f"observation[{at}]",
                    "replay admits this record but no `observe → established` "
                    "event is recorded",
                )
        elif row is not None and row.get("state") == expected_state:
            report.verified(f"observation[{at}]", expected_state)
        else:
            report.mismatch(
                f"observation[{at}]",
                f"replay derives `{expected_state}`, record shows "
                f"{row and row.get('state')}",
            )

    # -- obligation + requirement replay --------------------------------------
    for obligation in evaluation.get("obligations", []):
        oid = obligation["subject"]
        rederived = replay.obligation_states.get(oid)
        if rederived is None:
            report.not_checked(
                f"obligation[{oid}]", "not produced by the replay"
            )
        elif obligation.get("state") == rederived:
            report.verified(
                f"obligation[{oid}]", f"{obligation.get('rule')}: {rederived}"
            )
        else:
            report.mismatch(
                f"obligation[{oid}]",
                f"record claims `{obligation.get('state')}`, replay derives `{rederived}`",
            )

    verdicts = replay.requirement_reports()
    for rid, recorded in (evaluation.get("requirements") or {}).items():
        expected = verdicts.get(rid)
        if expected is None:
            report.not_checked(f"requirement[{rid}]", "not replayed")
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
