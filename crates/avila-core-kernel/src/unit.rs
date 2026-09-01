use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    CORE_S1102, CORE_T2001, CORE_T2102, ExactNumber, KernelError, Repair, RepairApplicability,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitDefinition {
    symbol: String,
    factor: ExactNumber,
}

impl UnitDefinition {
    pub fn new(symbol: impl Into<String>, factor: ExactNumber) -> Result<Self, KernelError> {
        let symbol = symbol.into();
        if symbol.is_empty() || !factor.is_positive() {
            return Err(KernelError::new(
                CORE_S1102,
                "unit symbol must be nonempty and its exact factor must be positive",
            ));
        }
        Ok(Self { symbol, factor })
    }

    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    #[must_use]
    pub fn factor(&self) -> &ExactNumber {
        &self.factor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindDefinition {
    id: String,
    canonical_unit: String,
    units: Vec<UnitDefinition>,
}

impl KindDefinition {
    pub fn new(
        id: impl Into<String>,
        canonical_unit: impl Into<String>,
        units: Vec<UnitDefinition>,
    ) -> Result<Self, KernelError> {
        let id = id.into();
        let canonical_unit = canonical_unit.into();
        if id.is_empty() || canonical_unit.is_empty() || units.is_empty() {
            return Err(KernelError::new(
                CORE_S1102,
                "kind id, canonical unit, and unit class are required",
            ));
        }

        let mut symbols = BTreeSet::new();
        for unit in &units {
            if !symbols.insert(unit.symbol.as_str()) {
                return Err(KernelError::new(
                    CORE_S1102,
                    format!("kind `{id}` declares duplicate unit `{}`", unit.symbol),
                ));
            }
        }
        let canonical = units
            .iter()
            .find(|unit| unit.symbol == canonical_unit)
            .ok_or_else(|| {
                KernelError::new(
                    CORE_S1102,
                    format!("kind `{id}` does not include its canonical unit"),
                )
            })?;
        if !canonical.factor.is_one() {
            return Err(KernelError::new(
                CORE_S1102,
                format!("kind `{id}` canonical-unit factor must be exactly one"),
            ));
        }

        Ok(Self {
            id,
            canonical_unit,
            units,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn canonical_unit(&self) -> &str {
        &self.canonical_unit
    }

    #[must_use]
    pub fn units(&self) -> &[UnitDefinition] {
        &self.units
    }

    fn unit(&self, symbol: &str) -> Option<&UnitDefinition> {
        self.units.iter().find(|unit| unit.symbol == symbol)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanonicalQuantity {
    pub canonical_unit: String,
    pub value: ExactNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    pub value: ExactNumber,
    pub unit: String,
}

#[derive(Debug, Default)]
pub struct KindRegistry {
    kinds: Vec<KindDefinition>,
    kind_indexes: BTreeMap<String, usize>,
    conversions: BTreeMap<(String, String), Vec<String>>,
}

impl KindRegistry {
    pub fn insert_kind(&mut self, kind: KindDefinition) -> Result<(), KernelError> {
        if self.kind_indexes.contains_key(kind.id()) {
            return Err(KernelError::new(
                CORE_S1102,
                format!("duplicate kind `{}`", kind.id()),
            ));
        }
        let index = self.kinds.len();
        self.kind_indexes.insert(kind.id().into(), index);
        self.kinds.push(kind);
        Ok(())
    }

    pub fn register_conversion(
        &mut self,
        source_kind: impl Into<String>,
        target_kind: impl Into<String>,
        capability_type: impl Into<String>,
    ) -> Result<(), KernelError> {
        let source_kind = source_kind.into();
        let target_kind = target_kind.into();
        let capability_type = capability_type.into();
        if source_kind.is_empty() || target_kind.is_empty() || capability_type.is_empty() {
            return Err(KernelError::new(
                CORE_S1102,
                "conversion source, target, and capability type are required",
            ));
        }
        let candidates = self
            .conversions
            .entry((source_kind, target_kind))
            .or_default();
        if !candidates.contains(&capability_type) {
            candidates.push(capability_type);
        }
        Ok(())
    }

    pub fn scale_quantity(
        &self,
        kind_id: &str,
        value: &ExactNumber,
        unit_symbol: &str,
    ) -> Result<CanonicalQuantity, KernelError> {
        let kind = self.kind(kind_id).ok_or_else(|| {
            KernelError::new(CORE_S1102, format!("unknown quantity kind `{kind_id}`"))
        })?;

        if let Some(unit) = kind.unit(unit_symbol) {
            return Ok(CanonicalQuantity {
                canonical_unit: kind.canonical_unit.clone(),
                value: value.checked_mul(unit.factor())?,
            });
        }

        let source_kinds: Vec<_> = self
            .kinds
            .iter()
            .filter(|candidate| candidate.unit(unit_symbol).is_some())
            .collect();
        if !source_kinds.is_empty() {
            let mut candidates = Vec::new();
            for source in source_kinds {
                if let Some(registered) =
                    self.conversions.get(&(source.id.clone(), kind.id.clone()))
                {
                    for candidate in registered {
                        if !candidates.contains(candidate) {
                            candidates.push(candidate.clone());
                        }
                    }
                }
            }
            return Err(KernelError::with_repair(
                CORE_T2102,
                format!(
                    "unit `{unit_symbol}` belongs to a different quantity kind than `{kind_id}`"
                ),
                Repair {
                    applicability: RepairApplicability::MethodOwnerJudgment,
                    candidates,
                },
            ));
        }

        Err(KernelError::with_repair(
            CORE_T2001,
            format!("unit symbol `{unit_symbol}` is not admitted for kind `{kind_id}`"),
            Repair {
                applicability: RepairApplicability::ConstrainedChoice,
                candidates: kind.units.iter().map(|unit| unit.symbol.clone()).collect(),
            },
        ))
    }

    pub fn scale(
        &self,
        kind_id: &str,
        quantity: &Quantity,
    ) -> Result<CanonicalQuantity, KernelError> {
        self.scale_quantity(kind_id, &quantity.value, &quantity.unit)
    }

    #[must_use]
    pub fn kind(&self, id: &str) -> Option<&KindDefinition> {
        self.kind_indexes
            .get(id)
            .and_then(|index| self.kinds.get(*index))
    }
}
