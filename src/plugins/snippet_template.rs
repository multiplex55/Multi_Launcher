use std::collections::{HashMap, HashSet};
use std::fmt;

use super::snippets::SnippetFieldDefinition;

const OPEN: &[u8; 2] = b"{{";
const CLOSE: &[u8; 2] = b"}}";

enum PlaceholderBoundary {
    Closing(usize),
    Nested(usize),
    End,
}

/// A parsed snippet template, with literal text and placeholder segments kept in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedTemplate {
    pub(crate) segments: Vec<TemplateSegment>,
    pub(crate) field_keys: Vec<String>,
}

/// Segments used by the parser and the later pure renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplateSegment {
    Literal(String),
    Field(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderIssueKind {
    DuplicateFieldDefinition,
    MissingFieldDefinition,
    MissingValue,
    RequiredValueEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderIssue {
    pub(crate) key: String,
    pub(crate) kind: RenderIssueKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderErrorKind {
    InvalidConfiguration,
    MissingValues,
    RequiredValues,
}

impl RenderErrorKind {
    fn reason(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid field configuration",
            Self::MissingValues => "missing field values",
            Self::RequiredValues => "required fields need values",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TemplateRenderError {
    pub(crate) kind: RenderErrorKind,
    pub(crate) issues: Vec<RenderIssue>,
}

impl fmt::Display for TemplateRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ({} field issue(s))",
            self.kind.reason(),
            self.issues.len()
        )
    }
}

impl std::error::Error for TemplateRenderError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedPreview {
    pub(crate) text: String,
    pub(crate) validation_errors: Vec<RenderIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplateErrorKind {
    UnclosedPlaceholder,
    EmptyPlaceholder,
    InvalidIdentifier,
    NestedPlaceholder,
    UnexpectedClosing,
}

impl TemplateErrorKind {
    fn reason(self) -> &'static str {
        match self {
            Self::UnclosedPlaceholder => "unclosed placeholder",
            Self::EmptyPlaceholder => "empty placeholder",
            Self::InvalidIdentifier => "invalid placeholder identifier",
            Self::NestedPlaceholder => "nested placeholder",
            Self::UnexpectedClosing => "unexpected closing braces",
        }
    }
}

/// A byte span in the original UTF-8 template. It never stores template content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TemplateSpan {
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TemplateError {
    pub(crate) kind: TemplateErrorKind,
    pub(crate) span: TemplateSpan,
}

impl TemplateError {
    fn new(kind: TemplateErrorKind, start_byte: usize, end_byte: usize) -> Self {
        Self {
            kind,
            span: TemplateSpan {
                start_byte,
                end_byte,
            },
        }
    }
}

impl fmt::Display for TemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at bytes {}..{}",
            self.kind.reason(),
            self.span.start_byte,
            self.span.end_byte
        )
    }
}

impl std::error::Error for TemplateError {}

/// Parse placeholders left to right without interpreting ordinary backslashes.
///
/// `\{{...}}` is a literal opening and consumes through the next `}}`, even if
/// its interior would be invalid as a placeholder. If that escaped opening is
/// unfinished, it becomes a literal `{{` and the rest of the text is preserved.
/// With two backslashes before an opening, the first remains literal and only
/// the adjacent second backslash is consumed as the escape.
pub(crate) fn parse_template(source: &str) -> Result<ParsedTemplate, TemplateError> {
    let bytes = source.as_bytes();
    let mut segments = Vec::new();
    let mut field_keys = Vec::new();
    let mut seen_keys = HashSet::new();
    let mut index = 0;
    let mut literal_start = 0;

    while index < bytes.len() {
        if bytes[index] == b'\\' && starts_with_pair(bytes, index + 1, OPEN) {
            push_literal(&mut segments, &source[literal_start..index]);
            if let Some(close) = find_pair(bytes, index + 3, CLOSE) {
                push_literal(&mut segments, &source[index + 1..close + 2]);
                index = close + 2;
                literal_start = index;
            } else {
                push_literal(&mut segments, &source[index + 1..]);
                return Ok(ParsedTemplate {
                    segments,
                    field_keys,
                });
            }
            continue;
        }

        if starts_with_pair(bytes, index, OPEN) {
            push_literal(&mut segments, &source[literal_start..index]);
            let identifier_start = index + 2;
            let close = match find_placeholder_boundary(bytes, identifier_start) {
                PlaceholderBoundary::Closing(close) => close,
                PlaceholderBoundary::Nested(nested) => {
                    return Err(TemplateError::new(
                        TemplateErrorKind::NestedPlaceholder,
                        nested,
                        nested + 2,
                    ));
                }
                PlaceholderBoundary::End => {
                    return Err(TemplateError::new(
                        TemplateErrorKind::UnclosedPlaceholder,
                        index,
                        bytes.len(),
                    ));
                }
            };
            if identifier_start == close {
                return Err(TemplateError::new(
                    TemplateErrorKind::EmptyPlaceholder,
                    index,
                    close + 2,
                ));
            }
            let identifier = &source[identifier_start..close];
            if !is_valid_identifier(identifier.as_bytes()) {
                return Err(TemplateError::new(
                    TemplateErrorKind::InvalidIdentifier,
                    identifier_start,
                    close,
                ));
            }
            let key = identifier.to_owned();
            if seen_keys.insert(key.clone()) {
                field_keys.push(key.clone());
            }
            segments.push(TemplateSegment::Field(key));
            index = close + 2;
            literal_start = index;
            continue;
        }

        if starts_with_pair(bytes, index, CLOSE) {
            return Err(TemplateError::new(
                TemplateErrorKind::UnexpectedClosing,
                index,
                index + 2,
            ));
        }

        let character = source[index..]
            .chars()
            .next()
            .expect("index is before the end of the UTF-8 source");
        index += character.len_utf8();
    }

    push_literal(&mut segments, &source[literal_start..]);
    Ok(ParsedTemplate {
        segments,
        field_keys,
    })
}

/// Render a completed template only when configuration and required values validate.
pub(crate) fn render_for_copy(
    template: &ParsedTemplate,
    fields: &[SnippetFieldDefinition],
    values: &HashMap<String, String>,
) -> Result<String, TemplateRenderError> {
    let rendered = evaluate_template(template, fields, values)?;
    if rendered.validation_errors.is_empty() {
        Ok(rendered.text)
    } else {
        Err(TemplateRenderError {
            kind: RenderErrorKind::RequiredValues,
            issues: rendered.validation_errors,
        })
    }
}

/// Render the current inputs for preview, including required-field errors.
/// Missing values or invalid field configuration return an error without partial text.
pub(crate) fn render_preview(
    template: &ParsedTemplate,
    fields: &[SnippetFieldDefinition],
    values: &HashMap<String, String>,
) -> Result<RenderedPreview, TemplateRenderError> {
    let rendered = evaluate_template(template, fields, values)?;
    Ok(RenderedPreview {
        text: rendered.text,
        validation_errors: rendered.validation_errors,
    })
}

struct EvaluatedTemplate {
    text: String,
    validation_errors: Vec<RenderIssue>,
}

fn evaluate_template(
    template: &ParsedTemplate,
    fields: &[SnippetFieldDefinition],
    values: &HashMap<String, String>,
) -> Result<EvaluatedTemplate, TemplateRenderError> {
    let mut definitions = HashMap::with_capacity(fields.len());
    let mut duplicate_definitions = Vec::new();
    for field in fields {
        if definitions.insert(field.name.as_str(), field).is_some() {
            duplicate_definitions.push(RenderIssue {
                key: field.name.clone(),
                kind: RenderIssueKind::DuplicateFieldDefinition,
            });
        }
    }
    if !duplicate_definitions.is_empty() {
        return Err(TemplateRenderError {
            kind: RenderErrorKind::InvalidConfiguration,
            issues: duplicate_definitions,
        });
    }

    let missing_definitions = template
        .field_keys
        .iter()
        .filter(|key| !definitions.contains_key(key.as_str()))
        .map(|key| RenderIssue {
            key: key.clone(),
            kind: RenderIssueKind::MissingFieldDefinition,
        })
        .collect::<Vec<_>>();
    if !missing_definitions.is_empty() {
        return Err(TemplateRenderError {
            kind: RenderErrorKind::InvalidConfiguration,
            issues: missing_definitions,
        });
    }

    let missing_values = template
        .field_keys
        .iter()
        .filter(|key| !values.contains_key(key.as_str()))
        .map(|key| RenderIssue {
            key: key.clone(),
            kind: RenderIssueKind::MissingValue,
        })
        .collect::<Vec<_>>();
    if !missing_values.is_empty() {
        return Err(TemplateRenderError {
            kind: RenderErrorKind::MissingValues,
            issues: missing_values,
        });
    }

    let validation_errors = template
        .field_keys
        .iter()
        .filter_map(|key| {
            let definition = definitions.get(key.as_str())?;
            let value = values.get(key.as_str())?;
            (definition.required && value.trim().is_empty()).then(|| RenderIssue {
                key: key.clone(),
                kind: RenderIssueKind::RequiredValueEmpty,
            })
        })
        .collect();

    let mut text = String::new();
    for segment in &template.segments {
        match segment {
            TemplateSegment::Literal(literal) => text.push_str(literal),
            TemplateSegment::Field(key) => {
                let Some(value) = values.get(key) else {
                    return Err(TemplateRenderError {
                        kind: RenderErrorKind::MissingValues,
                        issues: vec![RenderIssue {
                            key: key.clone(),
                            kind: RenderIssueKind::MissingValue,
                        }],
                    });
                };
                text.push_str(value);
            }
        }
    }

    Ok(EvaluatedTemplate {
        text,
        validation_errors,
    })
}

fn starts_with_pair(source: &[u8], index: usize, pair: &[u8; 2]) -> bool {
    source
        .get(index..)
        .is_some_and(|remaining| remaining.starts_with(pair))
}

fn find_pair(source: &[u8], from: usize, pair: &[u8; 2]) -> Option<usize> {
    source
        .get(from..)?
        .windows(2)
        .position(|window| window == pair.as_slice())
        .map(|offset| from + offset)
}

fn find_placeholder_boundary(source: &[u8], from: usize) -> PlaceholderBoundary {
    let mut index = from;
    while index + 1 < source.len() {
        if starts_with_pair(source, index, CLOSE) {
            return PlaceholderBoundary::Closing(index);
        }
        if starts_with_pair(source, index, OPEN) {
            return PlaceholderBoundary::Nested(index);
        }
        index += 1;
    }
    PlaceholderBoundary::End
}

fn is_valid_identifier(identifier: &[u8]) -> bool {
    let Some(first) = identifier.first() else {
        return false;
    };
    matches!(first, b'A'..=b'Z' | b'a'..=b'z' | b'_')
        && identifier[1..]
            .iter()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

fn push_literal(segments: &mut Vec<TemplateSegment>, literal: &str) {
    if literal.is_empty() {
        return;
    }
    if let Some(TemplateSegment::Literal(previous)) = segments.last_mut() {
        previous.push_str(literal);
    } else {
        segments.push(TemplateSegment::Literal(literal.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedTemplate, RenderErrorKind, RenderIssueKind, TemplateErrorKind, TemplateSegment,
        TemplateSpan, parse_template, render_for_copy, render_preview,
    };
    use crate::plugins::snippets::{SnippetFieldDefinition, SnippetInputKind};
    use std::collections::HashMap;

    fn field(name: &str, required: bool, default_value: &str) -> SnippetFieldDefinition {
        SnippetFieldDefinition {
            name: name.into(),
            label: format!("Label for {name}"),
            default_value: default_value.into(),
            required,
            input_kind: SnippetInputKind::Multiline,
        }
    }

    #[test]
    fn preserves_literal_utf8_multiline_and_ordinary_backslashes() {
        let source = "λ {ordinary}\r\n第二行 🚀\\tail\n";

        let parsed = parse_template(source).unwrap();

        assert_eq!(parsed.field_keys, Vec::<String>::new());
        assert_eq!(
            parsed.segments,
            vec![TemplateSegment::Literal(source.to_owned())]
        );
    }

    #[test]
    fn discovers_unique_case_sensitive_keys_in_first_occurrence_order() {
        let parsed =
            parse_template("λ {{name}}{{ticket_id}} / {{name}} / {{Name}} {{_}} {{_field_9}}")
                .unwrap();

        assert_eq!(
            parsed.field_keys,
            vec!["name", "ticket_id", "Name", "_", "_field_9"]
        );
        assert_eq!(
            parsed.segments,
            vec![
                TemplateSegment::Literal("λ ".into()),
                TemplateSegment::Field("name".into()),
                TemplateSegment::Field("ticket_id".into()),
                TemplateSegment::Literal(" / ".into()),
                TemplateSegment::Field("name".into()),
                TemplateSegment::Literal(" / ".into()),
                TemplateSegment::Field("Name".into()),
                TemplateSegment::Literal(" ".into()),
                TemplateSegment::Field("_".into()),
                TemplateSegment::Literal(" ".into()),
                TemplateSegment::Field("_field_9".into()),
            ]
        );
    }

    #[test]
    fn escaped_openings_are_literal_and_consume_only_the_adjacent_escape() {
        let parsed = parse_template(r"{{name}} \{{bad-key}} \\{{also bad}} \{{").unwrap();

        assert_eq!(parsed.field_keys, vec!["name"]);
        assert_eq!(
            parsed.segments,
            vec![
                TemplateSegment::Field("name".into()),
                TemplateSegment::Literal(r" {{bad-key}} \{{also bad}} {{".into()),
            ]
        );
    }

    #[test]
    fn escaped_opening_ignores_invalid_nested_interior_through_next_close() {
        let parsed = parse_template(r"\{{bad key {{nested}} tail {{name}}").unwrap();

        assert_eq!(parsed.field_keys, vec!["name"]);
        assert_eq!(
            parsed.segments,
            vec![
                TemplateSegment::Literal("{{bad key {{nested}} tail ".into()),
                TemplateSegment::Field("name".into()),
            ]
        );
    }

    #[test]
    fn literal_only_and_date_placeholders_are_ordinary_parse_results() {
        assert_eq!(
            parse_template("literal only").unwrap(),
            ParsedTemplate {
                segments: vec![TemplateSegment::Literal("literal only".into())],
                field_keys: Vec::new(),
            }
        );
        assert_eq!(parse_template("{{date}}").unwrap().field_keys, vec!["date"]);
    }

    #[test]
    fn malformed_tokens_report_safe_reasons_and_byte_spans() {
        let cases = [
            ("PRIVATE {{", TemplateErrorKind::UnclosedPlaceholder, 8, 10),
            ("{{}}", TemplateErrorKind::EmptyPlaceholder, 0, 4),
            ("{{bad-key}}", TemplateErrorKind::InvalidIdentifier, 2, 9),
            ("{{9lives}}", TemplateErrorKind::InvalidIdentifier, 2, 8),
            (
                "{{name value}}",
                TemplateErrorKind::InvalidIdentifier,
                2,
                12,
            ),
            (
                "{{outer{{inner}}",
                TemplateErrorKind::NestedPlaceholder,
                7,
                9,
            ),
            ("PRIVATE }}", TemplateErrorKind::UnexpectedClosing, 8, 10),
            ("{{名字}}", TemplateErrorKind::InvalidIdentifier, 2, 8),
        ];

        for (source, kind, start_byte, end_byte) in cases {
            let error = parse_template(source).unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(
                error.span,
                TemplateSpan {
                    start_byte,
                    end_byte
                }
            );
            assert!(
                !error.to_string().contains(source),
                "error must not expose source text"
            );
        }
    }

    #[test]
    fn strict_render_preserves_unicode_newlines_backslashes_and_opaque_repeated_values() {
        let template =
            parse_template("λ:\r\n{{name}} / {{name}}\r\nTicket: {{ticket}}\\end").unwrap();
        let fields = vec![
            field("name", true, "unused name default"),
            field("ticket", true, ""),
        ];
        let name = "Ada {{ticket}}\n雪".to_owned();
        let ticket = "T-7\r\nB".to_owned();
        let values = HashMap::from([
            ("name".to_owned(), name.clone()),
            ("ticket".to_owned(), ticket.clone()),
        ]);

        let rendered = render_for_copy(&template, &fields, &values).unwrap();
        let preview = render_preview(&template, &fields, &values).unwrap();

        assert_eq!(
            rendered,
            format!("λ:\r\n{name} / {name}\r\nTicket: {ticket}\\end")
        );
        assert_eq!(preview.text, rendered);
        assert!(preview.validation_errors.is_empty());
    }

    #[test]
    fn required_values_reject_blank_but_preserve_nonblank_whitespace() {
        let template = parse_template("Name={{name}}").unwrap();
        let fields = vec![field("name", true, "CONFIGURED_DEFAULT_SECRET")];

        for blank in ["", " \t\r\n"] {
            let values = HashMap::from([("name".to_owned(), blank.to_owned())]);
            let error = render_for_copy(&template, &fields, &values).unwrap_err();
            assert_eq!(error.kind, RenderErrorKind::RequiredValues);
            assert_eq!(error.issues.len(), 1);
            assert_eq!(error.issues[0].key, "name");
            assert_eq!(error.issues[0].kind, RenderIssueKind::RequiredValueEmpty);
            assert!(!format!("{error:?} {error}").contains("CONFIGURED_DEFAULT_SECRET"));

            let preview = render_preview(&template, &fields, &values).unwrap();
            assert_eq!(preview.text, format!("Name={blank}"));
            assert_eq!(preview.validation_errors, error.issues);
        }

        let preserved = "  Ada  ".to_owned();
        let values = HashMap::from([("name".to_owned(), preserved.clone())]);
        assert_eq!(
            render_for_copy(&template, &fields, &values).unwrap(),
            format!("Name={preserved}")
        );
    }

    #[test]
    fn optional_empty_is_valid_but_every_referenced_value_must_be_present() {
        let template = parse_template("before{{notes}}after").unwrap();
        let fields = vec![field("notes", false, "DEFAULT_NOT_USED")];
        let values = HashMap::from([("notes".to_owned(), String::new())]);

        assert_eq!(
            render_for_copy(&template, &fields, &values).unwrap(),
            "beforeafter"
        );

        let error = render_for_copy(&template, &fields, &HashMap::new()).unwrap_err();
        assert_eq!(error.kind, RenderErrorKind::MissingValues);
        assert_eq!(error.issues[0].key, "notes");
        assert_eq!(error.issues[0].kind, RenderIssueKind::MissingValue);
    }

    #[test]
    fn configured_defaults_are_not_renderer_fallbacks_or_restored_after_clear() {
        let template = parse_template("{{name}}").unwrap();
        let fields = vec![field("name", true, "DEFAULT_SECRET")];

        let overridden = HashMap::from([("name".to_owned(), "Grace".to_owned())]);
        assert_eq!(
            render_for_copy(&template, &fields, &overridden).unwrap(),
            "Grace"
        );

        let cleared = HashMap::from([("name".to_owned(), String::new())]);
        let error = render_for_copy(&template, &fields, &cleared).unwrap_err();
        assert_eq!(error.kind, RenderErrorKind::RequiredValues);
        assert!(!format!("{error:?} {error}").contains("DEFAULT_SECRET"));

        let missing = render_for_copy(&template, &fields, &HashMap::new()).unwrap_err();
        assert_eq!(missing.kind, RenderErrorKind::MissingValues);
    }

    #[test]
    fn preview_and_copy_share_validation_and_never_return_partial_text() {
        let source = "OUTPUT_SENTINEL {{name}}";
        let template = parse_template(source).unwrap();
        let fields = vec![field("name", true, "DEFAULT_SENTINEL")];
        let blank = HashMap::from([("name".to_owned(), " \t".to_owned())]);

        let preview = render_preview(&template, &fields, &blank).unwrap();
        assert_eq!(preview.text, "OUTPUT_SENTINEL  \t");
        assert_eq!(preview.validation_errors.len(), 1);
        let copy_error = render_for_copy(&template, &fields, &blank).unwrap_err();
        assert_eq!(copy_error.issues, preview.validation_errors);

        let unrelated_value =
            HashMap::from([("unrelated".to_owned(), "USER_VALUE_SENTINEL".to_owned())]);
        let missing = render_preview(&template, &fields, &unrelated_value).unwrap_err();
        assert_eq!(missing.kind, RenderErrorKind::MissingValues);
        let diagnostics = format!("{missing:?} {missing}");
        assert!(!diagnostics.contains("OUTPUT_SENTINEL"));
        assert!(!diagnostics.contains("DEFAULT_SENTINEL"));
        assert!(!diagnostics.contains("USER_VALUE_SENTINEL"));
    }

    #[test]
    fn duplicate_or_missing_definitions_are_safe_configuration_errors() {
        let template = parse_template("{{name}}").unwrap();
        let duplicated = vec![
            field("name", true, "FIRST_DEFAULT_SECRET"),
            field("name", false, "SECOND_DEFAULT_SECRET"),
        ];
        let values = HashMap::from([("name".to_owned(), "USER_VALUE_SECRET".to_owned())]);

        let duplicate_error = render_for_copy(&template, &duplicated, &values).unwrap_err();
        assert_eq!(duplicate_error.kind, RenderErrorKind::InvalidConfiguration);
        assert_eq!(
            duplicate_error.issues[0].kind,
            RenderIssueKind::DuplicateFieldDefinition
        );
        let missing_error = render_for_copy(&template, &[], &values).unwrap_err();
        assert_eq!(missing_error.kind, RenderErrorKind::InvalidConfiguration);
        assert_eq!(
            missing_error.issues[0].kind,
            RenderIssueKind::MissingFieldDefinition
        );
        for error in [&duplicate_error, &missing_error] {
            let diagnostic = format!("{error:?} {error}");
            assert!(!diagnostic.contains("FIRST_DEFAULT_SECRET"));
            assert!(!diagnostic.contains("SECOND_DEFAULT_SECRET"));
            assert!(!diagnostic.contains("USER_VALUE_SECRET"));
        }
    }
}
