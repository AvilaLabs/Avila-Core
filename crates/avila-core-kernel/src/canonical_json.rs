use std::collections::BTreeSet;
use std::fmt;

use icu_normalizer::ComposingNormalizerBorrowed;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{CORE_S1102, CORE_S1103, KernelError};

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_992;

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

impl CanonicalJsonValue {
    /// Resolves an RFC 6901 JSON Pointer against this value.
    ///
    /// The empty pointer names the whole value. A pointer that does not
    /// resolve returns `None`; it is never an error.
    #[must_use]
    pub fn pointer(&self, pointer: &str) -> Option<&Self> {
        if pointer.is_empty() {
            return Some(self);
        }
        let mut current = self;
        for token in pointer.strip_prefix('/')?.split('/') {
            let token = unescape_pointer_token(token);
            current = match current {
                Self::Object(entries) => entries
                    .iter()
                    .find(|(key, _)| key == &token)
                    .map(|(_, value)| value)?,
                Self::Array(values) => {
                    if token != "0" && (token.starts_with('0') || token.is_empty()) {
                        return None;
                    }
                    values.get(token.parse::<usize>().ok()?)?
                }
                _ => return None,
            };
        }
        Some(current)
    }
}

/// Reads authoritative JSON bytes into the canonical value profile.
///
/// A refusal names the JSON Pointer of the offending value so callers can
/// report an exact source location. Syntax errors point at the container that
/// was being read when the bytes stopped making sense.
pub fn read_authoritative_json(input: &[u8]) -> Result<CanonicalJsonValue, KernelError> {
    let mut state = ReaderState::default();
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let value = CanonicalSeed { state: &mut state }
        .deserialize(&mut deserializer)
        .map_err(|error| {
            KernelError::new(state.code.unwrap_or(CORE_S1102), error.to_string())
                .at_pointer(json_pointer(&state.path))
        })?;
    deserializer
        .end()
        .map_err(|error| KernelError::new(CORE_S1102, error.to_string()).at_pointer(""))?;
    Ok(value)
}

pub fn canonicalize_json(input: &[u8]) -> Result<Vec<u8>, KernelError> {
    let value = read_authoritative_json(input)?;
    serde_json::to_vec(&value).map_err(|error| KernelError::new(CORE_S1102, error.to_string()))
}

/// Formats a path from the document root as an RFC 6901 JSON Pointer.
fn json_pointer(path: &[PathSegment]) -> String {
    let mut pointer = String::new();
    for segment in path {
        pointer.push('/');
        match segment {
            PathSegment::Key(key) => pointer.push_str(&escape_pointer_token(key)),
            PathSegment::Index(index) => pointer.push_str(&index.to_string()),
        }
    }
    pointer
}

fn escape_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn unescape_pointer_token(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSegment {
    Key(String),
    Index(usize),
}

/// Location and refusal code tracked while the reader descends the document.
///
/// Segments are popped only after a child value is accepted, so a refusal
/// leaves the path pointing at the value that caused it.
#[derive(Debug, Default)]
struct ReaderState {
    path: Vec<PathSegment>,
    code: Option<&'static str>,
}

impl<'de> Deserialize<'de> for CanonicalJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut state = ReaderState::default();
        CanonicalSeed { state: &mut state }.deserialize(deserializer)
    }
}

struct CanonicalValueVisitor<'s> {
    state: &'s mut ReaderState,
}

impl<'de> Visitor<'de> for CanonicalValueVisitor<'_> {
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
        let mut index = 0usize;
        loop {
            self.state.path.push(PathSegment::Index(index));
            let Some(value) = sequence.next_element_seed(CanonicalSeed {
                state: &mut *self.state,
            })?
            else {
                self.state.path.pop();
                break;
            };
            self.state.path.pop();
            values.push(value);
            index += 1;
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
            self.state.path.push(PathSegment::Key(key.clone()));
            validate_nfc::<A::Error>(&key)?;
            if !seen.insert(key.clone()) {
                self.state.code = Some(CORE_S1103);
                return Err(<A::Error as de::Error>::custom(format!(
                    "duplicate object key `{key}`"
                )));
            }
            let value = map.next_value_seed(CanonicalSeed {
                state: &mut *self.state,
            })?;
            self.state.path.pop();
            entries.push((key, value));
        }
        entries.sort_by(|left, right| utf16_cmp(&left.0, &right.0));
        Ok(CanonicalJsonValue::Object(entries))
    }
}

struct CanonicalSeed<'s> {
    state: &'s mut ReaderState,
}

impl<'de> DeserializeSeed<'de> for CanonicalSeed<'_> {
    type Value = CanonicalJsonValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(CanonicalValueVisitor { state: self.state })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_name_the_offending_value_by_json_pointer() {
        for (input, pointer, code) in [
            (
                r#"{"workflow":[{"parameters":{"x":1.5}}]}"#,
                "/workflow/0/parameters/x",
                CORE_S1102,
            ),
            (r#"{"optional":null}"#, "/optional", CORE_S1102),
            (
                r#"{"outer":{"same":1,"same":2}}"#,
                "/outer/same",
                CORE_S1103,
            ),
            (r#"{"a/b":[1,{"~":"é"}]}"#, "/a~1b/1/~0", CORE_S1102),
            (r#"[1, 2, 99999999999999999999]"#, "/2", CORE_S1102),
            (r"1.5", "", CORE_S1102),
        ] {
            let error = read_authoritative_json(input.as_bytes()).unwrap_err();
            assert_eq!(error.code(), code, "{input}");
            assert_eq!(error.pointer(), Some(pointer), "{input}");
        }
    }

    #[test]
    fn syntax_errors_point_at_the_enclosing_container() {
        let error = read_authoritative_json(br#"{"workflow":[{"step_id":"a",}]}"#).unwrap_err();
        assert_eq!(error.code(), CORE_S1102);
        assert_eq!(error.pointer(), Some("/workflow/0"));

        let trailing = read_authoritative_json(br#"{"a":1} x"#).unwrap_err();
        assert_eq!(trailing.pointer(), Some(""));
    }

    #[test]
    fn pointer_lookup_follows_rfc_6901() {
        let value = read_authoritative_json(br#"{"a/b":[10,{"~":true}],"":0}"#).unwrap();
        assert_eq!(value.pointer(""), Some(&value));
        assert_eq!(value.pointer("/"), Some(&CanonicalJsonValue::Integer(0)));
        assert_eq!(
            value.pointer("/a~1b/0"),
            Some(&CanonicalJsonValue::Integer(10))
        );
        assert_eq!(
            value.pointer("/a~1b/1/~0"),
            Some(&CanonicalJsonValue::Bool(true))
        );
        assert_eq!(value.pointer("/a~1b/01"), None);
        assert_eq!(value.pointer("/missing"), None);
        assert_eq!(value.pointer("a"), None);
    }
}
