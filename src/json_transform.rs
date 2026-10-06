use std::collections::HashMap;
use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;

/// A strict JSON parse or serialization error.
///
/// Parse failures include the line and column reported by `serde_json` so UI
/// callers can present a location without interpreting the display string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonTransformError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl fmt::Display for JsonTransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for JsonTransformError {}

impl From<serde_json::Error> for JsonTransformError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            line: Some(error.line()),
            column: Some(error.column()),
        }
    }
}

impl JsonTransformError {
    fn serialization(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            line: None,
            column: None,
        }
    }
}

/// A parsed JSON document that can be rendered in either supported format.
///
/// Object members retain their first-seen input order. Repeated member names
/// keep the last value, matching `serde_json::Value`'s existing parse
/// semantics while avoiding its default sorted-key map representation.
#[derive(Debug, Clone)]
pub struct JsonDocument {
    value: OrderedJsonValue,
}

impl JsonDocument {
    /// Parses one strict JSON value and rejects any non-whitespace trailing data.
    pub fn parse(input: &str) -> Result<Self, JsonTransformError> {
        let mut deserializer = serde_json::Deserializer::from_str(input);
        let value = OrderedJsonValue::deserialize(&mut deserializer)?;
        deserializer.end()?;
        Ok(Self { value })
    }

    /// Serializes the document with two-space indentation.
    pub fn pretty(&self) -> Result<String, JsonTransformError> {
        serde_json::to_string_pretty(&self.value).map_err(JsonTransformError::serialization)
    }

    /// Serializes the document without insignificant whitespace.
    pub fn minify(&self) -> Result<String, JsonTransformError> {
        serde_json::to_string(&self.value).map_err(JsonTransformError::serialization)
    }
}

#[derive(Debug, Clone)]
enum OrderedJsonValue {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<OrderedJsonValue>),
    Object(Vec<(String, OrderedJsonValue)>),
}

impl<'de> Deserialize<'de> for OrderedJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(OrderedJsonVisitor)
    }
}

struct OrderedJsonVisitor;

impl<'de> Visitor<'de> for OrderedJsonVisitor {
    type Value = OrderedJsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a strict JSON value")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(OrderedJsonValue::Number)
            .ok_or_else(|| E::custom("JSON numbers must be finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(OrderedJsonValue::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element()? {
            values.push(value);
        }
        Ok(OrderedJsonValue::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut members: Vec<(String, OrderedJsonValue)> = Vec::new();
        let mut indices: HashMap<String, usize> = HashMap::new();
        while let Some((key, value)) = object.next_entry::<String, OrderedJsonValue>()? {
            if let Some(index) = indices.get(&key).copied() {
                members[index].1 = value;
            } else {
                indices.insert(key.clone(), members.len());
                members.push((key, value));
            }
        }
        Ok(OrderedJsonValue::Object(members))
    }
}

impl Serialize for OrderedJsonValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) => value.serialize(serializer),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(value)?;
                }
                sequence.end()
            }
            Self::Object(members) => {
                let mut object = serializer.serialize_map(Some(members.len()))?;
                for (key, value) in members {
                    object.serialize_entry(key, value)?;
                }
                object.end()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_formats_objects_arrays_and_nested_values_in_input_order() {
        let document =
            JsonDocument::parse(r#"{"z":[1,{"second":true,"first":null}],"a":"text"}"#).unwrap();

        assert_eq!(
            document.pretty().unwrap(),
            "{\n  \"z\": [\n    1,\n    {\n      \"second\": true,\n      \"first\": null\n    }\n  ],\n  \"a\": \"text\"\n}"
        );
    }

    #[test]
    fn strict_json_primitives_and_escaped_strings_are_supported() {
        for (input, expected) in [
            ("\"\"", "\"\""),
            (r#""a\n\"b""#, r#""a\n\"b""#),
            ("-12", "-12"),
            ("true", "true"),
            ("false", "false"),
            ("null", "null"),
        ] {
            let document = JsonDocument::parse(input).unwrap();
            assert_eq!(document.minify().unwrap(), expected);
            assert!(document.pretty().is_ok(), "input {input:?}");
        }
    }

    #[test]
    fn minify_removes_whitespace_without_changing_order_or_values() {
        let document =
            JsonDocument::parse(" { \"third\" : [ true, null ], \"first\" : { \"n\" : 2.5 } } ")
                .unwrap();

        assert_eq!(
            document.minify().unwrap(),
            r#"{"third":[true,null],"first":{"n":2.5}}"#
        );
    }

    #[test]
    fn duplicate_object_members_keep_last_value_and_first_position() {
        let document = JsonDocument::parse(r#"{"b":1,"a":2,"b":3}"#).unwrap();
        assert_eq!(document.minify().unwrap(), r#"{"b":3,"a":2}"#);
    }

    #[test]
    fn malformed_input_exposes_structured_line_and_column() {
        let error = JsonDocument::parse("{\n  \"value\": }\n").unwrap_err();
        assert_eq!(error.line, Some(2));
        assert!(error.column.is_some_and(|column| column > 0));
        assert!(error.message.contains("line 2 column"));
    }

    #[test]
    fn valid_json_followed_by_trailing_data_is_rejected() {
        let error = JsonDocument::parse("{\"ok\":true} trailing").unwrap_err();
        assert_eq!(error.line, Some(1));
        assert!(error.column.is_some_and(|column| column > 0));
        assert!(error.message.contains("trailing characters"));
    }

    #[test]
    fn comments_and_trailing_commas_are_rejected() {
        for input in ["[1,]", "{\"a\":1,}", "{\"a\":1 // comment\n}"] {
            assert!(
                JsonDocument::parse(input).is_err(),
                "strict JSON parser accepted {input:?}"
            );
        }
    }
}
