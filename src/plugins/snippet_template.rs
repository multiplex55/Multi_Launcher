use std::collections::HashSet;
use std::fmt;

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
    use super::{ParsedTemplate, TemplateErrorKind, TemplateSegment, TemplateSpan, parse_template};

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
}
