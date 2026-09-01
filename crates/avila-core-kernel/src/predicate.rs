use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CORE_S1102, KernelError, KindRegistry, Quantity};

const DEFAULT_MAX_DEPTH: usize = 128;
const DEFAULT_MAX_NODES: usize = 10_000;

/// Strong Kleene truth value. `Unknown` is never accepted as `True`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruthValue {
    True,
    False,
    Unknown,
}

impl TruthValue {
    #[must_use]
    pub const fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    Always(bool),
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
    ParamInRange(RangePredicate),
    InputAttributeIn(AttributeSetPredicate),
    EnvironmentImageIn(Vec<String>),
    Fact(FactPredicate),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangePredicate {
    pub param: String,
    #[serde(default)]
    pub min: Option<Quantity>,
    #[serde(default)]
    pub max: Option<Quantity>,
    #[serde(default)]
    pub min_inclusive: bool,
    #[serde(default)]
    pub max_inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeSetPredicate {
    pub slot: String,
    pub attribute: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactOperator {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactPredicate {
    pub name: String,
    pub op: FactOperator,
    pub value: FactValue,
    pub source_requirement: SourceRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum FactValue {
    Quantity(Quantity),
    String(String),
    Bool(bool),
    Integer(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRequirement {
    pub class: String,
    pub validator: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicabilityContext {
    #[serde(default)]
    pub params: BTreeMap<String, Quantity>,
    #[serde(default)]
    pub inputs: BTreeMap<String, InputContext>,
    #[serde(default)]
    pub environment: Option<EnvironmentContext>,
    #[serde(default)]
    pub facts: BTreeMap<String, FactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputContext {
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentContext {
    pub image_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactRecord {
    pub value: FactValue,
    pub source: FactSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactSource {
    pub class: String,
    pub identity: String,
    pub validator: String,
    pub receipt: String,
}

/// Pure predicate evaluator over an explicit immutable context.
pub struct ApplicabilityEvaluator<'a> {
    kinds: &'a KindRegistry,
    parameter_kinds: BTreeMap<String, String>,
    fact_kinds: BTreeMap<String, String>,
    max_depth: usize,
    max_nodes: usize,
}

impl<'a> ApplicabilityEvaluator<'a> {
    #[must_use]
    pub fn new(kinds: &'a KindRegistry) -> Self {
        Self {
            kinds,
            parameter_kinds: BTreeMap::new(),
            fact_kinds: BTreeMap::new(),
            max_depth: DEFAULT_MAX_DEPTH,
            max_nodes: DEFAULT_MAX_NODES,
        }
    }

    pub fn register_parameter_kind(
        &mut self,
        parameter: impl Into<String>,
        kind: impl Into<String>,
    ) -> Result<(), KernelError> {
        insert_type_binding(&mut self.parameter_kinds, parameter.into(), kind.into())
    }

    pub fn register_fact_kind(
        &mut self,
        fact: impl Into<String>,
        kind: impl Into<String>,
    ) -> Result<(), KernelError> {
        insert_type_binding(&mut self.fact_kinds, fact.into(), kind.into())
    }

    pub fn evaluate(
        &self,
        predicate: &Predicate,
        context: &ApplicabilityContext,
    ) -> Result<TruthValue, KernelError> {
        let mut budget = EvaluationBudget {
            remaining_nodes: self.max_nodes,
            max_depth: self.max_depth,
        };
        self.evaluate_at(predicate, context, 0, &mut budget)
    }

    fn evaluate_at(
        &self,
        predicate: &Predicate,
        context: &ApplicabilityContext,
        depth: usize,
        budget: &mut EvaluationBudget,
    ) -> Result<TruthValue, KernelError> {
        budget.consume(depth)?;
        match predicate {
            Predicate::Always(value) => Ok(if *value {
                TruthValue::True
            } else {
                TruthValue::False
            }),
            Predicate::Not(inner) => Ok(self.evaluate_at(inner, context, depth + 1, budget)?.not()),
            Predicate::All(items) => {
                if items.is_empty() {
                    return Err(invalid_predicate("`all` requires at least one predicate"));
                }
                let mut result = TruthValue::True;
                for item in items {
                    match self.evaluate_at(item, context, depth + 1, budget)? {
                        TruthValue::False => return Ok(TruthValue::False),
                        TruthValue::Unknown => result = TruthValue::Unknown,
                        TruthValue::True => {}
                    }
                }
                Ok(result)
            }
            Predicate::Any(items) => {
                if items.is_empty() {
                    return Err(invalid_predicate("`any` requires at least one predicate"));
                }
                let mut result = TruthValue::False;
                for item in items {
                    match self.evaluate_at(item, context, depth + 1, budget)? {
                        TruthValue::True => return Ok(TruthValue::True),
                        TruthValue::Unknown => result = TruthValue::Unknown,
                        TruthValue::False => {}
                    }
                }
                Ok(result)
            }
            Predicate::ParamInRange(range) => self.evaluate_range(range, context),
            Predicate::InputAttributeIn(set) => Ok(evaluate_attribute_set(set, context)),
            Predicate::EnvironmentImageIn(allowed) => {
                Ok(context
                    .environment
                    .as_ref()
                    .map_or(TruthValue::Unknown, |environment| {
                        if allowed.contains(&environment.image_digest) {
                            TruthValue::True
                        } else {
                            TruthValue::False
                        }
                    }))
            }
            Predicate::Fact(fact) => self.evaluate_fact(fact, context),
        }
    }

    fn evaluate_range(
        &self,
        range: &RangePredicate,
        context: &ApplicabilityContext,
    ) -> Result<TruthValue, KernelError> {
        if range.min.is_none() && range.max.is_none() {
            return Err(invalid_predicate(
                "a parameter range requires a minimum or maximum",
            ));
        }
        let Some(actual) = context.params.get(&range.param) else {
            return Ok(TruthValue::Unknown);
        };
        let kind = self.parameter_kinds.get(&range.param).ok_or_else(|| {
            invalid_predicate(format!("parameter `{}` has no quantity kind", range.param))
        })?;
        let actual = self.kinds.scale(kind, actual)?;

        if let Some(minimum) = &range.min {
            let minimum = self.kinds.scale(kind, minimum)?;
            let ordering = actual.value.checked_cmp(&minimum.value)?;
            if ordering == Ordering::Less || (!range.min_inclusive && ordering == Ordering::Equal) {
                return Ok(TruthValue::False);
            }
        }
        if let Some(maximum) = &range.max {
            let maximum = self.kinds.scale(kind, maximum)?;
            let ordering = actual.value.checked_cmp(&maximum.value)?;
            if ordering == Ordering::Greater
                || (!range.max_inclusive && ordering == Ordering::Equal)
            {
                return Ok(TruthValue::False);
            }
        }
        Ok(TruthValue::True)
    }

    fn evaluate_fact(
        &self,
        predicate: &FactPredicate,
        context: &ApplicabilityContext,
    ) -> Result<TruthValue, KernelError> {
        let Some(actual) = context.facts.get(&predicate.name) else {
            return Ok(TruthValue::Unknown);
        };
        if actual.source.class != predicate.source_requirement.class
            || actual.source.validator != predicate.source_requirement.validator
        {
            return Ok(TruthValue::Unknown);
        }

        match (&actual.value, &predicate.value) {
            (FactValue::Quantity(actual), FactValue::Quantity(expected)) => {
                let kind = self.fact_kinds.get(&predicate.name).ok_or_else(|| {
                    invalid_predicate(format!("fact `{}` has no quantity kind", predicate.name))
                })?;
                let actual = self.kinds.scale(kind, actual)?;
                let expected = self.kinds.scale(kind, expected)?;
                Ok(compare(
                    predicate.op,
                    actual.value.checked_cmp(&expected.value)?,
                ))
            }
            (FactValue::String(actual), FactValue::String(expected)) => {
                compare_scalar(predicate.op, actual, expected)
            }
            (FactValue::Bool(actual), FactValue::Bool(expected)) => {
                compare_scalar(predicate.op, actual, expected)
            }
            (FactValue::Integer(actual), FactValue::Integer(expected)) => {
                Ok(compare(predicate.op, actual.cmp(expected)))
            }
            _ => Ok(TruthValue::Unknown),
        }
    }
}

struct EvaluationBudget {
    remaining_nodes: usize,
    max_depth: usize,
}

impl EvaluationBudget {
    fn consume(&mut self, depth: usize) -> Result<(), KernelError> {
        if depth > self.max_depth || self.remaining_nodes == 0 {
            return Err(invalid_predicate(
                "predicate exceeds the semantic evaluation resource limit",
            ));
        }
        self.remaining_nodes -= 1;
        Ok(())
    }
}

fn evaluate_attribute_set(
    predicate: &AttributeSetPredicate,
    context: &ApplicabilityContext,
) -> TruthValue {
    let Some(value) = context
        .inputs
        .get(&predicate.slot)
        .and_then(|input| input.attributes.get(&predicate.attribute))
        .and_then(Value::as_str)
    else {
        return TruthValue::Unknown;
    };
    if predicate.values.iter().any(|candidate| candidate == value) {
        TruthValue::True
    } else {
        TruthValue::False
    }
}

fn compare_scalar<T: Ord>(
    operator: FactOperator,
    actual: &T,
    expected: &T,
) -> Result<TruthValue, KernelError> {
    if !matches!(operator, FactOperator::Eq | FactOperator::Ne) {
        return Err(invalid_predicate(
            "ordered fact comparison requires an ordered numeric value",
        ));
    }
    Ok(compare(operator, actual.cmp(expected)))
}

const fn compare(operator: FactOperator, ordering: Ordering) -> TruthValue {
    let matched = match operator {
        FactOperator::Eq => ordering.is_eq(),
        FactOperator::Ne => !ordering.is_eq(),
        FactOperator::Lt => ordering.is_lt(),
        FactOperator::Le => ordering.is_le(),
        FactOperator::Gt => ordering.is_gt(),
        FactOperator::Ge => ordering.is_ge(),
    };
    if matched {
        TruthValue::True
    } else {
        TruthValue::False
    }
}

fn insert_type_binding(
    bindings: &mut BTreeMap<String, String>,
    name: String,
    kind: String,
) -> Result<(), KernelError> {
    if name.is_empty() || kind.is_empty() {
        return Err(invalid_predicate("type binding names must be nonempty"));
    }
    if bindings.insert(name.clone(), kind).is_some() {
        return Err(invalid_predicate(format!(
            "duplicate type binding for `{name}`"
        )));
    }
    Ok(())
}

fn invalid_predicate(detail: impl Into<String>) -> KernelError {
    KernelError::new(CORE_S1102, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_kleene_operators_preserve_unknown() {
        assert_eq!(TruthValue::Unknown.not(), TruthValue::Unknown);
        assert_eq!(TruthValue::True.not(), TruthValue::False);
        assert_eq!(TruthValue::False.not(), TruthValue::True);
    }

    #[test]
    fn strong_kleene_all_and_any_tables_are_exhaustive() {
        let registry = KindRegistry::default();
        let evaluator = ApplicabilityEvaluator::new(&registry);
        let context = ApplicabilityContext {
            params: BTreeMap::new(),
            inputs: BTreeMap::new(),
            environment: None,
            facts: BTreeMap::new(),
        };
        let predicates = [
            (TruthValue::True, Predicate::Always(true)),
            (TruthValue::False, Predicate::Always(false)),
            (
                TruthValue::Unknown,
                Predicate::EnvironmentImageIn(vec!["sha256:missing".into()]),
            ),
        ];

        for (left_value, left) in &predicates {
            for (right_value, right) in &predicates {
                let all = evaluator
                    .evaluate(&Predicate::All(vec![left.clone(), right.clone()]), &context)
                    .unwrap();
                let expected_all = if [*left_value, *right_value].contains(&TruthValue::False) {
                    TruthValue::False
                } else if [*left_value, *right_value].contains(&TruthValue::Unknown) {
                    TruthValue::Unknown
                } else {
                    TruthValue::True
                };
                assert_eq!(all, expected_all);

                let any = evaluator
                    .evaluate(&Predicate::Any(vec![left.clone(), right.clone()]), &context)
                    .unwrap();
                let expected_any = if [*left_value, *right_value].contains(&TruthValue::True) {
                    TruthValue::True
                } else if [*left_value, *right_value].contains(&TruthValue::Unknown) {
                    TruthValue::Unknown
                } else {
                    TruthValue::False
                };
                assert_eq!(any, expected_any);
            }
        }
    }

    #[test]
    fn deeply_nested_predicate_fails_closed() {
        let registry = KindRegistry::default();
        let evaluator = ApplicabilityEvaluator::new(&registry);
        let context = ApplicabilityContext {
            params: BTreeMap::new(),
            inputs: BTreeMap::new(),
            environment: None,
            facts: BTreeMap::new(),
        };
        let mut predicate = Predicate::Always(true);
        for _ in 0..=DEFAULT_MAX_DEPTH {
            predicate = Predicate::Not(Box::new(predicate));
        }
        assert_eq!(
            evaluator.evaluate(&predicate, &context).unwrap_err().code(),
            CORE_S1102
        );
    }
}
