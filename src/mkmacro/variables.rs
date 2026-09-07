use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum MkValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Point(MkPoint),
    Null,
}

/// Types that may be declared in reusable macro signatures. Null is a runtime sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkValueType {
    String,
    Number,
    Boolean,
    Point,
}

impl MkValue {
    pub fn value_type(&self) -> Option<MkValueType> {
        match self {
            Self::String(_) => Some(MkValueType::String),
            Self::Number(_) => Some(MkValueType::Number),
            Self::Boolean(_) => Some(MkValueType::Boolean),
            Self::Point(_) => Some(MkValueType::Point),
            Self::Null => None,
        }
    }
}

impl MkValueType {
    pub fn accepts(self, value: &MkValue) -> bool {
        value.value_type() == Some(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MkValueSource {
    Literal(MkValue),
    Variable { name: String },
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkPoint {
    pub x: i32,
    pub y: i32,
}
pub type RuntimeVariables = BTreeMap<String, MkValue>;
pub const BUILT_INS: &[&str] = &[
    "mouse.x",
    "mouse.y",
    "screen.width",
    "screen.height",
    "active_window.title",
    "active_window.process",
    "macro.id",
    "macro.name",
    "step.id",
    "iteration",
    "last_action_success",
    "last_window_result",
    "last_image_result",
    "last_image_found",
    "last_image_x",
    "last_image_y",
    "last_image.x",
    "last_image.y",
    "last_pixel_result",
    "last_pixel_found",
    "last_pixel_x",
    "last_pixel_y",
    "last_point.x",
    "last_point.y",
];
pub fn is_builtin(name: &str) -> bool {
    BUILT_INS.contains(&name)
}
pub fn validate_variable_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("variable name cannot be empty");
    }
    if is_builtin(name) {
        return Err("built-in variable is read-only");
    }
    let mut chars = name.chars();
    if !chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        || !chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
    {
        return Err(
            "variable names must start with a letter or underscore and contain only ASCII letters, digits, and underscores",
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_types_accept_only_matching_non_null_values() {
        let values = [
            MkValue::String("text".into()),
            MkValue::Number(3.5),
            MkValue::Boolean(true),
            MkValue::Point(MkPoint { x: -1, y: 2 }),
        ];
        let types = [
            MkValueType::String,
            MkValueType::Number,
            MkValueType::Boolean,
            MkValueType::Point,
        ];
        for (type_index, value_type) in types.iter().copied().enumerate() {
            assert!(!value_type.accepts(&MkValue::Null));
            for (value_index, value) in values.iter().enumerate() {
                assert_eq!(value_type.accepts(value), type_index == value_index);
            }
            assert_eq!(
                serde_json::from_value::<MkValueType>(serde_json::to_value(value_type).unwrap())
                    .unwrap(),
                value_type
            );
        }
        assert_eq!(MkValue::Null.value_type(), None);
        assert!(serde_json::from_str::<MkValueType>("\"null\"").is_err());
    }
    #[test]
    fn names() {
        assert!(validate_variable_name("valid_1").is_ok());
        assert!(validate_variable_name("1bad").is_err());
        assert!(validate_variable_name("mouse.x").is_err())
    }
}
