//! Platform-neutral runtime-variable interpolation.
//!
//! `${name}` substitutes one value and `$${name}` produces the literal
//! `${name}`. All other dollars, braces, and text are copied verbatim. Empty or
//! unclosed placeholders are errors. Substituted values are appended as-is and
//! are never scanned again, so interpolation is deliberately non-recursive.

use super::{DiagnosticKind, ExecResult, ExecutionDiagnostic, MkValue, RuntimeVariables};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplatePart<'a> {
    Text(&'a str),
    Reference(&'a str),
    EscapedReference(&'a str),
}

/// Shared streaming grammar for runtime expansion, validation and static reads.
/// Reference keys are exact. A later parse error never overtakes an earlier
/// resolution error, and the scanner itself adds no allocation.
pub fn scan_template(template: &str) -> TemplateScanner<'_> {
    TemplateScanner {
        template,
        cursor: 0,
        finished: false,
    }
}

pub struct TemplateScanner<'a> {
    template: &'a str,
    cursor: usize,
    finished: bool,
}

impl<'a> Iterator for TemplateScanner<'a> {
    type Item = Result<TemplatePart<'a>, &'static str>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished || self.cursor == self.template.len() {
            return None;
        }
        let start = self.cursor;
        while self.cursor < self.template.len() {
            let rest = &self.template[self.cursor..];
            let escaped = rest.starts_with("$${");
            if escaped || rest.starts_with("${") {
                if start < self.cursor {
                    return Some(Ok(TemplatePart::Text(&self.template[start..self.cursor])));
                }
                let name_start = self.cursor + if escaped { 3 } else { 2 };
                let Some(end) = self.template[name_start..]
                    .find('}')
                    .map(|end| name_start + end)
                else {
                    self.finished = true;
                    return Some(Err(if escaped {
                        "unclosed escaped interpolation placeholder"
                    } else {
                        "unclosed interpolation placeholder"
                    }));
                };
                if end == name_start {
                    self.finished = true;
                    return Some(Err(if escaped {
                        "empty escaped interpolation placeholder"
                    } else {
                        "empty interpolation placeholder"
                    }));
                }
                self.cursor = end + 1;
                let name = &self.template[name_start..end];
                return Some(Ok(if escaped {
                    TemplatePart::EscapedReference(name)
                } else {
                    TemplatePart::Reference(name)
                }));
            }
            self.cursor += rest.chars().next().unwrap().len_utf8();
        }
        Some(Ok(TemplatePart::Text(&self.template[start..self.cursor])))
    }
}

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
    for part in scan_template(template) {
        match part.map_err(malformed)? {
            TemplatePart::Text(text) => output.push_str(text),
            TemplatePart::EscapedReference(name) => {
                output.push_str("${");
                output.push_str(name);
                output.push('}');
            }
            TemplatePart::Reference(name) => {
                let value = variables.get(name).ok_or_else(|| {
                    ExecutionDiagnostic::new(
                        DiagnosticKind::InvalidTarget,
                        "interpolation variable is undefined",
                    )
                    .context("variable", name)
                })?;
                output.push_str(&format_interpolation_value(name, value)?);
            }
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
    fn streaming_scanner_preserves_first_resolution_error_and_exact_reads() {
        let error = interpolate("${missing} ${", &variables()).unwrap_err();
        assert_eq!(
            error.context.get("variable").map(String::as_str),
            Some("missing")
        );
        let reads: Vec<_> = scan_template("$${escaped} ${挨拶} ${mouse.x} ${ exact }")
            .filter_map(|part| match part.unwrap() {
                TemplatePart::Reference(name) => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(reads, ["挨拶", "mouse.x", " exact "]);
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
