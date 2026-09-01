use std::collections::BTreeSet;
use std::fmt;

use icu_normalizer::ComposingNormalizerBorrowed;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{CORE_S1102, CORE_S1103, KernelError};

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_992;
const DUPLICATE_SENTINEL: &str = "avila-core:duplicate-key";

/// JSON restricted to Core's authoritative canonical profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalJsonValue {
    Bool(bool),
    Integer(i64),
    String(String),
    Array(Vec<Self>),
    /// Keys are stored in JCS UTF-16 code-unit order.
    Object(Vec<(String, Self)>),
}

pub fn read_authoritative_json(input: &[u8]) -> Result<CanonicalJsonValue, KernelError> {
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let value = CanonicalJsonValue::deserialize(&mut deserializer).map_err(|error| {
        let detail = error.to_string();
        let code = if detail.contains(DUPLICATE_SENTINEL) {
            CORE_S1103
        } else {
            CORE_S1102
        };
        KernelError::new(code, detail)
    })?;
    deserializer
        .end()
        .map_err(|error| KernelError::new(CORE_S1102, error.to_string()))?;
    Ok(value)
}

pub fn canonicalize_json(input: &[u8]) -> Result<Vec<u8>, KernelError> {
    let value = read_authoritative_json(input)?;
    serde_json::to_vec(&value).map_err(|error| KernelError::new(CORE_S1102, error.to_string()))
}

impl<'de> Deserialize<'de> for CanonicalJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(CanonicalValueVisitor)
    }
}

struct CanonicalValueVisitor;

impl<'de> Visitor<'de> for CanonicalValueVisitor {
    type Value = CanonicalJsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a value in the Avila Core canonical JSON profile")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(CanonicalJsonValue::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value) {
            return Err(E::custom("JSON integer exceeds the exact ±2^53 profile"));
        }
        Ok(CanonicalJsonValue::Integer(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value = i64::try_from(value)
            .map_err(|_| E::custom("JSON integer exceeds the exact ±2^53 profile"))?;
        self.visit_i64(value)
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Err(E::custom(
            "binary floating-point JSON numbers are not authoritative values",
        ))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        validate_nfc(value)?;
        Ok(CanonicalJsonValue::String(value.into()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        validate_nfc(&value)?;
        Ok(CanonicalJsonValue::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Err(E::custom("null is not an alias for an absent field"))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_none()
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(16_384));
        while let Some(value) = sequence.next_element_seed(CanonicalSeed)? {
            values.push(value);
        }
        Ok(CanonicalJsonValue::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = BTreeSet::new();
        let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0).min(16_384));
        while let Some(key) = map.next_key::<String>()? {
            validate_nfc::<A::Error>(&key)?;
            if !seen.insert(key.clone()) {
                return Err(<A::Error as de::Error>::custom(format!(
                    "{DUPLICATE_SENTINEL}: {key}"
                )));
            }
            let value = map.next_value_seed(CanonicalSeed)?;
            entries.push((key, value));
        }
        entries.sort_by(|left, right| utf16_cmp(&left.0, &right.0));
        Ok(CanonicalJsonValue::Object(entries))
    }
}

struct CanonicalSeed;

impl<'de> DeserializeSeed<'de> for CanonicalSeed {
    type Value = CanonicalJsonValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        CanonicalJsonValue::deserialize(deserializer)
    }
}

impl Serialize for CanonicalJsonValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Integer(value) => serializer.serialize_i64(*value),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(value)?;
                }
                sequence.end()
            }
            Self::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

fn validate_nfc<E: de::Error>(value: &str) -> Result<(), E> {
    if ComposingNormalizerBorrowed::new_nfc().is_normalized(value) {
        Ok(())
    } else {
        Err(E::custom(
            "authoritative strings must already be Unicode NFC",
        ))
    }
}

fn utf16_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}
