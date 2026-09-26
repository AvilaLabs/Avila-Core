//! Expression evaluation over the §7.A primitive rules.
//!
//! Two surface forms exist: `implementation.body` writes calls such as
//! `interval.sub(initial_clearance, displacement)`, while `ensures`
//! expressions write infix arithmetic such as
//! `output = calibration * (reading_a + reading_b)`. Both lower to the same
//! `interval.add`/`interval.sub`/`interval.mul` rules — there is no division
//! rule — over operands whose claim must be `exact` or `enclosure`. A nominal
//! operand makes the rule inapplicable; the product of two kinds requires a
//! declared `kind_products` row.

use std::collections::BTreeMap;

use super::model::{ClaimModel, NumericValue, SemanticValue, ValueState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    /// A bound name: an input slot inside a signature, a binding in a body.
    Name(String),
    /// `interval.add` / `interval.sub` / `interval.mul` applied in order.
    Call(PrimitiveRule, Vec<Expression>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveRule {
    Add,
    Sub,
    Mul,
}

impl PrimitiveRule {
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "interval.add",
            Self::Sub => "interval.sub",
            Self::Mul => "interval.mul",
        }
    }
}

/// An expression that does not parse, classified so the analyzer can pick
/// the finding kind: malformed syntax, unknown rule name, or a boundedness
/// refusal rather than a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFailure {
    pub kind: FailureKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Malformed,
    /// Unknown primitive rule name — `unsupported`, not `malformed`.
    Unsupported,
    /// Exceeded a declared profile bound (depth or term count) — `budget`.
    Budget,
}

/// Profile bounds (spec §11): elaboration and evaluation are finite. A
/// document exceeding them produces a `budget` finding, never a crash.
pub const MAX_EXPRESSION_DEPTH: usize = 128;
pub const MAX_EXPRESSION_TERMS: usize = 4096;

/// Parses `lhs = rhs` (ensures form) or a bare expression (infer/impl form).
pub fn parse_ensures(expression: &str) -> Result<Expression, ParseFailure> {
    let Some((lhs, rhs)) = expression.split_once('=') else {
        return Err(ParseFailure {
            kind: FailureKind::Malformed,
            detail: format!("ensures expression `{expression}` has no `=`"),
        });
    };
    if lhs.trim() != "output" {
        return Err(ParseFailure {
            kind: FailureKind::Malformed,
            detail: format!("ensures expression `{expression}` must bind `output` on the left"),
        });
    }
    parse(rhs.trim())
}

/// Parses call form (`interval.add(a, b)`) and infix form (`a * (b + c)`).
/// Recursion is depth-bounded; the term count is bounded — a document beyond
/// the profile's limits is refused, not crashed on.
pub fn parse(input: &str) -> Result<Expression, ParseFailure> {
    let mut parser = Parser {
        input,
        position: 0,
        depth: 0,
        terms: 0,
    };
    let expression = parser.parse_expr()?;
    parser.skip_ws();
    if parser.position != input.len() {
        return Err(ParseFailure {
            kind: FailureKind::Malformed,
            detail: format!("trailing input at byte {} of `{input}`", parser.position),
        });
    }
    Ok(expression)
}

struct Parser<'a> {
    input: &'a str,
    position: usize,
    depth: usize,
    terms: usize,
}

impl<'a> Parser<'a> {
    fn parse_expr(&mut self) -> Result<Expression, ParseFailure> {
        let mut left = self.parse_term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.position += 1;
                    let right = self.parse_term()?;
                    left = Expression::Call(PrimitiveRule::Add, vec![left, right]);
                }
                Some('-') => {
                    self.position += 1;
                    let right = self.parse_term()?;
                    left = Expression::Call(PrimitiveRule::Sub, vec![left, right]);
                }
                _ => return Ok(left),
            }
        }
    }

    fn parse_term(&mut self) -> Result<Expression, ParseFailure> {
        let mut left = self.parse_factor()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.position += 1;
                    let right = self.parse_factor()?;
                    left = Expression::Call(PrimitiveRule::Mul, vec![left, right]);
                }
                _ => return Ok(left),
            }
        }
    }

    fn parse_factor(&mut self) -> Result<Expression, ParseFailure> {
        self.skip_ws();
        if self.depth >= MAX_EXPRESSION_DEPTH {
            return Err(self.failure_at(
                FailureKind::Budget,
                &format!("expression nesting exceeds the depth bound {MAX_EXPRESSION_DEPTH}"),
            ));
        }
        match self.peek() {
            Some('(') => {
                self.position += 1;
                self.depth += 1;
                let inner = self.parse_expr();
                self.depth -= 1;
                let inner = inner?;
                self.skip_ws();
                if self.peek() != Some(')') {
                    return Err(self.failure("expected `)`"));
                }
                self.position += 1;
                Ok(inner)
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let start = self.position;
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@' | '/' | ':') {
                        self.position += 1;
                    } else {
                        break;
                    }
                }
                let name = &self.input[start..self.position];
                self.skip_ws();
                if self.peek() == Some('(') {
                    self.position += 1;
                    self.depth += 1;
                    let arguments = self.parse_arguments();
                    self.depth -= 1;
                    let arguments = arguments?;
                    let rule = match name {
                        "interval.add" => PrimitiveRule::Add,
                        "interval.sub" => PrimitiveRule::Sub,
                        "interval.mul" => PrimitiveRule::Mul,
                        other => {
                            return Err(self.failure_at(
                                FailureKind::Unsupported,
                                &format!("unknown primitive rule `{other}`"),
                            ));
                        }
                    };
                    self.term()?;
                    Ok(Expression::Call(rule, arguments))
                } else {
                    self.term()?;
                    Ok(Expression::Name(name.to_string()))
                }
            }
            _ => Err(self.failure("expected an identifier or `(`")),
        }
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expression>, ParseFailure> {
        let mut arguments = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(')') {
                self.position += 1;
                return Ok(arguments);
            }
            arguments.push(self.parse_expr()?);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.position += 1;
                }
                Some(')') => {
                    self.position += 1;
                    return Ok(arguments);
                }
                _ => return Err(self.failure("expected `,` or `)`")),
            }
        }
    }

    /// Counts one produced term against the bounded term budget.
    fn term(&mut self) -> Result<(), ParseFailure> {
        self.terms += 1;
        if self.terms > MAX_EXPRESSION_TERMS {
            return Err(self.failure_at(
                FailureKind::Budget,
                &format!("expression exceeds the term bound {MAX_EXPRESSION_TERMS}"),
            ));
        }
        Ok(())
    }

    fn skip_ws(&mut self) {
        while self
            .input
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    fn failure(&self, detail: &str) -> ParseFailure {
        self.failure_at(FailureKind::Malformed, detail)
    }

    fn failure_at(&self, kind: FailureKind, detail: &str) -> ParseFailure {
        ParseFailure {
            kind,
            detail: format!("{detail} at byte {} of `{}`", self.position, self.input),
        }
    }
}

/// Why a rule application could not produce a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleFailure {
    /// A nominal operand: the rule is inapplicable — no nominal arithmetic.
    NominalOperand { operand: String },
    /// add/sub on different quantity kinds.
    KindMismatch { left: String, right: String },
    /// add/sub on equal kinds carrying different units — the rules have no
    /// conversion, so coincident units are required.
    UnitMismatch { left: String, right: String },
    /// mul with no declared kind_products row.
    NoProductRow { lhs: String, rhs: String },
    /// Operands disagree on an entity for a shared relation key.
    RelationConflict {
        key: String,
        left: String,
        right: String,
    },
    /// Evaluation exceeded the declared boundedness budget.
    Budget(String),
    /// Arithmetic overflowed the exact-number budget.
    Arithmetic(String),
}

/// The kind table multiplication consults.
pub trait KindProducts {
    fn product(&self, lhs: &str, rhs: &str) -> Option<(String, String)>;
}

/// Evaluates a parsed expression over named operands, producing one typed
/// value. Every operand map unions into the result; shared keys must agree.
/// Evaluation is depth-bounded — the parser already refuses deeper terms,
/// and this bound additionally protects callers constructing expressions
/// programmatically.
pub fn eval(
    expression: &Expression,
    operands: &BTreeMap<String, SemanticValue>,
    products: &impl KindProducts,
) -> Result<SemanticValue, RuleFailure> {
    eval_within(expression, operands, products, MAX_EXPRESSION_DEPTH)
}

fn eval_within(
    expression: &Expression,
    operands: &BTreeMap<String, SemanticValue>,
    products: &impl KindProducts,
    remaining_depth: usize,
) -> Result<SemanticValue, RuleFailure> {
    if remaining_depth == 0 {
        return Err(RuleFailure::Budget(format!(
            "expression evaluation exceeds the depth bound {MAX_EXPRESSION_DEPTH}"
        )));
    }
    match expression {
        Expression::Name(name) => operands
            .get(name)
            .cloned()
            .ok_or_else(|| RuleFailure::Arithmetic(format!("unbound operand `{name}`"))),
        Expression::Call(rule, arguments) => {
            eval_call(*rule, arguments, operands, products, remaining_depth - 1)
        }
    }
}

fn eval_call(
    rule: PrimitiveRule,
    arguments: &[Expression],
    operands: &BTreeMap<String, SemanticValue>,
    products: &impl KindProducts,
    remaining_depth: usize,
) -> Result<SemanticValue, RuleFailure> {
    let arity = 2;
    if arguments.len() != arity {
        return Err(RuleFailure::Arithmetic(format!(
            "{} takes 2 operands; {} supplied",
            rule.name(),
            arguments.len()
        )));
    }
    let left = eval_within(&arguments[0], operands, products, remaining_depth)?;
    let right = eval_within(&arguments[1], operands, products, remaining_depth)?;
    apply_binary(
        rule,
        &left,
        &right,
        products,
        &argument_label(&arguments[0]),
        &argument_label(&arguments[1]),
    )
}

fn argument_label(expression: &Expression) -> String {
    match expression {
        Expression::Name(name) => name.clone(),
        Expression::Call(rule, _) => rule.name().to_string(),
    }
}

fn apply_binary(
    rule: PrimitiveRule,
    left: &SemanticValue,
    right: &SemanticValue,
    products: &impl KindProducts,
    left_name: &str,
    right_name: &str,
) -> Result<SemanticValue, RuleFailure> {
    for (name, operand) in [(left_name, left), (right_name, right)] {
        if operand.ty.claim == ClaimModel::Nominal {
            return Err(RuleFailure::NominalOperand {
                operand: name.to_string(),
            });
        }
    }
    let (quantity_kind, unit) = match rule {
        PrimitiveRule::Add | PrimitiveRule::Sub => {
            if left.ty.quantity_kind != right.ty.quantity_kind {
                return Err(RuleFailure::KindMismatch {
                    left: left.ty.quantity_kind.clone(),
                    right: right.ty.quantity_kind.clone(),
                });
            }
            if left.unit != right.unit {
                return Err(RuleFailure::UnitMismatch {
                    left: left.unit.clone(),
                    right: right.unit.clone(),
                });
            }
            (left.ty.quantity_kind.clone(), left.unit.clone())
        }
        PrimitiveRule::Mul => products
            .product(&left.ty.quantity_kind, &right.ty.quantity_kind)
            .ok_or_else(|| RuleFailure::NoProductRow {
                lhs: left.ty.quantity_kind.clone(),
                rhs: right.ty.quantity_kind.clone(),
            })?,
    };
    let mut relations = left.ty.relations.clone();
    for (key, entity) in &right.ty.relations {
        match relations.get(key) {
            Some(existing) if existing != entity => {
                return Err(RuleFailure::RelationConflict {
                    key: key.clone(),
                    left: existing.clone(),
                    right: entity.clone(),
                });
            }
            Some(_) => {}
            None => {
                relations.insert(key.clone(), entity.clone());
            }
        }
    }
    let claim = if left.ty.claim == ClaimModel::Exact && right.ty.claim == ClaimModel::Exact {
        ClaimModel::Exact
    } else {
        ClaimModel::Enclosure
    };
    let value = match (left.value(), right.value()) {
        (Some(a), Some(b)) => {
            let result = match rule {
                PrimitiveRule::Add => a.add(b),
                PrimitiveRule::Sub => a.sub(b),
                PrimitiveRule::Mul => a.mul(b),
            }
            .map_err(RuleFailure::Arithmetic)?;
            if claim == ClaimModel::Exact && result.lower == result.upper {
                Some(NumericValue::Exact(result.lower))
            } else {
                Some(NumericValue::Enclosure(result))
            }
        }
        _ => None,
    };
    let state = match (left.state, right.state) {
        (ValueState::Established, ValueState::Established) => ValueState::Established,
        (ValueState::Unestablished, _) | (_, ValueState::Unestablished) => {
            ValueState::Unestablished
        }
        _ => ValueState::Declared,
    };
    let mut edges = left.edges.clone();
    edges.extend(right.edges.iter().cloned());
    let mut assumptions = left.assumptions.clone();
    for assumption in &right.assumptions {
        if !assumptions.contains(assumption) {
            assumptions.push(assumption.clone());
        }
    }
    Ok(SemanticValue {
        ty: super::model::QuantityType {
            quantity_kind,
            claim,
            relations,
        },
        unit,
        value,
        state,
        edges,
        assumptions,
    })
}
