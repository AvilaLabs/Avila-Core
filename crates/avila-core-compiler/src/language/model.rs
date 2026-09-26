//! The semantic model the analyzer carries: typed quantities with claim
//! models, relation maps, provenance edges, and residual assumptions.

use std::collections::{BTreeMap, BTreeSet};

use avila_core_kernel::ExactNumber;

use super::document::ScopedAssertionDecl;

/// The closed claim set (spec §6): `exact ⊑ enclosure` is the only widening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimModel {
    Exact,
    Enclosure,
    Nominal,
}

impl ClaimModel {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "exact" => Some(Self::Exact),
            "enclosure" => Some(Self::Enclosure),
            "nominal" => Some(Self::Nominal),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Enclosure => "enclosure",
            Self::Nominal => "nominal",
        }
    }

    /// `self` satisfies a slot or target claiming `other`.
    pub fn satisfies(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Exact, Self::Exact)
                | (Self::Exact, Self::Enclosure)
                | (Self::Enclosure, Self::Enclosure)
                | (Self::Nominal, Self::Nominal)
        )
    }
}

/// Relation-name → entity identifier. Absent keys mean "not carried".
pub type RelationMap = BTreeMap<String, String>;

/// A quantity's declared type: kind, claim, and the entity map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantityType {
    pub quantity_kind: String,
    pub claim: ClaimModel,
    pub relations: RelationMap,
}

/// An exact rational point or a closed interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumericValue {
    Exact(ExactNumber),
    Enclosure(Enclosure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enclosure {
    pub lower: ExactNumber,
    pub upper: ExactNumber,
}

impl NumericValue {
    /// Both bounds as exact rationals; an exact value is the point enclosure.
    pub fn bounds(&self) -> (ExactNumber, ExactNumber) {
        match self {
            Self::Exact(v) => (v.clone(), v.clone()),
            Self::Enclosure(e) => (e.lower.clone(), e.upper.clone()),
        }
    }

    pub fn add(&self, other: &Self) -> Result<Enclosure, String> {
        let (al, au) = self.bounds();
        let (bl, bu) = other.bounds();
        Ok(Enclosure {
            lower: al.checked_add(&bl).map_err(|e| e.to_string())?,
            upper: au.checked_add(&bu).map_err(|e| e.to_string())?,
        })
    }

    pub fn sub(&self, other: &Self) -> Result<Enclosure, String> {
        let (al, au) = self.bounds();
        let (bl, bu) = other.bounds();
        Ok(Enclosure {
            lower: al.checked_sub(&bu).map_err(|e| e.to_string())?,
            upper: au.checked_sub(&bl).map_err(|e| e.to_string())?,
        })
    }

    pub fn mul(&self, other: &Self) -> Result<Enclosure, String> {
        let (al, au) = self.bounds();
        let (bl, bu) = other.bounds();
        let mut lo = al.checked_mul(&bl).map_err(|e| e.to_string())?;
        let mut hi = lo.clone();
        for product in [
            al.checked_mul(&bu),
            au.checked_mul(&bl),
            au.checked_mul(&bu),
        ] {
            let p = product.map_err(|e| e.to_string())?;
            if p.checked_cmp(&lo).is_ok_and(|o| o.is_lt()) {
                lo = p.clone();
            }
            if p.checked_cmp(&hi).is_ok_and(|o| o.is_gt()) {
                hi = p;
            }
        }
        Ok(Enclosure {
            lower: lo,
            upper: hi,
        })
    }
}

/// Whether the numeric content stands or awaits observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueState {
    /// Computed or bound within this analysis.
    Established,
    /// Typed by a declared postcondition; the observation that discharges it
    /// is a runtime obligation.
    Declared,
    /// No value reaches this binding.
    Unestablished,
}

/// A proposition at a concrete scope: `(name, relation map)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScopedProposition {
    pub proposition: String,
    pub at: RelationMap,
}

impl From<&ScopedAssertionDecl> for ScopedProposition {
    fn from(decl: &ScopedAssertionDecl) -> Self {
        Self {
            proposition: decl.proposition.clone(),
            at: decl.at.clone(),
        }
    }
}

/// The unit-carried semantic value bound to a name.
#[derive(Debug, Clone)]
pub struct SemanticValue {
    pub ty: QuantityType,
    pub unit: String,
    pub value: Option<NumericValue>,
    pub state: ValueState,
    /// Recorded source edges this value's provenance rides on.
    pub edges: BTreeSet<String>,
    /// Residual scoped assumptions still attached to this value.
    pub assumptions: Vec<ScopedProposition>,
}

impl SemanticValue {
    pub fn value(&self) -> Option<&NumericValue> {
        self.value.as_ref()
    }

    /// Renders the value for the analysis record (`"100"`, `[1/10, 1/5]`).
    pub fn value_text(&self) -> Option<String> {
        match self.value.as_ref()? {
            NumericValue::Exact(v) => Some(v.canonical_rational()),
            NumericValue::Enclosure(e) => Some(format!(
                "[{}, {}]",
                e.lower.canonical_rational(),
                e.upper.canonical_rational()
            )),
        }
    }
}
