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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    fn names() {
        assert!(validate_variable_name("valid_1").is_ok());
        assert!(validate_variable_name("1bad").is_err());
        assert!(validate_variable_name("mouse.x").is_err())
    }
}
