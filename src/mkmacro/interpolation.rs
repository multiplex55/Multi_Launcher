//! Platform-neutral runtime-variable interpolation.
//!
//! `${name}` substitutes one value and `$${name}` produces the literal
//! `${name}`. All other dollars, braces, and text are copied verbatim. Empty or
//! unclosed placeholders are errors. Substituted values are appended as-is and
//! are never scanned again, so interpolation is deliberately non-recursive.

use super::{DiagnosticKind, ExecResult, ExecutionDiagnostic, MkValue, RuntimeVariables};

/// Formats a runtime value for interpolation and UI previews.
///
/// Strings are unchanged, numbers use Rust's locale-independent display,
/// booleans are `true`/`false`, and points are `x,y`. Null cannot be formatted:
/// treating it as an empty answer would hide an absent runtime value.
pub fn format_interpolation_value(name: &str, value: &MkValue) -> ExecResult<String> {
    match value {
        MkValue::String(value) => Ok(value.clone()),
        MkValue::Number(value) => Ok(value.to_string()),
        MkValue::Boolean(value) => Ok(value.to_string()),
        MkValue::Point(point) => Ok(format!("{},{}", point.x, point.y)),
        MkValue::Null => Err(ExecutionDiagnostic::new(
            DiagnosticKind::TypeMismatch,
            "null variable cannot be interpolated",
        )
        .context("variable", name)),
    }
}

/// Expands runtime variables in `template` in one left-to-right pass.
pub fn interpolate(template: &str, variables: &RuntimeVariables) -> ExecResult<String> {
    let mut output = String::with_capacity(template.len());
    let mut cursor = 0;
    while cursor < template.len() {
        let rest = &template[cursor..];
        if rest.starts_with("$${") {
            let name_start = cursor + 3;
            let Some(relative_end) = template[name_start..].find('}') else {
                return Err(malformed("unclosed escaped interpolation placeholder"));
            };
            let end = name_start + relative_end;
            if end == name_start {
                return Err(malformed("empty escaped interpolation placeholder"));
            }
            output.push_str("${");
            output.push_str(&template[name_start..end]);
            output.push('}');
            cursor = end + 1;
        } else if rest.starts_with("${") {
            let name_start = cursor + 2;
            let Some(relative_end) = template[name_start..].find('}') else {
                return Err(malformed("unclosed interpolation placeholder"));
            };
            let end = name_start + relative_end;
            if end == name_start {
                return Err(malformed("empty interpolation placeholder"));
            }
            let name = &template[name_start..end];
            let value = variables.get(name).ok_or_else(|| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "interpolation variable is undefined",
                )
                .context("variable", name)
            })?;
            output.push_str(&format_interpolation_value(name, value)?);
            cursor = end + 1;
        } else {
            let ch = rest
                .chars()
                .next()
                .expect("cursor is on a character boundary");
            output.push(ch);
            cursor += ch.len_utf8();
        }
    }
    Ok(output)
}

fn malformed(message: &'static str) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::InvalidTarget, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::MkPoint;

    fn variables() -> RuntimeVariables {
        [
            ("name".into(), MkValue::String("world".into())),
            ("other".into(), MkValue::String("again".into())),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn grammar_and_references() {
        let vars = variables();
        assert_eq!(
            interpolate("plain $ and {braces}", &vars).unwrap(),
            "plain $ and {braces}"
        );
        assert_eq!(interpolate("hello ${name}", &vars).unwrap(), "hello world");
        assert_eq!(interpolate("${name}${other}", &vars).unwrap(), "worldagain");
        assert_eq!(
            interpolate("${name}/${name}", &vars).unwrap(),
            "world/world"
        );
        assert_eq!(interpolate("$${missing}", &vars).unwrap(), "${missing}");
    }

    #[test]
    fn malformed_and_undefined_are_diagnostics() {
        for template in ["${}", "${open", "$${}", "$${open"] {
            assert_eq!(
                interpolate(template, &variables()).unwrap_err().kind,
                DiagnosticKind::InvalidTarget
            );
        }
        let error = interpolate("${missing}", &variables()).unwrap_err();
        assert_eq!(
            error.context.get("variable").map(String::as_str),
            Some("missing")
        );
        assert!(!error.message.contains("${missing}"));
    }

    #[test]
    fn every_value_has_stable_formatting_and_null_fails() {
        assert_eq!(
            format_interpolation_value("v", &MkValue::String(" exact ".into())).unwrap(),
            " exact "
        );
        assert_eq!(
            format_interpolation_value("v", &MkValue::Number(1234.5)).unwrap(),
            "1234.5"
        );
        assert_eq!(
            format_interpolation_value("v", &MkValue::Boolean(true)).unwrap(),
            "true"
        );
        assert_eq!(
            format_interpolation_value("v", &MkValue::Point(MkPoint { x: -2, y: 7 })).unwrap(),
            "-2,7"
        );
        assert_eq!(
            format_interpolation_value("v", &MkValue::Null)
                .unwrap_err()
                .kind,
            DiagnosticKind::TypeMismatch
        );
    }

    #[test]
    fn substitution_is_non_recursive_and_unicode_safe() {
        let vars = [
            ("outer".into(), MkValue::String("${other}".into())),
            ("other".into(), MkValue::String("ignored".into())),
            ("macro.id".into(), MkValue::Number(42.0)),
            ("挨拶".into(), MkValue::String("世界🌍".into())),
        ]
        .into_iter()
        .collect();
        assert_eq!(interpolate("${outer}", &vars).unwrap(), "${other}");
        assert_eq!(
            interpolate("Привет ${挨拶} ${macro.id}", &vars).unwrap(),
            "Привет 世界🌍 42"
        );
    }
}
